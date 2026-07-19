use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::time::Duration;
use tokio::sync::mpsc as async_mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::terminal::protocol::ControlMsg;

pub struct RunnerConfig {
    pub server: String,
    pub name: String,
    pub shell: String,
    /// If Some, spawned via `sh -c <command>` instead of `shell`.
    pub command: Option<String>,
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .unwrap_or_else(|_| "unknown".to_string())
        .trim()
        .to_string()
}

pub fn default_name() -> String {
    hostname()
}

/// Drains PTY output for `dur` while there is no active WebSocket.
/// Returns true if the shell has exited (channel closed).
async fn drain_for(rx: &mut async_mpsc::UnboundedReceiver<Vec<u8>>, dur: Duration) -> bool {
    let sleep = tokio::time::sleep(dur);
    tokio::pin!(sleep);
    loop {
        tokio::select! {
            _ = &mut sleep => return false,
            msg = rx.recv() => {
                if msg.is_none() {
                    return true;
                }
                // discard while disconnected
            }
        }
    }
}

pub async fn run(config: RunnerConfig) -> Result<()> {
    let ws_url = if let Some(rest) = config.server.strip_prefix("http://") {
        format!("ws://{rest}")
    } else if let Some(rest) = config.server.strip_prefix("https://") {
        format!("wss://{rest}")
    } else {
        config.server.clone()
    };
    let url = format!("{}/ws/runner", ws_url.trim_end_matches('/'));

    // PTY + shell + IO threads set up once. They outlive any single WS session.
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut cmd = match &config.command {
        Some(c) => {
            let mut b = CommandBuilder::new("sh");
            b.arg("-c");
            b.arg(c);
            b
        }
        None => CommandBuilder::new(&config.shell),
    };
    // xterm.js is xterm-compatible. Force TERM so the child gets colors/keys
    // regardless of how the runner was launched (systemd services have no TERM).
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    let _child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    let (pty_out_tx, mut pty_out_rx) = async_mpsc::unbounded_channel::<Vec<u8>>();
    let (stdin_tx, stdin_rx) = std::sync::mpsc::channel::<Vec<u8>>();

    let mut pty_reader = pair.master.try_clone_reader()?;
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match std::io::Read::read(&mut pty_reader, &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if pty_out_tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut pty_writer = pair.master.take_writer()?;
    std::thread::spawn(move || {
        while let Ok(data) = stdin_rx.recv() {
            if std::io::Write::write_all(&mut pty_writer, &data).is_err() {
                break;
            }
        }
    });

    let master = pair.master;

    let initial_backoff = Duration::from_secs(1);
    let max_backoff = Duration::from_secs(30);
    let mut backoff = initial_backoff;

    'outer: loop {
        eprintln!("Connecting to {url} as {:?}", config.name);

        // Race connect with pty draining so output during a slow connect doesn't pile up.
        let connect_fut = connect_async(&url);
        tokio::pin!(connect_fut);
        let ws = loop {
            tokio::select! {
                r = &mut connect_fut => break r,
                msg = pty_out_rx.recv() => {
                    if msg.is_none() {
                        return Ok(());
                    }
                }
            }
        };

        let ws = match ws {
            Ok((w, _)) => w,
            Err(e) => {
                eprintln!("Connect failed: {e}. Retrying in {:?}", backoff);
                if drain_for(&mut pty_out_rx, backoff).await {
                    return Ok(());
                }
                backoff = (backoff * 2).min(max_backoff);
                continue 'outer;
            }
        };
        backoff = initial_backoff;

        let (mut sink, mut stream) = ws.split();

        let hello = ControlMsg::Hello {
            name: config.name.clone(),
        };
        if let Err(e) = sink.send(Message::Text(serde_json::to_string(&hello)?)).await {
            eprintln!("Hello send failed: {e}. Reconnecting.");
            continue 'outer;
        }
        eprintln!("Connected");

        loop {
            tokio::select! {
                data = pty_out_rx.recv() => {
                    match data {
                        Some(bytes) => {
                            if sink.send(Message::Binary(bytes)).await.is_err() {
                                eprintln!("WS send failed. Reconnecting.");
                                continue 'outer;
                            }
                        }
                        None => {
                            eprintln!("Shell exited.");
                            let _ = sink.close().await;
                            return Ok(());
                        }
                    }
                }
                msg = stream.next() => {
                    match msg {
                        Some(Ok(Message::Binary(data))) => {
                            let _ = stdin_tx.send(data);
                        }
                        Some(Ok(Message::Text(txt))) => {
                            match serde_json::from_str::<ControlMsg>(&txt) {
                                Ok(ControlMsg::Resize { cols, rows }) => {
                                    let _ = master.resize(PtySize {
                                        rows,
                                        cols,
                                        pixel_width: 0,
                                        pixel_height: 0,
                                    });
                                }
                                Ok(other) => {
                                    eprintln!("Unexpected control msg: {:?}", other);
                                }
                                Err(e) => {
                                    eprintln!("Bad text frame: {e}: {txt}");
                                }
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = sink.send(Message::Pong(data)).await;
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            eprintln!("Server closed connection. Reconnecting.");
                            continue 'outer;
                        }
                        Some(Ok(_)) => {}
                        Some(Err(e)) => {
                            eprintln!("WS error: {e}. Reconnecting.");
                            continue 'outer;
                        }
                    }
                }
            }
        }
    }
}

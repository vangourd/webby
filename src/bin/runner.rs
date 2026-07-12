use anyhow::Result;
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc as async_mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

// Minimal inline copy of ControlMsg — avoids importing from webby lib (which pulls in Leptos)
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ControlMsg {
    Hello { name: String },
    Resize { cols: u16, rows: u16 },
}

#[derive(Parser, Debug)]
#[command(name = "webby-runner", about = "PTY relay runner for webby")]
struct Args {
    /// WebSocket server URL (e.g. ws://localhost:8080)
    #[arg(long)]
    server: String,

    /// Human-readable name for this runner (defaults to hostname)
    #[arg(long, default_value_t = hostname())]
    name: String,

    /// Shell to spawn
    #[arg(long, default_value = "bash")]
    shell: String,
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .unwrap_or_else(|_| "unknown".to_string())
        .trim()
        .to_string()
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let url = format!("{}/ws/runner", args.server.trim_end_matches('/'));
    eprintln!("Connecting to {url} as {:?}", args.name);

    let (ws_stream, _) = connect_async(&url).await?;
    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    // Send Hello
    let hello = ControlMsg::Hello {
        name: args.name.clone(),
    };
    ws_sink
        .send(Message::Text(serde_json::to_string(&hello)?))
        .await?;
    eprintln!("Sent Hello, waiting for PTY data...");

    // Open PTY
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    // Spawn shell
    let cmd = CommandBuilder::new(&args.shell);
    let _child = pair.slave.spawn_command(cmd)?;
    // Drop slave fd so we get EOF when child exits
    drop(pair.slave);

    // Channel: PTY reader → async task
    let (pty_out_tx, mut pty_out_rx) = async_mpsc::unbounded_channel::<Vec<u8>>();

    // Channel: async task → PTY writer thread
    let (stdin_tx, stdin_rx) = std::sync::mpsc::channel::<Vec<u8>>();

    // Spawn blocking thread: reads PTY output and sends to async channel
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

    // Spawn blocking thread: receives bytes from async task and writes to PTY stdin
    let mut pty_writer = pair.master.take_writer()?;
    std::thread::spawn(move || {
        while let Ok(data) = stdin_rx.recv() {
            if std::io::Write::write_all(&mut pty_writer, &data).is_err() {
                break;
            }
        }
    });

    // Main select loop
    loop {
        tokio::select! {
            // PTY output → WebSocket binary frame
            data = pty_out_rx.recv() => {
                match data {
                    Some(bytes) => {
                        if ws_sink.send(Message::Binary(bytes)).await.is_err() {
                            eprintln!("WebSocket send error, exiting");
                            break;
                        }
                    }
                    None => {
                        eprintln!("PTY closed, exiting");
                        break;
                    }
                }
            }

            // WebSocket message → PTY or resize
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        let _ = stdin_tx.send(data);
                    }
                    Some(Ok(Message::Text(txt))) => {
                        match serde_json::from_str::<ControlMsg>(&txt) {
                            Ok(ControlMsg::Resize { cols, rows }) => {
                                let _ = pair.master.resize(PtySize {
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
                        let _ = ws_sink.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        eprintln!("Server closed connection, exiting");
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        eprintln!("WebSocket error: {e}");
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

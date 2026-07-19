#[cfg(feature = "ssr")]
use {
    crate::terminal::protocol::{ControlMsg, RunnerInfo},
    axum::{
        extract::{Extension, Path, WebSocketUpgrade},
        response::IntoResponse,
        Json,
    },
    axum::extract::ws::{Message, WebSocket},
    std::{
        collections::HashMap,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
        time::Duration,
    },
    tokio::sync::{mpsc, oneshot, RwLock},
};

#[cfg(feature = "ssr")]
const RECONNECT_GRACE: Duration = Duration::from_secs(30);

#[cfg(feature = "ssr")]
pub struct RunnerHandle {
    pub name: String,
    /// Replaced when a new runner connection takes over.
    pub to_runner: RwLock<mpsc::UnboundedSender<Message>>,
    /// All attached browser watchers.
    pub watchers: RwLock<HashMap<u64, mpsc::UnboundedSender<Message>>>,
    pub next_watcher_id: AtomicU64,
    /// Last cols/rows we saw from a browser — replayed to a reconnected runner.
    pub last_size: RwLock<Option<(u16, u16)>>,
    /// Bumped whenever a new runner connection takes over. A handler is "current"
    /// while its local gen equals `current_gen`.
    pub current_gen: AtomicU64,
    /// Signals the current handler to exit (used when a newer one takes over).
    pub displace: RwLock<Option<oneshot::Sender<()>>>,
}

#[cfg(feature = "ssr")]
pub struct RunnerRegistry {
    // runner_id -> handle
    runners: RwLock<HashMap<String, Arc<RunnerHandle>>>,
    // name -> runner_id (for reconnect lookup)
    name_index: RwLock<HashMap<String, String>>,
}

#[cfg(feature = "ssr")]
impl RunnerRegistry {
    pub fn new() -> Self {
        Self {
            runners: RwLock::new(HashMap::new()),
            name_index: RwLock::new(HashMap::new()),
        }
    }

    pub async fn list(&self) -> Vec<RunnerInfo> {
        let runners = self.runners.read().await;
        runners
            .iter()
            .map(|(id, h)| RunnerInfo {
                runner_id: id.clone(),
                name: h.name.clone(),
            })
            .collect()
    }
}

#[cfg(feature = "ssr")]
pub type Registry = Arc<RunnerRegistry>;

#[cfg(feature = "ssr")]
pub async fn runner_ws_handler(
    ws: WebSocketUpgrade,
    Extension(registry): Extension<Registry>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_runner(socket, registry))
}

#[cfg(feature = "ssr")]
pub async fn terminal_ws_handler(
    ws: WebSocketUpgrade,
    Path(runner_id): Path<String>,
    Extension(registry): Extension<Registry>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_terminal(socket, runner_id, registry))
}

#[cfg(feature = "ssr")]
pub async fn list_runners_handler(
    Extension(registry): Extension<Registry>,
) -> Json<Vec<RunnerInfo>> {
    let runners = registry.runners.read().await;
    let list = runners
        .iter()
        .map(|(id, handle)| RunnerInfo {
            runner_id: id.clone(),
            name: handle.name.clone(),
        })
        .collect();
    Json(list)
}

#[cfg(feature = "ssr")]
async fn handle_runner(mut socket: WebSocket, registry: Registry) {
    let hello_name = match socket.recv().await {
        Some(Ok(Message::Text(txt))) => match serde_json::from_str::<ControlMsg>(&txt) {
            Ok(ControlMsg::Hello { name }) => name,
            _ => {
                tracing::warn!("Runner sent unexpected first message: {txt}");
                return;
            }
        },
        other => {
            tracing::warn!("Runner did not send Hello, got: {:?}", other);
            return;
        }
    };

    let (to_runner_tx, mut to_runner_rx) = mpsc::unbounded_channel::<Message>();
    let (displace_tx, mut displace_rx) = oneshot::channel::<()>();

    // Get-or-create handle by name. On reconnect we reuse the existing handle,
    // swap in the new sender, bump the generation, and displace the old handler
    // (if any is still running).
    let (runner_id, handle, my_gen, is_reconnect) = {
        let mut names = registry.name_index.write().await;
        let mut runners = registry.runners.write().await;

        let existing = names
            .get(&hello_name)
            .cloned()
            .and_then(|rid| runners.get(&rid).cloned().map(|h| (rid, h)));

        match existing {
            Some((rid, h)) => {
                *h.to_runner.write().await = to_runner_tx;
                let gen = h.current_gen.fetch_add(1, Ordering::AcqRel) + 1;
                if let Some(old) = h.displace.write().await.replace(displace_tx) {
                    let _ = old.send(());
                }
                (rid, h, gen, true)
            }
            None => {
                let rid = uuid::Uuid::new_v4().to_string();
                let h = Arc::new(RunnerHandle {
                    name: hello_name.clone(),
                    to_runner: RwLock::new(to_runner_tx),
                    watchers: RwLock::new(HashMap::new()),
                    next_watcher_id: AtomicU64::new(0),
                    last_size: RwLock::new(None),
                    current_gen: AtomicU64::new(0),
                    displace: RwLock::new(Some(displace_tx)),
                });
                names.insert(hello_name.clone(), rid.clone());
                runners.insert(rid.clone(), h.clone());
                (rid, h, 0, false)
            }
        }
    };

    tracing::info!(
        "Runner {}: id={runner_id} name={hello_name} gen={my_gen}",
        if is_reconnect { "reconnected" } else { "registered" }
    );

    // Replay last known size to reconnected runner so the PTY matches the browser.
    if is_reconnect {
        if let Some((cols, rows)) = *handle.last_size.read().await {
            let msg = ControlMsg::Resize { cols, rows };
            if let Ok(txt) = serde_json::to_string(&msg) {
                let _ = socket.send(Message::Text(txt)).await;
            }
        }
    }

    let mut displaced = false;
    loop {
        tokio::select! {
            _ = &mut displace_rx => {
                displaced = true;
                break;
            }
            msg = to_runner_rx.recv() => {
                match msg {
                    Some(m) => {
                        if socket.send(m).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        let watchers = handle.watchers.read().await;
                        for tx in watchers.values() {
                            let _ = tx.send(Message::Binary(data.clone()));
                        }
                    }
                    Some(Ok(Message::Text(txt))) => {
                        tracing::debug!("Runner control msg: {txt}");
                    }
                    Some(Ok(Message::Ping(d))) => {
                        let _ = socket.send(Message::Pong(d)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("Runner socket closed: id={runner_id} gen={my_gen}");
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        tracing::warn!("Runner socket error: {e}");
                        break;
                    }
                }
            }
        }
    }

    if displaced {
        tracing::info!("Runner handler displaced: id={runner_id} gen={my_gen}");
        return;
    }

    // Grace-period cleanup: if the runner reconnects within RECONNECT_GRACE,
    // the generation counter will have advanced and this task no-ops.
    tokio::spawn(grace_cleanup(
        registry.clone(),
        runner_id.clone(),
        hello_name.clone(),
        handle.clone(),
        my_gen,
    ));
}

#[cfg(feature = "ssr")]
async fn grace_cleanup(
    registry: Registry,
    runner_id: String,
    name: String,
    handle: Arc<RunnerHandle>,
    my_gen: u64,
) {
    tokio::time::sleep(RECONNECT_GRACE).await;
    if handle.current_gen.load(Ordering::Acquire) != my_gen {
        // A new runner took over during the grace period.
        return;
    }

    tracing::info!("Runner cleanup after grace: id={runner_id} name={name}");

    let mut names = registry.name_index.write().await;
    let mut runners = registry.runners.write().await;
    // Re-check under lock in case something raced.
    if handle.current_gen.load(Ordering::Acquire) != my_gen {
        return;
    }
    runners.remove(&runner_id);
    // Only remove name mapping if it still points at us.
    if names.get(&name) == Some(&runner_id) {
        names.remove(&name);
    }
    drop(runners);
    drop(names);

    // Tell watchers the runner is really gone, then drop their channels
    // so their WS loops exit.
    let mut watchers = handle.watchers.write().await;
    let msg = ControlMsg::RunnerDisconnected;
    if let Ok(txt) = serde_json::to_string(&msg) {
        for tx in watchers.values() {
            let _ = tx.send(Message::Text(txt.clone()));
        }
    }
    watchers.clear();
}

#[cfg(feature = "ssr")]
async fn handle_terminal(mut socket: WebSocket, runner_id: String, registry: Registry) {
    let handle = {
        let guard = registry.runners.read().await;
        match guard.get(&runner_id) {
            Some(h) => h.clone(),
            None => {
                tracing::warn!("Terminal request for unknown runner: {runner_id}");
                let msg = ControlMsg::RunnerDisconnected;
                if let Ok(txt) = serde_json::to_string(&msg) {
                    let _ = socket.send(Message::Text(txt)).await;
                }
                return;
            }
        }
    };

    let watcher_id = handle.next_watcher_id.fetch_add(1, Ordering::Relaxed);
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    handle.watchers.write().await.insert(watcher_id, tx);

    let connected_msg = ControlMsg::Connected {
        runner_id: runner_id.clone(),
    };
    if let Ok(txt) = serde_json::to_string(&connected_msg) {
        if socket.send(Message::Text(txt)).await.is_err() {
            handle.watchers.write().await.remove(&watcher_id);
            return;
        }
    }

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(m) => {
                        if socket.send(m).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        let tx = handle.to_runner.read().await.clone();
                        let _ = tx.send(Message::Binary(data));
                    }
                    Some(Ok(Message::Text(txt))) => {
                        match serde_json::from_str::<ControlMsg>(&txt) {
                            Ok(ControlMsg::Resize { cols, rows }) => {
                                *handle.last_size.write().await = Some((cols, rows));
                                let tx = handle.to_runner.read().await.clone();
                                let _ = tx.send(Message::Text(txt));
                            }
                            Ok(other) => {
                                tracing::debug!("Browser sent control: {:?}", other);
                            }
                            Err(e) => {
                                tracing::warn!("Bad control msg from browser: {e}");
                            }
                        }
                    }
                    Some(Ok(Message::Ping(d))) => {
                        let _ = socket.send(Message::Pong(d)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("Browser disconnected from terminal {runner_id}");
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        tracing::warn!("Browser socket error: {e}");
                        break;
                    }
                }
            }
        }
    }

    handle.watchers.write().await.remove(&watcher_id);
}

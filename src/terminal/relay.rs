#[cfg(feature = "ssr")]
use {
    crate::terminal::protocol::{ControlMsg, RunnerInfo},
    axum::{
        extract::{Extension, Path, WebSocketUpgrade},
        response::IntoResponse,
        Json,
    },
    axum::extract::ws::{Message, WebSocket},
    std::{collections::HashMap, sync::Arc},
    tokio::sync::{mpsc, RwLock},
};

#[cfg(feature = "ssr")]
pub struct RunnerHandle {
    pub name: String,
    pub to_runner: mpsc::UnboundedSender<Message>,
    pub to_browser: RwLock<Option<mpsc::UnboundedSender<Message>>>,
}

#[cfg(feature = "ssr")]
pub struct RunnerRegistry {
    runners: RwLock<HashMap<String, Arc<RunnerHandle>>>,
}

#[cfg(feature = "ssr")]
impl RunnerRegistry {
    pub fn new() -> Self {
        Self {
            runners: RwLock::new(HashMap::new()),
        }
    }

    pub async fn list(&self) -> Vec<crate::terminal::protocol::RunnerInfo> {
        let runners = self.runners.read().await;
        runners
            .iter()
            .map(|(id, h)| crate::terminal::protocol::RunnerInfo {
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
    // First message must be Hello
    let hello_msg = match socket.recv().await {
        Some(Ok(Message::Text(txt))) => {
            match serde_json::from_str::<ControlMsg>(&txt) {
                Ok(ControlMsg::Hello { name }) => name,
                _ => {
                    tracing::warn!("Runner sent unexpected first message: {txt}");
                    return;
                }
            }
        }
        other => {
            tracing::warn!("Runner did not send Hello, got: {:?}", other);
            return;
        }
    };

    let runner_id = uuid::Uuid::new_v4().to_string();
    tracing::info!("Runner registered: id={runner_id} name={hello_msg}");

    let (to_runner_tx, mut to_runner_rx) = mpsc::unbounded_channel::<Message>();

    let handle = Arc::new(RunnerHandle {
        name: hello_msg,
        to_runner: to_runner_tx,
        to_browser: RwLock::new(None),
    });

    registry
        .runners
        .write()
        .await
        .insert(runner_id.clone(), handle.clone());

    loop {
        tokio::select! {
            // Outbound: messages queued for the runner (from browser)
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
            // Inbound: messages from the runner
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        // Forward raw PTY bytes to browser if one is attached
                        let guard = handle.to_browser.read().await;
                        if let Some(tx) = guard.as_ref() {
                            let _ = tx.send(Message::Binary(data));
                        }
                    }
                    Some(Ok(Message::Text(txt))) => {
                        tracing::debug!("Runner control msg: {txt}");
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("Runner disconnected: {runner_id}");
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

    // Clean up
    registry.runners.write().await.remove(&runner_id);

    // Notify browser that runner is gone
    let guard = handle.to_browser.read().await;
    if let Some(tx) = guard.as_ref() {
        let msg = ControlMsg::RunnerDisconnected;
        if let Ok(txt) = serde_json::to_string(&msg) {
            let _ = tx.send(Message::Text(txt));
        }
    }
}

#[cfg(feature = "ssr")]
async fn handle_terminal(mut socket: WebSocket, runner_id: String, registry: Registry) {
    // Look up runner
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

    // Create browser channel and register it with the handle
    let (to_browser_tx, mut to_browser_rx) = mpsc::unbounded_channel::<Message>();
    {
        let mut guard = handle.to_browser.write().await;
        *guard = Some(to_browser_tx);
    }

    // Send Connected message to browser
    let connected_msg = ControlMsg::Connected {
        runner_id: runner_id.clone(),
    };
    if let Ok(txt) = serde_json::to_string(&connected_msg) {
        if socket.send(Message::Text(txt)).await.is_err() {
            return;
        }
    }

    loop {
        tokio::select! {
            // Outbound: messages from runner to browser
            msg = to_browser_rx.recv() => {
                match msg {
                    Some(m) => {
                        if socket.send(m).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            // Inbound: messages from browser
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        // Forward raw input to runner
                        let _ = handle.to_runner.send(Message::Binary(data));
                    }
                    Some(Ok(Message::Text(txt))) => {
                        // Forward resize (and other control) messages to runner
                        match serde_json::from_str::<ControlMsg>(&txt) {
                            Ok(ControlMsg::Resize { .. }) => {
                                let _ = handle.to_runner.send(Message::Text(txt));
                            }
                            Ok(other) => {
                                tracing::debug!("Browser sent control: {:?}", other);
                            }
                            Err(e) => {
                                tracing::warn!("Bad control msg from browser: {e}");
                            }
                        }
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

    // Clear the browser channel from the handle
    let mut guard = handle.to_browser.write().await;
    *guard = None;
}

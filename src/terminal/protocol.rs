use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlMsg {
    /// Runner → Server: register with a human-readable name
    Hello { name: String },
    /// Browser → Server: terminal resize request
    Resize { cols: u16, rows: u16 },
    /// Server → Browser: a runner has connected and been assigned an ID
    Connected { runner_id: String },
    /// Server → Browser: the runner this terminal was attached to disconnected
    RunnerDisconnected,
    /// Server → Browser: current list of registered runners
    RunnerList { runners: Vec<RunnerInfo> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerInfo {
    pub runner_id: String,
    pub name: String,
}

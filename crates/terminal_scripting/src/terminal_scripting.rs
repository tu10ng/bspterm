mod handlers;
mod protocol;
mod server;
mod session;
mod tracking;

pub use protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
pub use server::{ScriptingServer, ScriptingServerHandle};
pub use session::{TerminalRegistry, TerminalSession};
pub use tracking::{CommandId, OutputTracker, ReaderId};

use gpui::App;
#[cfg(not(target_os = "windows"))]
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub enum ConnectionInfo {
    #[cfg(not(target_os = "windows"))]
    UnixSocket(PathBuf),
    #[cfg(target_os = "windows")]
    TcpAddress(std::net::SocketAddr),
}

impl ConnectionInfo {
    pub fn to_env_string(&self) -> String {
        match self {
            #[cfg(not(target_os = "windows"))]
            ConnectionInfo::UnixSocket(path) => path.to_string_lossy().into_owned(),
            #[cfg(target_os = "windows")]
            ConnectionInfo::TcpAddress(addr) => format!("tcp://{}", addr),
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn socket_path() -> PathBuf {
    let runtime_dir = dirs::runtime_dir()
        .or_else(|| dirs::cache_dir())
        .unwrap_or_else(|| std::env::temp_dir());
    runtime_dir.join(format!("bspterm-{}.sock", std::process::id()))
}

pub fn init(cx: &mut App) {
    TerminalRegistry::init(cx);
    ScriptingServer::init(cx);
}

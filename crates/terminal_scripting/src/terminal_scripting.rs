mod handlers;
mod protocol;
mod server;
mod session;

pub use protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
pub use server::{ScriptingServer, ScriptingServerHandle};
pub use session::{TerminalRegistry, TerminalSession};

use gpui::App;
use std::path::PathBuf;

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

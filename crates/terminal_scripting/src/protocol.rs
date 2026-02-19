use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const JSONRPC_VERSION: &str = "2.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    pub id: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: Value,
}

impl JsonRpcResponse {
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn error(id: Value, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            result: None,
            error: Some(error),
            id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    pub fn parse_error() -> Self {
        Self {
            code: -32700,
            message: "Parse error".to_string(),
            data: None,
        }
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: -32600,
            message: message.into(),
            data: None,
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("Method not found: {}", method),
            data: None,
        }
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
            data: None,
        }
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: message.into(),
            data: None,
        }
    }

    pub fn terminal_not_found(terminal_id: &str) -> Self {
        Self {
            code: -32000,
            message: format!("Terminal not found: {}", terminal_id),
            data: None,
        }
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self {
            code: -32001,
            message: message.into(),
            data: None,
        }
    }

    pub fn disconnected(message: impl Into<String>) -> Self {
        Self {
            code: -32002,
            message: message.into(),
            data: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub session_type: String,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenContent {
    pub text: String,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub rows: usize,
    pub cols: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSshParams {
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub passphrase: Option<String>,
}

fn default_ssh_port() -> u16 {
    22
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTelnetParams {
    pub host: String,
    #[serde(default = "default_telnet_port")]
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

fn default_telnet_port() -> u16 {
    23
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendParams {
    pub terminal_id: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadParams {
    pub terminal_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitForParams {
    pub terminal_id: String,
    pub pattern: String,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

fn default_timeout() -> u64 {
    30000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunParams {
    pub terminal_id: String,
    pub command: String,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    pub prompt_pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseParams {
    pub terminal_id: String,
}

// TODO: For future session.new_terminal method
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTerminalParams {
    pub ssh: Option<CreateSshParams>,
    pub telnet: Option<CreateTelnetParams>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentTerminalParams {
    pub terminal_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackStartParams {
    pub terminal_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackReadParams {
    pub terminal_id: String,
    pub reader_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackStopParams {
    pub terminal_id: String,
    pub reader_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMarkedParams {
    pub terminal_id: String,
    pub command: String,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    pub prompt_pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadCommandOutputParams {
    pub terminal_id: String,
    pub command_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadTimeRangeParams {
    pub terminal_id: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendCmdParams {
    pub terminal_id: String,
    pub command: String,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    pub prompt_pattern: Option<String>,
    #[serde(default = "default_strip_echo")]
    pub strip_echo: bool,
}

fn default_strip_echo() -> bool {
    true
}

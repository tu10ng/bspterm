use gpui::{App, AppContext, AsyncApp, Entity};
use regex::Regex;
use serde_json::{Value, json};
use settings::Settings;
use std::time::Duration;
use terminal::connection::ssh::{SshAuthConfig, SshConfig};
use terminal::connection::telnet::TelnetConfig;
use terminal::terminal_settings::{self, AlternateScroll, TerminalSettings};
use terminal::{Terminal, TerminalBuilder};
use util::paths::PathStyle;
use uuid::Uuid;

use crate::protocol::{
    CloseParams, CreateSshParams, CreateTelnetParams, CurrentTerminalParams, JsonRpcError,
    JsonRpcRequest, JsonRpcResponse, ReadParams, RunParams, ScreenContent, SendParams,
    WaitForParams,
};
use crate::session::TerminalRegistry;

pub async fn handle_request(request: JsonRpcRequest, cx: &mut AsyncApp) -> JsonRpcResponse {
    let result = match request.method.as_str() {
        "session.current" => handle_session_current(&request, cx).await,
        "session.list" => handle_session_list(cx).await,
        "session.new_terminal" => handle_new_terminal().await,
        "session.create_ssh" => handle_create_ssh(&request, cx).await,
        "session.create_telnet" => handle_create_telnet(&request, cx).await,
        "terminal.send" => handle_terminal_send(&request, cx).await,
        "terminal.read" => handle_terminal_read(&request, cx).await,
        "terminal.wait_for" => handle_terminal_wait_for(&request, cx).await,
        "terminal.run" => handle_terminal_run(&request, cx).await,
        "terminal.close" => handle_terminal_close(&request, cx).await,
        _ => Err(JsonRpcError::method_not_found(&request.method)),
    };

    match result {
        Ok(value) => JsonRpcResponse::success(request.id, value),
        Err(error) => JsonRpcResponse::error(request.id, error),
    }
}

async fn handle_session_current(
    request: &JsonRpcRequest,
    cx: &mut AsyncApp,
) -> Result<Value, JsonRpcError> {
    let params: CurrentTerminalParams = serde_json::from_value(request.params.clone())
        .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;

    cx.update(|cx| {
        if let Some(terminal_id) = params.terminal_id {
            let id = Uuid::parse_str(&terminal_id)
                .map_err(|_| JsonRpcError::invalid_params("Invalid terminal ID"))?;
            let terminal = TerminalRegistry::get_terminal(id, cx)
                .ok_or_else(|| JsonRpcError::terminal_not_found(&terminal_id))?;
            let info = get_terminal_info(id, &terminal, cx);
            Ok(json!(info))
        } else if let Some((id, terminal)) = TerminalRegistry::get_focused(cx) {
            let info = get_terminal_info(id, &terminal, cx);
            Ok(json!(info))
        } else {
            Err(JsonRpcError::internal_error("No focused terminal"))
        }
    })
}

async fn handle_session_list(cx: &mut AsyncApp) -> Result<Value, JsonRpcError> {
    Ok(cx.update(|cx| {
        let sessions = TerminalRegistry::list(cx);
        json!(sessions)
    }))
}

async fn handle_new_terminal() -> Result<Value, JsonRpcError> {
    Err(JsonRpcError::internal_error(
        "new_terminal not yet implemented - requires workspace integration",
    ))
}

async fn handle_create_ssh(
    request: &JsonRpcRequest,
    cx: &mut AsyncApp,
) -> Result<Value, JsonRpcError> {
    let params: CreateSshParams = serde_json::from_value(request.params.clone())
        .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;

    let auth = if let Some(password) = params.password {
        SshAuthConfig::Password(password)
    } else if let Some(key_path) = params.private_key_path {
        SshAuthConfig::PrivateKey {
            path: key_path.into(),
            passphrase: params.passphrase,
        }
    } else {
        SshAuthConfig::Auto
    };

    let mut ssh_config = SshConfig::new(&params.host, params.port);
    ssh_config.username = params.username;
    ssh_config.auth = auth;

    let terminal_task = cx.update(|cx| {
        let settings = TerminalSettings::get_global(cx);
        let cursor_shape = settings.cursor_shape;
        let alternate_scroll = if settings.alternate_scroll != terminal_settings::AlternateScroll::Off {
            AlternateScroll::On
        } else {
            AlternateScroll::Off
        };

        TerminalBuilder::new_with_ssh(
            ssh_config,
            cursor_shape,
            alternate_scroll,
            settings.max_scroll_history_lines,
            0, // window_id not needed for background terminal
            cx,
            PathStyle::local(),
        )
    });

    let terminal_builder = terminal_task
        .await
        .map_err(|e| JsonRpcError::internal_error(&format!("SSH connection failed: {}", e)))?;

    let host = params.host.clone();
    let port = params.port;
    let terminal_id = cx.update(|cx| {
        let terminal = cx.new(|cx| terminal_builder.subscribe(cx));
        let name = format!("ssh://{}:{}", host, port);
        TerminalRegistry::register(&terminal, name, cx)
    });

    Ok(json!({
        "id": terminal_id.to_string(),
        "type": "ssh",
        "host": host,
        "port": port
    }))
}

async fn handle_create_telnet(
    request: &JsonRpcRequest,
    cx: &mut AsyncApp,
) -> Result<Value, JsonRpcError> {
    let params: CreateTelnetParams = serde_json::from_value(request.params.clone())
        .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;

    let mut telnet_config = TelnetConfig::new(&params.host, params.port);
    telnet_config.username = params.username;
    telnet_config.password = params.password;

    let terminal_task = cx.update(|cx| {
        let settings = TerminalSettings::get_global(cx);
        let cursor_shape = settings.cursor_shape;
        let alternate_scroll = if settings.alternate_scroll != terminal_settings::AlternateScroll::Off {
            AlternateScroll::On
        } else {
            AlternateScroll::Off
        };

        TerminalBuilder::new_with_telnet(
            telnet_config,
            cursor_shape,
            alternate_scroll,
            settings.max_scroll_history_lines,
            0, // window_id not needed for background terminal
            cx,
            PathStyle::local(),
        )
    });

    let terminal_builder = terminal_task
        .await
        .map_err(|e| JsonRpcError::internal_error(&format!("Telnet connection failed: {}", e)))?;

    let host = params.host.clone();
    let port = params.port;
    let terminal_id = cx.update(|cx| {
        let terminal = cx.new(|cx| terminal_builder.subscribe(cx));
        let name = format!("telnet://{}:{}", host, port);
        TerminalRegistry::register(&terminal, name, cx)
    });

    Ok(json!({
        "id": terminal_id.to_string(),
        "type": "telnet",
        "host": host,
        "port": port
    }))
}

async fn handle_terminal_send(
    request: &JsonRpcRequest,
    cx: &mut AsyncApp,
) -> Result<Value, JsonRpcError> {
    let params: SendParams = serde_json::from_value(request.params.clone())
        .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;

    cx.update(|cx| {
        let terminal = TerminalRegistry::get_by_id_str(&params.terminal_id, cx)
            .map_err(|e| JsonRpcError::terminal_not_found(&e.to_string()))?;

        terminal.update(cx, |terminal, _cx| {
            terminal.input(params.data.as_bytes().to_vec());
        });

        Ok(json!({"success": true}))
    })
}

async fn handle_terminal_read(
    request: &JsonRpcRequest,
    cx: &mut AsyncApp,
) -> Result<Value, JsonRpcError> {
    let params: ReadParams = serde_json::from_value(request.params.clone())
        .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;

    cx.update(|cx| {
        let terminal = TerminalRegistry::get_by_id_str(&params.terminal_id, cx)
            .map_err(|e| JsonRpcError::terminal_not_found(&e.to_string()))?;

        let content = terminal.read(cx).get_content();
        let last_content = terminal.read(cx).last_content();

        let screen = ScreenContent {
            text: content,
            cursor_row: last_content.cursor.point.line.0 as usize,
            cursor_col: last_content.cursor.point.column.0,
            rows: last_content.terminal_bounds.num_lines(),
            cols: last_content.terminal_bounds.num_columns(),
        };

        Ok(json!(screen))
    })
}

async fn handle_terminal_wait_for(
    request: &JsonRpcRequest,
    cx: &mut AsyncApp,
) -> Result<Value, JsonRpcError> {
    let params: WaitForParams = serde_json::from_value(request.params.clone())
        .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;

    let regex = Regex::new(&params.pattern)
        .map_err(|e| JsonRpcError::invalid_params(format!("Invalid regex: {}", e)))?;

    let terminal_id = params.terminal_id.clone();
    let timeout = Duration::from_millis(params.timeout_ms);
    let start = std::time::Instant::now();

    loop {
        let content = cx.update(|cx| {
            let terminal = TerminalRegistry::get_by_id_str(&terminal_id, cx)
                .map_err(|e| JsonRpcError::terminal_not_found(&e.to_string()))?;
            Ok::<_, JsonRpcError>(terminal.read(cx).get_content())
        })?;

        if regex.is_match(&content) {
            return Ok(json!({
                "matched": true,
                "content": content
            }));
        }

        if start.elapsed() >= timeout {
            return Err(JsonRpcError::timeout(format!(
                "Pattern '{}' not found within timeout",
                params.pattern
            )));
        }

        // This handler runs in async context from Unix socket server, outside GPUI executor
        #[allow(clippy::disallowed_methods)]
        smol::Timer::after(Duration::from_millis(100)).await;
    }
}

async fn handle_terminal_run(
    request: &JsonRpcRequest,
    cx: &mut AsyncApp,
) -> Result<Value, JsonRpcError> {
    let params: RunParams = serde_json::from_value(request.params.clone())
        .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;

    let prompt_pattern = params
        .prompt_pattern
        .unwrap_or_else(|| r"[$#>]\s*$".to_string());
    let regex = Regex::new(&prompt_pattern)
        .map_err(|e| JsonRpcError::invalid_params(format!("Invalid prompt pattern: {}", e)))?;

    let terminal_id = params.terminal_id.clone();
    let timeout = Duration::from_millis(params.timeout_ms);
    let start = std::time::Instant::now();

    let content_before = cx.update(|cx| {
        let terminal = TerminalRegistry::get_by_id_str(&terminal_id, cx)
            .map_err(|e| JsonRpcError::terminal_not_found(&e.to_string()))?;
        Ok::<_, JsonRpcError>(terminal.read(cx).get_content())
    })?;

    let line_count_before = content_before.lines().count();

    cx.update(|cx| {
        let terminal = TerminalRegistry::get_by_id_str(&terminal_id, cx)
            .map_err(|e| JsonRpcError::terminal_not_found(&e.to_string()))?;
        terminal.update(cx, |terminal, _cx| {
            let command_with_newline = format!("{}\n", params.command);
            terminal.input(command_with_newline.as_bytes().to_vec());
        });
        Ok::<_, JsonRpcError>(())
    })?;

    // This handler runs in async context from Unix socket server, outside GPUI executor
    #[allow(clippy::disallowed_methods)]
    smol::Timer::after(Duration::from_millis(50)).await;

    loop {
        let content = cx.update(|cx| {
            let terminal = TerminalRegistry::get_by_id_str(&terminal_id, cx)
                .map_err(|e| JsonRpcError::terminal_not_found(&e.to_string()))?;
            Ok::<_, JsonRpcError>(terminal.read(cx).get_content())
        })?;

        let lines: Vec<&str> = content.lines().collect();
        if lines.len() > line_count_before {
            let last_line = lines.last().unwrap_or(&"");
            if regex.is_match(last_line) {
                let output_lines: Vec<&str> = lines
                    .iter()
                    .skip(line_count_before)
                    .take(lines.len() - line_count_before - 1)
                    .copied()
                    .collect();
                let output = output_lines.join("\n");
                return Ok(json!({
                    "output": output,
                    "success": true
                }));
            }
        }

        if start.elapsed() >= timeout {
            return Err(JsonRpcError::timeout("Command did not complete within timeout"));
        }

        // This handler runs in async context from Unix socket server, outside GPUI executor
        #[allow(clippy::disallowed_methods)]
        smol::Timer::after(Duration::from_millis(100)).await;
    }
}

async fn handle_terminal_close(
    request: &JsonRpcRequest,
    cx: &mut AsyncApp,
) -> Result<Value, JsonRpcError> {
    let params: CloseParams = serde_json::from_value(request.params.clone())
        .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;

    cx.update(|cx| {
        let id = Uuid::parse_str(&params.terminal_id)
            .map_err(|_| JsonRpcError::invalid_params("Invalid terminal ID"))?;
        TerminalRegistry::unregister(id, cx);
        Ok(json!({"success": true}))
    })
}

fn get_terminal_info(id: Uuid, terminal: &Entity<Terminal>, cx: &App) -> Value {
    let term = terminal.read(cx);
    let connected = !term.is_disconnected();
    let session_type = if term.connection_info().is_some() {
        "remote"
    } else {
        "local"
    };

    json!({
        "id": id.to_string(),
        "connected": connected,
        "type": session_type
    })
}

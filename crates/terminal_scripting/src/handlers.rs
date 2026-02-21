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
    JsonRpcRequest, JsonRpcResponse, ReadCommandOutputParams, ReadParams, ReadTimeRangeParams,
    RunMarkedParams, RunParams, ScreenContent, SendCmdParams, SendParams, TrackReadParams,
    TrackStartParams, TrackStopParams, WaitForParams,
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
        "terminal.sendcmd" => handle_terminal_sendcmd(&request, cx).await,
        "terminal.close" => handle_terminal_close(&request, cx).await,
        "terminal.track_start" => handle_track_start(&request, cx).await,
        "terminal.track_read" => handle_track_read(&request, cx).await,
        "terminal.track_stop" => handle_track_stop(&request, cx).await,
        "terminal.run_marked" => handle_run_marked(&request, cx).await,
        "terminal.read_command_output" => handle_read_command_output(&request, cx).await,
        "terminal.read_time_range" => handle_read_time_range(&request, cx).await,
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

    log::info!(
        "[terminal.run] Start processing command: {:?}, timeout_ms: {}",
        params.command,
        params.timeout_ms
    );

    let prompt_pattern = params
        .prompt_pattern
        .unwrap_or_else(|| r"[$#>]\s*$".to_string());
    let regex = Regex::new(&prompt_pattern)
        .map_err(|e| JsonRpcError::invalid_params(format!("Invalid prompt pattern: {}", e)))?;

    let terminal_id = params.terminal_id.clone();
    let timeout = Duration::from_millis(params.timeout_ms);
    let start = std::time::Instant::now();

    let update_start = std::time::Instant::now();
    let content_before = cx.update(|cx| {
        let terminal = TerminalRegistry::get_by_id_str(&terminal_id, cx)
            .map_err(|e| JsonRpcError::terminal_not_found(&e.to_string()))?;
        Ok::<_, JsonRpcError>(terminal.read(cx).get_content())
    })?;
    log::info!(
        "[terminal.run] First cx.update (get content) took {:?}",
        update_start.elapsed()
    );

    let content_before_trimmed = content_before.trim_end();
    let line_count_before = content_before_trimmed.lines().count();
    log::info!(
        "[terminal.run] line_count_before: {}, last_line_before: {:?}",
        line_count_before,
        content_before_trimmed.lines().last()
    );

    let update_start = std::time::Instant::now();
    cx.update(|cx| {
        let terminal = TerminalRegistry::get_by_id_str(&terminal_id, cx)
            .map_err(|e| JsonRpcError::terminal_not_found(&e.to_string()))?;
        terminal.update(cx, |terminal, _cx| {
            let command_with_newline = format!("{}\n", params.command);
            terminal.input(command_with_newline.as_bytes().to_vec());
        });
        Ok::<_, JsonRpcError>(())
    })?;
    log::info!(
        "[terminal.run] Second cx.update (send command) took {:?}",
        update_start.elapsed()
    );

    // This handler runs in async context from Unix socket server, outside GPUI executor
    #[allow(clippy::disallowed_methods)]
    smol::Timer::after(Duration::from_millis(50)).await;

    let mut poll_count = 0u32;
    loop {
        poll_count += 1;
        let update_start = std::time::Instant::now();
        let content = cx.update(|cx| {
            let terminal = TerminalRegistry::get_by_id_str(&terminal_id, cx)
                .map_err(|e| JsonRpcError::terminal_not_found(&e.to_string()))?;
            Ok::<_, JsonRpcError>(terminal.read(cx).get_content())
        })?;
        let update_elapsed = update_start.elapsed();
        if update_elapsed > Duration::from_millis(100) {
            log::warn!(
                "[terminal.run] Poll #{} cx.update took {:?} (slow!)",
                poll_count,
                update_elapsed
            );
        }

        let lines: Vec<&str> = content.trim_end().lines().collect();
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
                log::info!(
                    "[terminal.run] Command completed after {:?}, {} polls",
                    start.elapsed(),
                    poll_count
                );
                return Ok(json!({
                    "output": output,
                    "success": true
                }));
            }
        }

        if start.elapsed() >= timeout {
            let last_line = lines.last().unwrap_or(&"");
            log::warn!(
                "[terminal.run] Timeout after {:?}, {} polls. lines.len(): {}, line_count_before: {}, last_line: {:?}, regex_match: {}",
                start.elapsed(),
                poll_count,
                lines.len(),
                line_count_before,
                last_line,
                regex.is_match(last_line)
            );
            return Err(JsonRpcError::timeout("Command did not complete within timeout"));
        }

        // This handler runs in async context from Unix socket server, outside GPUI executor
        #[allow(clippy::disallowed_methods)]
        smol::Timer::after(Duration::from_millis(100)).await;
    }
}

async fn handle_terminal_sendcmd(
    request: &JsonRpcRequest,
    cx: &mut AsyncApp,
) -> Result<Value, JsonRpcError> {
    let params: SendCmdParams = serde_json::from_value(request.params.clone())
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

    let line_count_before = content_before.trim_end().lines().count();

    cx.update(|cx| {
        let terminal = TerminalRegistry::get_by_id_str(&terminal_id, cx)
            .map_err(|e| JsonRpcError::terminal_not_found(&e.to_string()))?;
        terminal.update(cx, |terminal, _cx| {
            let command_with_newline = format!("{}\n", params.command);
            terminal.input(command_with_newline.as_bytes().to_vec());
        });
        Ok::<_, JsonRpcError>(())
    })?;

    #[allow(clippy::disallowed_methods)]
    smol::Timer::after(Duration::from_millis(50)).await;

    loop {
        let content = cx.update(|cx| {
            let terminal = TerminalRegistry::get_by_id_str(&terminal_id, cx)
                .map_err(|e| JsonRpcError::terminal_not_found(&e.to_string()))?;
            Ok::<_, JsonRpcError>(terminal.read(cx).get_content())
        })?;

        let lines: Vec<&str> = content.trim_end().lines().collect();
        if lines.len() > line_count_before {
            let last_line = lines.last().unwrap_or(&"");
            if regex.is_match(last_line) {
                let skip_count = if params.strip_echo {
                    line_count_before + 1
                } else {
                    line_count_before
                };

                let take_count = lines.len().saturating_sub(skip_count).saturating_sub(1);

                let output_lines: Vec<&str> = lines
                    .iter()
                    .skip(skip_count)
                    .take(take_count)
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

async fn handle_track_start(
    request: &JsonRpcRequest,
    cx: &mut AsyncApp,
) -> Result<Value, JsonRpcError> {
    let params: TrackStartParams = serde_json::from_value(request.params.clone())
        .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;

    cx.update(|cx| {
        let terminal_id = Uuid::parse_str(&params.terminal_id)
            .map_err(|_| JsonRpcError::invalid_params("Invalid terminal ID"))?;

        let reader_id = TerminalRegistry::create_reader(terminal_id, cx)
            .ok_or_else(|| JsonRpcError::terminal_not_found(&params.terminal_id))?;

        Ok(json!({
            "reader_id": reader_id.to_string()
        }))
    })
}

async fn handle_track_read(
    request: &JsonRpcRequest,
    cx: &mut AsyncApp,
) -> Result<Value, JsonRpcError> {
    let params: TrackReadParams = serde_json::from_value(request.params.clone())
        .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;

    cx.update(|cx| {
        let terminal_id = Uuid::parse_str(&params.terminal_id)
            .map_err(|_| JsonRpcError::invalid_params("Invalid terminal ID"))?;
        let reader_id = Uuid::parse_str(&params.reader_id)
            .map_err(|_| JsonRpcError::invalid_params("Invalid reader ID"))?;

        let (content, has_more) = TerminalRegistry::read_new(terminal_id, reader_id, cx)
            .ok_or_else(|| JsonRpcError::invalid_params("Reader not found or expired"))?;

        Ok(json!({
            "content": content,
            "has_more": has_more
        }))
    })
}

async fn handle_track_stop(
    request: &JsonRpcRequest,
    cx: &mut AsyncApp,
) -> Result<Value, JsonRpcError> {
    let params: TrackStopParams = serde_json::from_value(request.params.clone())
        .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;

    cx.update(|cx| {
        let terminal_id = Uuid::parse_str(&params.terminal_id)
            .map_err(|_| JsonRpcError::invalid_params("Invalid terminal ID"))?;
        let reader_id = Uuid::parse_str(&params.reader_id)
            .map_err(|_| JsonRpcError::invalid_params("Invalid reader ID"))?;

        let stopped = TerminalRegistry::stop_reader(terminal_id, reader_id, cx);

        Ok(json!({
            "success": stopped
        }))
    })
}

async fn handle_run_marked(
    request: &JsonRpcRequest,
    cx: &mut AsyncApp,
) -> Result<Value, JsonRpcError> {
    let params: RunMarkedParams = serde_json::from_value(request.params.clone())
        .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;

    let prompt_pattern = params
        .prompt_pattern
        .unwrap_or_else(|| r"[$#>]\s*$".to_string());
    let regex = Regex::new(&prompt_pattern)
        .map_err(|e| JsonRpcError::invalid_params(format!("Invalid prompt pattern: {}", e)))?;

    let terminal_id_str = params.terminal_id.clone();
    let terminal_id = Uuid::parse_str(&terminal_id_str)
        .map_err(|_| JsonRpcError::invalid_params("Invalid terminal ID"))?;

    let timeout = Duration::from_millis(params.timeout_ms);
    let start = std::time::Instant::now();

    let command_id = cx.update(|cx| {
        TerminalRegistry::get_terminal(terminal_id, cx)
            .ok_or_else(|| JsonRpcError::terminal_not_found(&terminal_id_str))?;

        let cmd_id = TerminalRegistry::start_command(terminal_id, params.command.clone(), cx)
            .ok_or_else(|| JsonRpcError::terminal_not_found(&terminal_id_str))?;

        Ok::<_, JsonRpcError>(cmd_id)
    })?;

    let line_count_before = cx.update(|cx| {
        let terminal = TerminalRegistry::get_by_id_str(&terminal_id_str, cx)
            .map_err(|e| JsonRpcError::terminal_not_found(&e.to_string()))?;
        Ok::<_, JsonRpcError>(terminal.read(cx).get_content().trim_end().lines().count())
    })?;

    cx.update(|cx| {
        let terminal = TerminalRegistry::get_by_id_str(&terminal_id_str, cx)
            .map_err(|e| JsonRpcError::terminal_not_found(&e.to_string()))?;
        terminal.update(cx, |terminal, _cx| {
            let command_with_newline = format!("{}\n", params.command);
            terminal.input(command_with_newline.as_bytes().to_vec());
        });
        Ok::<_, JsonRpcError>(())
    })?;

    #[allow(clippy::disallowed_methods)]
    smol::Timer::after(Duration::from_millis(50)).await;

    loop {
        let content = cx.update(|cx| {
            let terminal = TerminalRegistry::get_by_id_str(&terminal_id_str, cx)
                .map_err(|e| JsonRpcError::terminal_not_found(&e.to_string()))?;
            let content = terminal.read(cx).get_content();

            TerminalRegistry::record_output(terminal_id, content.clone(), cx);

            Ok::<_, JsonRpcError>(content)
        })?;

        let lines: Vec<&str> = content.trim_end().lines().collect();
        if lines.len() > line_count_before {
            let last_line = lines.last().unwrap_or(&"");
            if regex.is_match(last_line) {
                cx.update(|cx| {
                    TerminalRegistry::complete_command(terminal_id, command_id, cx);
                });

                let output_lines: Vec<&str> = lines
                    .iter()
                    .skip(line_count_before)
                    .take(lines.len() - line_count_before - 1)
                    .copied()
                    .collect();
                let output = output_lines.join("\n");

                return Ok(json!({
                    "command_id": command_id.to_string(),
                    "output": output,
                    "exit_code": null
                }));
            }
        }

        if start.elapsed() >= timeout {
            cx.update(|cx| {
                TerminalRegistry::complete_command(terminal_id, command_id, cx);
            });
            return Err(JsonRpcError::timeout("Command did not complete within timeout"));
        }

        #[allow(clippy::disallowed_methods)]
        smol::Timer::after(Duration::from_millis(100)).await;
    }
}

async fn handle_read_command_output(
    request: &JsonRpcRequest,
    cx: &mut AsyncApp,
) -> Result<Value, JsonRpcError> {
    let params: ReadCommandOutputParams = serde_json::from_value(request.params.clone())
        .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;

    cx.update(|cx| {
        let terminal_id = Uuid::parse_str(&params.terminal_id)
            .map_err(|_| JsonRpcError::invalid_params("Invalid terminal ID"))?;
        let command_id = Uuid::parse_str(&params.command_id)
            .map_err(|_| JsonRpcError::invalid_params("Invalid command ID"))?;

        let output = TerminalRegistry::get_command_output(terminal_id, command_id, cx)
            .ok_or_else(|| JsonRpcError::invalid_params("Command not found or not completed"))?;

        Ok(json!({
            "output": output
        }))
    })
}

async fn handle_read_time_range(
    request: &JsonRpcRequest,
    cx: &mut AsyncApp,
) -> Result<Value, JsonRpcError> {
    let params: ReadTimeRangeParams = serde_json::from_value(request.params.clone())
        .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;

    cx.update(|cx| {
        let terminal_id = Uuid::parse_str(&params.terminal_id)
            .map_err(|_| JsonRpcError::invalid_params("Invalid terminal ID"))?;

        let content = TerminalRegistry::read_time_range(terminal_id, params.start_ms, params.end_ms, cx)
            .ok_or_else(|| JsonRpcError::invalid_params("No tracker found for terminal"))?;

        Ok(json!({
            "content": content
        }))
    })
}

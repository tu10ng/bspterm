use anyhow::{Context, Result};
use futures::FutureExt;
use gpui::{App, AsyncApp, Global, Task};
use net::async_net::{UnixListener, UnixStream};
use parking_lot::RwLock;
use smol::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use std::path::PathBuf;
use std::sync::Arc;

use crate::handlers::handle_request;
use crate::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

struct GlobalScriptingServer(Arc<RwLock<Option<ScriptingServerHandle>>>);

impl Global for GlobalScriptingServer {}

pub struct ScriptingServerHandle {
    pub socket_path: PathBuf,
    shutdown_tx: smol::channel::Sender<()>,
    _server_task: Task<()>,
}

impl ScriptingServerHandle {
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    pub fn shutdown(&self) {
        self.shutdown_tx.try_send(()).ok();
    }
}

impl Drop for ScriptingServerHandle {
    fn drop(&mut self) {
        self.shutdown_tx.try_send(()).ok();
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path).ok();
        }
    }
}

pub struct ScriptingServer;

impl ScriptingServer {
    pub fn init(cx: &mut App) {
        cx.set_global(GlobalScriptingServer(Arc::new(RwLock::new(None))));

        let socket_path = crate::socket_path();
        if socket_path.exists() {
            std::fs::remove_file(&socket_path).ok();
        }

        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let (shutdown_tx, shutdown_rx) = smol::channel::bounded::<()>(1);

        let server_task = cx.spawn({
            let socket_path = socket_path.clone();
            async move |cx: &mut AsyncApp| {
                if let Err(e) = Self::run_server(socket_path, shutdown_rx, cx).await {
                    log::error!("Scripting server error: {}", e);
                }
            }
        });

        let handle = ScriptingServerHandle {
            socket_path,
            shutdown_tx,
            _server_task: server_task,
        };

        let global = cx.global::<GlobalScriptingServer>();
        *global.0.write() = Some(handle);
    }

    pub fn get(cx: &App) -> Option<PathBuf> {
        let global = cx.global::<GlobalScriptingServer>();
        let inner = global.0.read();
        inner.as_ref().map(|h| h.socket_path.clone())
    }

    async fn run_server(
        socket_path: PathBuf,
        shutdown_rx: smol::channel::Receiver<()>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        let listener =
            UnixListener::bind(&socket_path).context("Failed to bind Unix socket")?;

        log::info!("Scripting server listening on {:?}", socket_path);

        loop {
            futures::select! {
                result = listener.accept().fuse() => {
                    match result {
                        Ok((stream, _)) => {
                            if let Err(e) = Self::handle_client(stream, cx).await {
                                log::error!("Client error: {}", e);
                            }
                        }
                        Err(e) => {
                            log::error!("Accept error: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv().fuse() => {
                    log::info!("Scripting server shutting down");
                    break;
                }
            }
        }

        if socket_path.exists() {
            std::fs::remove_file(&socket_path).ok();
        }

        Ok(())
    }

    async fn handle_client(
        stream: UnixStream,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        let (reader, writer) = smol::io::split(stream);
        let mut reader = BufReader::new(reader);
        let mut writer = writer;
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;
            if bytes_read == 0 {
                break;
            }

            log::debug!("[scripting] Received request: {}", line.trim());
            let request_start = std::time::Instant::now();

            let response = Self::process_message(&line, cx).await;

            log::debug!(
                "[scripting] Request processed in {:?}",
                request_start.elapsed()
            );

            let response_json = serde_json::to_string(&response)? + "\n";
            match writer.write_all(response_json.as_bytes()).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                    log::warn!(
                        "[scripting] Client disconnected before response could be sent (request took {:?})",
                        request_start.elapsed()
                    );
                    break;
                }
                Err(e) => return Err(e.into()),
            }
            if let Err(e) = writer.flush().await {
                if e.kind() == std::io::ErrorKind::BrokenPipe {
                    log::warn!("[scripting] Client disconnected during flush");
                    break;
                }
                return Err(e.into());
            }
        }

        Ok(())
    }

    async fn process_message(message: &str, cx: &mut AsyncApp) -> JsonRpcResponse {
        let request: JsonRpcRequest = match serde_json::from_str(message) {
            Ok(req) => req,
            Err(_) => {
                return JsonRpcResponse::error(
                    serde_json::Value::Null,
                    JsonRpcError::parse_error(),
                );
            }
        };

        if request.jsonrpc != "2.0" {
            return JsonRpcResponse::error(
                request.id,
                JsonRpcError::invalid_request("Invalid JSON-RPC version"),
            );
        }

        handle_request(request, cx).await
    }
}

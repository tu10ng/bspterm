use anyhow::{Context, Result};
use futures::FutureExt;
use gpui::{App, AsyncApp, Global, Task};
#[cfg(not(target_os = "windows"))]
use net::async_net::{UnixListener, UnixStream};
#[cfg(target_os = "windows")]
use smol::net::{TcpListener, TcpStream};
use parking_lot::RwLock;
use smol::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(not(target_os = "windows"))]
use std::path::PathBuf;
use std::sync::Arc;

use crate::handlers::handle_request;
use crate::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use crate::ConnectionInfo;

struct GlobalScriptingServer(Arc<RwLock<Option<ScriptingServerHandle>>>);

impl Global for GlobalScriptingServer {}

pub struct ScriptingServerHandle {
    connection_info: ConnectionInfo,
    shutdown_tx: smol::channel::Sender<()>,
    _server_task: Task<()>,
}

impl ScriptingServerHandle {
    pub fn connection_info(&self) -> &ConnectionInfo {
        &self.connection_info
    }

    pub fn shutdown(&self) {
        self.shutdown_tx.try_send(()).ok();
    }
}

impl Drop for ScriptingServerHandle {
    fn drop(&mut self) {
        self.shutdown_tx.try_send(()).ok();
        #[cfg(not(target_os = "windows"))]
        {
            let ConnectionInfo::UnixSocket(ref path) = self.connection_info;
            if path.exists() {
                std::fs::remove_file(path).ok();
            }
        }
    }
}

pub struct ScriptingServer;

impl ScriptingServer {
    #[cfg(not(target_os = "windows"))]
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
                if let Err(e) = Self::run_server_unix(socket_path, shutdown_rx, cx).await {
                    log::error!("Scripting server error: {}", e);
                }
            }
        });

        let handle = ScriptingServerHandle {
            connection_info: ConnectionInfo::UnixSocket(socket_path),
            shutdown_tx,
            _server_task: server_task,
        };

        let global = cx.global::<GlobalScriptingServer>();
        *global.0.write() = Some(handle);
    }

    #[cfg(target_os = "windows")]
    pub fn init(cx: &mut App) {
        cx.set_global(GlobalScriptingServer(Arc::new(RwLock::new(None))));

        let (shutdown_tx, shutdown_rx) = smol::channel::bounded::<()>(1);

        // Bind synchronously on main thread to avoid deadlock
        let std_listener = match std::net::TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(e) => {
                log::error!("Failed to bind TCP socket for scripting server: {}", e);
                return;
            }
        };
        let local_addr = match std_listener.local_addr() {
            Ok(addr) => addr,
            Err(e) => {
                log::error!("Failed to get local address for scripting server: {}", e);
                return;
            }
        };

        // Set non-blocking for async use
        if let Err(e) = std_listener.set_nonblocking(true) {
            log::error!("Failed to set non-blocking mode: {}", e);
            return;
        }

        log::info!("Scripting server will listen on {:?}", local_addr);

        let server_task = cx.spawn({
            async move |cx: &mut AsyncApp| {
                let listener = TcpListener::from(std_listener);
                if let Err(e) = Self::run_server_tcp_with_listener(listener, shutdown_rx, cx).await
                {
                    log::error!("Scripting server error: {}", e);
                }
            }
        });

        let handle = ScriptingServerHandle {
            connection_info: ConnectionInfo::TcpAddress(local_addr),
            shutdown_tx,
            _server_task: server_task,
        };

        let global = cx.global::<GlobalScriptingServer>();
        *global.0.write() = Some(handle);
    }

    pub fn get(cx: &App) -> Option<ConnectionInfo> {
        let global = cx.global::<GlobalScriptingServer>();
        let inner = global.0.read();
        inner.as_ref().map(|h| h.connection_info.clone())
    }

    #[cfg(not(target_os = "windows"))]
    async fn run_server_unix(
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
                            if let Err(e) = Self::handle_unix_client(stream, cx).await {
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

    #[cfg(target_os = "windows")]
    async fn run_server_tcp_with_listener(
        listener: TcpListener,
        shutdown_rx: smol::channel::Receiver<()>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        log::info!("Scripting server listening on {:?}", listener.local_addr()?);

        loop {
            futures::select! {
                result = listener.accept().fuse() => {
                    match result {
                        Ok((stream, _)) => {
                            if let Err(e) = Self::handle_tcp_client(stream, cx).await {
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

        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    async fn handle_unix_client(
        stream: UnixStream,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        let (reader, writer) = smol::io::split(stream);
        Self::handle_client_stream(reader, writer, cx).await
    }

    #[cfg(target_os = "windows")]
    async fn handle_tcp_client(
        stream: TcpStream,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        let (reader, writer) = smol::io::split(stream);
        Self::handle_client_stream(reader, writer, cx).await
    }

    async fn handle_client_stream<R, W>(
        reader: R,
        mut writer: W,
        cx: &mut AsyncApp,
    ) -> Result<()>
    where
        R: smol::io::AsyncRead + Unpin,
        W: smol::io::AsyncWrite + Unpin,
    {
        let mut reader = BufReader::new(reader);
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

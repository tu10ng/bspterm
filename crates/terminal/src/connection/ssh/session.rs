use std::sync::Arc;
use std::time::Duration;

use alacritty_terminal::event::WindowSize;
use anyhow::{Context as _, Result};
use gpui::Task;
use parking_lot::RwLock;
use russh::client::{Config, Handle};
use russh::ChannelId;
use tokio::sync::RwLock as TokioRwLock;
use tokio::time::timeout;

use super::auth::{authenticate, SshAuthMethod};
use super::{SshConfig, SshHostKey};
use crate::connection::ConnectionState;

struct SshClientHandler {
    host_key_verified: bool,
}

impl SshClientHandler {
    fn new() -> Self {
        Self {
            host_key_verified: false,
        }
    }
}

impl russh::client::Handler for SshClientHandler {
    type Error = anyhow::Error;

    fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send {
        self.host_key_verified = true;
        async { Ok(true) }
    }
}

/// An established SSH session to a remote host.
/// This holds the authenticated connection and can open multiple channels.
pub struct SshSession {
    host_key: SshHostKey,
    handle: TokioRwLock<Option<Handle<SshClientHandler>>>,
    state: RwLock<ConnectionState>,
    #[allow(dead_code)]
    keepalive_task: Option<Task<()>>,
    auth_method: SshAuthMethod,
    terminal_type: String,
}

impl SshSession {
    pub async fn connect(config: &SshConfig) -> Result<Arc<Self>> {
        let connection_timeout = config
            .connection_timeout
            .unwrap_or_else(|| Duration::from_secs(3));

        let ssh_config = Arc::new(Config {
            keepalive_interval: config.keepalive_interval,
            keepalive_max: 3,
            ..Config::default()
        });

        let addr = format!("{}:{}", config.host, config.port);
        let handler = SshClientHandler::new();

        let mut handle = timeout(
            connection_timeout,
            russh::client::connect(ssh_config, &addr, handler),
        )
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "SSH connection timed out after {} seconds",
                connection_timeout.as_secs()
            )
        })?
        .with_context(|| format!("failed to connect to {}", addr))?;

        let username = config
            .username
            .clone()
            .or_else(|| std::env::var("USER").ok())
            .or_else(|| std::env::var("USERNAME").ok())
            .unwrap_or_else(|| "root".to_string());

        let auth_method = authenticate(&mut handle, &username, &config.auth)
            .await
            .context("SSH authentication failed")?;

        let host_key = SshHostKey::from(config);

        let session = Arc::new(Self {
            host_key,
            handle: TokioRwLock::new(Some(handle)),
            state: RwLock::new(ConnectionState::Connected),
            keepalive_task: None,
            auth_method,
            terminal_type: config.terminal_type.clone(),
        });

        Ok(session)
    }

    pub fn host_key(&self) -> &SshHostKey {
        &self.host_key
    }

    pub fn state(&self) -> ConnectionState {
        self.state.read().clone()
    }

    pub fn is_connected(&self) -> bool {
        self.state.read().is_connected()
    }

    pub fn auth_method(&self) -> &SshAuthMethod {
        &self.auth_method
    }

    /// Open a new terminal channel with a PTY.
    pub async fn open_terminal_channel(
        &self,
        initial_size: WindowSize,
        env: &collections::HashMap<String, String>,
    ) -> Result<SshChannel> {
        let handle_guard = self.handle.read().await;
        let handle = handle_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("SSH session is closed"))?;

        let channel = handle
            .channel_open_session()
            .await
            .context("failed to open SSH channel")?;

        let channel_id = channel.id();

        for (key, value) in env {
            if let Err(error) = channel.set_env(true, key, value).await {
                log::warn!("Failed to set SSH environment variable {}: {}", key, error);
            }
        }

        channel
            .request_pty(
                true,
                &self.terminal_type,
                initial_size.num_cols as u32,
                initial_size.num_lines as u32,
                initial_size.cell_width as u32,
                initial_size.cell_height as u32,
                &[],
            )
            .await
            .context("failed to request PTY")?;

        channel
            .request_shell(true)
            .await
            .context("failed to request shell")?;

        Ok(SshChannel {
            channel,
            channel_id,
        })
    }

    pub async fn close(&self) {
        *self.state.write() = ConnectionState::Disconnected;
        if let Some(handle) = self.handle.write().await.take() {
            let _ = handle
                .disconnect(russh::Disconnect::ByApplication, "", "en")
                .await;
        }
    }
}

impl Drop for SshSession {
    fn drop(&mut self) {
        *self.state.write() = ConnectionState::Disconnected;
    }
}

/// A channel within an SSH session.
pub struct SshChannel {
    pub channel: russh::Channel<russh::client::Msg>,
    pub channel_id: ChannelId,
}

impl SshChannel {
    pub async fn write(&mut self, data: &[u8]) -> Result<()> {
        self.channel
            .data(data)
            .await
            .context("failed to write to SSH channel")
    }

    pub async fn resize(&mut self, size: WindowSize) -> Result<()> {
        self.channel
            .window_change(
                size.num_cols as u32,
                size.num_lines as u32,
                size.cell_width as u32,
                size.cell_height as u32,
            )
            .await
            .context("failed to resize SSH channel")
    }

    pub async fn close(&mut self) -> Result<()> {
        self.channel
            .close()
            .await
            .context("failed to close SSH channel")
    }
}

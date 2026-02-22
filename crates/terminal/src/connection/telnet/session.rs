use std::time::Duration;

use anyhow::{Context as _, Result};
use parking_lot::RwLock;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::time::timeout;

use super::TelnetConfig;
use crate::connection::ConnectionState;

pub struct TelnetSession {
    state: RwLock<ConnectionState>,
}

impl TelnetSession {
    pub async fn connect(config: &TelnetConfig) -> Result<(Self, OwnedReadHalf, OwnedWriteHalf)> {
        let connection_timeout = config
            .connection_timeout
            .unwrap_or_else(|| Duration::from_secs(3));

        let addr = format!("{}:{}", config.host, config.port);

        let stream = timeout(connection_timeout, TcpStream::connect(&addr))
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "Telnet connection timed out after {} seconds",
                    connection_timeout.as_secs()
                )
            })?
            .with_context(|| format!("failed to connect to {}", addr))?;

        stream.set_nodelay(true).ok();

        let (read_half, write_half) = stream.into_split();

        let session = Self {
            state: RwLock::new(ConnectionState::Connected),
        };

        Ok((session, read_half, write_half))
    }

    pub fn state(&self) -> ConnectionState {
        self.state.read().clone()
    }

    pub fn set_state(&self, state: ConnectionState) {
        *self.state.write() = state;
    }

    pub fn is_connected(&self) -> bool {
        self.state.read().is_connected()
    }
}

impl Drop for TelnetSession {
    fn drop(&mut self) {
        *self.state.write() = ConnectionState::Disconnected;
    }
}

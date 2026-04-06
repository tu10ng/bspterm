use std::sync::Arc;

use anyhow::Result;
use collections::HashMap;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};

use crate::connection::ssh::{SftpClient, SshConfig, SshHostKey, SshSession};

pub enum SftpStoreEvent {
    ClientConnected(SshHostKey),
    ClientDisconnected(SshHostKey),
}

pub struct SftpStore {
    clients: HashMap<SshHostKey, Arc<SftpClient>>,
    /// Keep `SshSession` alive via `Arc` — dropping the session closes the SSH connection,
    /// which would invalidate the SFTP client. This field is intentionally only written to.
    sessions: HashMap<SshHostKey, Arc<SshSession>>,
}

impl SftpStore {
    pub fn new() -> Self {
        Self {
            clients: HashMap::default(),
            sessions: HashMap::default(),
        }
    }

    /// Get an existing SFTP client for the given host, or connect and create one.
    pub fn get_or_connect(
        &self,
        config: SshConfig,
        cx: &mut Context<Self>,
    ) -> Task<Result<Arc<SftpClient>>> {
        let host_key = SshHostKey::from(&config);

        if let Some(client) = self.clients.get(&host_key) {
            return Task::ready(Ok(client.clone()));
        }

        let tokio_handle = gpui_tokio::Tokio::handle(cx);

        cx.spawn(async move |this, cx| {
            let (session, client) = tokio_handle
                .spawn(async move {
                    let session = SshSession::connect(&config).await?;
                    let channel = session.open_sftp_channel().await?;
                    let client = Arc::new(SftpClient::new(channel).await?);
                    anyhow::Ok((session, client))
                })
                .await??;
            let host_key_clone = host_key.clone();
            let client_clone = client.clone();
            let session_clone = session.clone();

            this.update(cx, |this, cx| {
                this.clients.insert(host_key_clone.clone(), client_clone);
                this.sessions.insert(host_key_clone.clone(), session_clone);
                cx.emit(SftpStoreEvent::ClientConnected(host_key_clone));
            })?;

            Ok(client)
        })
    }

    /// Disconnect and remove the SFTP client for the given host.
    pub fn disconnect(&mut self, host_key: &SshHostKey, cx: &mut Context<Self>) {
        self.clients.remove(host_key);
        self.sessions.remove(host_key);
        cx.emit(SftpStoreEvent::ClientDisconnected(host_key.clone()));
    }

    /// Get an existing SFTP client if one is connected.
    pub fn get_client(&self, host_key: &SshHostKey) -> Option<Arc<SftpClient>> {
        self.clients.get(host_key).cloned()
    }

    /// Get the SSH session for the given host, if connected.
    pub fn get_session(&self, host_key: &SshHostKey) -> Option<Arc<SshSession>> {
        self.sessions.get(host_key).cloned()
    }
}

impl EventEmitter<SftpStoreEvent> for SftpStore {}

pub struct SftpStoreEntity;

impl SftpStoreEntity {
    pub fn init(cx: &mut App) {
        if cx.try_global::<GlobalSftpStore>().is_some() {
            return;
        }
        let entity = cx.new(|_| SftpStore::new());
        cx.set_global(GlobalSftpStore(entity));
    }

    pub fn global(cx: &App) -> Entity<SftpStore> {
        cx.global::<GlobalSftpStore>().0.clone()
    }

    pub fn try_global(cx: &App) -> Option<Entity<SftpStore>> {
        cx.try_global::<GlobalSftpStore>()
            .map(|g| g.0.clone())
    }
}

struct GlobalSftpStore(Entity<SftpStore>);

impl Global for GlobalSftpStore {}

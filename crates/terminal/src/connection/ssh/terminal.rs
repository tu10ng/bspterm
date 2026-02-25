use std::borrow::Cow;
use std::sync::Arc;

use alacritty_terminal::event::{Event as AlacTermEvent, WindowSize};
use anyhow::Result;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use parking_lot::{Mutex, RwLock};
use tokio::task::JoinHandle;

use super::session::{SshChannel, SshSession};
use super::SshConfig;
use crate::connection::{ConnectionState, ProcessInfoProvider, TerminalConnection};

/// Commands sent to the SSH channel task.
pub enum ChannelCommand {
    Write(Vec<u8>),
    Resize(WindowSize),
    Close,
}

/// A terminal connection over SSH.
/// Implements the TerminalConnection trait to allow transparent use
/// by the Terminal struct.
pub struct SshTerminalConnection {
    session: Arc<SshSession>,
    command_tx: UnboundedSender<ChannelCommand>,
    state: Arc<RwLock<ConnectionState>>,
    #[allow(dead_code)]
    channel_task: Mutex<Option<JoinHandle<()>>>,
    #[allow(dead_code)]
    initial_size: WindowSize,
    incoming_buffer: Arc<Mutex<Vec<u8>>>,
    tokio_handle: tokio::runtime::Handle,
}

impl SshTerminalConnection {
    pub async fn new(
        session: Arc<SshSession>,
        config: &SshConfig,
        initial_size: WindowSize,
        event_tx: UnboundedSender<AlacTermEvent>,
        tokio_handle: tokio::runtime::Handle,
    ) -> Result<Self> {
        let state = Arc::new(RwLock::new(ConnectionState::Connecting));

        let channel = session
            .open_terminal_channel(initial_size, &config.env)
            .await?;

        let (command_tx, command_rx) = unbounded();

        *state.write() = ConnectionState::Connected;

        let incoming_buffer = Arc::new(Mutex::new(Vec::new()));

        let channel_task = spawn_channel_task(
            channel,
            command_rx,
            event_tx,
            state.clone(),
            config.initial_command.clone(),
            incoming_buffer.clone(),
            tokio_handle.clone(),
            config.host.clone(),
        );

        Ok(Self {
            session,
            command_tx,
            state,
            channel_task: Mutex::new(Some(channel_task)),
            initial_size,
            incoming_buffer,
            tokio_handle,
        })
    }

    pub fn session(&self) -> Arc<SshSession> {
        self.session.clone()
    }
}

impl TerminalConnection for SshTerminalConnection {
    fn write(&self, data: Cow<'static, [u8]>) -> Result<()> {
        self.command_tx
            .unbounded_send(ChannelCommand::Write(data.into_owned()))
            .map_err(|_| anyhow::anyhow!("SSH channel closed"))
    }

    fn resize(&self, size: WindowSize) -> Result<()> {
        self.command_tx
            .unbounded_send(ChannelCommand::Resize(size))
            .map_err(|_| anyhow::anyhow!("SSH channel closed"))
    }

    fn shutdown(&self) -> Result<()> {
        *self.state.write() = ConnectionState::Disconnected;
        self.command_tx.unbounded_send(ChannelCommand::Close).ok();

        // Close the SSH session in a background task to properly disconnect
        // the russh handle and stop keepalive packets
        let session = self.session.clone();
        self.tokio_handle.spawn(async move {
            session.close().await;
        });

        Ok(())
    }

    fn state(&self) -> ConnectionState {
        self.state.read().clone()
    }

    fn process_info(&self) -> Option<Arc<dyn ProcessInfoProvider>> {
        None
    }

    fn read(&self) -> Option<Vec<u8>> {
        let mut buffer = self.incoming_buffer.lock();
        if buffer.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut *buffer))
        }
    }
}

impl Drop for SshTerminalConnection {
    fn drop(&mut self) {
        self.command_tx.unbounded_send(ChannelCommand::Close).ok();

        // Close the SSH session to properly disconnect the russh handle
        // and stop keepalive packets
        let session = self.session.clone();
        self.tokio_handle.spawn(async move {
            session.close().await;
        });
    }
}

fn spawn_channel_task(
    mut channel: SshChannel,
    mut command_rx: UnboundedReceiver<ChannelCommand>,
    event_tx: UnboundedSender<AlacTermEvent>,
    state: Arc<RwLock<ConnectionState>>,
    initial_command: Option<String>,
    incoming_buffer: Arc<Mutex<Vec<u8>>>,
    tokio_handle: tokio::runtime::Handle,
    host: String,
) -> JoinHandle<()> {
    tokio_handle.spawn(async move {
        use futures::StreamExt;
        use std::time::{Duration, Instant};

        const DEBOUNCE_DELAY: Duration = Duration::from_millis(10);
        const PING_CHECK_INTERVAL: Duration = Duration::from_secs(10);
        const MAX_PING_FAILURES: u32 = 2;

        if let Some(command) = initial_command {
            let command_with_newline = format!("{}\n", command);
            if let Err(error) = channel.write(command_with_newline.as_bytes()).await {
                log::error!("Failed to send initial command: {}", error);
            }
        }

        let mut pending_wakeup_deadline: Option<Instant> = None;
        let mut last_data_received = Instant::now();
        let mut consecutive_ping_failures: u32 = 0;

        loop {
            let timeout_duration = pending_wakeup_deadline
                .map(|deadline| deadline.saturating_duration_since(Instant::now()))
                .unwrap_or(Duration::MAX);

            tokio::select! {
                biased;

                _ = tokio::time::sleep(timeout_duration), if pending_wakeup_deadline.is_some() => {
                    event_tx.unbounded_send(AlacTermEvent::Wakeup).ok();
                    pending_wakeup_deadline = None;
                }
                command = command_rx.next() => {
                    match command {
                        Some(ChannelCommand::Write(data)) => {
                            if let Err(error) = channel.write(&data).await {
                                log::error!("Failed to write to SSH channel: {}", error);
                                *state.write() = ConnectionState::Error(error.to_string());
                                break;
                            }
                        }
                        Some(ChannelCommand::Resize(size)) => {
                            if let Err(error) = channel.resize(size).await {
                                log::warn!("Failed to resize SSH channel: {}", error);
                            }
                        }
                        Some(ChannelCommand::Close) | None => {
                            let _ = channel.close().await;
                            *state.write() = ConnectionState::Disconnected;
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(PING_CHECK_INTERVAL) => {
                    if last_data_received.elapsed() > PING_CHECK_INTERVAL {
                        let host_clone = host.clone();
                        let reachable = tokio::task::spawn_blocking(move || {
                            smol::block_on(util::reachability::ping_check(&host_clone))
                        })
                        .await
                        .ok()
                        .flatten()
                        .unwrap_or(true);

                        if !reachable {
                            consecutive_ping_failures += 1;
                            log::warn!(
                                "[SSH] Ping failed for {} ({}/{})",
                                host,
                                consecutive_ping_failures,
                                MAX_PING_FAILURES
                            );
                            if consecutive_ping_failures >= MAX_PING_FAILURES {
                                log::info!("[SSH] Host unreachable (ping timeout)");
                                *state.write() = ConnectionState::Error("Host unreachable".to_string());
                                event_tx.unbounded_send(AlacTermEvent::Exit).ok();
                                break;
                            }
                        } else {
                            consecutive_ping_failures = 0;
                        }
                    }
                }
                data = channel.channel.wait() => {
                    match data {
                        Some(russh::ChannelMsg::Data { data }) => {
                            last_data_received = Instant::now();
                            consecutive_ping_failures = 0;
                            incoming_buffer.lock().extend_from_slice(&data);
                            if pending_wakeup_deadline.is_none() {
                                pending_wakeup_deadline = Some(Instant::now() + DEBOUNCE_DELAY);
                            }
                        }
                        Some(russh::ChannelMsg::ExtendedData { data, .. }) => {
                            last_data_received = Instant::now();
                            consecutive_ping_failures = 0;
                            incoming_buffer.lock().extend_from_slice(&data);
                            if pending_wakeup_deadline.is_none() {
                                pending_wakeup_deadline = Some(Instant::now() + DEBOUNCE_DELAY);
                            }
                        }
                        Some(russh::ChannelMsg::Eof) => {
                            if pending_wakeup_deadline.is_some() {
                                event_tx.unbounded_send(AlacTermEvent::Wakeup).ok();
                            }
                            *state.write() = ConnectionState::Disconnected;
                            event_tx.unbounded_send(AlacTermEvent::Exit).ok();
                            break;
                        }
                        Some(russh::ChannelMsg::ExitStatus { exit_status }) => {
                            log::debug!("SSH channel exit status: {}", exit_status);
                            event_tx.unbounded_send(AlacTermEvent::ChildExit(exit_status as i32)).ok();
                        }
                        Some(russh::ChannelMsg::Close) => {
                            if pending_wakeup_deadline.is_some() {
                                event_tx.unbounded_send(AlacTermEvent::Wakeup).ok();
                            }
                            *state.write() = ConnectionState::Disconnected;
                            event_tx.unbounded_send(AlacTermEvent::Exit).ok();
                            break;
                        }
                        None => {
                            *state.write() = ConnectionState::Disconnected;
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
    })
}

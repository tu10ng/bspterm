use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use alacritty_terminal::event::{Event as AlacTermEvent, WindowSize};
use anyhow::Result;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use parking_lot::{Mutex, RwLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::task::JoinHandle;

use super::protocol::{TelnetNegotiator, escape_data_for_send, IAC, NOP};
use super::session::TelnetSession;
use super::TelnetConfig;
use crate::connection::{ConnectionState, ProcessInfoProvider, TerminalConnection};

struct TelnetConnectionStats {
    keepalive_count: AtomicU64,
    naws_count: AtomicU64,
    connect_time: std::time::Instant,
    target_addr: String,
}

impl TelnetConnectionStats {
    fn new(target_addr: String) -> Self {
        Self {
            keepalive_count: AtomicU64::new(0),
            naws_count: AtomicU64::new(0),
            connect_time: std::time::Instant::now(),
            target_addr,
        }
    }

    fn log_disconnect(&self, reason: &str) {
        let duration = self.connect_time.elapsed();
        let keepalives = self.keepalive_count.load(Ordering::Relaxed);
        let naws_changes = self.naws_count.load(Ordering::Relaxed);
        log::info!(
            "[TELNET] Disconnected from {} ({}). Stats: keepalives={}, naws_changes={}, duration={}",
            self.target_addr,
            reason,
            keepalives,
            naws_changes,
            format_duration(duration)
        );
    }
}

fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        format!("{}h{}m", hours, minutes)
    }
}

pub enum TelnetChannelCommand {
    Write(Vec<u8>),
    Resize(WindowSize),
    Close,
}

pub struct TelnetTerminalConnection {
    command_tx: UnboundedSender<TelnetChannelCommand>,
    state: Arc<RwLock<ConnectionState>>,
    #[allow(dead_code)]
    channel_task: Mutex<Option<JoinHandle<()>>>,
    #[allow(dead_code)]
    initial_size: WindowSize,
    incoming_buffer: Arc<Mutex<Vec<u8>>>,
    #[allow(dead_code)]
    stats: Arc<TelnetConnectionStats>,
}

impl TelnetTerminalConnection {
    pub async fn new(
        session: TelnetSession,
        read_half: OwnedReadHalf,
        write_half: OwnedWriteHalf,
        config: &TelnetConfig,
        initial_size: WindowSize,
        event_tx: UnboundedSender<AlacTermEvent>,
        tokio_handle: tokio::runtime::Handle,
    ) -> Result<Self> {
        let state = Arc::new(RwLock::new(session.state()));

        let (command_tx, command_rx) = unbounded();

        let incoming_buffer = Arc::new(Mutex::new(Vec::new()));

        let target_addr = format!("{}:{}", config.host, config.port);
        let stats = Arc::new(TelnetConnectionStats::new(target_addr));

        let channel_task = spawn_channel_task(
            read_half,
            write_half,
            command_rx,
            event_tx,
            state.clone(),
            config.terminal_type.clone(),
            initial_size,
            incoming_buffer.clone(),
            config.keepalive_interval,
            stats.clone(),
            tokio_handle,
        );

        Ok(Self {
            command_tx,
            state,
            channel_task: Mutex::new(Some(channel_task)),
            initial_size,
            incoming_buffer,
            stats,
        })
    }
}

impl TerminalConnection for TelnetTerminalConnection {
    fn write(&self, data: Cow<'static, [u8]>) -> Result<()> {
        self.command_tx
            .unbounded_send(TelnetChannelCommand::Write(data.into_owned()))
            .map_err(|_| anyhow::anyhow!("Telnet channel closed"))
    }

    fn resize(&self, size: WindowSize) -> Result<()> {
        self.command_tx
            .unbounded_send(TelnetChannelCommand::Resize(size))
            .map_err(|_| anyhow::anyhow!("Telnet channel closed"))
    }

    fn shutdown(&self) -> Result<()> {
        *self.state.write() = ConnectionState::Disconnected;
        self.command_tx
            .unbounded_send(TelnetChannelCommand::Close)
            .ok();
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

impl Drop for TelnetTerminalConnection {
    fn drop(&mut self) {
        self.command_tx
            .unbounded_send(TelnetChannelCommand::Close)
            .ok();
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_channel_task(
    mut read_half: OwnedReadHalf,
    mut write_half: OwnedWriteHalf,
    mut command_rx: UnboundedReceiver<TelnetChannelCommand>,
    event_tx: UnboundedSender<AlacTermEvent>,
    state: Arc<RwLock<ConnectionState>>,
    terminal_type: String,
    initial_size: WindowSize,
    incoming_buffer: Arc<Mutex<Vec<u8>>>,
    keepalive_interval: Option<std::time::Duration>,
    stats: Arc<TelnetConnectionStats>,
    tokio_handle: tokio::runtime::Handle,
) -> JoinHandle<()> {
    tokio_handle.spawn(async move {
        use futures::StreamExt;
        use std::time::{Duration, Instant};

        const DEBOUNCE_DELAY: Duration = Duration::from_millis(10);
        const PING_CHECK_INTERVAL: Duration = Duration::from_secs(10);
        const MAX_PING_FAILURES: u32 = 2;

        let keepalive_duration = keepalive_interval.unwrap_or(Duration::from_secs(30));

        log::info!("[TELNET] Connected to {}", stats.target_addr);

        let mut negotiator = TelnetNegotiator::new(terminal_type);
        let mut read_buf = [0u8; 4096];
        let mut sent_initial_naws = false;
        let mut pending_wakeup_deadline: Option<Instant> = None;
        let mut last_activity = Instant::now();
        let mut last_data_received = Instant::now();
        let mut consecutive_ping_failures: u32 = 0;
        let host = stats
            .target_addr
            .split(':')
            .next()
            .unwrap_or(&stats.target_addr)
            .to_string();

        loop {
            let timeout_duration = pending_wakeup_deadline
                .map(|deadline| deadline.saturating_duration_since(Instant::now()))
                .unwrap_or(Duration::MAX);

            let time_since_activity = last_activity.elapsed();
            let keepalive_timeout = if time_since_activity >= keepalive_duration {
                Duration::ZERO
            } else {
                keepalive_duration - time_since_activity
            };

            tokio::select! {
                biased;

                _ = tokio::time::sleep(timeout_duration), if pending_wakeup_deadline.is_some() => {
                    event_tx.unbounded_send(AlacTermEvent::Wakeup).ok();
                    pending_wakeup_deadline = None;
                }
                _ = tokio::time::sleep(keepalive_timeout), if keepalive_interval.is_some() => {
                    if let Err(error) = write_half.write_all(&[IAC, NOP]).await {
                        log::warn!("[TELNET] Failed to send keepalive to {}: {}", stats.target_addr, error);
                        stats.log_disconnect(&format!("keepalive failed: {}", error));
                        *state.write() = ConnectionState::Error(error.to_string());
                        let _ = write_half.shutdown().await;
                        break;
                    }
                    let count = stats.keepalive_count.fetch_add(1, Ordering::Relaxed) + 1;
                    log::debug!("[TELNET] Keepalive #{} sent to {}", count, stats.target_addr);
                    last_activity = Instant::now();
                }
                command = command_rx.next() => {
                    match command {
                        Some(TelnetChannelCommand::Write(data)) => {
                            let escaped = escape_data_for_send(&data);
                            if let Err(error) = write_half.write_all(&escaped).await {
                                log::error!("[TELNET] Failed to write to {}: {}", stats.target_addr, error);
                                stats.log_disconnect(&format!("write failed: {}", error));
                                *state.write() = ConnectionState::Error(error.to_string());
                                let _ = write_half.shutdown().await;
                                break;
                            }
                            last_activity = Instant::now();
                        }
                        Some(TelnetChannelCommand::Resize(size)) => {
                            let naws_packet = negotiator.build_naws(size);
                            if !naws_packet.is_empty() {
                                stats.naws_count.fetch_add(1, Ordering::Relaxed);
                                log::debug!(
                                    "[TELNET] NAWS: {}x{} to {}",
                                    size.num_cols,
                                    size.num_lines,
                                    stats.target_addr
                                );
                                if let Err(error) = write_half.write_all(&naws_packet).await {
                                    log::warn!("[TELNET] Failed to send NAWS to {}: {}", stats.target_addr, error);
                                }
                            }
                            last_activity = Instant::now();
                        }
                        Some(TelnetChannelCommand::Close) | None => {
                            stats.log_disconnect("user closed");
                            *state.write() = ConnectionState::Disconnected;
                            let _ = write_half.shutdown().await;
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
                                "[TELNET] Ping failed for {} ({}/{})",
                                stats.target_addr,
                                consecutive_ping_failures,
                                MAX_PING_FAILURES
                            );
                            if consecutive_ping_failures >= MAX_PING_FAILURES {
                                stats.log_disconnect("host unreachable (ping timeout)");
                                *state.write() = ConnectionState::Error("Host unreachable".to_string());
                                let _ = write_half.shutdown().await;
                                event_tx.unbounded_send(AlacTermEvent::Exit).ok();
                                break;
                            }
                        } else {
                            consecutive_ping_failures = 0;
                        }
                    }
                }
                result = read_half.read(&mut read_buf) => {
                    match result {
                        Ok(0) => {
                            if pending_wakeup_deadline.is_some() {
                                event_tx.unbounded_send(AlacTermEvent::Wakeup).ok();
                            }
                            stats.log_disconnect("remote closed");
                            *state.write() = ConnectionState::Disconnected;
                            let _ = write_half.shutdown().await;
                            event_tx.unbounded_send(AlacTermEvent::Exit).ok();
                            break;
                        }
                        Ok(n) => {
                            last_activity = Instant::now();
                            last_data_received = Instant::now();
                            consecutive_ping_failures = 0;
                            let process_result = negotiator.process_incoming(&read_buf[..n]);

                            // Send any protocol responses
                            if !process_result.responses.is_empty() {
                                if let Err(error) = write_half.write_all(&process_result.responses).await {
                                    log::error!("[TELNET] Failed to send protocol responses to {}: {}", stats.target_addr, error);
                                    stats.log_disconnect(&format!("protocol response failed: {}", error));
                                    *state.write() = ConnectionState::Error(error.to_string());
                                    let _ = write_half.shutdown().await;
                                    break;
                                }

                                // After NAWS is enabled, send initial window size
                                if !sent_initial_naws && negotiator.is_naws_enabled() {
                                    let naws_packet = negotiator.build_naws(initial_size);
                                    if !naws_packet.is_empty() {
                                        log::debug!(
                                            "[TELNET] Initial NAWS: {}x{} to {}",
                                            initial_size.num_cols,
                                            initial_size.num_lines,
                                            stats.target_addr
                                        );
                                        if let Err(error) = write_half.write_all(&naws_packet).await {
                                            log::warn!("[TELNET] Failed to send initial NAWS to {}: {}", stats.target_addr, error);
                                        }
                                        sent_initial_naws = true;
                                    }
                                }
                            }

                            // Buffer terminal data and set debounce deadline
                            if !process_result.data.is_empty() {
                                incoming_buffer.lock().extend_from_slice(&process_result.data);
                                if pending_wakeup_deadline.is_none() {
                                    pending_wakeup_deadline = Some(Instant::now() + DEBOUNCE_DELAY);
                                }
                            }
                        }
                        Err(error) => {
                            log::error!("[TELNET] Read error from {}: {}", stats.target_addr, error);
                            stats.log_disconnect(&format!("read error: {}", error));
                            *state.write() = ConnectionState::Error(error.to_string());
                            let _ = write_half.shutdown().await;
                            event_tx.unbounded_send(AlacTermEvent::Exit).ok();
                            break;
                        }
                    }
                }
            }
        }
    })
}

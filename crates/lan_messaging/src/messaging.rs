use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use futures::channel::mpsc;
use futures::StreamExt;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};
use local_user::LocalUserStoreEntity;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const LAN_MESSAGING_PORT: u16 = 53722;

/// Identifies a user for messaging.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserIdentity {
    pub employee_id: String,
    pub name: String,
}

impl UserIdentity {
    pub fn new(employee_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            employee_id: employee_id.into(),
            name: name.into(),
        }
    }

    pub fn initials(&self) -> String {
        self.name.chars().take(2).collect::<String>().to_uppercase()
    }
}

/// A chat message between two users.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: Uuid,
    pub from: UserIdentity,
    pub to: UserIdentity,
    pub content: String,
    pub timestamp: u64,
    pub session_context: Option<Uuid>,
}

impl ChatMessage {
    pub fn new(
        from: UserIdentity,
        to: UserIdentity,
        content: impl Into<String>,
        session_context: Option<Uuid>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            from,
            to,
            content: content.into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            session_context,
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = serde_json::to_vec(self)?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

/// Events emitted by the messaging service.
#[derive(Clone, Debug)]
pub enum LanMessagingEvent {
    MessageReceived(ChatMessage),
    MessageSent(ChatMessage),
}

/// Message history storage.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MessageHistory {
    pub version: u32,
    pub conversations: HashMap<String, Vec<ChatMessage>>,
}

impl MessageHistory {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            conversations: HashMap::new(),
        }
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    fn conversation_key(user1: &str, user2: &str) -> String {
        let mut parts = [user1, user2];
        parts.sort();
        parts.join(":")
    }

    pub fn add_message(&mut self, message: ChatMessage) {
        let key = Self::conversation_key(&message.from.employee_id, &message.to.employee_id);
        self.conversations
            .entry(key)
            .or_default()
            .push(message);
    }

    pub fn get_conversation(&self, user1: &str, user2: &str) -> &[ChatMessage] {
        let key = Self::conversation_key(user1, user2);
        self.conversations
            .get(&key)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

/// Global marker for cx.global access.
pub struct GlobalLanMessaging(pub Entity<LanMessagingEntity>);
impl Global for GlobalLanMessaging {}

/// GPUI Entity for LAN messaging.
pub struct LanMessagingEntity {
    history: MessageHistory,
    listener_task: Option<Task<()>>,
    save_task: Option<Task<()>>,
}

impl EventEmitter<LanMessagingEvent> for LanMessagingEntity {}

impl LanMessagingEntity {
    /// Initialize global LAN messaging on app startup.
    pub fn init(cx: &mut App) {
        let history = MessageHistory::load_from_file(Self::history_file()).unwrap_or_else(|err| {
            log::error!("Failed to load message history: {}", err);
            MessageHistory::new()
        });

        let entity = cx.new(|_| Self {
            history,
            listener_task: None,
            save_task: None,
        });

        cx.set_global(GlobalLanMessaging(entity.clone()));

        entity.update(cx, |this, cx| {
            this.start_listener(cx);
        });
    }

    /// Get global instance.
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalLanMessaging>().0.clone()
    }

    /// Try to get global instance.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalLanMessaging>().map(|g| g.0.clone())
    }

    fn history_file() -> &'static std::path::PathBuf {
        static HISTORY_FILE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        HISTORY_FILE.get_or_init(|| paths::config_dir().join("chat_history.json"))
    }

    /// Get conversation history with another user.
    pub fn get_conversation(&self, other_employee_id: &str, cx: &App) -> Vec<ChatMessage> {
        let Some(user_store) = LocalUserStoreEntity::try_global(cx) else {
            return Vec::new();
        };
        let user_store = user_store.read(cx);
        let Some(profile) = user_store.profile() else {
            return Vec::new();
        };

        self.history
            .get_conversation(&profile.employee_id, other_employee_id)
            .to_vec()
    }

    /// Send a message to another user.
    pub fn send_message(
        &mut self,
        to_ip: IpAddr,
        to: UserIdentity,
        content: String,
        session_context: Option<Uuid>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let Some(user_store) = LocalUserStoreEntity::try_global(cx) else {
            return Task::ready(Err(anyhow::anyhow!("User not logged in")));
        };
        let user_store = user_store.read(cx);
        let Some(profile) = user_store.profile() else {
            return Task::ready(Err(anyhow::anyhow!("User not logged in")));
        };

        let from = UserIdentity::new(&profile.employee_id, &profile.name);
        let message = ChatMessage::new(from, to, content, session_context);
        let message_for_history = message.clone();

        self.history.add_message(message_for_history.clone());
        self.schedule_save(cx);
        cx.emit(LanMessagingEvent::MessageSent(message_for_history));

        let message_bytes = match message.to_bytes() {
            Ok(bytes) => bytes,
            Err(e) => return Task::ready(Err(e)),
        };

        cx.background_spawn(async move {
            let addr = SocketAddr::new(to_ip, LAN_MESSAGING_PORT);
            let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))?;
            stream.write_all(&message_bytes)?;
            stream.flush()?;
            Ok(())
        })
    }

    fn start_listener(&mut self, cx: &mut Context<Self>) {
        let (tx, mut rx) = mpsc::unbounded::<ChatMessage>();

        std::thread::spawn(move || {
            let listener = match TcpListener::bind(SocketAddr::new(
                IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
                LAN_MESSAGING_PORT,
            )) {
                Ok(l) => l,
                Err(e) => {
                    log::error!("Failed to bind TCP listener for messaging: {}", e);
                    return;
                }
            };

            if let Err(e) = listener.set_nonblocking(true) {
                log::error!("Failed to set non-blocking on TCP listener: {}", e);
            }

            loop {
                match listener.accept() {
                    Ok((stream, _addr)) => {
                        let tx = tx.clone();
                        std::thread::spawn(move || {
                            let reader = BufReader::new(stream);
                            for line in reader.lines().map_while(Result::ok) {
                                if let Ok(message) = ChatMessage::from_bytes(line.as_bytes()) {
                                    let _ = tx.unbounded_send(message);
                                }
                            }
                        });
                    }
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        log::trace!("Error accepting connection: {}", e);
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        });

        self.listener_task = Some(cx.spawn(async move |this, cx| {
            while let Some(message) = rx.next().await {
                let _ = cx.update(|cx| {
                    if let Some(entity) = this.upgrade() {
                        entity.update(cx, |this, cx| {
                            this.handle_received_message(message, cx);
                        });
                    }
                });
            }
        }));
    }

    fn handle_received_message(&mut self, message: ChatMessage, cx: &mut Context<Self>) {
        self.history.add_message(message.clone());
        self.schedule_save(cx);
        cx.emit(LanMessagingEvent::MessageReceived(message));
        cx.notify();
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let history = self.history.clone();
        self.save_task = Some(cx.spawn(async move |_, _| {
            if let Err(err) = history.save_to_file(Self::history_file()) {
                log::error!("Failed to save message history: {}", err);
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_identity() {
        let user = UserIdentity::new("12345", "Zhang San");
        assert_eq!(user.employee_id, "12345");
        assert_eq!(user.name, "Zhang San");
        assert_eq!(user.initials(), "ZH");
    }

    #[test]
    fn test_chat_message_serialization() {
        let from = UserIdentity::new("12345", "Zhang San");
        let to = UserIdentity::new("67890", "Li Si");
        let message = ChatMessage::new(from, to, "Hello!", None);

        let bytes = message.to_bytes().unwrap();
        let restored = ChatMessage::from_bytes(&bytes[..bytes.len() - 1]).unwrap();

        assert_eq!(restored.content, message.content);
        assert_eq!(restored.from.employee_id, message.from.employee_id);
        assert_eq!(restored.to.employee_id, message.to.employee_id);
    }

    #[test]
    fn test_message_history() {
        let mut history = MessageHistory::new();

        let from = UserIdentity::new("12345", "Zhang San");
        let to = UserIdentity::new("67890", "Li Si");
        let message = ChatMessage::new(from, to, "Hello!", None);

        history.add_message(message);

        let conversation = history.get_conversation("12345", "67890");
        assert_eq!(conversation.len(), 1);
        assert_eq!(conversation[0].content, "Hello!");

        let conversation_reversed = history.get_conversation("67890", "12345");
        assert_eq!(conversation_reversed.len(), 1);
    }
}

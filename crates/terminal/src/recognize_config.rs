//! Configuration for auto-recognition of connection strings.
//!
//! This module provides a JSON-configurable approach to parsing connection strings
//! in the Quick Add area. Instead of hardcoded logic, recognition rules are defined
//! via configuration that can be customized by users.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::Result;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};
use serde::{Deserialize, Serialize};

/// Embedded default config JSON (only written when file is missing or has an older version).
const DEFAULT_RECOGNIZE_CONFIG: &[u8] =
    include_bytes!("../../../assets/settings/default_recognize_config.json");

/// Default port for connections.
const DEFAULT_PORT: u16 = 23;

/// Default protocol for connections.
const DEFAULT_PROTOCOL: &str = "telnet";

/// Events emitted by the recognize config store.
#[derive(Clone, Debug)]
pub enum RecognizeConfigEvent {
    Changed,
}

/// Global marker for cx.global access.
pub struct GlobalRecognizeConfig(pub Entity<RecognizeConfigEntity>);
impl Global for GlobalRecognizeConfig {}

/// Default configuration values.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DefaultConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

fn default_protocol() -> String {
    DEFAULT_PROTOCOL.to_string()
}

impl Default for DefaultConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            protocol: DEFAULT_PROTOCOL.to_string(),
        }
    }
}

/// Separator configuration for entry and field splitting.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeparatorConfig {
    /// Separators between entries (e.g., newline, comma).
    #[serde(default = "default_entry_separators")]
    pub entry: Vec<String>,
    /// Separators between fields within an entry (e.g., tab, space, slash).
    #[serde(default = "default_field_separators")]
    pub field: Vec<String>,
}

fn default_entry_separators() -> Vec<String> {
    vec!["\n".to_string(), ",".to_string()]
}

fn default_field_separators() -> Vec<String> {
    vec!["\t".to_string(), " ".to_string(), "/".to_string()]
}

impl Default for SeparatorConfig {
    fn default() -> Self {
        Self {
            entry: default_entry_separators(),
            field: default_field_separators(),
        }
    }
}

/// Protocol prefix configuration (e.g., "环境" -> telnet, "后台" -> ssh).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProtocolPrefix {
    pub prefix: String,
    pub protocol: String,
    pub default_port: u16,
}

/// Token classifier rule.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenClassifier {
    /// Name of this classifier for debugging.
    pub name: String,
    /// Token type to assign if matched: "ip", "port", "username", "password", "label".
    pub token_type: String,
    /// Match type: "ipv4", "port_range", "starts_with", "starts_uppercase_mixed",
    /// "contains_any", "contains_any_word", "has_non_ascii", "alphanumeric_mix", "regex".
    pub match_type: String,
    /// Values for starts_with, contains_any, contains_any_word matchers.
    #[serde(default)]
    pub values: Vec<String>,
    /// Range for port_range matcher [min, max].
    #[serde(default)]
    pub range: Option<(u32, u32)>,
    /// Max length for alphanumeric_mix matcher.
    #[serde(default)]
    pub max_length: Option<usize>,
    /// Regex pattern for regex matcher.
    #[serde(default)]
    pub pattern: Option<String>,
}

impl Default for TokenClassifier {
    fn default() -> Self {
        Self {
            name: String::new(),
            token_type: String::new(),
            match_type: String::new(),
            values: Vec::new(),
            range: None,
            max_length: None,
            pattern: None,
        }
    }
}

/// The main recognition configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecognizeConfig {
    pub version: u32,
    #[serde(default)]
    pub defaults: DefaultConfig,
    #[serde(default)]
    pub separators: SeparatorConfig,
    #[serde(default)]
    pub protocol_prefixes: Vec<ProtocolPrefix>,
    #[serde(default)]
    pub port_protocol_map: HashMap<u16, String>,
    #[serde(default)]
    pub token_classifiers: Vec<TokenClassifier>,
    #[serde(default = "default_fallback_order")]
    pub fallback_order: Vec<String>,
}

fn default_fallback_order() -> Vec<String> {
    vec![
        "username".to_string(),
        "password".to_string(),
        "label".to_string(),
    ]
}

impl RecognizeConfig {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            defaults: DefaultConfig::default(),
            separators: SeparatorConfig::default(),
            protocol_prefixes: Vec::new(),
            port_protocol_map: HashMap::new(),
            token_classifiers: Vec::new(),
            fallback_order: default_fallback_order(),
        }
    }

    /// Create a config with default rules.
    pub fn with_defaults() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            defaults: DefaultConfig::default(),
            separators: SeparatorConfig::default(),
            protocol_prefixes: vec![
                ProtocolPrefix {
                    prefix: "环境".to_string(),
                    protocol: "telnet".to_string(),
                    default_port: 23,
                },
                ProtocolPrefix {
                    prefix: "后台".to_string(),
                    protocol: "ssh".to_string(),
                    default_port: 22,
                },
            ],
            port_protocol_map: [(22, "ssh".to_string()), (23, "telnet".to_string())]
                .into_iter()
                .collect(),
            token_classifiers: Self::default_classifiers(),
            fallback_order: default_fallback_order(),
        }
    }

    /// Default token classifiers.
    /// Order matters: more specific classifiers should come first.
    fn default_classifiers() -> Vec<TokenClassifier> {
        vec![
            // IP addresses first
            TokenClassifier {
                name: "ipv4".to_string(),
                token_type: "ip".to_string(),
                match_type: "ipv4".to_string(),
                ..Default::default()
            },
            // Port numbers
            TokenClassifier {
                name: "port".to_string(),
                token_type: "port".to_string(),
                match_type: "port_range".to_string(),
                range: Some((1, 65535)),
                ..Default::default()
            },
            // Passwords with special characters (must come BEFORE username detection)
            TokenClassifier {
                name: "password_special".to_string(),
                token_type: "password".to_string(),
                match_type: "contains_any".to_string(),
                values: vec!["@", "!", "#", "$", "%", "^", "&", "*"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                ..Default::default()
            },
            // Common usernames (after password to avoid misclassifying "Admin@123")
            TokenClassifier {
                name: "common_usernames".to_string(),
                token_type: "username".to_string(),
                match_type: "starts_with".to_string(),
                values: vec![
                    "root", "admin", "administrator", "user", "test", "guest", "huawei", "cisco",
                    "zte", "nokia", "juniper", "oracle", "mysql", "postgres", "ubuntu", "centos",
                    "debian",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                ..Default::default()
            },
            // Chinese text labels
            TokenClassifier {
                name: "chinese_label".to_string(),
                token_type: "label".to_string(),
                match_type: "has_non_ascii".to_string(),
                ..Default::default()
            },
            // Label keywords (slot, clc, etc.)
            TokenClassifier {
                name: "label_keywords".to_string(),
                token_type: "label".to_string(),
                match_type: "contains_any_word".to_string(),
                values: vec!["slot", "clc", "ccc", "mpu", "lpu", "sfu"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                ..Default::default()
            },
            TokenClassifier {
                name: "short_alphanumeric".to_string(),
                token_type: "label".to_string(),
                match_type: "alphanumeric_mix".to_string(),
                max_length: Some(6),
                ..Default::default()
            },
        ]
    }

    /// Load from file, falling back to defaults if the file doesn't exist.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::with_defaults());
        }
        let content = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Save the config to a file.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}

impl Default for RecognizeConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Token type enum for internal classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    Ip,
    Port,
    Username,
    Password,
    Label,
    Unknown,
}

impl TokenType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "ip" => TokenType::Ip,
            "port" => TokenType::Port,
            "username" => TokenType::Username,
            "password" => TokenType::Password,
            "label" => TokenType::Label,
            _ => TokenType::Unknown,
        }
    }
}

/// Connection protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionProtocol {
    Ssh,
    Telnet,
}

impl ConnectionProtocol {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "ssh" => ConnectionProtocol::Ssh,
            _ => ConnectionProtocol::Telnet,
        }
    }
}

/// A parsed connection from the input text.
#[derive(Clone, Debug)]
pub struct ParsedConnection {
    pub name: Option<String>,
    pub host: String,
    pub port: u16,
    pub protocol: ConnectionProtocol,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl ParsedConnection {
    pub fn new(host: String, port: u16, protocol: ConnectionProtocol) -> Self {
        Self {
            name: None,
            host,
            port,
            protocol,
            username: None,
            password: None,
        }
    }

    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    pub fn with_username(mut self, username: String) -> Self {
        self.username = Some(username);
        self
    }

    pub fn with_password(mut self, password: String) -> Self {
        self.password = Some(password);
        self
    }
}

/// GPUI Entity wrapping RecognizeConfig.
pub struct RecognizeConfigEntity {
    config: RecognizeConfig,
    save_task: Option<Task<()>>,
}

impl EventEmitter<RecognizeConfigEvent> for RecognizeConfigEntity {}

/// Ensure default config is installed to user directory.
/// Only writes the default config if the file doesn't exist or has an older version.
fn ensure_default_config() {
    let config_path = paths::recognize_config_file();

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            log::error!("Failed to create config directory: {}", error);
            return;
        }
    }

    // Check if the file already exists and has a current version
    if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => {
                // Try to parse just the version field to decide whether to overwrite
                #[derive(Deserialize)]
                struct VersionOnly {
                    #[serde(default)]
                    version: u32,
                }
                match serde_json::from_str::<VersionOnly>(&content) {
                    Ok(existing) if existing.version >= RecognizeConfig::CURRENT_VERSION => {
                        log::debug!("Recognize config is up to date (version {})", existing.version);
                        return;
                    }
                    Ok(existing) => {
                        log::info!(
                            "Upgrading recognize config from version {} to {}",
                            existing.version,
                            RecognizeConfig::CURRENT_VERSION
                        );
                    }
                    Err(error) => {
                        log::warn!("Failed to parse existing recognize config version: {}", error);
                    }
                }
            }
            Err(error) => {
                log::warn!("Failed to read existing recognize config: {}", error);
            }
        }
    }

    if let Err(error) = fs::write(&config_path, DEFAULT_RECOGNIZE_CONFIG) {
        log::error!("Failed to write default recognize config: {}", error);
    } else {
        log::info!("Installed default recognize config to {:?}", config_path);
    }
}

impl RecognizeConfigEntity {
    /// Create a new entity with default configuration (for testing or fallback).
    pub fn new_with_defaults() -> Self {
        Self {
            config: RecognizeConfig::with_defaults(),
            save_task: None,
        }
    }

    /// Initialize global recognize config on app startup.
    pub fn init(cx: &mut App) {
        if cx.try_global::<GlobalRecognizeConfig>().is_some() {
            return;
        }

        // Ensure default config is installed to user directory
        ensure_default_config();

        // Load from file (just written or user modified)
        let config =
            RecognizeConfig::load_from_file(paths::recognize_config_file()).unwrap_or_else(|err| {
                log::error!("Failed to load recognize config: {}", err);
                RecognizeConfig::with_defaults()
            });

        let entity = cx.new(|_| Self {
            config,
            save_task: None,
        });

        cx.set_global(GlobalRecognizeConfig(entity));
    }

    /// Get global instance.
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalRecognizeConfig>().0.clone()
    }

    /// Try to get global instance, returns None if not initialized.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalRecognizeConfig>()
            .map(|g| g.0.clone())
    }

    /// Read-only access to config.
    pub fn config(&self) -> &RecognizeConfig {
        &self.config
    }

    /// Update the config and trigger save.
    pub fn update_config(
        &mut self,
        update_fn: impl FnOnce(&mut RecognizeConfig),
        cx: &mut Context<Self>,
    ) {
        update_fn(&mut self.config);
        self.schedule_save(cx);
        cx.emit(RecognizeConfigEvent::Changed);
        cx.notify();
    }

    /// Reset to default config.
    pub fn reset_to_defaults(&mut self, cx: &mut Context<Self>) {
        self.config = RecognizeConfig::with_defaults();
        self.schedule_save(cx);
        cx.emit(RecognizeConfigEvent::Changed);
        cx.notify();
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let config = self.config.clone();
        self.save_task = Some(cx.spawn(async move |_, _| {
            if let Err(err) = config.save_to_file(paths::recognize_config_file()) {
                log::error!("Failed to save recognize config: {}", err);
            }
        }));
    }

    // =========================================================================
    // Parsing Functions
    // =========================================================================

    /// Parse connection text using the configured rules.
    pub fn parse_connection_text(&self, input: &str) -> Vec<ParsedConnection> {
        let input = input.trim();
        if input.is_empty() {
            return Vec::new();
        }

        let mut connections = Vec::new();

        // Split by entry separators
        let entries = self.split_entries(input);

        // Check for multi-line single connection format
        if entries.len() >= 2 && entries.len() <= 4 {
            if self.is_multiline_single_connection(&entries) {
                if let Some(conn) = self.parse_multiline_connection(&entries) {
                    connections.push(conn);
                    return connections;
                }
            }
        }

        for entry in entries {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }

            let parsed = self.parse_single_entry(entry);
            connections.extend(parsed);
        }

        connections
    }

    /// Split input by entry separators.
    fn split_entries<'a>(&self, input: &'a str) -> Vec<&'a str> {
        // Check for newline first (most common multi-entry case)
        if input.contains('\n') {
            return input.lines().collect();
        }

        // Check for comma separator
        if input.contains(',') {
            return input.split(',').collect();
        }

        // Single entry
        vec![input]
    }

    /// Check if the lines form a single connection with credentials on separate lines.
    fn is_multiline_single_connection(&self, lines: &[&str]) -> bool {
        let first_line = lines[0].trim();

        // First line must contain an IP
        if self.find_ipv4_position(first_line).is_none() {
            return false;
        }

        // Subsequent lines must NOT contain IPs
        for line in &lines[1..] {
            let trimmed = line.trim();
            if self.find_ipv4_position(trimmed).is_some() {
                return false;
            }
        }

        true
    }

    /// Parse multi-line connection where credentials are on separate lines.
    fn parse_multiline_connection(&self, lines: &[&str]) -> Option<ParsedConnection> {
        let first_line = lines[0].trim();
        let mut parsed = self.parse_single_entry(first_line);

        if parsed.is_empty() {
            return None;
        }

        let mut conn = parsed.remove(0);
        let mut username: Option<String> = None;
        let mut password: Option<String> = None;

        for line in &lines[1..] {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let token_type = self.classify_token(trimmed);
            match token_type {
                TokenType::Username => {
                    if username.is_none() {
                        username = Some(trimmed.to_string());
                    } else if password.is_none() {
                        password = Some(trimmed.to_string());
                    }
                }
                TokenType::Password => {
                    if password.is_none() {
                        password = Some(trimmed.to_string());
                    }
                }
                _ => {
                    // Fallback: first unknown token goes to username, second to password
                    if username.is_none() {
                        username = Some(trimmed.to_string());
                    } else if password.is_none() {
                        password = Some(trimmed.to_string());
                    }
                }
            }
        }

        if username.is_some() {
            conn.username = username;
        }
        if password.is_some() {
            conn.password = password;
        }

        Some(conn)
    }

    /// Parse a single entry which may produce multiple connections (for multiple ports).
    fn parse_single_entry(&self, entry: &str) -> Vec<ParsedConnection> {
        let mut connections = Vec::new();

        // Check for protocol prefixes (环境, 后台)
        for prefix_config in &self.config.protocol_prefixes {
            if let Some(rest) = entry.strip_prefix(&prefix_config.prefix) {
                if let Some(conns) =
                    self.parse_with_protocol_prefix(rest.trim(), prefix_config)
                {
                    return conns;
                }
            }
        }

        // Find IP position
        let Some((ip_start, ip_end)) = self.find_ipv4_position(entry) else {
            return connections;
        };

        let host = &entry[ip_start..ip_end];
        if !Self::is_valid_ipv4(host) {
            return connections;
        }

        // Parse ports after IP (including :port syntax and space-separated ports)
        let mut ports = Vec::new();
        let mut host_port_end = ip_end;

        let remaining_after_ip = &entry[ip_end..];

        // Check for :port syntax
        if remaining_after_ip.starts_with(':') {
            let port_start = 1;
            let port_end = remaining_after_ip[port_start..]
                .find(|c: char| !c.is_ascii_digit())
                .map(|i| port_start + i)
                .unwrap_or(remaining_after_ip.len());

            if port_end > port_start {
                if let Ok(parsed_port) = remaining_after_ip[port_start..port_end].parse::<u16>() {
                    ports.push(parsed_port);
                    host_port_end = ip_end + port_end;
                }
            }
        }

        // Build name from prefix + IP + first port
        let name_base = entry[..host_port_end].trim().to_string();

        // Parse remaining fields (credentials, additional ports, labels)
        let remaining = entry[host_port_end..].trim();
        let (username, password, extra_ports, label) = self.parse_credentials_flexible(remaining);

        // Add extra ports
        ports.extend(extra_ports);

        // If no ports found, use default
        if ports.is_empty() {
            ports.push(self.config.defaults.port);
        }

        // Determine final name
        let final_name = if let Some(extra_label) = label {
            format!("{} {}", name_base, extra_label)
        } else {
            name_base
        };

        // Create a connection for each port
        for port in ports {
            let protocol = self.determine_protocol(port);

            let mut conn = ParsedConnection::new(host.to_string(), port, protocol);
            conn.name = Some(final_name.clone());
            conn.username = username.clone();
            conn.password = password.clone();

            connections.push(conn);
        }

        connections
    }

    /// Parse entry that starts with a protocol prefix (环境, 后台).
    fn parse_with_protocol_prefix(
        &self,
        rest: &str,
        prefix_config: &ProtocolPrefix,
    ) -> Option<Vec<ParsedConnection>> {
        // Split by tab for protocol prefix format
        let parts: Vec<&str> = rest.split('\t').collect();
        if parts.is_empty() {
            return None;
        }

        let host_port = parts[0].trim();
        let (host, port) = self.parse_host_port(host_port, prefix_config.default_port)?;

        if !Self::is_valid_ipv4(&host) && !Self::is_valid_hostname(&host) {
            return None;
        }

        let username = parts
            .get(1)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let password = parts
            .get(2)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let protocol = ConnectionProtocol::from_str(&prefix_config.protocol);

        let conn = ParsedConnection {
            name: None,
            host,
            port,
            protocol,
            username,
            password,
        };

        Some(vec![conn])
    }

    /// Parse host:port or just host with default port.
    fn parse_host_port(&self, input: &str, default_port: u16) -> Option<(String, u16)> {
        if let Some((host, port_str)) = input.rsplit_once(':') {
            let port = port_str.parse::<u16>().ok()?;
            Some((host.to_string(), port))
        } else {
            Some((input.to_string(), default_port))
        }
    }

    /// Parse credentials and additional fields from remaining text.
    fn parse_credentials_flexible(
        &self,
        input: &str,
    ) -> (Option<String>, Option<String>, Vec<u16>, Option<String>) {
        let input = input.trim();
        if input.is_empty() {
            return (None, None, Vec::new(), None);
        }

        // Check for tab separator
        if input.contains('\t') {
            let parts: Vec<&str> = input.split('\t').collect();
            let username = parts
                .first()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let password = parts
                .get(1)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            return (username, password, Vec::new(), None);
        }

        // Check for slash separator (user/pass format)
        if let Some((user, pass)) = input.split_once('/') {
            let user = user.trim();
            let pass = pass.trim();
            if !user.is_empty() {
                return (
                    Some(user.to_string()),
                    if pass.is_empty() {
                        None
                    } else {
                        Some(pass.to_string())
                    },
                    Vec::new(),
                    None,
                );
            }
        }

        // Space-separated tokens
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return (None, None, Vec::new(), None);
        }

        let mut username: Option<String> = None;
        let mut password: Option<String> = None;
        let mut ports: Vec<u16> = Vec::new();
        let mut labels: Vec<String> = Vec::new();

        for part in parts {
            let token_type = self.classify_token(part);
            match token_type {
                TokenType::Port => {
                    if let Ok(port) = part.parse::<u16>() {
                        ports.push(port);
                    }
                }
                TokenType::Username => {
                    if username.is_none() {
                        username = Some(part.to_string());
                    } else if password.is_none() {
                        password = Some(part.to_string());
                    } else {
                        labels.push(part.to_string());
                    }
                }
                TokenType::Password => {
                    if password.is_none() {
                        password = Some(part.to_string());
                    }
                }
                TokenType::Label => {
                    labels.push(part.to_string());
                }
                TokenType::Ip => {}
                TokenType::Unknown => {
                    // Use fallback_order to determine what to do with unknown tokens
                    // Default order: username -> password -> label
                    if username.is_none() {
                        username = Some(part.to_string());
                    } else if password.is_none() {
                        password = Some(part.to_string());
                    } else {
                        labels.push(part.to_string());
                    }
                }
            }
        }

        let label = if labels.is_empty() {
            None
        } else {
            Some(labels.join(" "))
        };

        (username, password, ports, label)
    }

    /// Classify a token according to the configured classifiers.
    fn classify_token(&self, token: &str) -> TokenType {
        for classifier in &self.config.token_classifiers {
            if self.matches_classifier(token, classifier) {
                return TokenType::from_str(&classifier.token_type);
            }
        }

        // Fallback: use fallback_order to assign type
        TokenType::Unknown
    }

    /// Check if a token matches a classifier.
    fn matches_classifier(&self, token: &str, classifier: &TokenClassifier) -> bool {
        match classifier.match_type.as_str() {
            "ipv4" => Self::is_valid_ipv4(token),
            "port_range" => {
                if let Some((min, max)) = classifier.range {
                    token
                        .parse::<u32>()
                        .map(|n| n >= min && n <= max)
                        .unwrap_or(false)
                        && token.chars().all(|c| c.is_ascii_digit())
                } else {
                    false
                }
            }
            "starts_with" => {
                let lower = token.to_lowercase();
                classifier
                    .values
                    .iter()
                    .any(|v| lower.starts_with(&v.to_lowercase()))
            }
            "starts_uppercase_mixed" => {
                // First char uppercase + contains digits
                let first_upper = token.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
                let has_digit = token.chars().any(|c| c.is_ascii_digit());
                first_upper && has_digit
            }
            "contains_any" => classifier.values.iter().any(|v| token.contains(v.as_str())),
            "contains_any_word" => {
                let lower = token.to_lowercase();
                classifier
                    .values
                    .iter()
                    .any(|v| lower.contains(&v.to_lowercase()))
            }
            "has_non_ascii" => !token.is_ascii(),
            "alphanumeric_mix" => {
                let has_alpha = token.chars().any(|c| c.is_ascii_alphabetic());
                let has_digit = token.chars().any(|c| c.is_ascii_digit());
                let within_length = classifier
                    .max_length
                    .map(|max| token.len() <= max)
                    .unwrap_or(true);
                has_alpha && has_digit && within_length && !Self::is_valid_ipv4(token)
            }
            "regex" => {
                if let Some(pattern) = &classifier.pattern {
                    regex::Regex::new(pattern)
                        .map(|re| re.is_match(token))
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Find IPv4 position in text.
    fn find_ipv4_position(&self, input: &str) -> Option<(usize, usize)> {
        let bytes = input.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            if bytes[i].is_ascii_digit() {
                let start = i;
                let mut octet_count = 0;
                let mut valid = true;

                while i < len && octet_count < 4 {
                    let octet_start = i;
                    while i < len && bytes[i].is_ascii_digit() {
                        i += 1;
                    }

                    if i == octet_start {
                        valid = false;
                        break;
                    }

                    let octet_str = &input[octet_start..i];
                    if octet_str.parse::<u8>().is_err() {
                        valid = false;
                        break;
                    }

                    octet_count += 1;

                    if octet_count < 4 {
                        if i < len && bytes[i] == b'.' {
                            i += 1;
                        } else {
                            valid = false;
                            break;
                        }
                    }
                }

                if valid && octet_count == 4 {
                    return Some((start, i));
                }

                i = start + 1;
            } else {
                i += 1;
            }
        }

        None
    }

    /// Determine protocol based on port.
    fn determine_protocol(&self, port: u16) -> ConnectionProtocol {
        if let Some(protocol) = self.config.port_protocol_map.get(&port) {
            ConnectionProtocol::from_str(protocol)
        } else {
            ConnectionProtocol::from_str(&self.config.defaults.protocol)
        }
    }

    /// Validate IPv4 address.
    fn is_valid_ipv4(s: &str) -> bool {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 4 {
            return false;
        }

        parts.iter().all(|part| part.parse::<u8>().is_ok())
    }

    /// Validate hostname.
    fn is_valid_hostname(s: &str) -> bool {
        if s.is_empty() || s.len() > 253 {
            return false;
        }

        s.chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '.')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> RecognizeConfigEntity {
        RecognizeConfigEntity {
            config: RecognizeConfig::with_defaults(),
            save_task: None,
        }
    }

    #[test]
    fn test_parse_single_ip() {
        let entity = create_test_config();
        let result = entity.parse_connection_text("192.168.1.1");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 23);
        assert_eq!(result[0].protocol, ConnectionProtocol::Telnet);
    }

    #[test]
    fn test_parse_ip_with_port() {
        let entity = create_test_config();
        let result = entity.parse_connection_text("192.168.1.1:22");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 22);
        assert_eq!(result[0].protocol, ConnectionProtocol::Ssh);
    }

    #[test]
    fn test_parse_multiple_ports() {
        let entity = create_test_config();
        let result = entity.parse_connection_text("192.168.1.1:2323 2222");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 2323);
        assert_eq!(result[0].protocol, ConnectionProtocol::Telnet);

        assert_eq!(result[1].host, "192.168.1.1");
        assert_eq!(result[1].port, 2222);
        assert_eq!(result[1].protocol, ConnectionProtocol::Telnet);
    }

    #[test]
    fn test_parse_ip_with_credentials() {
        let entity = create_test_config();
        let result = entity.parse_connection_text("192.168.1.1 admin password123");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 23);
        assert_eq!(result[0].username, Some("admin".to_string()));
        assert_eq!(result[0].password, Some("password123".to_string()));
    }

    #[test]
    fn test_parse_smart_credentials() {
        let entity = create_test_config();
        let result = entity.parse_connection_text("192.168.1.1 root123 Root@123");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].username, Some("root123".to_string()));
        assert_eq!(result[0].password, Some("Root@123".to_string()));
    }

    #[test]
    fn test_parse_chinese_prefix() {
        let entity = create_test_config();
        let result = entity.parse_connection_text("管理网口192.168.1.1 huawei Admin@123");

        assert_eq!(result.len(), 1);
        assert!(result[0].name.as_ref().unwrap().contains("管理网口"));
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].username, Some("huawei".to_string()));
        assert_eq!(result[0].password, Some("Admin@123".to_string()));
    }

    #[test]
    fn test_parse_protocol_prefix_telnet() {
        let entity = create_test_config();
        let result = entity.parse_connection_text("环境192.168.1.1\troot\tpassword");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 23);
        assert_eq!(result[0].protocol, ConnectionProtocol::Telnet);
        assert_eq!(result[0].username, Some("root".to_string()));
        assert_eq!(result[0].password, Some("password".to_string()));
    }

    #[test]
    fn test_parse_protocol_prefix_ssh() {
        let entity = create_test_config();
        let result = entity.parse_connection_text("后台192.168.1.1\tadmin\tsecret");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 22);
        assert_eq!(result[0].protocol, ConnectionProtocol::Ssh);
        assert_eq!(result[0].username, Some("admin".to_string()));
        assert_eq!(result[0].password, Some("secret".to_string()));
    }

    #[test]
    fn test_parse_multiline_single_connection() {
        let entity = create_test_config();
        let result = entity.parse_connection_text("6.6.62.23 slot23\nhuawei\nRouter@202508");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "6.6.62.23");
        assert!(result[0].name.as_ref().unwrap().contains("slot23"));
        assert_eq!(result[0].username, Some("huawei".to_string()));
        assert_eq!(result[0].password, Some("Router@202508".to_string()));
    }

    #[test]
    fn test_parse_multiple_entries_newline() {
        let entity = create_test_config();
        let result = entity.parse_connection_text("192.168.1.1\n192.168.1.2");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[1].host, "192.168.1.2");
    }

    #[test]
    fn test_parse_multiple_entries_comma() {
        let entity = create_test_config();
        let result = entity.parse_connection_text("192.168.1.1, 192.168.1.2");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[1].host, "192.168.1.2");
    }

    #[test]
    fn test_parse_slash_separator() {
        let entity = create_test_config();
        let result = entity.parse_connection_text("192.168.1.1 user/pass");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].username, Some("user".to_string()));
        assert_eq!(result[0].password, Some("pass".to_string()));
    }

    #[test]
    fn test_parse_tab_separator() {
        let entity = create_test_config();
        let result = entity.parse_connection_text("192.168.1.1\troot\tpassword");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].username, Some("root".to_string()));
        assert_eq!(result[0].password, Some("password".to_string()));
    }

    #[test]
    fn test_config_serialization() {
        let config = RecognizeConfig::with_defaults();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let deserialized: RecognizeConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.version, RecognizeConfig::CURRENT_VERSION);
        assert!(!deserialized.token_classifiers.is_empty());
        assert!(!deserialized.protocol_prefixes.is_empty());
    }

    #[test]
    fn test_is_valid_ipv4() {
        assert!(RecognizeConfigEntity::is_valid_ipv4("192.168.1.1"));
        assert!(RecognizeConfigEntity::is_valid_ipv4("0.0.0.0"));
        assert!(RecognizeConfigEntity::is_valid_ipv4("255.255.255.255"));
        assert!(!RecognizeConfigEntity::is_valid_ipv4("256.1.1.1"));
        assert!(!RecognizeConfigEntity::is_valid_ipv4("192.168.1"));
        assert!(!RecognizeConfigEntity::is_valid_ipv4("not.an.ip.address"));
    }

    #[test]
    fn test_classify_token() {
        let entity = create_test_config();

        assert_eq!(
            entity.classify_token("192.168.1.1"),
            TokenType::Ip
        );
        assert_eq!(entity.classify_token("22"), TokenType::Port);
        assert_eq!(entity.classify_token("root"), TokenType::Username);
        assert_eq!(entity.classify_token("Admin@123"), TokenType::Password);
        assert_eq!(entity.classify_token("slot23"), TokenType::Label);
        assert_eq!(entity.classify_token("管理网口"), TokenType::Label);
    }

    #[test]
    fn test_empty_input() {
        let entity = create_test_config();
        let result = entity.parse_connection_text("");

        assert!(result.is_empty());
    }

    #[test]
    fn test_whitespace_input() {
        let entity = create_test_config();
        let result = entity.parse_connection_text("   ");

        assert!(result.is_empty());
    }

    #[test]
    fn test_multiple_ports_share_credentials() {
        let entity = create_test_config();
        let result = entity.parse_connection_text("192.168.1.1:2323 2222 root Admin@123");

        assert_eq!(result.len(), 2);
        // Both connections should have the same credentials
        assert_eq!(result[0].username, Some("root".to_string()));
        assert_eq!(result[0].password, Some("Admin@123".to_string()));
        assert_eq!(result[1].username, Some("root".to_string()));
        assert_eq!(result[1].password, Some("Admin@123".to_string()));
    }

    #[test]
    fn test_embedded_default_config_parses() {
        // Verify that the embedded JSON can be parsed correctly
        let config: RecognizeConfig =
            serde_json::from_slice(super::DEFAULT_RECOGNIZE_CONFIG).unwrap();

        assert_eq!(config.version, RecognizeConfig::CURRENT_VERSION);
        assert!(!config.token_classifiers.is_empty());
        assert!(!config.protocol_prefixes.is_empty());
        assert_eq!(config.defaults.port, 23);
        assert_eq!(config.defaults.protocol, "telnet");
    }
}

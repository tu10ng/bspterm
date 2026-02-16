use editor::Editor;
use gpui::{App, Entity, IntoElement, ParentElement, Styled, Window};
use ui::{prelude::*, Color, Icon, IconName, IconSize, Label, LabelSize, h_flex, v_flex};

const SESSION_ENV_PREFIX_TELNET: &str = "环境";
const SESSION_ENV_PREFIX_SSH: &str = "后台";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionProtocol {
    Ssh,
    Telnet,
}

#[derive(Clone, Debug)]
pub struct ParsedConnection {
    pub host: String,
    pub port: u16,
    pub protocol: ConnectionProtocol,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl ParsedConnection {
    pub fn telnet(host: String, port: u16) -> Self {
        Self {
            host,
            port,
            protocol: ConnectionProtocol::Telnet,
            username: None,
            password: None,
        }
    }

    pub fn telnet_with_credentials(
        host: String,
        port: u16,
        username: String,
        password: String,
    ) -> Self {
        Self {
            host,
            port,
            protocol: ConnectionProtocol::Telnet,
            username: Some(username),
            password: Some(password),
        }
    }

    pub fn ssh(host: String, port: u16) -> Self {
        Self {
            host,
            port,
            protocol: ConnectionProtocol::Ssh,
            username: None,
            password: None,
        }
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

pub struct AutoRecognizeSection {
    editor: Entity<Editor>,
}

impl AutoRecognizeSection {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        let editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("IP, IP:port, IP user pass...", window, cx);
            editor
        });

        Self { editor }
    }

    pub fn get_input(&self, cx: &App) -> String {
        self.editor.read(cx).text(cx)
    }

    pub fn clear_input(&mut self, window: &mut Window, cx: &mut App) {
        self.editor.update(cx, |editor, cx| {
            editor.set_text("", window, cx);
        });
    }

    pub fn editor(&self) -> &Entity<Editor> {
        &self.editor
    }

    pub fn render(&self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        v_flex()
            .w_full()
            .gap_1()
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Icon::new(IconName::MagnifyingGlass)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new("Auto-recognize")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .border_1()
                    .border_color(theme.colors().border)
                    .rounded_sm()
                    .px_1()
                    .py_px()
                    .child(self.editor.clone()),
            )
            .child(
                Label::new("Supports: IP, IP:port, IP user pass, 环境/后台 format")
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
    }
}

pub fn parse_connection_text(input: &str) -> Vec<ParsedConnection> {
    let input = input.trim();
    if input.is_empty() {
        return Vec::new();
    }

    let mut connections = Vec::new();

    let entries: Vec<&str> = if input.contains('\n') {
        input.lines().collect()
    } else if input.contains(',') {
        input.split(',').collect()
    } else {
        vec![input]
    };

    for entry in entries {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }

        if let Some(connection) = parse_single_entry(entry) {
            connections.push(connection);
        }
    }

    connections
}

fn parse_host_port_with_default(input: &str, default_port: u16) -> Option<(String, u16)> {
    if let Some((host, port_str)) = input.rsplit_once(':') {
        let port = port_str.parse::<u16>().ok()?;
        Some((host.to_string(), port))
    } else {
        Some((input.to_string(), default_port))
    }
}

fn parse_session_env_info_entry(entry: &str) -> Option<ParsedConnection> {
    let entry = entry.trim();

    let (protocol, rest) = if let Some(rest) = entry.strip_prefix(SESSION_ENV_PREFIX_TELNET) {
        (ConnectionProtocol::Telnet, rest)
    } else if let Some(rest) = entry.strip_prefix(SESSION_ENV_PREFIX_SSH) {
        (ConnectionProtocol::Ssh, rest)
    } else {
        return None;
    };

    let default_port = match protocol {
        ConnectionProtocol::Telnet => 23,
        ConnectionProtocol::Ssh => 22,
    };

    let parts: Vec<&str> = rest.split('\t').collect();
    if parts.is_empty() {
        return None;
    }

    let host_port = parts[0].trim();
    let (host, port) = parse_host_port_with_default(host_port, default_port)?;

    if !is_valid_ipv4(&host) && !is_valid_hostname(&host) {
        return None;
    }

    let username = parts.get(1).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let password = parts.get(2).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

    Some(ParsedConnection {
        host,
        port,
        protocol,
        username,
        password,
    })
}

pub fn is_session_env_info_format(input: &str) -> bool {
    input.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with(SESSION_ENV_PREFIX_TELNET) || trimmed.starts_with(SESSION_ENV_PREFIX_SSH)
    })
}

fn parse_single_entry(entry: &str) -> Option<ParsedConnection> {
    let trimmed = entry.trim();
    if trimmed.starts_with(SESSION_ENV_PREFIX_TELNET) || trimmed.starts_with(SESSION_ENV_PREFIX_SSH) {
        return parse_session_env_info_entry(trimmed);
    }

    let parts: Vec<&str> = entry.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let host_port = parts[0];
    let (host, port) = parse_host_port(host_port)?;

    if !is_valid_ipv4(&host) && !is_valid_hostname(&host) {
        return None;
    }

    let default_port = if port == 22 { 22 } else { port };

    match parts.len() {
        1 => {
            if port == 22 {
                Some(ParsedConnection::ssh(host, port))
            } else {
                Some(ParsedConnection::telnet(host, default_port.max(23)))
            }
        }
        2 => Some(ParsedConnection::telnet(host, default_port.max(23)).with_username(parts[1].to_string())),
        3 => Some(ParsedConnection::telnet_with_credentials(
            host,
            default_port.max(23),
            parts[1].to_string(),
            parts[2].to_string(),
        )),
        4 => {
            let custom_port = parts[3].parse::<u16>().unwrap_or(23);
            Some(ParsedConnection::telnet_with_credentials(
                host,
                custom_port,
                parts[1].to_string(),
                parts[2].to_string(),
            ))
        }
        _ => None,
    }
}

fn parse_host_port(input: &str) -> Option<(String, u16)> {
    if let Some((host, port_str)) = input.rsplit_once(':') {
        let port = port_str.parse::<u16>().ok()?;
        Some((host.to_string(), port))
    } else {
        Some((input.to_string(), 23))
    }
}

fn is_valid_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }

    parts.iter().all(|part| {
        part.parse::<u8>().is_ok()
    })
}

fn is_valid_hostname(s: &str) -> bool {
    if s.is_empty() || s.len() > 253 {
        return false;
    }

    s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_ip() {
        let result = parse_connection_text("192.168.1.1");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 23);
        assert_eq!(result[0].protocol, ConnectionProtocol::Telnet);
    }

    #[test]
    fn test_parse_ip_with_port() {
        let result = parse_connection_text("192.168.1.1:22");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 22);
        assert_eq!(result[0].protocol, ConnectionProtocol::Ssh);
    }

    #[test]
    fn test_parse_ip_with_credentials() {
        let result = parse_connection_text("192.168.1.1 admin password123");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 23);
        assert_eq!(result[0].username, Some("admin".to_string()));
        assert_eq!(result[0].password, Some("password123".to_string()));
    }

    #[test]
    fn test_parse_ip_with_credentials_and_port() {
        let result = parse_connection_text("192.168.1.1 admin password123 2323");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 2323);
        assert_eq!(result[0].username, Some("admin".to_string()));
        assert_eq!(result[0].password, Some("password123".to_string()));
    }

    #[test]
    fn test_parse_multiple_ips_comma() {
        let result = parse_connection_text("192.168.1.1, 192.168.1.2");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[1].host, "192.168.1.2");
    }

    #[test]
    fn test_parse_multiple_ips_newline() {
        let result = parse_connection_text("192.168.1.1\n192.168.1.2");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[1].host, "192.168.1.2");
    }

    #[test]
    fn test_parse_empty() {
        let result = parse_connection_text("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_is_valid_ipv4() {
        assert!(is_valid_ipv4("192.168.1.1"));
        assert!(is_valid_ipv4("0.0.0.0"));
        assert!(is_valid_ipv4("255.255.255.255"));
        assert!(!is_valid_ipv4("256.1.1.1"));
        assert!(!is_valid_ipv4("192.168.1"));
        assert!(!is_valid_ipv4("not.an.ip.address"));
    }

    #[test]
    fn test_parse_session_env_info_telnet() {
        let result = parse_connection_text("环境192.168.1.1\troot\tpassword");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 23);
        assert_eq!(result[0].protocol, ConnectionProtocol::Telnet);
        assert_eq!(result[0].username, Some("root".to_string()));
        assert_eq!(result[0].password, Some("password".to_string()));
    }

    #[test]
    fn test_parse_session_env_info_ssh() {
        let result = parse_connection_text("后台192.168.1.1\tadmin\tsecret");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 22);
        assert_eq!(result[0].protocol, ConnectionProtocol::Ssh);
        assert_eq!(result[0].username, Some("admin".to_string()));
        assert_eq!(result[0].password, Some("secret".to_string()));
    }

    #[test]
    fn test_parse_session_env_info_with_port() {
        let result = parse_connection_text("环境192.168.1.1:2323\troot\tpassword");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 2323);
        assert_eq!(result[0].protocol, ConnectionProtocol::Telnet);
    }

    #[test]
    fn test_parse_session_env_info_ssh_with_port() {
        let result = parse_connection_text("后台192.168.1.1:2222\tadmin\tsecret");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 2222);
        assert_eq!(result[0].protocol, ConnectionProtocol::Ssh);
    }

    #[test]
    fn test_parse_session_env_info_multiple() {
        let input = "环境192.168.1.1\troot\tpass1\n后台192.168.1.1\tadmin\tpass2\n环境192.168.1.2\troot\tpass3";
        let result = parse_connection_text(input);
        assert_eq!(result.len(), 3);

        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 23);
        assert_eq!(result[0].protocol, ConnectionProtocol::Telnet);

        assert_eq!(result[1].host, "192.168.1.1");
        assert_eq!(result[1].port, 22);
        assert_eq!(result[1].protocol, ConnectionProtocol::Ssh);

        assert_eq!(result[2].host, "192.168.1.2");
        assert_eq!(result[2].port, 23);
        assert_eq!(result[2].protocol, ConnectionProtocol::Telnet);
    }

    #[test]
    fn test_is_session_env_info_format() {
        assert!(is_session_env_info_format("环境192.168.1.1\troot\tpass"));
        assert!(is_session_env_info_format("后台192.168.1.1\tadmin\tpass"));
        assert!(is_session_env_info_format("环境192.168.1.1\troot\tpass1\n后台192.168.1.2\tadmin\tpass2"));
        assert!(!is_session_env_info_format("192.168.1.1"));
        assert!(!is_session_env_info_format("192.168.1.1 root password"));
    }

    #[test]
    fn test_parse_session_env_info_no_credentials() {
        let result = parse_connection_text("环境192.168.1.1");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 23);
        assert_eq!(result[0].protocol, ConnectionProtocol::Telnet);
        assert_eq!(result[0].username, None);
        assert_eq!(result[0].password, None);
    }

    #[test]
    fn test_parse_session_env_info_username_only() {
        let result = parse_connection_text("环境192.168.1.1\troot");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].username, Some("root".to_string()));
        assert_eq!(result[0].password, None);
    }
}

use editor::Editor;
use gpui::{App, Entity, IntoElement, ParentElement, Styled, Window};
use i18n::t;
use ui::{prelude::*, Color, Icon, IconName, IconSize, Label, LabelSize, h_flex, v_flex};

const SESSION_ENV_PREFIX_TELNET: &str = "环境";
const SESSION_ENV_PREFIX_SSH: &str = "后台";

const COMMON_USERNAMES: &[&str] = &[
    "root", "admin", "administrator", "user", "test", "guest",
    "huawei", "cisco", "zte", "nokia", "juniper",
    "oracle", "mysql", "postgres",
    "ubuntu", "centos", "debian",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenType {
    Ip,
    Port,
    Username,
    Password,
    Label,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionProtocol {
    Ssh,
    Telnet,
}

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
    pub fn telnet(host: String, port: u16) -> Self {
        Self {
            name: None,
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
            name: None,
            host,
            port,
            protocol: ConnectionProtocol::Telnet,
            username: Some(username),
            password: Some(password),
        }
    }

    pub fn ssh(host: String, port: u16) -> Self {
        Self {
            name: None,
            host,
            port,
            protocol: ConnectionProtocol::Ssh,
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

pub struct AutoRecognizeSection {
    editor: Entity<Editor>,
}

impl AutoRecognizeSection {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        let editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(&t("remote_explorer.auto_recognize_hint"), window, cx);
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
                        Label::new(t("remote_explorer.auto_recognize"))
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
                Label::new(t("remote_explorer.auto_recognize_hint"))
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

    if input.contains('\n') {
        let lines: Vec<&str> = input.lines().collect();

        if is_multiline_single_connection(&lines) {
            if let Some(conn) = parse_multiline_connection(&lines) {
                connections.push(conn);
                return connections;
            }
        }

        for entry in lines {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }

            if let Some(connection) = parse_single_entry(entry) {
                connections.push(connection);
            }
        }
    } else if input.contains(',') {
        let entries: Vec<&str> = input.split(',').collect();
        for entry in entries {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }

            if let Some(connection) = parse_single_entry(entry) {
                connections.push(connection);
            }
        }
    } else if let Some(connection) = parse_single_entry(input) {
        connections.push(connection);
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
        name: None,
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

    let (ip_start, ip_end) = find_ipv4_position(trimmed)?;

    let host = &trimmed[ip_start..ip_end];
    if !is_valid_ipv4(host) {
        return None;
    }

    let mut host_port_end = ip_end;
    let mut port = 23u16;

    let remaining_after_ip = &trimmed[ip_end..];
    if remaining_after_ip.starts_with(':') {
        let port_start = 1;
        let port_end = remaining_after_ip[port_start..]
            .find(|c: char| !c.is_ascii_digit())
            .map(|i| port_start + i)
            .unwrap_or(remaining_after_ip.len());

        if port_end > port_start {
            if let Ok(parsed_port) = remaining_after_ip[port_start..port_end].parse::<u16>() {
                port = parsed_port;
                host_port_end = ip_end + port_end;
            }
        }
    }

    let name = trimmed[..host_port_end].trim().to_string();

    let remaining = trimmed[host_port_end..].trim();
    let (username, password, custom_port, label) = parse_credentials_flexible(remaining);

    if let Some(p) = custom_port {
        port = p;
    }

    let final_name = if let Some(extra_label) = label {
        format!("{} {}", name, extra_label)
    } else {
        name
    };

    let protocol = if port == 22 {
        ConnectionProtocol::Ssh
    } else {
        ConnectionProtocol::Telnet
    };

    Some(ParsedConnection {
        name: Some(final_name),
        host: host.to_string(),
        port,
        protocol,
        username,
        password,
    })
}

fn find_ipv4_position(input: &str) -> Option<(usize, usize)> {
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

fn parse_credentials_flexible(input: &str) -> (Option<String>, Option<String>, Option<u16>, Option<String>) {
    let input = input.trim();
    if input.is_empty() {
        return (None, None, None, None);
    }

    if input.contains('\t') {
        let parts: Vec<&str> = input.split('\t').collect();
        let username = parts.first().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let password = parts.get(1).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        return (username, password, None, None);
    }

    if let Some((user, pass)) = input.split_once('/') {
        let user = user.trim();
        let pass = pass.trim();
        if !user.is_empty() {
            return (Some(user.to_string()), if pass.is_empty() { None } else { Some(pass.to_string()) }, None, None);
        }
    }

    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.is_empty() {
        return (None, None, None, None);
    }

    let mut username: Option<String> = None;
    let mut password: Option<String> = None;
    let mut port: Option<u16> = None;
    let mut labels: Vec<String> = Vec::new();

    for part in parts {
        let token_type = classify_token(part);
        match token_type {
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
            TokenType::Port => {
                if port.is_none() {
                    port = part.parse().ok();
                }
            }
            TokenType::Label => {
                labels.push(part.to_string());
            }
            TokenType::Ip => {}
        }
    }

    let label = if labels.is_empty() {
        None
    } else {
        Some(labels.join(" "))
    };

    (username, password, port, label)
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

fn is_common_username(s: &str) -> bool {
    let lower = s.to_lowercase();
    COMMON_USERNAMES.iter().any(|u| *u == lower)
}

fn starts_with_common_username(s: &str) -> bool {
    let lower = s.to_lowercase();
    COMMON_USERNAMES.iter().any(|u| lower.starts_with(*u))
}

fn is_likely_username(s: &str) -> bool {
    if is_likely_password(s) {
        return false;
    }
    if is_common_username(s) {
        return true;
    }
    if starts_with_common_username(s) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return true;
    }
    if s.len() < 3 || s.len() > 20 {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphabetic() || c == '_' || c == '-')
}

fn is_likely_password(s: &str) -> bool {
    let special_chars = "@!#$%^&*()_+-=[]{}|;':\",./<>?~`";
    if s.chars().any(|c| special_chars.contains(c)) {
        return true;
    }
    if s.len() >= 8 {
        let has_upper = s.chars().any(|c| c.is_ascii_uppercase());
        let has_lower = s.chars().any(|c| c.is_ascii_lowercase());
        let has_digit = s.chars().any(|c| c.is_ascii_digit());
        if (has_upper && has_lower) || (has_upper && has_digit) || (has_lower && has_digit) {
            return true;
        }
    }
    false
}

fn is_likely_label(s: &str) -> bool {
    if !s.is_ascii() {
        return true;
    }
    let has_alpha = s.chars().any(|c| c.is_ascii_alphabetic());
    let has_digit = s.chars().any(|c| c.is_ascii_digit());
    if has_alpha && has_digit && !is_valid_ipv4(s) && !is_common_username(s) {
        return true;
    }
    false
}

fn classify_token(token: &str) -> TokenType {
    if is_valid_ipv4(token) {
        return TokenType::Ip;
    }
    if token.chars().all(|c| c.is_ascii_digit()) && !token.is_empty() {
        if token.parse::<u16>().is_ok() {
            return TokenType::Port;
        }
    }
    if is_likely_password(token) {
        return TokenType::Password;
    }
    if is_likely_username(token) {
        return TokenType::Username;
    }
    if is_likely_label(token) {
        return TokenType::Label;
    }
    TokenType::Username
}

fn is_multiline_single_connection(lines: &[&str]) -> bool {
    if lines.len() < 2 || lines.len() > 4 {
        return false;
    }

    let first_line = lines[0].trim();
    if find_ipv4_position(first_line).is_none() {
        return false;
    }

    for line in &lines[1..] {
        let trimmed = line.trim();
        if find_ipv4_position(trimmed).is_some() {
            return false;
        }
    }

    true
}

fn parse_multiline_connection(lines: &[&str]) -> Option<ParsedConnection> {
    let first_line = lines[0].trim();
    let mut conn = parse_single_entry(first_line)?;

    let mut username: Option<String> = None;
    let mut password: Option<String> = None;

    for line in &lines[1..] {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let token_type = classify_token(trimmed);
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

    #[test]
    fn test_parse_with_name_prefix() {
        let result = parse_connection_text("管理网口127.0.0.1");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, Some("管理网口127.0.0.1".to_string()));
        assert_eq!(result[0].host, "127.0.0.1");
        assert_eq!(result[0].port, 23);
        assert_eq!(result[0].protocol, ConnectionProtocol::Telnet);
    }

    #[test]
    fn test_parse_slash_separator() {
        let result = parse_connection_text("192.168.1.1 user/pass");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, Some("192.168.1.1".to_string()));
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 23);
        assert_eq!(result[0].username, Some("user".to_string()));
        assert_eq!(result[0].password, Some("pass".to_string()));
    }

    #[test]
    fn test_parse_mixed_format() {
        let result = parse_connection_text("管理网口127.0.0.1 root123/Root@123");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, Some("管理网口127.0.0.1".to_string()));
        assert_eq!(result[0].host, "127.0.0.1");
        assert_eq!(result[0].port, 23);
        assert_eq!(result[0].username, Some("root123".to_string()));
        assert_eq!(result[0].password, Some("Root@123".to_string()));
    }

    #[test]
    fn test_parse_name_with_spaces() {
        let result = parse_connection_text("dev server 192.168.1.1 root/pass");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, Some("dev server 192.168.1.1".to_string()));
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].port, 23);
        assert_eq!(result[0].username, Some("root".to_string()));
        assert_eq!(result[0].password, Some("pass".to_string()));
    }

    #[test]
    fn test_parse_name_includes_ip() {
        let result = parse_connection_text("192.168.1.1");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, Some("192.168.1.1".to_string()));
        assert_eq!(result[0].host, "192.168.1.1");
    }

    #[test]
    fn test_parse_name_with_port() {
        let result = parse_connection_text("测试10.0.0.1:22 admin pass");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, Some("测试10.0.0.1:22".to_string()));
        assert_eq!(result[0].host, "10.0.0.1");
        assert_eq!(result[0].port, 22);
        assert_eq!(result[0].protocol, ConnectionProtocol::Ssh);
        assert_eq!(result[0].username, Some("admin".to_string()));
        assert_eq!(result[0].password, Some("pass".to_string()));
    }

    #[test]
    fn test_find_ipv4_position() {
        assert_eq!(find_ipv4_position("192.168.1.1"), Some((0, 11)));
        assert_eq!(find_ipv4_position("管理网口127.0.0.1"), Some((12, 21)));
        assert_eq!(find_ipv4_position("no ip here"), None);
        assert_eq!(find_ipv4_position("prefix 10.0.0.1 suffix"), Some((7, 15)));
    }

    #[test]
    fn test_parse_credentials_flexible() {
        assert_eq!(parse_credentials_flexible(""), (None, None, None, None));
        assert_eq!(parse_credentials_flexible("user/pass"), (Some("user".to_string()), Some("pass".to_string()), None, None));
        assert_eq!(parse_credentials_flexible("user\tpass"), (Some("user".to_string()), Some("pass".to_string()), None, None));
        assert_eq!(parse_credentials_flexible("user pass"), (Some("user".to_string()), Some("pass".to_string()), None, None));
        assert_eq!(parse_credentials_flexible("user"), (Some("user".to_string()), None, None, None));
        assert_eq!(parse_credentials_flexible("user pass 2323"), (Some("user".to_string()), Some("pass".to_string()), Some(2323), None));
    }

    #[test]
    fn test_multiline_ip_name_user_pass() {
        let input = "6.6.62.23 slot23\nhuawei\nRouter@202508";
        let result = parse_connection_text(input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "6.6.62.23");
        assert!(result[0].name.as_ref().unwrap().contains("slot23"));
        assert_eq!(result[0].username, Some("huawei".to_string()));
        assert_eq!(result[0].password, Some("Router@202508".to_string()));
    }

    #[test]
    fn test_smart_credential_detection() {
        let input = "6.6.62.23 root123 Root@123 slot23";
        let result = parse_connection_text(input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].host, "6.6.62.23");
        assert_eq!(result[0].username, Some("root123".to_string()));
        assert_eq!(result[0].password, Some("Root@123".to_string()));
        assert!(result[0].name.as_ref().unwrap().contains("slot23"));
    }

    #[test]
    fn test_chinese_prefix_with_credentials() {
        let input = "管理网口192.168.1.1 huawei Admin@123";
        let result = parse_connection_text(input);
        assert_eq!(result.len(), 1);
        assert!(result[0].name.as_ref().unwrap().contains("管理网口"));
        assert_eq!(result[0].host, "192.168.1.1");
        assert_eq!(result[0].username, Some("huawei".to_string()));
        assert_eq!(result[0].password, Some("Admin@123".to_string()));
    }

    #[test]
    fn test_is_likely_password() {
        assert!(is_likely_password("Root@123"));
        assert!(is_likely_password("Admin#456"));
        assert!(is_likely_password("Password1!"));
        assert!(!is_likely_password("root"));
        assert!(!is_likely_password("admin"));
    }

    #[test]
    fn test_is_likely_username() {
        assert!(is_likely_username("root"));
        assert!(is_likely_username("admin"));
        assert!(is_likely_username("huawei"));
        assert!(is_likely_username("user123"));
        assert!(!is_likely_username("Root@123"));
    }

    #[test]
    fn test_classify_token() {
        assert_eq!(classify_token("192.168.1.1"), TokenType::Ip);
        assert_eq!(classify_token("22"), TokenType::Port);
        assert_eq!(classify_token("root"), TokenType::Username);
        assert_eq!(classify_token("Admin@123"), TokenType::Password);
        assert_eq!(classify_token("slot23"), TokenType::Label);
        assert_eq!(classify_token("管理网口"), TokenType::Label);
    }

    #[test]
    fn test_multiline_single_connection_detection() {
        let lines: Vec<&str> = vec!["6.6.62.23 slot23", "huawei", "Router@202508"];
        assert!(is_multiline_single_connection(&lines));

        let lines: Vec<&str> = vec!["192.168.1.1", "192.168.1.2"];
        assert!(!is_multiline_single_connection(&lines));

        let lines: Vec<&str> = vec!["no ip here", "huawei", "pass"];
        assert!(!is_multiline_single_connection(&lines));
    }
}

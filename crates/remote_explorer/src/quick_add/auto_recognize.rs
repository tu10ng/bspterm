use editor::Editor;
use gpui::{App, Entity, IntoElement, ParentElement, Styled, Window};
use i18n::t;
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
    let (username, password, custom_port) = parse_credentials_flexible(remaining);

    if let Some(p) = custom_port {
        port = p;
    }

    let protocol = if port == 22 {
        ConnectionProtocol::Ssh
    } else {
        ConnectionProtocol::Telnet
    };

    Some(ParsedConnection {
        name: Some(name),
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

fn parse_credentials_flexible(input: &str) -> (Option<String>, Option<String>, Option<u16>) {
    let input = input.trim();
    if input.is_empty() {
        return (None, None, None);
    }

    if input.contains('\t') {
        let parts: Vec<&str> = input.split('\t').collect();
        let username = parts.first().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let password = parts.get(1).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        return (username, password, None);
    }

    if let Some((user, pass)) = input.split_once('/') {
        let user = user.trim();
        let pass = pass.trim();
        if !user.is_empty() {
            return (Some(user.to_string()), if pass.is_empty() { None } else { Some(pass.to_string()) }, None);
        }
    }

    let parts: Vec<&str> = input.split_whitespace().collect();
    match parts.len() {
        0 => (None, None, None),
        1 => (Some(parts[0].to_string()), None, None),
        2 => (Some(parts[0].to_string()), Some(parts[1].to_string()), None),
        3 => {
            let port = parts[2].parse::<u16>().ok();
            (Some(parts[0].to_string()), Some(parts[1].to_string()), port)
        }
        _ => (Some(parts[0].to_string()), Some(parts[1].to_string()), None),
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
        assert_eq!(parse_credentials_flexible(""), (None, None, None));
        assert_eq!(parse_credentials_flexible("user/pass"), (Some("user".to_string()), Some("pass".to_string()), None));
        assert_eq!(parse_credentials_flexible("user\tpass"), (Some("user".to_string()), Some("pass".to_string()), None));
        assert_eq!(parse_credentials_flexible("user pass"), (Some("user".to_string()), Some("pass".to_string()), None));
        assert_eq!(parse_credentials_flexible("user"), (Some("user".to_string()), None, None));
        assert_eq!(parse_credentials_flexible("user pass 2323"), (Some("user".to_string()), Some("pass".to_string()), Some(2323)));
    }
}

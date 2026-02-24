use std::fs::{File, OpenOptions};
use std::io::{LineWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use chrono::{DateTime, Local};

use crate::terminal_settings::SessionLoggingSettings;

pub struct SessionMetadata {
    pub session_name: String,
    pub protocol: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub start_time: DateTime<Local>,
}

impl SessionMetadata {
    pub fn new_local() -> Self {
        Self {
            session_name: "local".to_string(),
            protocol: "local".to_string(),
            host: None,
            port: None,
            username: None,
            start_time: Local::now(),
        }
    }

    pub fn new_ssh(host: String, port: u16, username: Option<String>) -> Self {
        let session_name = if let Some(ref user) = username {
            format!("{}@{}_{}", user, host, port)
        } else {
            format!("{}_{}", host, port)
        };
        Self {
            session_name,
            protocol: "ssh".to_string(),
            host: Some(host),
            port: Some(port),
            username,
            start_time: Local::now(),
        }
    }

    pub fn new_telnet(host: String, port: u16, username: Option<String>) -> Self {
        let session_name = if let Some(ref user) = username {
            format!("{}@{}_{}", user, host, port)
        } else {
            format!("{}_{}", host, port)
        };
        Self {
            session_name,
            protocol: "telnet".to_string(),
            host: Some(host),
            port: Some(port),
            username,
            start_time: Local::now(),
        }
    }
}

pub struct SessionLogger {
    settings: SessionLoggingSettings,
    metadata: SessionMetadata,
    writer: Option<LineWriter<File>>,
    current_file_path: Option<PathBuf>,
    line_start: bool,
}

impl SessionLogger {
    pub fn new(settings: SessionLoggingSettings, metadata: SessionMetadata) -> Self {
        Self {
            settings,
            metadata,
            writer: None,
            current_file_path: None,
            line_start: true,
        }
    }

    pub fn start(&mut self) -> Result<()> {
        log::debug!("SessionLogger::start() called");
        if self.writer.is_some() {
            log::debug!("SessionLogger already started, returning early");
            return Ok(());
        }

        let log_dir = expand_path(&self.settings.log_directory)?;
        log::debug!("Session log directory: {:?}", log_dir);
        std::fs::create_dir_all(&log_dir)
            .with_context(|| format!("Failed to create log directory: {:?}", log_dir))?;

        let filename = generate_filename(&self.settings.filename_pattern, &self.metadata);
        let file_path = log_dir.join(&filename);
        log::debug!("Session log file path: {:?}", file_path);

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .with_context(|| format!("Failed to open log file: {:?}", file_path))?;

        self.writer = Some(LineWriter::new(file));
        self.current_file_path = Some(file_path.clone());
        self.line_start = true;

        log::info!("Created session log file: {:?}", file_path);

        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        if let Some(ref mut writer) = self.writer {
            writer
                .flush()
                .with_context(|| "Failed to flush log file")?;
        }
        self.writer = None;
        Ok(())
    }

    pub fn log_output(&mut self, data: &[u8], timestamp: DateTime<Local>) -> Result<()> {
        let writer = match self.writer.as_mut() {
            Some(w) => w,
            None => return Ok(()),
        };

        let processed_data = if self.settings.include_ansi_codes {
            data.to_vec()
        } else {
            strip_ansi_codes(data)
        };

        if processed_data.is_empty() {
            return Ok(());
        }

        if self.settings.timestamp_format.is_empty() {
            writer.write_all(&processed_data)?;
        } else {
            for byte in processed_data.iter() {
                if self.line_start {
                    let ts = timestamp.format(&self.settings.timestamp_format).to_string();
                    writer.write_all(ts.as_bytes())?;
                    self.line_start = false;
                }

                writer.write_all(&[*byte])?;

                if *byte == b'\n' {
                    self.line_start = true;
                }
            }
        }

        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        if let Some(ref mut writer) = self.writer {
            writer
                .flush()
                .with_context(|| "Failed to flush log file")?;
        }
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        self.writer.is_some()
    }

    pub fn current_file_path(&self) -> Option<&Path> {
        self.current_file_path.as_deref()
    }

    pub fn settings(&self) -> &SessionLoggingSettings {
        &self.settings
    }
}

pub fn expand_path(path: &Path) -> Result<PathBuf> {
    let path_str = path.to_string_lossy();

    if path_str.starts_with('~') {
        let home = home_dir()?;
        let rest = path_str.strip_prefix("~/").unwrap_or(
            path_str.strip_prefix('~').unwrap_or(&path_str),
        );
        if rest.is_empty() {
            Ok(home)
        } else {
            Ok(home.join(rest))
        }
    } else {
        Ok(path.to_path_buf())
    }
}

fn home_dir() -> Result<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .with_context(|| "HOME environment variable not set")
    }

    #[cfg(windows)]
    {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .with_context(|| "USERPROFILE environment variable not set")
    }
}

fn generate_filename(pattern: &str, metadata: &SessionMetadata) -> String {
    let now = metadata.start_time;

    let mut result = pattern.to_string();

    result = result.replace("${session_name}", &metadata.session_name);
    result = result.replace("${protocol}", &metadata.protocol);
    result = result.replace(
        "${host}",
        metadata.host.as_deref().unwrap_or("unknown"),
    );
    result = result.replace(
        "${port}",
        &metadata.port.map(|p| p.to_string()).unwrap_or_else(|| "0".to_string()),
    );
    result = result.replace(
        "${username}",
        metadata.username.as_deref().unwrap_or("unknown"),
    );

    result = result.replace("%Y", &now.format("%Y").to_string());
    result = result.replace("%m", &now.format("%m").to_string());
    result = result.replace("%d", &now.format("%d").to_string());
    result = result.replace("%H", &now.format("%H").to_string());
    result = result.replace("%M", &now.format("%M").to_string());
    result = result.replace("%S", &now.format("%S").to_string());

    sanitize_filename(&result)
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

fn strip_ansi_codes(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len());
    let mut i = 0;

    while i < data.len() {
        if data[i] == 0x1B {
            if i + 1 < data.len() && data[i + 1] == b'[' {
                i += 2;
                while i < data.len() {
                    let c = data[i];
                    i += 1;
                    if (b'@'..=b'~').contains(&c) {
                        break;
                    }
                }
                continue;
            } else if i + 1 < data.len() && data[i + 1] == b']' {
                i += 2;
                while i < data.len() {
                    if data[i] == 0x07 {
                        i += 1;
                        break;
                    }
                    if data[i] == 0x1B && i + 1 < data.len() && data[i + 1] == b'\\' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
                continue;
            } else if i + 1 < data.len() {
                let next = data[i + 1];
                if (b'('..=b'/').contains(&next) {
                    i += 3;
                    continue;
                }
                if (b'@'..=b'_').contains(&next) {
                    i += 2;
                    continue;
                }
            }
        }

        result.push(data[i]);
        i += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_codes_basic() {
        let input = b"Hello \x1b[31mWorld\x1b[0m!";
        let output = strip_ansi_codes(input);
        assert_eq!(output, b"Hello World!");
    }

    #[test]
    fn test_strip_ansi_codes_complex() {
        let input = b"\x1b[1;32mBold Green\x1b[0m Normal";
        let output = strip_ansi_codes(input);
        assert_eq!(output, b"Bold Green Normal");
    }

    #[test]
    fn test_strip_ansi_codes_cursor_movement() {
        let input = b"\x1b[2J\x1b[HHello";
        let output = strip_ansi_codes(input);
        assert_eq!(output, b"Hello");
    }

    #[test]
    fn test_strip_ansi_codes_osc_sequence() {
        let input = b"\x1b]0;Window Title\x07Hello";
        let output = strip_ansi_codes(input);
        assert_eq!(output, b"Hello");
    }

    #[test]
    fn test_strip_ansi_codes_no_escape() {
        let input = b"Plain text without escapes";
        let output = strip_ansi_codes(input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_generate_filename() {
        let metadata = SessionMetadata {
            session_name: "testhost_22".to_string(),
            protocol: "ssh".to_string(),
            host: Some("testhost".to_string()),
            port: Some(22),
            username: Some("user".to_string()),
            start_time: Local::now(),
        };

        let pattern = "${session_name}_${protocol}.log";
        let filename = generate_filename(pattern, &metadata);
        assert_eq!(filename, "testhost_22_ssh.log");
    }

    #[test]
    fn test_sanitize_filename() {
        let name = "file:name/with\\bad*chars?";
        let sanitized = sanitize_filename(name);
        assert_eq!(sanitized, "file_name_with_bad_chars_");
    }

    #[test]
    fn test_expand_path_no_tilde() {
        let path = PathBuf::from("/absolute/path");
        let expanded = expand_path(&path).unwrap();
        assert_eq!(expanded, path);
    }
}

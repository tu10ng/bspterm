use std::fs;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Configuration for connecting to a remote code server via SSH + Docker.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CodeServerConfig {
    pub ssh_host: String,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    pub ssh_user: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,
    pub container_id: String,
    pub code_root: String,
}

fn default_ssh_port() -> u16 {
    22
}

impl CodeServerConfig {
    pub fn new() -> Self {
        Self {
            ssh_host: String::new(),
            ssh_port: 22,
            ssh_user: String::new(),
            ssh_password: None,
            container_id: String::new(),
            code_root: "/usr1".to_string(),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.ssh_host.is_empty() && !self.ssh_user.is_empty()
    }

    pub fn display_host(&self) -> String {
        if self.ssh_port == 22 {
            format!("{}@{}", self.ssh_user, self.ssh_host)
        } else {
            format!("{}@{}:{}", self.ssh_user, self.ssh_host, self.ssh_port)
        }
    }

    pub fn load() -> Result<Self> {
        Self::load_from_file(paths::code_server_file())
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save(&self) -> Result<()> {
        self.save_to_file(paths::code_server_file())
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CodeServerConfig::new();
        assert!(!config.is_configured());
        assert_eq!(config.ssh_port, 22);
        assert_eq!(config.code_root, "/usr1");
    }

    #[test]
    fn test_display_host() {
        let mut config = CodeServerConfig::new();
        config.ssh_user = "root".to_string();
        config.ssh_host = "192.168.1.100".to_string();
        assert_eq!(config.display_host(), "root@192.168.1.100");

        config.ssh_port = 2222;
        assert_eq!(config.display_host(), "root@192.168.1.100:2222");
    }

    #[test]
    fn test_is_configured() {
        let mut config = CodeServerConfig::new();
        assert!(!config.is_configured());

        config.ssh_host = "host".to_string();
        assert!(!config.is_configured());

        config.ssh_user = "user".to_string();
        assert!(config.is_configured());

        // container_id is optional, should still be configured without it
        config.container_id = "container".to_string();
        assert!(config.is_configured());
    }

    #[test]
    fn test_serialization() {
        let config = CodeServerConfig {
            ssh_host: "192.168.1.100".to_string(),
            ssh_port: 22,
            ssh_user: "root".to_string(),
            ssh_password: Some("secret".to_string()),
            container_id: "my_container".to_string(),
            code_root: "/usr1".to_string(),
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: CodeServerConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.ssh_host, config.ssh_host);
        assert_eq!(parsed.ssh_password, config.ssh_password);
    }

    #[test]
    fn test_password_not_serialized_when_none() {
        let config = CodeServerConfig {
            ssh_host: "host".to_string(),
            ssh_port: 22,
            ssh_user: "user".to_string(),
            ssh_password: None,
            container_id: "container".to_string(),
            code_root: "/usr1".to_string(),
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("ssh_password"));
    }
}

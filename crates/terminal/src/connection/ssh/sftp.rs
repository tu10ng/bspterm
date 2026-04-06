use std::path::Path;

use anyhow::{Context as _, Result};
use russh_sftp::client::SftpSession;

/// A remote directory entry returned by SFTP operations.
#[derive(Clone, Debug)]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub permissions: u32,
    pub modified: Option<u64>,
}

/// Wraps an `russh_sftp::client::SftpSession` with convenience methods.
pub struct SftpClient {
    session: SftpSession,
}

impl SftpClient {
    /// Create a new SFTP client from an SSH channel that has the SFTP subsystem requested.
    pub async fn new(channel: russh::Channel<russh::client::Msg>) -> Result<Self> {
        let stream = channel.into_stream();
        let session = SftpSession::new(stream)
            .await
            .map_err(|e| anyhow::anyhow!("failed to initialize SFTP session: {}", e))?;
        Ok(Self { session })
    }

    /// List entries in a remote directory.
    pub async fn list_dir(&self, path: &str) -> Result<Vec<RemoteEntry>> {
        let read_dir = self
            .session
            .read_dir(path)
            .await
            .map_err(|e| anyhow::anyhow!("failed to read directory '{}': {}", path, e))?;

        let mut entries = Vec::new();
        for entry in read_dir {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let attrs = entry.metadata();
            let full_path = if path.ends_with('/') {
                format!("{}{}", path, name)
            } else {
                format!("{}/{}", path, name)
            };
            entries.push(RemoteEntry {
                name,
                path: full_path,
                is_dir: attrs.is_dir(),
                size: attrs.size.unwrap_or(0),
                permissions: attrs.permissions.unwrap_or(0),
                modified: attrs.mtime.map(|t| t as u64),
            });
        }

        // Sort: directories first, then by name
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        Ok(entries)
    }

    /// Read a remote file and return its contents as bytes (without writing to disk).
    pub async fn read_file_bytes(&self, remote_path: &str) -> Result<Vec<u8>> {
        self.session
            .read(remote_path)
            .await
            .map_err(|e| anyhow::anyhow!("failed to read remote file '{}': {}", remote_path, e))
    }

    /// Download a remote file to a local path.
    pub async fn read_file(&self, remote_path: &str, local_path: &Path) -> Result<()> {
        let data = self.read_file_bytes(remote_path).await?;
        std::fs::write(local_path, data)
            .context("failed to write local file")?;
        Ok(())
    }

    /// Upload a local file to a remote path.
    pub async fn write_file(&self, local_path: &Path, remote_path: &str) -> Result<()> {
        let data = std::fs::read(local_path)
            .context("failed to read local file")?;
        let mut file = self
            .session
            .create(remote_path)
            .await
            .map_err(|e| {
                anyhow::anyhow!("failed to create remote file '{}': {}", remote_path, e)
            })?;
        use tokio::io::AsyncWriteExt;
        file.write_all(&data).await.map_err(|e| {
            anyhow::anyhow!("failed to write remote file '{}': {}", remote_path, e)
        })?;
        Ok(())
    }

    /// Create a remote directory.
    pub async fn mkdir(&self, path: &str) -> Result<()> {
        self.session
            .create_dir(path)
            .await
            .map_err(|e| anyhow::anyhow!("failed to create directory '{}': {}", path, e))?;
        Ok(())
    }

    /// Remove a remote file.
    pub async fn remove_file(&self, path: &str) -> Result<()> {
        self.session
            .remove_file(path)
            .await
            .map_err(|e| anyhow::anyhow!("failed to remove file '{}': {}", path, e))?;
        Ok(())
    }

    /// Remove a remote directory.
    pub async fn remove_dir(&self, path: &str) -> Result<()> {
        self.session
            .remove_dir(path)
            .await
            .map_err(|e| anyhow::anyhow!("failed to remove directory '{}': {}", path, e))?;
        Ok(())
    }

    /// Rename a remote file or directory.
    pub async fn rename(&self, old_path: &str, new_path: &str) -> Result<()> {
        self.session
            .rename(old_path, new_path)
            .await
            .map_err(|e| {
                anyhow::anyhow!("failed to rename '{}' to '{}': {}", old_path, new_path, e)
            })?;
        Ok(())
    }

    /// Change permissions on a remote file.
    pub async fn chmod(&self, path: &str, mode: u32) -> Result<()> {
        let mut metadata = self
            .session
            .metadata(path)
            .await
            .map_err(|e| anyhow::anyhow!("failed to get metadata for '{}': {}", path, e))?;
        metadata.permissions = Some(mode);
        self.session
            .set_metadata(path, metadata)
            .await
            .map_err(|e| anyhow::anyhow!("failed to set permissions on '{}': {}", path, e))?;
        Ok(())
    }

    /// Get metadata for a remote path.
    pub async fn stat(&self, path: &str) -> Result<RemoteEntry> {
        let metadata = self
            .session
            .metadata(path)
            .await
            .map_err(|e| anyhow::anyhow!("failed to stat '{}': {}", path, e))?;
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        Ok(RemoteEntry {
            name,
            path: path.to_string(),
            is_dir: metadata.is_dir(),
            size: metadata.size.unwrap_or(0),
            permissions: metadata.permissions.unwrap_or(0),
            modified: metadata.mtime.map(|t| t as u64),
        })
    }

    /// Resolve a path to its canonical absolute form (e.g., resolve "~" to home directory).
    pub async fn realpath(&self, path: &str) -> Result<String> {
        self.session
            .canonicalize(path)
            .await
            .map_err(|e| anyhow::anyhow!("failed to resolve path '{}': {}", path, e))
    }

    /// Recursively upload a local path (file or directory) to a remote directory.
    pub async fn upload(&self, local_path: &Path, remote_dir: &str) -> Result<()> {
        let file_name = local_path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("invalid file name"))?
            .to_string_lossy();
        let remote_path = format!("{}/{}", remote_dir.trim_end_matches('/'), file_name);

        if local_path.is_dir() {
            let _ = self.mkdir(&remote_path).await;
            let entries = std::fs::read_dir(local_path)
                .with_context(|| format!("failed to read local directory '{}'", local_path.display()))?;
            for entry in entries {
                let entry = entry?;
                Box::pin(self.upload(&entry.path(), &remote_path)).await?;
            }
        } else {
            self.write_file(local_path, &remote_path).await?;
        }
        Ok(())
    }

    /// Create an empty file at the given remote path.
    pub async fn create_empty_file(&self, path: &str) -> Result<()> {
        self.session
            .write(path, &[])
            .await
            .map_err(|e| anyhow::anyhow!("failed to create file '{}': {}", path, e))?;
        Ok(())
    }

    /// Recursively remove a directory and all its contents.
    pub async fn recursive_remove(&self, path: &str) -> Result<()> {
        let entries = self.list_dir(path).await?;
        for entry in entries {
            if entry.is_dir {
                Box::pin(self.recursive_remove(&entry.path)).await?;
            } else {
                self.remove_file(&entry.path).await?;
            }
        }
        self.remove_dir(path).await?;
        Ok(())
    }

    /// Recursively download a remote path (file or directory) to a local path.
    pub async fn recursive_download(&self, remote_path: &str, local_path: &Path) -> Result<()> {
        let metadata = self
            .session
            .metadata(remote_path)
            .await
            .map_err(|e| anyhow::anyhow!("failed to stat '{}': {}", remote_path, e))?;

        if metadata.is_dir() {
            std::fs::create_dir_all(local_path)
                .with_context(|| format!("failed to create local directory '{}'", local_path.display()))?;
            let entries = self.list_dir(remote_path).await?;
            for entry in entries {
                let child_local = local_path.join(&entry.name);
                Box::pin(self.recursive_download(&entry.path, &child_local)).await?;
            }
        } else {
            if let Some(parent) = local_path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create parent directory '{}'", parent.display()))?;
            }
            self.read_file(remote_path, local_path).await?;
        }
        Ok(())
    }

    /// Close the SFTP session.
    pub async fn close(&self) -> Result<()> {
        self.session
            .close()
            .await
            .map_err(|e| anyhow::anyhow!("failed to close SFTP session: {}", e))?;
        Ok(())
    }
}

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use async_trait::async_trait;
use regex::Regex;
use russh::client::{AuthResult, Config, Handle};
use russh::ChannelMsg;
use tokio::sync::Mutex;
use tokio::time::timeout;

use super::{CodeSource, FileCache, FunctionLocation};
use crate::code_server_config::CodeServerConfig;

struct SshClientHandler;

impl russh::client::Handler for SshClientHandler {
    type Error = anyhow::Error;

    fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send {
        async { Ok(true) }
    }
}

pub struct SshDockerCodeSource {
    config: CodeServerConfig,
    cache: FileCache,
    connection: Mutex<Option<Handle<SshClientHandler>>>,
}

impl SshDockerCodeSource {
    pub fn new(config: CodeServerConfig) -> Self {
        Self {
            config,
            cache: FileCache::new(100),
            connection: Mutex::new(None),
        }
    }

    pub fn with_cache(mut self, cache: FileCache) -> Self {
        self.cache = cache;
        self
    }

    async fn ensure_connected(&self) -> Result<()> {
        let mut conn_guard = self.connection.lock().await;

        if let Some(ref handle) = *conn_guard {
            if !handle.is_closed() {
                return Ok(());
            }
        }

        let ssh_config = Arc::new(Config {
            ..Config::default()
        });

        let addr = format!("{}:{}", self.config.ssh_host, self.config.ssh_port);

        log::info!(
            "[SshDockerCodeSource] Connecting to SSH server at {}",
            addr
        );

        let mut handle = timeout(
            Duration::from_secs(10),
            russh::client::connect(ssh_config, &addr, SshClientHandler),
        )
        .await
        .map_err(|_| anyhow::anyhow!("SSH connection timed out after 10 seconds"))?
        .with_context(|| format!("Failed to connect to SSH server at {}", addr))?;

        let password = self.config.ssh_password.as_deref().unwrap_or("");

        let result = handle
            .authenticate_password(&self.config.ssh_user, password)
            .await
            .context("SSH password authentication failed")?;

        match result {
            AuthResult::Success => {}
            AuthResult::Failure { .. } => {
                bail!("SSH authentication failed: invalid credentials");
            }
        }

        log::info!(
            "[SshDockerCodeSource] SSH connection established to {}",
            addr
        );

        *conn_guard = Some(handle);
        Ok(())
    }

    async fn exec_command(&self, command: &str) -> Result<String> {
        self.ensure_connected().await?;

        let conn_guard = self.connection.lock().await;
        let handle = conn_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("SSH connection not available"))?;

        let mut channel = handle
            .channel_open_session()
            .await
            .context("Failed to open SSH channel")?;

        log::debug!("[SshDockerCodeSource] Executing command: {}", command);

        channel
            .exec(true, command)
            .await
            .context("Failed to execute command")?;

        drop(conn_guard);

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) => {
                    stdout.extend_from_slice(&data);
                }
                Some(ChannelMsg::ExtendedData { data, ext }) => {
                    if ext == 1 {
                        stderr.extend_from_slice(&data);
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    if exit_status != 0 {
                        log::debug!(
                            "[SshDockerCodeSource] Command exited with status {}: {}",
                            exit_status,
                            String::from_utf8_lossy(&stderr)
                        );
                    }
                    break;
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                    break;
                }
                _ => {}
            }
        }

        Ok(String::from_utf8_lossy(&stdout).to_string())
    }

    fn build_command(&self, inner_command: &str) -> String {
        if self.config.container_id.is_empty() {
            // Execute directly on SSH server
            inner_command.to_string()
        } else {
            // Execute inside Docker container
            format!(
                "docker exec {} sh -c '{}'",
                &self.config.container_id,
                inner_command.replace('\'', "'\"'\"'")
            )
        }
    }

    fn parse_grep_output(&self, output: &str) -> Vec<(String, u32, String)> {
        let mut results = Vec::new();
        for line in output.lines() {
            let parts: Vec<&str> = line.splitn(3, ':').collect();
            if parts.len() >= 3 {
                let file = parts[0].to_string();
                if let Ok(line_num) = parts[1].parse::<u32>() {
                    let content = parts[2].to_string();
                    results.push((file, line_num, content));
                }
            }
        }
        results
    }
}

#[async_trait]
impl CodeSource for SshDockerCodeSource {
    fn root_path(&self) -> &str {
        &self.config.code_root
    }

    async fn find_function(&self, name: &str) -> Result<Vec<FunctionLocation>> {
        if let Some((_, loc)) = self.cache.find_function_in_cache(name) {
            return Ok(vec![loc]);
        }

        let pattern = format!(
            r"(function|fn|def|void|int|char|long|static|local\s+function)\s+{}\s*\(",
            regex::escape(name)
        );

        let inner_cmd = format!(
            "grep -rn -E '{}' {} 2>/dev/null || true",
            pattern, self.config.code_root
        );
        let cmd = self.build_command(&inner_cmd);

        let output = self.exec_command(&cmd).await?;
        let matches = self.parse_grep_output(&output);

        let mut results = Vec::new();
        for (file, line, content) in matches {
            let mut loc = FunctionLocation::new(&file, line);
            loc.signature = Some(content.trim().to_string());
            results.push(loc);
        }

        log::info!(
            "[SshDockerCodeSource] Found {} locations for function '{}'",
            results.len(),
            name
        );

        Ok(results)
    }

    async fn batch_find_functions(
        &self,
        names: &[String],
    ) -> Result<HashMap<String, Vec<FunctionLocation>>> {
        if names.is_empty() {
            return Ok(HashMap::new());
        }

        if self.config.container_id.is_empty() {
            log::info!(
                "[SshDockerCodeSource] Batch searching {} functions via SSH to {}",
                names.len(),
                self.config.ssh_host
            );
        } else {
            log::info!(
                "[SshDockerCodeSource] Batch searching {} functions via SSH to container {}",
                names.len(),
                self.config.container_id
            );
        }

        let patterns: Vec<String> = names
            .iter()
            .map(|n| {
                format!(
                    r"(function|fn|def|void|int|char|long|static|local\s+function)\s+{}\s*\(",
                    regex::escape(n)
                )
            })
            .collect();

        let combined_pattern = patterns.join("|");

        let inner_cmd = format!(
            "grep -rn -E '{}' {} 2>/dev/null || true",
            combined_pattern, self.config.code_root
        );
        let cmd = self.build_command(&inner_cmd);

        let output = self.exec_command(&cmd).await?;
        let matches = self.parse_grep_output(&output);

        let mut results: HashMap<String, Vec<FunctionLocation>> = HashMap::new();
        for name in names {
            results.insert(name.clone(), Vec::new());
        }

        let func_pattern = Regex::new(
            r"(?:function|fn|def|void|int|char|long|static|local\s+function)\s+(\w+)\s*\(",
        )?;

        for (file, line, content) in matches {
            if let Some(caps) = func_pattern.captures(&content) {
                if let Some(func_name) = caps.get(1) {
                    let name = func_name.as_str().to_string();
                    if let Some(locs) = results.get_mut(&name) {
                        let mut loc = FunctionLocation::new(&file, line);
                        loc.signature = Some(content.trim().to_string());
                        locs.push(loc);
                    }
                }
            }
        }

        log::info!(
            "[SshDockerCodeSource] Batch search complete, found matches for {} functions",
            results.values().filter(|v| !v.is_empty()).count()
        );

        Ok(results)
    }

    async fn read_file(&self, path: &str) -> Result<String> {
        if let Some(content) = self.cache.get_content(path) {
            return Ok(content);
        }

        let full_path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("{}/{}", self.config.code_root, path)
        };

        let inner_cmd = format!("cat '{}'", full_path.replace('\'', "'\"'\"'"));
        let cmd = self.build_command(&inner_cmd);

        let content = self.exec_command(&cmd).await?;

        if content.starts_with("cat:") && content.contains("No such file") {
            bail!("File not found: {}", full_path);
        }

        self.cache.put_content(path, content.clone());

        Ok(content)
    }

    async fn get_callees(&self, file: &str, function: &str) -> Result<Vec<String>> {
        if let Some(callees) = self.cache.get_callees(file, function) {
            return Ok(callees);
        }

        let content = self.read_file(file).await?;
        let callees = extract_callees(&content, function);

        log::info!(
            "[SshDockerCodeSource] Extracted {} callees from {}::{}",
            callees.len(),
            file,
            function
        );

        Ok(callees)
    }
}

fn extract_callees(content: &str, function: &str) -> Vec<String> {
    let mut callees = Vec::new();
    let mut in_function = false;
    let mut brace_depth = 0;

    let func_def_pattern = Regex::new(&format!(
        r"(?:function|fn|def|void|int|char|long|static)\s+{}\s*\(",
        regex::escape(function)
    ))
    .ok();
    let call_pattern = Regex::new(r"(\w+)\s*\(").ok();

    for line in content.lines() {
        if let Some(ref pattern) = func_def_pattern {
            if pattern.is_match(line) {
                in_function = true;
            }
        }

        if in_function {
            brace_depth += line.matches('{').count();
            brace_depth = brace_depth.saturating_sub(line.matches('}').count());

            if let Some(ref pattern) = call_pattern {
                for cap in pattern.captures_iter(line) {
                    if let Some(name) = cap.get(1) {
                        let callee = name.as_str().to_string();
                        if !is_keyword(&callee) && callee != function && !callees.contains(&callee)
                        {
                            callees.push(callee);
                        }
                    }
                }
            }

            if brace_depth == 0 && line.contains('}') {
                break;
            }
        }
    }

    callees
}

fn is_keyword(word: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "if", "else", "for", "while", "do", "switch", "case", "return", "sizeof", "typeof",
        "function", "local", "end", "then", "elseif", "printf", "sprintf", "fprintf", "memcpy",
        "memset", "malloc", "free", "strlen", "strcpy", "strcat",
    ];
    KEYWORDS.contains(&word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_callees() {
        let content = r#"
void my_function(int arg) {
    foo();
    bar(1, 2);
    if (condition) {
        baz();
    }
}
"#;
        let callees = extract_callees(content, "my_function");
        assert!(callees.contains(&"foo".to_string()));
        assert!(callees.contains(&"bar".to_string()));
        assert!(callees.contains(&"baz".to_string()));
        assert!(!callees.contains(&"if".to_string()));
    }

    #[test]
    fn test_parse_grep_output() {
        let config = CodeServerConfig::new();
        let source = SshDockerCodeSource::new(config);

        let output = "/usr1/foo.c:10:void my_func() {\n/usr1/bar.c:20:int other_func(int x) {";
        let results = source.parse_grep_output(output);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "/usr1/foo.c");
        assert_eq!(results[0].1, 10);
        assert_eq!(results[0].2, "void my_func() {");
    }
}

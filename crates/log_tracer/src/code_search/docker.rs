use std::collections::HashMap;

use anyhow::{Context as _, Result, bail};
use async_trait::async_trait;
use regex::Regex;

use super::{CodeSource, FileCache, FunctionLocation};

pub struct DockerCodeSource {
    container_id: String,
    code_root: String,
    cache: FileCache,
    docker_host: Option<String>,
}

impl DockerCodeSource {
    pub fn new(container_id: impl Into<String>, code_root: impl Into<String>) -> Self {
        Self {
            container_id: container_id.into(),
            code_root: code_root.into(),
            cache: FileCache::new(100),
            docker_host: None,
        }
    }

    pub fn with_docker_host(mut self, host: impl Into<String>) -> Self {
        self.docker_host = Some(host.into());
        self
    }

    pub fn with_cache(mut self, cache: FileCache) -> Self {
        self.cache = cache;
        self
    }

    async fn exec(&self, command: &str, args: &[&str]) -> Result<String> {
        let mut cmd = tokio::process::Command::new("docker");

        if let Some(ref host) = self.docker_host {
            cmd.arg("-H").arg(host);
        }

        cmd.arg("exec").arg(&self.container_id).arg(command);
        cmd.args(args);

        log::info!(
            "[DockerCodeSource] Executing: docker exec {} {} {}",
            self.container_id,
            command,
            args.join(" ")
        );

        let output = cmd
            .output()
            .await
            .context("Failed to execute docker command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.trim().is_empty() {
                log::warn!("[DockerCodeSource] Command stderr: {}", stderr);
            }
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
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
impl CodeSource for DockerCodeSource {
    fn root_path(&self) -> &str {
        &self.code_root
    }

    async fn find_function(&self, name: &str) -> Result<Vec<FunctionLocation>> {
        if let Some((_, loc)) = self.cache.find_function_in_cache(name) {
            return Ok(vec![loc]);
        }

        let pattern = format!(
            r"(function|fn|def|void|int|char|long|static|local\s+function)\s+{}\s*\(",
            regex::escape(name)
        );

        let output = self
            .exec("grep", &["-rn", "-E", &pattern, &self.code_root])
            .await?;

        let matches = self.parse_grep_output(&output);

        let mut results = Vec::new();
        for (file, line, content) in matches {
            let mut loc = FunctionLocation::new(&file, line);
            loc.signature = Some(content.trim().to_string());
            results.push(loc);
        }

        log::info!(
            "[DockerCodeSource] Found {} locations for function '{}'",
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

        log::info!(
            "[DockerCodeSource] Batch searching {} functions in container {}",
            names.len(),
            self.container_id
        );

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

        let output = self
            .exec("grep", &["-rn", "-E", &combined_pattern, &self.code_root])
            .await?;

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
            "[DockerCodeSource] Batch search complete, found matches for {} functions",
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
            format!("{}/{}", self.code_root, path)
        };

        let content = self.exec("cat", &[&full_path]).await?;

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
            "[DockerCodeSource] Extracted {} callees from {}::{}",
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

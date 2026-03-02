use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use regex::Regex;

use super::{CodeSource, FileCache, FunctionLocation};

pub struct LocalCodeSource {
    root_path: String,
    cache: FileCache,
}

impl LocalCodeSource {
    pub fn new(root_path: impl Into<String>) -> Self {
        Self {
            root_path: root_path.into(),
            cache: FileCache::new(100),
        }
    }

    pub fn with_cache(mut self, cache: FileCache) -> Self {
        self.cache = cache;
        self
    }

    fn parse_grep_output(&self, output: &str) -> Vec<(String, u32)> {
        let mut results = Vec::new();
        for line in output.lines() {
            if let Some((file, line_num)) = parse_grep_line(line) {
                results.push((file, line_num));
            }
        }
        results
    }
}

fn parse_grep_line(line: &str) -> Option<(String, u32)> {
    let parts: Vec<&str> = line.splitn(3, ':').collect();
    if parts.len() >= 2 {
        let file = parts[0].to_string();
        if let Ok(line_num) = parts[1].parse::<u32>() {
            return Some((file, line_num));
        }
    }
    None
}

#[async_trait]
impl CodeSource for LocalCodeSource {
    fn root_path(&self) -> &str {
        &self.root_path
    }

    async fn find_function(&self, name: &str) -> Result<Vec<FunctionLocation>> {
        if let Some((_, loc)) = self.cache.find_function_in_cache(name) {
            return Ok(vec![loc]);
        }

        let pattern = format!(
            r"(function|fn|def|void|int|char|local\s+function)\s+{}\s*\(",
            regex::escape(name)
        );

        let output = tokio::process::Command::new("grep")
            .args(["-rn", "-E", &pattern, &self.root_path])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .context("Failed to run grep")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let matches = self.parse_grep_output(&stdout);

        let mut results = Vec::new();
        for (file, line) in matches {
            results.push(FunctionLocation::new(file, line));
        }

        log::info!(
            "[LocalCodeSource] Found {} locations for function '{}'",
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
            "[LocalCodeSource] Batch searching {} functions",
            names.len()
        );

        let patterns: Vec<String> = names
            .iter()
            .map(|n| {
                format!(
                    r"(function|fn|def|void|int|char|local\s+function)\s+{}\s*\(",
                    regex::escape(n)
                )
            })
            .collect();

        let combined_pattern = patterns.join("|");

        let output = tokio::process::Command::new("grep")
            .args(["-rn", "-E", &combined_pattern, &self.root_path])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .context("Failed to run grep")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let matches = self.parse_grep_output(&stdout);

        let mut results: HashMap<String, Vec<FunctionLocation>> = HashMap::new();
        for name in names {
            results.insert(name.clone(), Vec::new());
        }

        let func_pattern = Regex::new(r"(?:function|fn|def|void|int|char|local\s+function)\s+(\w+)\s*\(")?;

        for (file, line) in matches {
            let file_content = self.read_file(&file).await.ok();
            if let Some(content) = file_content {
                let lines: Vec<&str> = content.lines().collect();
                if let Some(line_content) = lines.get((line - 1) as usize) {
                    if let Some(caps) = func_pattern.captures(line_content) {
                        if let Some(func_name) = caps.get(1) {
                            let name = func_name.as_str().to_string();
                            if let Some(locs) = results.get_mut(&name) {
                                locs.push(FunctionLocation::new(&file, line));
                            }
                        }
                    }
                }
            }
        }

        log::info!(
            "[LocalCodeSource] Batch search complete, found matches for {} functions",
            results.values().filter(|v| !v.is_empty()).count()
        );

        Ok(results)
    }

    async fn read_file(&self, path: &str) -> Result<String> {
        if let Some(content) = self.cache.get_content(path) {
            return Ok(content);
        }

        let full_path = if Path::new(path).is_absolute() {
            path.to_string()
        } else {
            format!("{}/{}", self.root_path, path)
        };

        let content = tokio::fs::read_to_string(&full_path)
            .await
            .context(format!("Failed to read file: {}", full_path))?;

        self.cache.put_content(path, content.clone());

        Ok(content)
    }

    async fn get_callees(&self, file: &str, function: &str) -> Result<Vec<String>> {
        if let Some(callees) = self.cache.get_callees(file, function) {
            return Ok(callees);
        }

        let content = self.read_file(file).await?;
        let callees = extract_callees_from_content(&content, function);

        log::info!(
            "[LocalCodeSource] Extracted {} callees from {}::{}",
            callees.len(),
            file,
            function
        );

        Ok(callees)
    }
}

fn extract_callees_from_content(content: &str, function: &str) -> Vec<String> {
    let mut callees = Vec::new();
    let mut in_function = false;
    let mut brace_depth = 0;

    let func_def_pattern =
        Regex::new(&format!(r"(?:function|fn|def|void|int|char)\s+{}\s*\(", regex::escape(function)))
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
                        if !is_keyword(&callee) && callee != function && !callees.contains(&callee) {
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
        "function", "local", "end", "then", "elseif",
    ];
    KEYWORDS.contains(&word)
}

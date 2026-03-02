mod docker;
mod file_cache;
mod local;
mod ssh_docker;

pub use docker::DockerCodeSource;
pub use file_cache::FileCache;
pub use local::LocalCodeSource;
pub use ssh_docker::SshDockerCodeSource;

use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeLocation {
    pub file: String,
    pub start_line: u32,
    pub end_line: u32,
    pub function_signature: Option<String>,
}

impl CodeLocation {
    pub fn new(file: impl Into<String>, start_line: u32, end_line: u32) -> Self {
        Self {
            file: file.into(),
            start_line,
            end_line,
            function_signature: None,
        }
    }

    pub fn with_signature(mut self, signature: impl Into<String>) -> Self {
        self.function_signature = Some(signature.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct FunctionLocation {
    pub file: String,
    pub line: u32,
    pub signature: Option<String>,
    pub end_line: Option<u32>,
}

impl FunctionLocation {
    pub fn new(file: impl Into<String>, line: u32) -> Self {
        Self {
            file: file.into(),
            line,
            signature: None,
            end_line: None,
        }
    }

    pub fn with_signature(mut self, signature: impl Into<String>) -> Self {
        self.signature = Some(signature.into());
        self
    }

    pub fn with_end_line(mut self, end_line: u32) -> Self {
        self.end_line = Some(end_line);
        self
    }

    pub fn to_code_location(&self) -> CodeLocation {
        CodeLocation {
            file: self.file.clone(),
            start_line: self.line,
            end_line: self.end_line.unwrap_or(self.line),
            function_signature: self.signature.clone(),
        }
    }
}

#[async_trait]
pub trait CodeSource: Send + Sync {
    fn root_path(&self) -> &str;

    async fn find_function(&self, name: &str) -> Result<Vec<FunctionLocation>>;

    async fn batch_find_functions(
        &self,
        names: &[String],
    ) -> Result<HashMap<String, Vec<FunctionLocation>>>;

    async fn read_file(&self, path: &str) -> Result<String>;

    async fn get_callees(&self, file: &str, function: &str) -> Result<Vec<String>>;
}

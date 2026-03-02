mod c_analyzer;
mod lua_analyzer;

pub use c_analyzer::CAnalyzer;
pub use lua_analyzer::LuaAnalyzer;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub start_line: u32,
    pub end_line: u32,
    pub signature: Option<String>,
    pub callees: Vec<String>,
}

impl FunctionDef {
    pub fn new(name: impl Into<String>, start_line: u32, end_line: u32) -> Self {
        Self {
            name: name.into(),
            start_line,
            end_line,
            signature: None,
            callees: Vec::new(),
        }
    }

    pub fn with_signature(mut self, signature: impl Into<String>) -> Self {
        self.signature = Some(signature.into());
        self
    }

    pub fn add_callee(&mut self, callee: impl Into<String>) {
        self.callees.push(callee.into());
    }
}

pub trait LanguageAnalyzer: Send + Sync {
    fn language_id(&self) -> &str;

    fn file_extensions(&self) -> &[&str];

    fn function_definition_pattern(&self, name: &str) -> String;

    fn extract_functions(&self, content: &str) -> Result<Vec<FunctionDef>>;

    fn extract_callees(&self, content: &str, function: &FunctionDef) -> Result<Vec<String>>;
}

pub struct LanguageRegistry {
    analyzers: HashMap<String, Arc<dyn LanguageAnalyzer>>,
    extension_map: HashMap<String, String>,
}

impl LanguageRegistry {
    pub fn new() -> Self {
        Self {
            analyzers: HashMap::new(),
            extension_map: HashMap::new(),
        }
    }

    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(CAnalyzer::new()));
        registry.register(Arc::new(LuaAnalyzer::new()));
        registry
    }

    pub fn register(&mut self, analyzer: Arc<dyn LanguageAnalyzer>) {
        let lang_id = analyzer.language_id().to_string();

        for ext in analyzer.file_extensions() {
            self.extension_map
                .insert(ext.to_string(), lang_id.clone());
        }

        self.analyzers.insert(lang_id, analyzer);
    }

    pub fn get(&self, language_id: &str) -> Option<Arc<dyn LanguageAnalyzer>> {
        self.analyzers.get(language_id).cloned()
    }

    pub fn for_file(&self, path: &str) -> Option<Arc<dyn LanguageAnalyzer>> {
        let ext = path.rsplit('.').next()?;
        let lang_id = self.extension_map.get(ext)?;
        self.analyzers.get(lang_id).cloned()
    }

    pub fn for_extension(&self, ext: &str) -> Option<Arc<dyn LanguageAnalyzer>> {
        let lang_id = self.extension_map.get(ext)?;
        self.analyzers.get(lang_id).cloned()
    }

    pub fn available_languages(&self) -> Vec<&str> {
        self.analyzers.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_registry() {
        let registry = LanguageRegistry::with_defaults();

        assert!(registry.get("c").is_some());
        assert!(registry.get("lua").is_some());
        assert!(registry.get("python").is_none());

        assert!(registry.for_file("main.c").is_some());
        assert!(registry.for_file("script.lua").is_some());
        assert!(registry.for_file("code.h").is_some());
        assert!(registry.for_file("unknown.xyz").is_none());
    }

    #[test]
    fn test_function_def() {
        let mut func = FunctionDef::new("process_data", 10, 25);
        func.signature = Some("int process_data(void* data, int len)".to_string());
        func.add_callee("validate");
        func.add_callee("transform");

        assert_eq!(func.name, "process_data");
        assert_eq!(func.start_line, 10);
        assert_eq!(func.end_line, 25);
        assert_eq!(func.callees.len(), 2);
    }
}

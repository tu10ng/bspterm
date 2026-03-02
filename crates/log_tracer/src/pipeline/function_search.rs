use anyhow::Result;

use super::{AnalysisContext, AnalysisStep};
use crate::AnalysisProgress;

pub struct FunctionSearchStep {
    batch_size: usize,
}

impl FunctionSearchStep {
    pub fn new() -> Self {
        Self { batch_size: 50 }
    }

    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }
}

impl Default for FunctionSearchStep {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisStep for FunctionSearchStep {
    fn name(&self) -> &str {
        "function_search"
    }

    fn process(&self, ctx: &mut AnalysisContext) -> Result<()> {
        let function_names = ctx.unique_function_names();

        if function_names.is_empty() {
            log::info!("[FunctionSearchStep] No function names to search");
            return Ok(());
        }

        let code_source = match &ctx.code_source {
            Some(source) => source.clone(),
            None => {
                log::info!("[FunctionSearchStep] No code source configured, skipping search");
                return Ok(());
            }
        };

        log::info!(
            "[FunctionSearchStep] Searching for {} unique functions in {}",
            function_names.len(),
            code_source.root_path()
        );

        ctx.progress = AnalysisProgress::Searching {
            current: 0,
            total: function_names.len(),
        };

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        let locations = runtime.block_on(async {
            code_source.batch_find_functions(&function_names).await
        })?;

        let mut found_count = 0;
        for (name, locs) in &locations {
            if let Some(loc) = locs.first() {
                ctx.locations.insert(name.clone(), loc.to_code_location());
                found_count += 1;
            }
        }

        log::info!(
            "[FunctionSearchStep] Found locations for {} out of {} functions",
            found_count,
            function_names.len()
        );

        if found_count < function_names.len() {
            let not_found: Vec<_> = function_names
                .iter()
                .filter(|n| !ctx.locations.contains_key(*n))
                .take(10)
                .collect();

            if !not_found.is_empty() {
                log::warn!(
                    "[FunctionSearchStep] Functions not found (showing first 10): {:?}",
                    not_found
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_search::{CodeSource, FunctionLocation};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MockCodeSource {
        functions: HashMap<String, Vec<FunctionLocation>>,
    }

    #[async_trait]
    impl CodeSource for MockCodeSource {
        fn root_path(&self) -> &str {
            "/mock"
        }

        async fn find_function(&self, name: &str) -> Result<Vec<FunctionLocation>> {
            Ok(self.functions.get(name).cloned().unwrap_or_default())
        }

        async fn batch_find_functions(
            &self,
            names: &[String],
        ) -> Result<HashMap<String, Vec<FunctionLocation>>> {
            let mut result = HashMap::new();
            for name in names {
                result.insert(
                    name.clone(),
                    self.functions.get(name).cloned().unwrap_or_default(),
                );
            }
            Ok(result)
        }

        async fn read_file(&self, _path: &str) -> Result<String> {
            Ok(String::new())
        }

        async fn get_callees(&self, _file: &str, _function: &str) -> Result<Vec<String>> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn test_function_search_step() {
        let mut functions = HashMap::new();
        functions.insert(
            "process_data".to_string(),
            vec![FunctionLocation::new("/src/main.c", 10)],
        );

        let source = Arc::new(MockCodeSource { functions });

        let mut ctx = AnalysisContext::new("test".to_string());
        ctx.code_source = Some(source);

        let mut entry = crate::parser::LogEntry::new(1, "calling process_data()");
        entry.add_function_name("process_data");
        ctx.entries.push(entry);

        let step = FunctionSearchStep::new();
        step.process(&mut ctx).unwrap();

        assert!(ctx.locations.contains_key("process_data"));
    }
}

use std::collections::HashMap;
use std::num::NonZeroUsize;

use lru::LruCache;
use parking_lot::Mutex;

use super::FunctionLocation;

pub struct ParsedFile {
    pub path: String,
    pub content: String,
    pub functions: Vec<CachedFunction>,
    pub calls: HashMap<String, Vec<String>>,
}

pub struct CachedFunction {
    pub name: String,
    pub start_line: u32,
    pub end_line: u32,
    pub signature: Option<String>,
}

impl CachedFunction {
    pub fn to_location(&self, file: &str) -> FunctionLocation {
        FunctionLocation {
            file: file.to_string(),
            line: self.start_line,
            signature: self.signature.clone(),
            end_line: Some(self.end_line),
        }
    }
}

pub struct FileCache {
    content_cache: Mutex<LruCache<String, String>>,
    parsed_cache: Mutex<HashMap<String, ParsedFile>>,
    max_content_size: usize,
}

impl FileCache {
    pub fn new(max_files: usize) -> Self {
        Self {
            content_cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(max_files).unwrap_or(NonZeroUsize::new(100).unwrap()),
            )),
            parsed_cache: Mutex::new(HashMap::new()),
            max_content_size: 1024 * 1024,
        }
    }

    pub fn get_content(&self, path: &str) -> Option<String> {
        self.content_cache.lock().get(path).cloned()
    }

    pub fn put_content(&self, path: &str, content: String) {
        if content.len() <= self.max_content_size {
            self.content_cache
                .lock()
                .put(path.to_string(), content);
        }
    }

    pub fn get_parsed(&self, path: &str) -> Option<ParsedFile> {
        let parsed = self.parsed_cache.lock();
        parsed.get(path).map(|p| ParsedFile {
            path: p.path.clone(),
            content: p.content.clone(),
            functions: p
                .functions
                .iter()
                .map(|f| CachedFunction {
                    name: f.name.clone(),
                    start_line: f.start_line,
                    end_line: f.end_line,
                    signature: f.signature.clone(),
                })
                .collect(),
            calls: p.calls.clone(),
        })
    }

    pub fn put_parsed(&self, parsed: ParsedFile) {
        self.parsed_cache.lock().insert(parsed.path.clone(), parsed);
    }

    pub fn find_function_in_cache(&self, name: &str) -> Option<(String, FunctionLocation)> {
        let parsed = self.parsed_cache.lock();
        for (path, file) in parsed.iter() {
            for func in &file.functions {
                if func.name == name {
                    return Some((path.clone(), func.to_location(path)));
                }
            }
        }
        None
    }

    pub fn get_callees(&self, file: &str, function: &str) -> Option<Vec<String>> {
        let parsed = self.parsed_cache.lock();
        parsed
            .get(file)
            .and_then(|p| p.calls.get(function).cloned())
    }

    pub fn clear(&self) {
        self.content_cache.lock().clear();
        self.parsed_cache.lock().clear();
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            content_entries: self.content_cache.lock().len(),
            parsed_entries: self.parsed_cache.lock().len(),
        }
    }
}

pub struct CacheStats {
    pub content_entries: usize,
    pub parsed_entries: usize,
}

impl Default for FileCache {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_cache() {
        let cache = FileCache::new(10);

        cache.put_content("/test/file.c", "int main() {}".to_string());
        assert!(cache.get_content("/test/file.c").is_some());
        assert!(cache.get_content("/test/other.c").is_none());

        let parsed = ParsedFile {
            path: "/test/file.c".to_string(),
            content: "int main() {}".to_string(),
            functions: vec![CachedFunction {
                name: "main".to_string(),
                start_line: 1,
                end_line: 1,
                signature: Some("int main()".to_string()),
            }],
            calls: HashMap::new(),
        };
        cache.put_parsed(parsed);

        let found = cache.find_function_in_cache("main");
        assert!(found.is_some());
        let (path, loc) = found.unwrap();
        assert_eq!(path, "/test/file.c");
        assert_eq!(loc.line, 1);
    }
}

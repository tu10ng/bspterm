use anyhow::Result;
use regex::Regex;

use super::{FunctionDef, LanguageAnalyzer};

pub struct CAnalyzer {
    func_def_pattern: Regex,
    func_call_pattern: Regex,
}

impl CAnalyzer {
    pub fn new() -> Self {
        Self {
            func_def_pattern: Regex::new(
                r"(?m)^(?:static\s+)?(?:inline\s+)?(?:const\s+)?(?:\w+\s+\*?\s*)+(\w+)\s*\([^)]*\)\s*\{",
            )
            .expect("Invalid C function pattern"),
            func_call_pattern: Regex::new(r"(\w+)\s*\(")
                .expect("Invalid C call pattern"),
        }
    }

    fn find_matching_brace(&self, content: &str, start_pos: usize) -> Option<usize> {
        let bytes = content.as_bytes();
        let mut depth = 0;
        let mut in_string = false;
        let mut in_char = false;
        let mut in_comment = false;
        let mut prev_char = '\0';

        for (i, &b) in bytes.iter().enumerate().skip(start_pos) {
            let ch = b as char;

            if in_comment {
                if prev_char == '*' && ch == '/' {
                    in_comment = false;
                }
                prev_char = ch;
                continue;
            }

            if prev_char == '/' && ch == '*' {
                in_comment = true;
                prev_char = ch;
                continue;
            }

            if prev_char == '/' && ch == '/' {
                while let Some(&next) = bytes.get(i) {
                    if next == b'\n' {
                        break;
                    }
                }
                prev_char = ch;
                continue;
            }

            if !in_char && ch == '"' && prev_char != '\\' {
                in_string = !in_string;
            }

            if !in_string && ch == '\'' && prev_char != '\\' {
                in_char = !in_char;
            }

            if !in_string && !in_char {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i);
                        }
                    }
                    _ => {}
                }
            }

            prev_char = ch;
        }

        None
    }

    fn count_newlines_until(&self, content: &str, pos: usize) -> u32 {
        content[..pos].matches('\n').count() as u32 + 1
    }
}

impl Default for CAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageAnalyzer for CAnalyzer {
    fn language_id(&self) -> &str {
        "c"
    }

    fn file_extensions(&self) -> &[&str] {
        &["c", "h", "cpp", "hpp", "cc", "cxx"]
    }

    fn function_definition_pattern(&self, name: &str) -> String {
        format!(
            r"(?:static\s+)?(?:inline\s+)?(?:const\s+)?(?:\w+\s+\*?\s*)+{}\s*\([^)]*\)\s*\{{",
            regex::escape(name)
        )
    }

    fn extract_functions(&self, content: &str) -> Result<Vec<FunctionDef>> {
        let mut functions = Vec::new();

        for caps in self.func_def_pattern.captures_iter(content) {
            let full_match = caps.get(0).unwrap();
            let func_name = caps.get(1).map(|m| m.as_str().to_string());

            if let Some(name) = func_name {
                if is_c_keyword(&name) {
                    continue;
                }

                let start_pos = full_match.end() - 1;
                let start_line = self.count_newlines_until(content, full_match.start());

                let end_line = if let Some(end_pos) = self.find_matching_brace(content, start_pos) {
                    self.count_newlines_until(content, end_pos)
                } else {
                    start_line + 10
                };

                let signature = full_match.as_str().trim_end_matches('{').trim().to_string();

                let func = FunctionDef::new(&name, start_line, end_line)
                    .with_signature(signature);

                functions.push(func);
            }
        }

        log::info!(
            "[CAnalyzer] Extracted {} functions from content",
            functions.len()
        );

        Ok(functions)
    }

    fn extract_callees(&self, content: &str, function: &FunctionDef) -> Result<Vec<String>> {
        let mut callees = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        let start_idx = (function.start_line - 1) as usize;
        let end_idx = (function.end_line) as usize;

        let function_body: String = lines
            .get(start_idx..end_idx.min(lines.len()))
            .map(|slice| slice.join("\n"))
            .unwrap_or_default();

        for caps in self.func_call_pattern.captures_iter(&function_body) {
            if let Some(m) = caps.get(1) {
                let callee = m.as_str().to_string();
                if !is_c_keyword(&callee)
                    && !is_c_builtin(&callee)
                    && callee != function.name
                    && !callees.contains(&callee)
                {
                    callees.push(callee);
                }
            }
        }

        Ok(callees)
    }
}

fn is_c_keyword(word: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "if", "else", "for", "while", "do", "switch", "case", "default", "break", "continue",
        "return", "goto", "sizeof", "typeof", "alignof", "void", "int", "char", "short", "long",
        "float", "double", "signed", "unsigned", "const", "volatile", "static", "extern", "auto",
        "register", "inline", "restrict", "typedef", "struct", "union", "enum", "true", "false",
        "NULL",
    ];
    KEYWORDS.contains(&word)
}

fn is_c_builtin(word: &str) -> bool {
    const BUILTINS: &[&str] = &[
        "printf", "sprintf", "snprintf", "fprintf", "scanf", "sscanf", "fscanf", "puts", "gets",
        "putchar", "getchar", "malloc", "calloc", "realloc", "free", "memcpy", "memmove", "memset",
        "memcmp", "strlen", "strcpy", "strncpy", "strcat", "strncat", "strcmp", "strncmp", "strchr",
        "strrchr", "strstr", "strtok", "atoi", "atol", "atof", "strtol", "strtoul", "strtod",
        "abs", "labs", "fabs", "sqrt", "pow", "sin", "cos", "tan", "log", "exp", "ceil", "floor",
        "fopen", "fclose", "fread", "fwrite", "fseek", "ftell", "fgets", "fputs", "feof", "ferror",
        "exit", "abort", "assert", "perror", "errno",
    ];
    BUILTINS.contains(&word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_c_functions() {
        let analyzer = CAnalyzer::new();
        let content = r#"
#include <stdio.h>

int process_data(void* data, int len) {
    validate(data);
    int result = transform(data, len);
    return result;
}

static void helper_function(int x) {
    printf("x = %d\n", x);
}
"#;

        let functions = analyzer.extract_functions(content).unwrap();
        assert_eq!(functions.len(), 2);

        let names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"process_data"));
        assert!(names.contains(&"helper_function"));
    }

    #[test]
    fn test_extract_callees() {
        let analyzer = CAnalyzer::new();
        let content = r#"
int process_data(void* data, int len) {
    validate(data);
    int result = transform(data, len);
    if (result < 0) {
        handle_error();
    }
    return result;
}
"#;

        let func = FunctionDef::new("process_data", 2, 9);
        let callees = analyzer.extract_callees(content, &func).unwrap();

        assert!(callees.contains(&"validate".to_string()));
        assert!(callees.contains(&"transform".to_string()));
        assert!(callees.contains(&"handle_error".to_string()));
        assert!(!callees.contains(&"printf".to_string()));
    }
}

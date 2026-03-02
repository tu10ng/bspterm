use anyhow::Result;
use regex::Regex;

use super::{FunctionDef, LanguageAnalyzer};

pub struct LuaAnalyzer {
    func_def_pattern: Regex,
    local_func_pattern: Regex,
    func_call_pattern: Regex,
}

impl LuaAnalyzer {
    pub fn new() -> Self {
        Self {
            func_def_pattern: Regex::new(r"(?m)^function\s+(\w+(?:\.\w+)*)\s*\([^)]*\)")
                .expect("Invalid Lua function pattern"),
            local_func_pattern: Regex::new(r"(?m)^local\s+function\s+(\w+)\s*\([^)]*\)")
                .expect("Invalid Lua local function pattern"),
            func_call_pattern: Regex::new(r"(\w+(?:\.\w+)*)\s*\(")
                .expect("Invalid Lua call pattern"),
        }
    }

    fn find_function_end(&self, content: &str, start_line: u32) -> u32 {
        let lines: Vec<&str> = content.lines().collect();
        let mut depth: usize = 0;
        let start_idx = (start_line - 1) as usize;

        for (i, line) in lines.iter().enumerate().skip(start_idx) {
            let trimmed = line.trim();

            if trimmed.starts_with("--") {
                continue;
            }

            // Count block starters - each opens a block ending with 'end' or 'until'
            // In Lua: function, if, for, while, repeat each start ONE block
            // 'then' and 'do' are part of if/for/while, not separate blocks
            for kw in ["function", "if", "for", "while", "repeat"] {
                if trimmed.starts_with(kw)
                    && trimmed
                        .chars()
                        .nth(kw.len())
                        .map(|c| !c.is_alphanumeric() && c != '_')
                        .unwrap_or(true)
                {
                    depth += 1;
                }
            }

            // Count standalone 'do' blocks (not part of for/while)
            if trimmed == "do" {
                depth += 1;
            }

            // Count block ends
            let end_count = trimmed
                .split_whitespace()
                .filter(|&w| w == "end" || w == "until")
                .count();

            depth = depth.saturating_sub(end_count);

            if depth == 0 && i > start_idx {
                return (i + 1) as u32;
            }
        }

        lines.len() as u32
    }

    fn count_newlines_until(&self, content: &str, pos: usize) -> u32 {
        content[..pos].matches('\n').count() as u32 + 1
    }
}

impl Default for LuaAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageAnalyzer for LuaAnalyzer {
    fn language_id(&self) -> &str {
        "lua"
    }

    fn file_extensions(&self) -> &[&str] {
        &["lua"]
    }

    fn function_definition_pattern(&self, name: &str) -> String {
        format!(
            r"(?:local\s+)?function\s+{}\s*\(",
            regex::escape(name)
        )
    }

    fn extract_functions(&self, content: &str) -> Result<Vec<FunctionDef>> {
        let mut functions = Vec::new();

        for caps in self.func_def_pattern.captures_iter(content) {
            let full_match = caps.get(0).unwrap();
            if let Some(name_match) = caps.get(1) {
                let name = name_match.as_str().to_string();
                let start_line = self.count_newlines_until(content, full_match.start());
                let end_line = self.find_function_end(content, start_line);
                let signature = full_match.as_str().to_string();

                let func = FunctionDef::new(&name, start_line, end_line)
                    .with_signature(signature);
                functions.push(func);
            }
        }

        for caps in self.local_func_pattern.captures_iter(content) {
            let full_match = caps.get(0).unwrap();
            if let Some(name_match) = caps.get(1) {
                let name = name_match.as_str().to_string();
                let start_line = self.count_newlines_until(content, full_match.start());
                let end_line = self.find_function_end(content, start_line);
                let signature = full_match.as_str().to_string();

                let func = FunctionDef::new(&name, start_line, end_line)
                    .with_signature(signature);
                functions.push(func);
            }
        }

        log::info!(
            "[LuaAnalyzer] Extracted {} functions from content",
            functions.len()
        );

        Ok(functions)
    }

    fn extract_callees(&self, content: &str, function: &FunctionDef) -> Result<Vec<String>> {
        let mut callees = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        let start_idx = (function.start_line - 1) as usize;
        let end_idx = function.end_line as usize;

        let function_body: String = lines
            .get(start_idx..end_idx.min(lines.len()))
            .map(|slice| slice.join("\n"))
            .unwrap_or_default();

        for caps in self.func_call_pattern.captures_iter(&function_body) {
            if let Some(m) = caps.get(1) {
                let callee = m.as_str().to_string();
                let base_name = callee.split('.').next().unwrap_or(&callee);

                if !is_lua_keyword(base_name)
                    && !is_lua_builtin(&callee)
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

fn is_lua_keyword(word: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if",
        "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
    ];
    KEYWORDS.contains(&word)
}

fn is_lua_builtin(name: &str) -> bool {
    const BUILTINS: &[&str] = &[
        "print",
        "pairs",
        "ipairs",
        "next",
        "type",
        "tonumber",
        "tostring",
        "assert",
        "error",
        "pcall",
        "xpcall",
        "select",
        "setmetatable",
        "getmetatable",
        "rawget",
        "rawset",
        "rawequal",
        "rawlen",
        "require",
        "load",
        "loadfile",
        "dofile",
        "collectgarbage",
        "string.format",
        "string.sub",
        "string.len",
        "string.find",
        "string.match",
        "string.gmatch",
        "string.gsub",
        "string.lower",
        "string.upper",
        "string.rep",
        "string.reverse",
        "string.byte",
        "string.char",
        "table.insert",
        "table.remove",
        "table.sort",
        "table.concat",
        "table.unpack",
        "table.pack",
        "math.abs",
        "math.ceil",
        "math.floor",
        "math.max",
        "math.min",
        "math.sqrt",
        "math.pow",
        "math.sin",
        "math.cos",
        "math.tan",
        "math.random",
        "math.randomseed",
        "os.time",
        "os.date",
        "os.clock",
        "os.execute",
        "os.exit",
        "os.getenv",
        "os.remove",
        "os.rename",
        "io.open",
        "io.close",
        "io.read",
        "io.write",
        "io.lines",
        "io.flush",
    ];
    BUILTINS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_lua_functions() {
        let analyzer = LuaAnalyzer::new();
        let content = r#"
function process_data(data, len)
    validate(data)
    local result = transform(data, len)
    return result
end

local function helper(x)
    print("x = " .. x)
end

function module.init()
    helper(42)
end
"#;

        let functions = analyzer.extract_functions(content).unwrap();
        assert_eq!(functions.len(), 3);

        let names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"process_data"));
        assert!(names.contains(&"helper"));
        assert!(names.contains(&"module.init"));
    }

    #[test]
    fn test_extract_lua_callees() {
        let analyzer = LuaAnalyzer::new();
        let content = r#"
function process_data(data, len)
    validate(data)
    local result = transform(data, len)
    if result < 0 then
        handle_error()
    end
    return result
end
"#;

        let func = FunctionDef::new("process_data", 2, 9);
        let callees = analyzer.extract_callees(content, &func).unwrap();

        assert!(callees.contains(&"validate".to_string()));
        assert!(callees.contains(&"transform".to_string()));
        assert!(callees.contains(&"handle_error".to_string()));
        assert!(!callees.contains(&"print".to_string()));
    }

    #[test]
    fn test_find_function_end() {
        let analyzer = LuaAnalyzer::new();
        let content = r#"function test()
    if true then
        print("nested")
    end
end

function other()
end
"#;

        let end_line = analyzer.find_function_end(content, 1);
        assert_eq!(end_line, 5);
    }
}

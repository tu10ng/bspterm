use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use regex::Regex;

pub struct KeywordMatcher {
    automaton: AhoCorasick,
    keywords: Vec<String>,
    detailed_patterns: Vec<(Regex, String)>,
}

impl KeywordMatcher {
    pub fn new(keywords: Vec<String>) -> Self {
        let automaton = AhoCorasickBuilder::new()
            .match_kind(MatchKind::LeftmostFirst)
            .build(&keywords)
            .expect("Failed to build Aho-Corasick automaton");

        Self {
            automaton,
            keywords,
            detailed_patterns: Vec::new(),
        }
    }

    pub fn with_patterns(mut self, patterns: Vec<(String, String)>) -> Self {
        for (pattern, group_name) in patterns {
            if let Ok(re) = Regex::new(&pattern) {
                self.detailed_patterns.push((re, group_name));
            }
        }
        self
    }

    pub fn has_match(&self, text: &str) -> bool {
        self.automaton.is_match(text)
    }

    pub fn find_keywords(&self, text: &str) -> Vec<String> {
        let mut found = Vec::new();

        for mat in self.automaton.find_iter(text) {
            let keyword = &self.keywords[mat.pattern().as_usize()];
            if !found.contains(keyword) {
                found.push(keyword.clone());
            }
        }

        found
    }

    pub fn extract_with_patterns(&self, text: &str) -> Vec<(String, String)> {
        let mut results = Vec::new();

        for (regex, group_name) in &self.detailed_patterns {
            for captures in regex.captures_iter(text) {
                if let Some(mat) = captures.name(group_name) {
                    results.push((group_name.clone(), mat.as_str().to_string()));
                } else if let Some(mat) = captures.get(1) {
                    results.push((group_name.clone(), mat.as_str().to_string()));
                }
            }
        }

        results
    }

    pub fn extract_function_names(&self, text: &str) -> Vec<String> {
        let mut functions = Vec::new();

        let function_patterns = [
            r"(?:calling|enter|invoke|>>>\s*)\s*(?P<func>\w+)\s*\(",
            r"(?P<func>\w+)\s*\(\s*\)",
            r"(?:exit|return|<<<\s*)\s*(?P<func>\w+)",
            r"\[(?:TRACE|DEBUG|INFO)\]\s+(?P<func>\w+):",
        ];

        for pattern in &function_patterns {
            if let Ok(re) = Regex::new(pattern) {
                for captures in re.captures_iter(text) {
                    if let Some(mat) = captures.name("func") {
                        let func_name = mat.as_str().to_string();
                        if !functions.contains(&func_name)
                            && !is_common_keyword(&func_name)
                            && func_name.len() > 1
                        {
                            functions.push(func_name);
                        }
                    }
                }
            }
        }

        functions
    }
}

fn is_common_keyword(word: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "if", "else", "for", "while", "do", "switch", "case", "break", "continue", "return",
        "void", "int", "char", "float", "double", "long", "short", "unsigned", "signed", "const",
        "static", "extern", "auto", "register", "volatile", "sizeof", "typedef", "struct", "union",
        "enum", "true", "false", "null", "nil", "NULL", "function", "local", "end", "then",
        "elseif", "repeat", "until", "and", "or", "not", "in",
    ];
    KEYWORDS.contains(&word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_matcher() {
        let matcher = KeywordMatcher::new(vec![
            "error".to_string(),
            "warning".to_string(),
            "fatal".to_string(),
        ]);

        assert!(matcher.has_match("An error occurred"));
        assert!(matcher.has_match("warning: something"));
        assert!(!matcher.has_match("all is fine"));

        let found = matcher.find_keywords("error and warning");
        assert_eq!(found.len(), 2);
        assert!(found.contains(&"error".to_string()));
        assert!(found.contains(&"warning".to_string()));
    }

    #[test]
    fn test_function_extraction() {
        let matcher = KeywordMatcher::new(vec![]);

        let funcs = matcher.extract_function_names("calling process_data()");
        assert!(funcs.contains(&"process_data".to_string()));

        let funcs2 = matcher.extract_function_names(">>> handle_request(item_id=123)");
        assert!(funcs2.contains(&"handle_request".to_string()));

        let funcs3 = matcher.extract_function_names("<<< validate");
        assert!(funcs3.contains(&"validate".to_string()));
    }

    #[test]
    fn test_keyword_filtering() {
        assert!(is_common_keyword("if"));
        assert!(is_common_keyword("function"));
        assert!(!is_common_keyword("process_data"));
    }
}

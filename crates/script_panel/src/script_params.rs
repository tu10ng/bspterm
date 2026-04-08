use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum ParamType {
    #[default]
    String,
    Number,
    Boolean,
    Choice,
    Password,
}

#[derive(Debug, Clone)]
pub struct ScriptParam {
    pub name: String,
    pub param_type: ParamType,
    pub description: Option<String>,
    pub required: bool,
    pub default: Option<String>,
    pub choices: Option<Vec<String>>,
}

impl ScriptParam {
    pub fn new(name: String) -> Self {
        Self {
            name,
            param_type: ParamType::String,
            description: None,
            required: false,
            default: None,
            choices: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScriptParams {
    pub params: Vec<ScriptParam>,
}

impl ScriptParams {
    pub fn parse_from_script(content: &str) -> Option<Self> {
        let docstring = extract_docstring(content)?;
        let params_block = extract_params_block(&docstring)?;
        let params = parse_params_block(&params_block);

        if params.is_empty() {
            return None;
        }

        Some(Self { params })
    }

    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
    }

    pub fn to_env_map(&self, values: &HashMap<String, String>) -> HashMap<String, String> {
        let mut env = HashMap::new();
        for param in &self.params {
            let key = format!("BSPTERM_PARAM_{}", param.name.to_uppercase());
            if let Some(value) = values.get(&param.name) {
                env.insert(key, value.clone());
            } else if let Some(default) = &param.default {
                env.insert(key, default.clone());
            }
        }
        env
    }
}

fn extract_docstring(content: &str) -> Option<String> {
    let content = content.trim();

    // Skip shebang, comment lines, and blank lines before docstring
    let content: String = content
        .lines()
        .skip_while(|line| {
            let trimmed = line.trim();
            trimmed.starts_with('#') || trimmed.is_empty()
        })
        .collect::<Vec<_>>()
        .join("\n");

    let content = content.trim();

    // Look for triple-quoted docstring at the start
    let (quote, rest) = if let Some(rest) = content.strip_prefix("\"\"\"") {
        ("\"\"\"", rest)
    } else if let Some(rest) = content.strip_prefix("'''") {
        ("'''", rest)
    } else {
        return None;
    };

    // Find the closing quote
    if let Some(end_pos) = rest.find(quote) {
        Some(rest[..end_pos].to_string())
    } else {
        None
    }
}

fn extract_params_block(docstring: &str) -> Option<String> {
    let start_marker = "@params";
    let end_marker = "@end_params";

    let start_pos = docstring.find(start_marker)?;
    let after_start = &docstring[start_pos + start_marker.len()..];
    let end_pos = after_start.find(end_marker)?;

    Some(after_start[..end_pos].trim().to_string())
}

fn parse_params_block(block: &str) -> Vec<ScriptParam> {
    let mut params = Vec::new();
    let mut current_param: Option<ScriptParam> = None;

    for line in block.lines() {
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        // Check if this is a new parameter definition (starts with "- name:")
        if let Some(rest) = line.strip_prefix("- ") {
            // Save current param if exists
            if let Some(param) = current_param.take() {
                params.push(param);
            }

            // Parse "- name: type" format
            let rest = rest.trim();
            if let Some((name, type_str)) = rest.split_once(':') {
                let name = name.trim().to_string();
                let type_str = type_str.trim();
                let param_type = parse_param_type(type_str);

                current_param = Some(ScriptParam {
                    name,
                    param_type,
                    description: None,
                    required: false,
                    default: None,
                    choices: None,
                });
            }
        } else if let Some(ref mut param) = current_param {
            // Parse attribute lines
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "description" => {
                        param.description = Some(value.to_string());
                    }
                    "required" => {
                        param.required = value == "true";
                    }
                    "default" => {
                        // Remove surrounding quotes if present
                        let value = value
                            .trim_start_matches('"')
                            .trim_end_matches('"')
                            .trim_start_matches('\'')
                            .trim_end_matches('\'');
                        param.default = Some(value.to_string());
                    }
                    "choices" => {
                        param.choices = Some(parse_choices(value));
                    }
                    _ => {}
                }
            }
        }
    }

    // Save final param
    if let Some(param) = current_param.take() {
        params.push(param);
    }

    params
}

fn parse_param_type(type_str: &str) -> ParamType {
    match type_str.to_lowercase().as_str() {
        "string" => ParamType::String,
        "number" => ParamType::Number,
        "boolean" => ParamType::Boolean,
        "choice" => ParamType::Choice,
        "password" => ParamType::Password,
        _ => ParamType::String,
    }
}

fn parse_choices(value: &str) -> Vec<String> {
    // Parse JSON-like array: ["Option1", "Option2"]
    let value = value.trim();
    if !value.starts_with('[') || !value.ends_with(']') {
        return vec![];
    }

    let inner = &value[1..value.len() - 1];
    inner
        .split(',')
        .map(|s| {
            s.trim()
                .trim_start_matches('"')
                .trim_end_matches('"')
                .trim_start_matches('\'')
                .trim_end_matches('\'')
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_docstring() {
        let script = r#"#!/usr/bin/env python3
"""
This is a docstring

@params
- host: string
@end_params
"""

from bspterm import params
"#;
        let docstring = extract_docstring(script).unwrap();
        assert!(docstring.contains("@params"));
        assert!(docstring.contains("host: string"));
    }

    #[test]
    fn test_extract_params_block() {
        let docstring = r#"
Some description

@params
- host: string
  description: Target host
  required: true
@end_params
"#;
        let block = extract_params_block(docstring).unwrap();
        assert!(block.contains("host: string"));
        assert!(block.contains("description: Target host"));
    }

    #[test]
    fn test_parse_full_script() {
        let script = r#"#!/usr/bin/env python3
"""
设备配置脚本

@params
- host: string
  description: 目标设备 IP 地址
  required: true
  default: "192.168.1.1"

- port: number
  description: 连接端口
  default: 22

- protocol: choice
  description: 连接协议
  choices: ["SSH", "Telnet"]
  default: "SSH"

- password: password
  description: 设备密码
  required: true

- verbose: boolean
  description: 启用详细输出
  default: false
@end_params
"""

from bspterm import params
"#;

        let params = ScriptParams::parse_from_script(script).unwrap();
        assert_eq!(params.params.len(), 5);

        let host = &params.params[0];
        assert_eq!(host.name, "host");
        assert_eq!(host.param_type, ParamType::String);
        assert_eq!(host.description.as_deref(), Some("目标设备 IP 地址"));
        assert!(host.required);
        assert_eq!(host.default.as_deref(), Some("192.168.1.1"));

        let port = &params.params[1];
        assert_eq!(port.name, "port");
        assert_eq!(port.param_type, ParamType::Number);
        assert_eq!(port.default.as_deref(), Some("22"));
        assert!(!port.required);

        let protocol = &params.params[2];
        assert_eq!(protocol.name, "protocol");
        assert_eq!(protocol.param_type, ParamType::Choice);
        assert_eq!(protocol.choices, Some(vec!["SSH".to_string(), "Telnet".to_string()]));

        let password = &params.params[3];
        assert_eq!(password.name, "password");
        assert_eq!(password.param_type, ParamType::Password);
        assert!(password.required);

        let verbose = &params.params[4];
        assert_eq!(verbose.name, "verbose");
        assert_eq!(verbose.param_type, ParamType::Boolean);
        assert_eq!(verbose.default.as_deref(), Some("false"));
    }

    #[test]
    fn test_extract_docstring_with_comments() {
        let script = r#"#!/usr/bin/env python3
# This is a comment
# Another comment
"""
@params
- host: string
@end_params
"""
"#;
        let docstring = extract_docstring(script).unwrap();
        assert!(docstring.contains("@params"));
        assert!(docstring.contains("host: string"));
    }

    #[test]
    fn test_to_env_map() {
        let params = ScriptParams {
            params: vec![
                ScriptParam {
                    name: "host".to_string(),
                    param_type: ParamType::String,
                    description: None,
                    required: true,
                    default: Some("127.0.0.1".to_string()),
                    choices: None,
                },
                ScriptParam {
                    name: "verbose".to_string(),
                    param_type: ParamType::Boolean,
                    description: None,
                    required: false,
                    default: Some("false".to_string()),
                    choices: None,
                },
            ],
        };

        let mut values = HashMap::new();
        values.insert("host".to_string(), "192.168.1.1".to_string());

        let env = params.to_env_map(&values);
        assert_eq!(env.get("BSPTERM_PARAM_HOST"), Some(&"192.168.1.1".to_string()));
        assert_eq!(env.get("BSPTERM_PARAM_VERBOSE"), Some(&"false".to_string()));
    }
}

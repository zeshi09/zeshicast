use std::collections::HashMap;
use std::time::SystemTime;

use chrono::{DateTime, Local};

use crate::{Calculator, format_number};

#[derive(Debug, Clone)]
pub(crate) struct PlaceholderContext {
    pub(crate) query: String,
    pub(crate) clipboard: String,
    pub(crate) args: HashMap<String, String>,
    pub(crate) preferences: HashMap<String, String>,
    pub(crate) now: SystemTime,
}

impl PlaceholderContext {
    pub(crate) fn new(query: &str, clipboard: Option<&String>) -> Self {
        Self {
            query: query.to_string(),
            clipboard: clipboard.cloned().unwrap_or_default(),
            args: HashMap::new(),
            preferences: HashMap::new(),
            now: SystemTime::now(),
        }
    }

    pub(crate) fn with_preferences(mut self, preferences: HashMap<String, String>) -> Self {
        self.preferences = preferences;
        self
    }
}

pub(crate) fn expand_placeholders(template: &str, context: &PlaceholderContext) -> String {
    let mut output = String::new();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Check for {{ ... }}
        if i + 1 < chars.len() && chars[i] == '{' && chars[i + 1] == '{' {
            let start = i + 2;
            if let Some(rel_end) = template[start..].find("}}") {
                let end = start + rel_end;
                let placeholder = &template[start..end];
                output.push_str(&render_placeholder(placeholder.trim(), context, true));
                i = end + 2;
                continue;
            }
        }
        // Check for single { ... }
        if chars[i] == '{' {
            let start = i + 1;
            if let Some(rel_end) = template[start..].find('}') {
                let end = start + rel_end;
                let placeholder = &template[start..end];
                if !placeholder.contains('{') && !placeholder.contains('\n') {
                    output.push_str(&render_placeholder(placeholder.trim(), context, false));
                    i = end + 1;
                    continue;
                }
            }
        }
        output.push(chars[i]);
        i += 1;
    }

    output
}

fn render_placeholder(placeholder: &str, context: &PlaceholderContext, double_brace: bool) -> String {
    let (name, argument) = placeholder
        .split_once(':')
        .map(|(name, argument)| (name.trim(), Some(argument.trim())))
        .unwrap_or((placeholder, None));

    match name {
        "query" => context.query.clone(),
        "clipboard" => context.clipboard.clone(),
        "arg" => argument
            .and_then(|name| context.args.get(name))
            .cloned()
            .unwrap_or_default(),
        "pref" => argument
            .and_then(|name| context.preferences.get(name))
            .cloned()
            .unwrap_or_default(),
        "date" => format_local_time(context.now, argument.unwrap_or("%Y-%m-%d")),
        "time" => format_local_time(context.now, argument.unwrap_or("%H:%M:%S")),
        "datetime" | "timestamp" => {
            format_local_time(context.now, argument.unwrap_or("%Y-%m-%d %H:%M:%S"))
        }
        "calc" => argument
            .and_then(|expr| Calculator::new(expr).parse().ok())
            .map(format_number)
            .unwrap_or_default(),
        "user" => std::env::var("USER").unwrap_or_else(|_| "user".to_string()),
        "hostname" => std::fs::read_to_string("/etc/hostname")
            .map(|h| h.trim().to_string())
            .unwrap_or_else(|_| "localhost".to_string()),
        "cursor" => String::new(),
        "uuid" => {
            let random = (context.now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() ^ 0x5a5a5a5a) as u64;
            format!("{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}", (random >> 32) as u32, (random >> 16) as u16, (random & 0xfff) as u16, (random >> 48) as u16 & 0xfff, random & 0xffffffffffff)
        }
        _ => {
            if double_brace {
                format!("{{{{{placeholder}}}}}")
            } else {
                format!("{{{placeholder}}}")
            }
        }
    }
}

pub(crate) fn format_local_time(time: SystemTime, format: &str) -> String {
    DateTime::<Local>::from(time).format(format).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_brace_and_double_brace_placeholders() {
        let ctx = PlaceholderContext::new("my query", Some(&"clipboard content".to_string()));
        let expanded = expand_placeholders("Query: {query}, Clip: {{clipboard}}", &ctx);
        assert_eq!(expanded, "Query: my query, Clip: clipboard content");
    }

    #[test]
    fn raycast_tokens_expand() {
        let ctx = PlaceholderContext::new("", None);
        let expanded = expand_placeholders("Hello {user} at {date:%Y}", &ctx);
        assert!(expanded.starts_with("Hello "));
        assert!(expanded.contains(" at "));
    }
}

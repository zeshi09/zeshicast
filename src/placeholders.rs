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

/// Expand placeholders verbatim. Use for non-shell contexts (URLs, snippets,
/// environment values).
pub(crate) fn expand_placeholders(template: &str, context: &PlaceholderContext) -> String {
    expand(template, context, false)
}

/// Expand placeholders for a string that will be passed to `sh -c`. Substituted
/// values (query, clipboard, arg, pref, …) are POSIX shell-quoted so untrusted
/// input (e.g. clipboard containing `$(rm -rf ~)` or `; reboot`) cannot break
/// out into command execution. Command authors therefore must NOT add their own
/// quotes around placeholders — the quoting is supplied here.
pub(crate) fn expand_placeholders_shell(template: &str, context: &PlaceholderContext) -> String {
    expand(template, context, true)
}

fn expand(template: &str, context: &PlaceholderContext, shell_escape: bool) -> String {
    let mut output = String::new();
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        let (before, after_start) = rest.split_at(start);
        output.push_str(before);

        let after_start = &after_start[2..];
        let Some(end) = after_start.find("}}") else {
            output.push_str("{{");
            output.push_str(after_start);
            return output;
        };

        let (placeholder, after_end) = after_start.split_at(end);
        match render_placeholder(placeholder.trim(), context) {
            Some(value) if shell_escape => output.push_str(&shell_quote(&value)),
            Some(value) => output.push_str(&value),
            // Unknown placeholder: emit it literally, unquoted.
            None => output.push_str(&format!("{{{{{}}}}}", placeholder.trim())),
        }
        rest = &after_end[2..];
    }

    output.push_str(rest);
    output
}

/// POSIX single-quote escaping: wrap in `'…'`, turning embedded `'` into `'\''`.
fn shell_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

/// Resolve a placeholder to its value, or `None` if the name is unknown.
fn render_placeholder(placeholder: &str, context: &PlaceholderContext) -> Option<String> {
    let (name, argument) = placeholder
        .split_once(':')
        .map(|(name, argument)| (name.trim(), Some(argument.trim())))
        .unwrap_or((placeholder, None));

    let value = match name {
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
        _ => return None,
    };
    Some(value)
}

pub(crate) fn format_local_time(time: SystemTime, format: &str) -> String {
    DateTime::<Local>::from(time).format(format).to_string()
}

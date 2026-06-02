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
        output.push_str(&render_placeholder(placeholder.trim(), context));
        rest = &after_end[2..];
    }

    output.push_str(rest);
    output
}

fn render_placeholder(placeholder: &str, context: &PlaceholderContext) -> String {
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
        _ => format!("{{{{{placeholder}}}}}"),
    }
}

pub(crate) fn format_local_time(time: SystemTime, format: &str) -> String {
    DateTime::<Local>::from(time).format(format).to_string()
}

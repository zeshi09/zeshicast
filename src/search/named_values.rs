use std::path::Path;

use crate::{
    Action, ActionKind, PlaceholderContext, clipboard_preview, expand_placeholders, fuzzy_score,
    load_lines,
};

#[derive(Debug, Clone)]
pub(crate) struct NamedValue {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) tags: Vec<String>,
}

impl NamedValue {
    fn search_text(&self) -> String {
        if self.tags.is_empty() {
            self.name.clone()
        } else {
            format!("{} {}", self.name, self.tags.join(" "))
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ActionTarget {
    OpenUrl,
    CopyText,
}

pub(crate) fn load_named_values(path: &Path) -> Vec<NamedValue> {
    load_lines(path)
        .into_iter()
        .filter_map(|line| parse_named_value(&line))
        .collect()
}

pub(crate) fn parse_named_value(line: &str) -> Option<NamedValue> {
    let (name_part, value) = line.split_once('=')?;
    let (name, tags) = match name_part.split_once('|') {
        Some((name, tags)) => (name.trim(), parse_tags(tags)),
        None => (name_part.trim(), Vec::new()),
    };

    if name.is_empty() {
        return None;
    }

    Some(NamedValue {
        name: name.to_string(),
        value: value.trim().to_string(),
        tags,
    })
}

fn parse_tags(tags: &str) -> Vec<String> {
    tags.split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
}

pub(crate) fn tagged_subtitle(value: &str, tags: &[String]) -> String {
    if tags.is_empty() {
        return value.to_string();
    }

    format!("{value}  [{}]", tags.join(", "))
}

pub(crate) fn search_named_values(
    category: &str,
    entries: &[NamedValue],
    query: &str,
    target: ActionTarget,
    context: &PlaceholderContext,
) -> Vec<Action> {
    entries
        .iter()
        .filter_map(|entry| {
            let score = if query.trim().is_empty() {
                0
            } else {
                fuzzy_score(&entry.search_text(), query)?
            };
            let value = expand_placeholders(&entry.value, context);
            let kind = match target {
                ActionTarget::OpenUrl => ActionKind::OpenUrl(value.clone()),
                ActionTarget::CopyText => ActionKind::Copy(value.clone()),
            };
            let icon_name = match target {
                ActionTarget::OpenUrl => "emblem-web-symbolic",
                ActionTarget::CopyText => "insert-text-symbolic",
            };
            let subtitle = match target {
                ActionTarget::OpenUrl => tagged_subtitle(&value, &entry.tags),
                ActionTarget::CopyText => tagged_subtitle(&clipboard_preview(&value), &entry.tags),
            };
            Some(
                Action::new(category, &entry.name, kind, score + 80)
                    .with_subtitle(subtitle)
                    .with_icon(icon_name),
            )
        })
        .collect()
}

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Action, ActionKind, ShellCommand, fuzzy_score};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserTab {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub browser: String,
    #[serde(default)]
    pub window_id: Option<u64>,
}

pub fn load_browser_tabs() -> Vec<BrowserTab> {
    let path = Path::new("/tmp/zeshicast-browser-tabs.json");
    if let Ok(content) = fs::read_to_string(path) {
        if let Ok(tabs) = serde_json::from_str::<Vec<BrowserTab>>(&content) {
            return tabs;
        }
    }
    Vec::new()
}

pub fn search_browser_tabs(tabs: &[BrowserTab], query: &str) -> Vec<Action> {
    if tabs.is_empty() {
        return Vec::new();
    }

    let lower = query.trim().to_lowercase();
    let explicit = lower.starts_with("tab ") || lower.starts_with("tabs ") || lower == "tab" || lower == "tabs";
    let search_query = if explicit {
        query.splitn(2, ' ').nth(1).unwrap_or("").trim()
    } else {
        query.trim()
    };

    if !explicit && search_query.len() < 2 {
        return Vec::new();
    }

    let mut matches: Vec<Action> = tabs
        .iter()
        .filter_map(|tab| {
            let haystack = format!("{} {}", tab.title, tab.url);
            let score = if search_query.is_empty() {
                20
            } else {
                fuzzy_score(&haystack, search_query)?
            };

            let subtitle = if !tab.browser.is_empty() {
                format!("{} • {}", tab.browser, tab.url)
            } else {
                tab.url.clone()
            };

            let action_cmd = if let Some(win_id) = tab.window_id {
                format!("niri msg action focus-window --id {win_id} || swaymsg '[con_id={win_id}] focus'")
            } else {
                format!("xdg-open '{}'", tab.url)
            };

            Some(
                Action::new(
                    "Browser Tab",
                    &tab.title,
                    ActionKind::Shell(ShellCommand::new(action_cmd)),
                    score + if explicit { 180 } else { 10 },
                )
                .with_subtitle(subtitle)
                .with_icon("web-browser-symbolic"),
            )
        })
        .collect();

    matches.sort_by(|a, b| b.score.cmp(&a.score).then(a.title.cmp(&b.title)));
    matches.truncate(15);
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_browser_tabs_matches_title_and_url() {
        let tabs = vec![
            BrowserTab {
                title: "Rust Documentation".to_string(),
                url: "https://doc.rust-lang.org".to_string(),
                browser: "Firefox".to_string(),
                window_id: Some(123),
            },
            BrowserTab {
                title: "GitHub Repository".to_string(),
                url: "https://github.com/rust-lang/rust".to_string(),
                browser: "Chromium".to_string(),
                window_id: None,
            },
        ];

        let results = search_browser_tabs(&tabs, "doc");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Rust Documentation");
        assert_eq!(results[0].category, "Browser Tab");
    }
}

use std::fs;
use std::path::Path;

use crate::{Action, ActionKind, ActionRisk};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserTab {
    pub title: String,
    pub url: String,
    pub browser: String,
    pub icon: String,
}

impl BrowserTab {
    pub fn new(
        title: impl Into<String>,
        url: impl Into<String>,
        browser: impl Into<String>,
        icon: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            url: url.into(),
            browser: browser.into(),
            icon: icon.into(),
        }
    }

    pub fn to_action(&self, score: i32) -> Action {
        let domain = extract_domain(&self.url);
        let subtitle = if domain.is_empty() {
            self.browser.clone()
        } else {
            format!("{} · {}", self.browser, domain)
        };

        Action {
            category: "Browser Tab".to_string(),
            title: self.title.clone(),
            subtitle,
            icon_name: self.icon.clone(),
            risk: ActionRisk::Normal,
            kind: ActionKind::OpenUrl(self.url.clone()),
            score,
        }
    }
}

pub fn search_browser_tabs(query: &str) -> Vec<Action> {
    let query_clean = query.strip_prefix("tab:").unwrap_or(query).trim().to_lowercase();
    if query_clean.is_empty() && !query.starts_with("tab:") {
        return Vec::new();
    }

    let tabs = collect_open_tabs();
    let mut actions = Vec::new();

    for tab in tabs {
        let title_lower = tab.title.to_lowercase();
        let url_lower = tab.url.to_lowercase();

        let mut score = 0;
        if query_clean.is_empty() {
            score = 60;
        } else if title_lower.starts_with(&query_clean) {
            score = 120;
        } else if title_lower.contains(&query_clean) {
            score = 90;
        } else if url_lower.contains(&query_clean) {
            score = 75;
        }

        if score > 0 {
            actions.push(tab.to_action(score));
        }
    }

    actions.sort_by(|a, b| b.score.cmp(&a.score));
    actions
}

pub fn collect_open_tabs() -> Vec<BrowserTab> {
    let mut tabs = Vec::new();
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());

    // 1. Firefox sessions
    let firefox_dir = format!("{home}/.mozilla/firefox");
    if let Ok(entries) = fs::read_dir(&firefox_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let recovery_json = path.join("sessionstore-backups/recovery.js");
                if recovery_json.is_file() {
                    if let Ok(content) = fs::read_to_string(&recovery_json) {
                        tabs.extend(parse_firefox_json(&content));
                    }
                }
            }
        }
    }

    // 2. Chromium / Chrome / Brave / Edge sessions
    let chromium_paths = [
        ("Brave", "brave-browser", format!("{home}/.config/BraveSoftware/Brave-Browser/Default/Bookmarks")),
        ("Chrome", "google-chrome", format!("{home}/.config/google-chrome/Default/Bookmarks")),
        ("Chromium", "chromium", format!("{home}/.config/chromium/Default/Bookmarks")),
    ];

    for (browser, icon, path_str) in chromium_paths {
        if Path::new(&path_str).is_file() {
            if let Ok(content) = fs::read_to_string(&path_str) {
                tabs.extend(parse_chromium_bookmarks(&content, browser, icon));
            }
        }
    }

    tabs
}

pub fn extract_domain(url: &str) -> String {
    let stripped = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);

    stripped
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_string()
}

pub fn parse_firefox_json(json_str: &str) -> Vec<BrowserTab> {
    let mut result = Vec::new();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) else {
        return result;
    };

    if let Some(windows) = v.get("windows").and_then(|w| w.as_array()) {
        for win in windows {
            if let Some(tabs) = win.get("tabs").and_then(|t| t.as_array()) {
                for tab in tabs {
                    if let Some(entries) = tab.get("entries").and_then(|e| e.as_array()) {
                        if let Some(last_entry) = entries.last() {
                            let title = last_entry
                                .get("title")
                                .and_then(|t| t.as_str())
                                .unwrap_or("Untitled")
                                .to_string();
                            let url = last_entry
                                .get("url")
                                .and_then(|u| u.as_str())
                                .unwrap_or("")
                                .to_string();

                            if !url.is_empty() && !url.starts_with("about:") {
                                result.push(BrowserTab::new(title, url, "Firefox", "firefox"));
                            }
                        }
                    }
                }
            }
        }
    }

    result
}

pub fn parse_chromium_bookmarks(json_str: &str, browser: &str, icon: &str) -> Vec<BrowserTab> {
    let mut result = Vec::new();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) else {
        return result;
    };

    fn recurse(node: &serde_json::Value, browser: &str, icon: &str, out: &mut Vec<BrowserTab>) {
        if node.get("type").and_then(|t| t.as_str()) == Some("url") {
            let title = node.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
            let url = node.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();
            if !url.is_empty() && !title.is_empty() {
                out.push(BrowserTab::new(title, url, browser, icon));
            }
        } else if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
            for child in children {
                recurse(child, browser, icon, out);
            }
        }
    }

    if let Some(roots) = v.get("roots").and_then(|r| r.as_object()) {
        for (_, root_node) in roots {
            recurse(root_node, browser, icon, &mut result);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_domain_formats() {
        assert_eq!(extract_domain("https://github.com/zeshi09/zeshicast"), "github.com");
        assert_eq!(extract_domain("http://localhost:3000/dashboard"), "localhost");
        assert_eq!(extract_domain("https://doc.rust-lang.org/std/"), "doc.rust-lang.org");
    }

    #[test]
    fn parse_firefox_json_sample() {
        let json = r#"{
            "windows": [
                {
                    "tabs": [
                        {
                            "entries": [
                                { "title": "GitHub - Zeshicast", "url": "https://github.com/zeshi09/zeshicast" },
                                { "title": "Rust Standard Library", "url": "https://doc.rust-lang.org/std/" }
                            ]
                        }
                    ]
                }
            ]
        }"#;

        let tabs = parse_firefox_json(json);
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].title, "Rust Standard Library");
        assert_eq!(tabs[0].url, "https://doc.rust-lang.org/std/");
        assert_eq!(tabs[0].browser, "Firefox");
    }

    #[test]
    fn parse_chromium_bookmarks_sample() {
        let json = r#"{
            "roots": {
                "bookmark_bar": {
                    "children": [
                        { "name": "Google", "type": "url", "url": "https://google.com" },
                        {
                            "name": "Dev",
                            "type": "folder",
                            "children": [
                                { "name": "NixOS", "type": "url", "url": "https://nixos.org" }
                            ]
                        }
                    ]
                }
            }
        }"#;

        let tabs = parse_chromium_bookmarks(json, "Brave", "brave-browser");
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs[0].title, "Google");
        assert_eq!(tabs[1].title, "NixOS");
    }
}

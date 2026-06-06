use std::fs;
use std::path::Path;

use crate::{Action, ActionKind, ShellCommand, fuzzy_score};

#[derive(Debug, Clone)]
pub(crate) struct SshEntry {
    pub(crate) host: String,
    pub(crate) user: Option<String>,
    pub(crate) hostname: Option<String>,
}

impl SshEntry {
    fn display(&self) -> String {
        match &self.user {
            Some(user) => format!("{user}@{}", self.host),
            None => self.host.clone(),
        }
    }

    fn subtitle(&self) -> String {
        self.hostname.as_deref().unwrap_or(&self.host).to_string()
    }
}

pub(crate) fn load_ssh_entries() -> Vec<SshEntry> {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = Path::new(&home).join(".ssh/config");
    let Ok(content) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    parse_ssh_config(&content)
}

fn parse_ssh_config(content: &str) -> Vec<SshEntry> {
    let mut entries = Vec::new();
    let mut current_host: Option<String> = None;
    let mut current_user: Option<String> = None;
    let mut current_hostname: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.split_once(|c: char| c.is_ascii_whitespace()) {
            Some(pair) => (pair.0.to_lowercase(), pair.1.trim().to_string()),
            None => continue,
        };

        match key.as_str() {
            "host" => {
                if let Some(host) = current_host.take() {
                    entries.push(SshEntry {
                        host,
                        user: current_user.take(),
                        hostname: current_hostname.take(),
                    });
                } else {
                    current_user = None;
                    current_hostname = None;
                }
                // skip wildcards
                if !value.contains('*') && !value.contains('?') {
                    current_host = Some(value);
                }
            }
            "user" if current_host.is_some() => current_user = Some(value),
            "hostname" if current_host.is_some() => current_hostname = Some(value),
            _ => {}
        }
    }
    if let Some(host) = current_host {
        entries.push(SshEntry {
            host,
            user: current_user,
            hostname: current_hostname,
        });
    }
    entries
}

pub(crate) fn search_ssh(entries: &[SshEntry], query: &str) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let is_explicit = lower.starts_with("ssh ");
    let needle = if is_explicit {
        query.trim()[4..].trim()
    } else if lower == "ssh" {
        ""
    } else {
        return Vec::new();
    };

    let mut results: Vec<Action> = entries
        .iter()
        .filter_map(|entry| {
            let score = if needle.is_empty() {
                100
            } else {
                fuzzy_score(&entry.host, needle).or_else(|| {
                    entry
                        .hostname
                        .as_deref()
                        .and_then(|h| fuzzy_score(h, needle))
                })?
            };
            let cmd = format!("{} ssh {}", preferred_terminal(), entry.display());
            Some(
                Action::new(
                    "SSH",
                    &entry.display(),
                    ActionKind::Shell(ShellCommand::new(cmd)),
                    score,
                )
                .with_subtitle(entry.subtitle())
                .with_icon("network-server-symbolic"),
            )
        })
        .collect();

    results.sort_by(|a, b| b.score.cmp(&a.score).then(a.title.cmp(&b.title)));
    results.truncate(8);
    results
}

fn preferred_terminal() -> &'static str {
    for term in &["kitty", "foot", "wezterm", "alacritty", "ghostty"] {
        if std::process::Command::new("which")
            .arg(term)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return term;
        }
    }
    "xterm"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_ssh_config() {
        let config = "Host myserver\n  HostName 192.168.1.10\n  User alice\n\nHost wildcard-*\n  User root\n";
        let entries = parse_ssh_config(config);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].host, "myserver");
        assert_eq!(entries[0].user.as_deref(), Some("alice"));
        assert_eq!(entries[0].hostname.as_deref(), Some("192.168.1.10"));
    }

    #[test]
    fn skip_wildcard_hosts() {
        let config = "Host *\n  ServerAliveInterval 60\nHost prod\n  User deploy\n";
        let entries = parse_ssh_config(config);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].host, "prod");
    }
}

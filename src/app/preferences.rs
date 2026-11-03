//! Preferences, pins, aliases and the recently-used list: what `Zeshicast`
//! keeps between runs besides the clipboard and the snippets (P5.3).

use super::*;

pub(crate) fn preference_enabled_value(
    preferences: &HashMap<String, String>,
    key: &str,
    default_value: bool,
) -> bool {
    preferences
        .get(key)
        .and_then(|value| parse_bool_preference(value))
        .unwrap_or(default_value)
}

fn parse_bool_preference(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

/// Tips shown under the empty search, one per palette opening (Q3.1).
pub(crate) const LAUNCHER_HINTS: &[&str] = &[
    "Bind zeshicast-gtk --toggle to a key: one press shows the palette, the next hides it",
    "Ctrl+K opens actions for the selected row: paste, pin, alias, hotkey binding",
    "Prefixes narrow the search: file · calc · clip · win · proc · shell · ai",
    "Enter on a clipboard or snippet row pastes it; Ctrl+Enter only copies",
    "Ctrl+D dashboard · Ctrl+T system monitor · Ctrl+H clipboard · Ctrl+U notifications",
    "Typed in the wrong layout? ашкуащч still finds Firefox",
];

/// How many openings show a tip before they stop for good.
pub(crate) const MAX_HINT_SHOWS: u32 = 30;

/// The tip for the `shown`-th opening, cycling through the list, or `None`
/// once the budget is spent.
pub(crate) fn hint_for(shown: u32) -> Option<&'static str> {
    if shown >= MAX_HINT_SHOWS {
        return None;
    }
    let index = usize::try_from(shown).ok()? % LAUNCHER_HINTS.len();
    LAUNCHER_HINTS.get(index).copied()
}

pub(crate) fn load_calc_history(path: &std::path::Path) -> Vec<CalcHistoryEntry> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(array) = serde_json::from_str::<serde_json::Value>(&content) else {
        return Vec::new();
    };
    let Some(items) = array.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let expr = item.get("e")?.as_str()?.to_string();
            let result = item.get("r")?.as_str()?.to_string();
            Some(CalcHistoryEntry { expr, result })
        })
        .collect()
}

fn save_calc_history(path: &std::path::Path, history: &[CalcHistoryEntry]) {
    let array: Vec<serde_json::Value> = history
        .iter()
        .map(|e| serde_json::json!({"e": e.expr, "r": e.result}))
        .collect();
    if let Ok(json) = serde_json::to_string(&array) {
        crate::config::write_file_atomic(path, json.as_bytes(), 0o600).ok();
    }
}

pub(crate) fn preference_script_dirs(
    preferences: &HashMap<String, String>,
    config_dir: &std::path::Path,
) -> Vec<std::path::PathBuf> {
    let custom = preferences.get("script_dirs").map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(expand_home_dir)
            .collect::<Vec<_>>()
    });
    custom.unwrap_or_else(|| vec![config_dir.join("scripts")])
}

/// Expand a leading `~` in a user-provided script directory (N-14). Without
/// this, `~/scripts` is read as a literal directory named `~` under the process
/// CWD — never the one the user meant.
fn expand_home_dir(path: &str) -> std::path::PathBuf {
    if path == "~" {
        return crate::config::home_dir();
    }
    match path.strip_prefix("~/") {
        Some(rest) => crate::config::home_dir().join(rest),
        None => std::path::PathBuf::from(path),
    }
}

impl Zeshicast {
    pub fn record_calc(&mut self, expr: &str, result: &str) {
        self.calc_history.retain(|e| e.expr != expr);
        self.calc_history.insert(
            0,
            CalcHistoryEntry {
                expr: expr.to_string(),
                result: result.to_string(),
            },
        );
        self.calc_history.truncate(20);
        save_calc_history(
            &self.config_dir.join("calc_history.json"),
            &self.calc_history,
        );
    }

    pub fn is_recent(&self, action: &Action) -> bool {
        let identity = action.identity().to_lowercase();
        self.recent.iter().any(|entry| entry == &identity)
    }

    pub fn recent_top_identities(&self, count: usize) -> Vec<String> {
        self.recent
            .iter()
            .filter(|id| !self.pins.contains(*id))
            .take(count)
            .cloned()
            .collect()
    }

    pub fn set_alias_for_action(&mut self, alias: &str, action: &Action) -> io::Result<String> {
        let alias = normalize_alias(alias);
        if alias.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "alias is empty",
            ));
        }

        let target = action.title.clone();
        append_alias(&self.config_dir, &alias, &target)?;
        self.aliases.insert(alias.clone(), target);
        Ok(alias)
    }

    pub fn is_pinned(&self, action: &Action) -> bool {
        let title = action.title.to_lowercase();
        let identity = action.identity().to_lowercase();
        self.pins.contains(&title) || self.pins.contains(&identity)
    }

    pub fn pin_action(&mut self, action: &Action) -> io::Result<()> {
        self.pins.insert(action.identity().to_lowercase());
        self.write_pins()
    }

    pub fn unpin_action(&mut self, action: &Action) -> io::Result<()> {
        let title = action.title.to_lowercase();
        let identity = action.identity().to_lowercase();
        self.pins.remove(&title);
        self.pins.remove(&identity);
        self.write_pins()
    }

    fn preference_enabled(&self, key: &str, default_value: bool) -> bool {
        preference_enabled_value(&self.preferences, key, default_value)
    }

    pub fn clipboard_history_enabled(&self) -> bool {
        self.preference_enabled("clipboard_history_enabled", true)
    }

    pub fn clipboard_private_mode(&self) -> bool {
        self.preference_enabled("clipboard_private_mode", false)
    }

    pub fn clipboard_capture_images(&self) -> bool {
        self.preference_enabled("clipboard_capture_images", true)
            && self.clipboard_history_enabled()
            && !self.clipboard_private_mode()
    }

    /// Apps whose copies are not recorded (`clipboard_ignore_apps`, Q6.4);
    /// password managers when the preference is not set.
    #[cfg_attr(not(feature = "gui"), allow(dead_code))]
    pub(crate) fn clipboard_ignored_apps(&self) -> Vec<String> {
        crate::services::clipboard_formats::parse_ignored_apps(
            self.preferences
                .get("clipboard_ignore_apps")
                .map(String::as_str)
                .unwrap_or(crate::services::clipboard_formats::DEFAULT_IGNORED_APPS),
        )
    }

    /// The tip for this palette opening, counting it (Q3.1). `None` when tips
    /// are off (`ui_hints = false`) or all have been shown enough. The count
    /// is state, so it lives in `hints_shown`, not in `preferences.toml`.
    pub fn next_launcher_hint(&mut self) -> Option<&'static str> {
        if !self.preference_enabled("ui_hints", true) {
            return None;
        }
        let path = self.config_dir.join("hints_shown");
        let shown = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| text.trim().parse::<u32>().ok())
            .unwrap_or(0);
        let hint = hint_for(shown)?;
        if let Err(error) =
            crate::config::write_file_atomic(&path, (shown + 1).to_string().as_bytes(), 0o600)
        {
            log::warn!("could not record the shown tip: {error}");
        }
        Some(hint)
    }

    pub fn notifications_history_enabled(&self) -> bool {
        self.preference_enabled("notifications_enabled", true)
            && self.preference_enabled("notifications_history_enabled", true)
    }

    pub fn get_preferences(&self) -> &HashMap<String, String> {
        &self.preferences
    }

    pub fn set_preference(&mut self, key: String, value: String) -> io::Result<()> {
        if !self.preferences_writable {
            // M-10: the file on disk is corrupt; writing now would erase it.
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "{} is not valid TOML; fix or remove it before changing preferences",
                    self.config_dir.join("preferences.toml").display()
                ),
            ));
        }
        let should_prune_clipboard = key == "clipboard_retention";
        if value.is_empty() {
            self.preferences.remove(&key);
        } else {
            self.preferences.insert(key, value);
        }
        write_preferences(&self.config_dir.join("preferences.toml"), &self.preferences)?;
        // Private mode also keeps notification bodies off the disk (Q6.5).
        crate::services::notifications::set_keep_bodies(!self.clipboard_private_mode());
        if should_prune_clipboard {
            self.enforce_clipboard_retention()?;
        }
        Ok(())
    }

    fn write_pins(&self) -> io::Result<()> {
        let mut pins = self.pins.iter().cloned().collect::<Vec<_>>();
        pins.sort();
        write_lines(&self.config_dir.join("pins.txt"), &pins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_dirs_expand_a_leading_tilde() {
        let preferences = HashMap::from([(
            "script_dirs".to_string(),
            "~/scripts, /absolute/dir, ~".to_string(),
        )]);

        let dirs = preference_script_dirs(&preferences, std::path::Path::new("/cfg"));
        let home = crate::config::home_dir();

        assert_eq!(dirs[0], home.join("scripts"));
        assert_eq!(dirs[1], std::path::PathBuf::from("/absolute/dir"));
        assert_eq!(dirs[2], home);
    }
}

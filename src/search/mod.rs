pub(crate) mod apps;
pub(crate) mod bluetooth;
pub(crate) mod browser_tabs;
pub(crate) mod calculator;
pub(crate) mod clipboard;
pub(crate) mod commands;
pub(crate) mod emoji;
pub(crate) mod extensions;
pub(crate) mod file_index;
pub(crate) mod files;
pub(crate) mod layout;
pub(crate) mod media;
pub(crate) mod mozlz4;
pub(crate) mod named_values;
pub(crate) mod notifications;
pub(crate) mod power;
pub(crate) mod processes;
pub(crate) mod scripts;
pub(crate) mod snapshot;
pub(crate) mod system;
pub(crate) mod units;
pub(crate) mod web;
pub(crate) mod windows;

/// How many results a provider may contribute before the palette truncates.
pub(crate) const MAX_RESULTS: usize = 40;

use crate::action::{Action, ActionKind};
use crate::placeholders::PlaceholderContext;
use crate::search::apps::{AppEntry, search_apps};
use crate::search::clipboard::search_clipboard;
use crate::search::commands::{CommandEntry, search_commands};
use crate::search::files::{FileEntry, is_file_query, search_files};
use crate::search::media::search_media_actions;
use crate::search::named_values::{ActionTarget, NamedValue, search_named_values};
use crate::search::notifications::search_notification_actions;
use crate::search::processes::search_processes;
use crate::search::system::{search_audio_actions, search_network_actions, search_system_actions};
use crate::search::web::{search_ai, search_translate};
use crate::search::windows::{
    search_hyprland_actions, search_niri_actions, search_sway_actions, search_windows,
};
use emoji::search_emoji;
use scripts::search_scripts;

pub(crate) struct SearchContext<'a> {
    pub(crate) query: &'a str,
    pub(crate) placeholders: &'a PlaceholderContext<'a>,
}

pub(crate) trait SearchProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action>;
}

pub(crate) struct AppsProvider<'a> {
    pub(crate) apps: &'a [AppEntry],
}

impl SearchProvider for AppsProvider<'_> {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_apps(self.apps, context.query)
    }
}

pub(crate) struct NamedValuesProvider<'a> {
    pub(crate) category: &'static str,
    pub(crate) entries: &'a [NamedValue],
    pub(crate) target: ActionTarget,
}

impl SearchProvider for NamedValuesProvider<'_> {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_named_values(
            self.category,
            self.entries,
            context.query,
            self.target,
            context.placeholders,
        )
    }
}

pub(crate) struct CommandsProvider<'a> {
    pub(crate) commands: &'a [CommandEntry],
}

impl SearchProvider for CommandsProvider<'_> {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_commands(self.commands, context.query, context.placeholders)
    }
}

pub(crate) struct SystemProvider;

impl SearchProvider for SystemProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_system_actions(context.query)
    }
}

pub(crate) struct AudioProvider;

impl SearchProvider for AudioProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_audio_actions(context.query)
    }
}

pub(crate) struct BluetoothProvider;

impl SearchProvider for BluetoothProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        bluetooth::search_bluetooth(context.query, crate::services::bluetooth::bluetooth_state)
    }
}

pub(crate) struct PowerProfilesProvider;

impl SearchProvider for PowerProfilesProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        power::search_power_profiles(context.query, crate::services::power::power_profiles)
    }
}

pub(crate) struct UnitsProvider;

impl SearchProvider for UnitsProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        units::search_units(context.query, crate::services::systemd_units::user_units)
    }
}

pub(crate) struct NetworkProvider;

impl SearchProvider for NetworkProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_network_actions(context.query)
    }
}

pub(crate) struct MediaProvider;

impl SearchProvider for MediaProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_media_actions(context.query)
    }
}

pub(crate) struct NotificationsProvider;

impl SearchProvider for NotificationsProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_notification_actions(context.query)
    }
}

pub(crate) struct NiriProvider;

impl SearchProvider for NiriProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_niri_actions(context.query)
    }
}

pub(crate) struct HyprlandProvider;

impl SearchProvider for HyprlandProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_hyprland_actions(context.query)
    }
}

pub(crate) struct SwayProvider;

impl SearchProvider for SwayProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_sway_actions(context.query)
    }
}

pub(crate) struct WindowsProvider;

impl SearchProvider for WindowsProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_windows(context.query)
    }
}

pub(crate) struct WebProvider;

impl SearchProvider for WebProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        let mut actions = search_ai(context.query, &context.placeholders.preferences);
        actions.extend(search_translate(
            context.query,
            &context.placeholders.preferences,
        ));
        actions
    }
}

pub(crate) struct ClipboardProvider<'a> {
    pub(crate) entries: &'a [String],
}

impl SearchProvider for ClipboardProvider<'_> {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        let query = context.query;
        let lower = query.trim().to_lowercase();
        if lower.starts_with("clip ") || lower.starts_with("clipboard ") {
            let needle = query
                .split_once(' ')
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            search_clipboard(self.entries, needle, true)
        } else if query.trim().len() >= 3 {
            search_clipboard(self.entries, query, false)
        } else {
            Vec::new()
        }
    }
}

/// Shorter fuzzy queries match almost everything in the index at full scan cost,
/// so they are left to the explicit `file `/`find ` prefixes (M-6).
const MIN_FUZZY_FILE_QUERY: usize = 3;

pub(crate) struct FilesProvider<'a> {
    pub(crate) files: &'a [FileEntry],
    /// Files in root search, without `file ` (`files_in_root`, Q6.2).
    pub(crate) in_root: bool,
}

impl SearchProvider for FilesProvider<'_> {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        let query = context.query;
        if is_file_query(query) {
            let needle = query
                .trim_start()
                .split_once(' ')
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            search_files(self.files, needle, true)
        } else if self.in_root && query.trim().len() >= MIN_FUZZY_FILE_QUERY {
            search_files(self.files, query, false)
        } else {
            Vec::new()
        }
    }
}

pub(crate) struct ProcessesProvider;

impl SearchProvider for ProcessesProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        let query = context.query;
        let lower = query.trim().to_lowercase();
        if lower.starts_with("proc ") || lower.starts_with("process ") {
            let needle = query
                .split_once(' ')
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            search_processes(needle)
        } else {
            Vec::new()
        }
    }
}

pub(crate) struct ScriptsProvider<'a> {
    pub(crate) entries: &'a [scripts::ScriptEntry],
}

impl SearchProvider for ScriptsProvider<'_> {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_scripts(self.entries, context.query)
    }
}

pub(crate) struct EmojiProvider;

impl SearchProvider for EmojiProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        search_emoji(context.query)
    }
}

pub(crate) fn fuzzy_score(text: &str, query: &str) -> Option<i32> {
    fuzzy_score_lower(&text.to_lowercase(), &query.trim().to_lowercase())
}

/// [`fuzzy_score`] for a haystack *and* a query that are already lowercase
/// (M-6).
///
/// The file index stores `name_lower` per entry and lowers the query once per
/// search, so scanning 10 000 entries never allocates a string per entry.
///
/// Tiers, best first, with non-overlapping ranges (Q1.1): exact (500) →
/// prefix (361–400) → substring at a word start (340–360) → acronym of word
/// starts, `vsc` → "Visual Studio Code" (330) → substring inside a word
/// (200–300) → a compact subsequence (< 200). Positions
/// and lengths count characters, not bytes, so Cyrillic names are not
/// penalised twice. A subsequence spread over the whole haystack is not a
/// match: `invoice` must not hit "Mastering_REST_APIs_…_Advanced.azw3".
pub(crate) fn fuzzy_score_lower(text_lower: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return None;
    }

    if text_lower == query {
        return Some(500);
    }
    if text_lower.starts_with(query) {
        return Some(400 - char_count(text_lower).min(39));
    }
    if let Some(score) = substring_score(text_lower, query) {
        return Some(score);
    }
    // Both remaining tiers need the query as an ordered subsequence; checking
    // that first without allocating rejects most of a 10 000-entry index.
    if !is_subsequence(text_lower, query) {
        return None;
    }
    if is_acronym(text_lower, query) {
        return Some(330);
    }
    subsequence_score(text_lower, query)
}

fn is_subsequence(text_lower: &str, query: &str) -> bool {
    let mut text = text_lower.chars();
    query
        .chars()
        .all(|wanted| text.by_ref().any(|actual| actual == wanted))
}

fn char_count(text: &str) -> i32 {
    i32::try_from(text.chars().count()).unwrap_or(i32::MAX)
}

/// Characters after which a new word starts.
fn is_word_separator(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '-' | '_' | '.' | '/' | '\\' | '(' | ')' | '[' | ']' | ':' | ','
        )
}

/// Best substring occurrence: one at a word start beats one inside a word, and
/// earlier beats later.
fn substring_score(text_lower: &str, query: &str) -> Option<i32> {
    let mut best: Option<i32> = None;
    for (byte, _) in text_lower.match_indices(query) {
        let position = char_count(&text_lower[..byte]);
        let at_word_start = text_lower[..byte]
            .chars()
            .next_back()
            .is_none_or(is_word_separator);
        let score = if at_word_start {
            360 - position.min(20)
        } else {
            300 - position.min(100)
        };
        best = best.max(Some(score));
        if at_word_start {
            break;
        }
    }
    best
}

/// `true` when every query character is the first letter of consecutive-or-
/// later words, in order, and the query names at least two words.
fn is_acronym(text_lower: &str, query: &str) -> bool {
    let mut previous: Option<char> = None;
    let mut initials = text_lower.chars().filter(move |&c| {
        let starts_word = !is_word_separator(c) && previous.is_none_or(is_word_separator);
        previous = Some(c);
        starts_word
    });
    let mut matched = 0;
    for wanted in query.chars().filter(|c| !c.is_whitespace()) {
        if !initials.any(|initial| initial == wanted) {
            return false;
        }
        matched += 1;
    }
    matched >= 2
}

/// Ordered subsequence match, rewarded for adjacency and word starts. Rejected
/// when the matched characters are spread too far apart to mean anything.
///
/// Tries the first few occurrences of the query's first character as starting
/// points, so `fx` in "f… firefox" can still find the compact match.
fn subsequence_score(text_lower: &str, query: &str) -> Option<i32> {
    const MAX_STARTS: usize = 8;

    let text: Vec<char> = text_lower.chars().collect();
    let wanted: Vec<char> = query.chars().collect();
    let first = *wanted.first()?;
    let max_span = wanted.len() * 3 + 6;

    text.iter()
        .enumerate()
        .filter(|(_, c)| **c == first)
        .take(MAX_STARTS)
        .filter_map(|(start, _)| subsequence_from(&text, &wanted, start, max_span))
        .max()
}

fn subsequence_from(text: &[char], wanted: &[char], start: usize, max_span: usize) -> Option<i32> {
    let at_word_start = |index: usize| index == 0 || is_word_separator(text[index - 1]);

    let mut score = 20 - i32::try_from(start).unwrap_or(20).min(20);
    if at_word_start(start) {
        score += 15;
    }
    let mut last = start;

    for &c in &wanted[1..] {
        let index = last
            + 1
            + text
                .get(last + 1..)?
                .iter()
                .position(|actual| *actual == c)?;
        if index - start + 1 > max_span {
            return None;
        }
        score += if index == last + 1 {
            20
        } else {
            10 - i32::try_from(index - last).unwrap_or(10).min(10)
        };
        if at_word_start(index) {
            score += 15;
        }
        last = index;
    }

    Some(score.min(199))
}

pub(crate) struct BrowserTabsProvider;

impl SearchProvider for BrowserTabsProvider {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        browser_tabs::search_browser_tabs(context.query)
    }
}

pub(crate) struct ExtensionsProvider<'a> {
    pub(crate) manifests: &'a [crate::extensions::ExtensionManifest],
}

impl SearchProvider for ExtensionsProvider<'_> {
    fn search(&self, context: &SearchContext<'_>) -> Vec<Action> {
        let mut actions = Vec::new();
        for manifest in self.manifests {
            if let Some(reason) = &manifest.binaries_blocked {
                actions.extend(
                    blocked_extension_row(&manifest.origin.name, reason, context.query)
                        .map(|row| row.with_origin(manifest.origin.id.clone())),
                );
                continue;
            }
            let capabilities = commands::parse_capabilities(&manifest.origin.capabilities);
            for binary in &manifest.binaries {
                actions.extend(
                    extensions::search_extension(
                        binary,
                        &manifest.origin.name,
                        context.query,
                        &capabilities,
                    )
                    .into_iter()
                    .map(|action| action.with_origin(manifest.origin.id.clone())),
                );
            }
        }
        actions
    }
}

/// An extension whose binaries were refused before any spawn (Q5.1) still
/// answers to its name, so the user learns why its results are missing.
fn blocked_extension_row(name: &str, reason: &str, query: &str) -> Option<Action> {
    let score = fuzzy_score(name, query.trim())?;
    Some(
        Action::new(
            format!("Extension: {name}"),
            name.to_string(),
            ActionKind::None,
            score,
        )
        .with_subtitle(reason.to_string())
        .with_icon("dialog-warning-symbolic"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_extension_refused_for_its_protocol_shows_why_without_running() {
        let manifest = crate::extensions::parse_extension_manifest(
            std::path::Path::new("/nonexistent/extension"),
            r#"
id = "example.future"
name = "Future Tools"
version = "1.0.0"
protocol = 9
capabilities = ["shell"]
binaries = ["bin/tool"]
"#,
        )
        .unwrap();
        let row = blocked_extension_row(
            &manifest.origin.name,
            manifest.binaries_blocked.as_deref().unwrap(),
            "future",
        )
        .expect("the name matches");
        assert!(matches!(row.kind, ActionKind::None));
        assert!(
            row.subtitle.contains("needs protocol 9"),
            "{}",
            row.subtitle
        );
        assert!(blocked_extension_row("Future Tools", "Blocked", "zzz").is_none());
    }

    fn score(text: &str, query: &str) -> i32 {
        fuzzy_score(text, query).unwrap_or_else(|| panic!("{query:?} should match {text:?}"))
    }

    #[test]
    fn tiers_rank_exact_prefix_word_start_acronym_inner_subsequence() {
        let exact = score("code", "code");
        let prefix = score("codex", "code");
        let word_start = score("visual studio code", "code");
        let acronym = score("visual studio code", "vsc");
        let inner = score("barcode scanner", "code");
        let subsequence = score("firefox", "frfx");

        assert!(exact > prefix, "{exact} > {prefix}");
        assert!(prefix > word_start, "{prefix} > {word_start}");
        assert!(word_start > acronym, "{word_start} > {acronym}");
        assert!(acronym > inner, "{acronym} > {inner}");
        assert!(inner > subsequence, "{inner} > {subsequence}");
    }

    #[test]
    fn acronyms_beat_an_inner_substring() {
        assert!(score("Visual Studio Code", "vsc") > score("dvsconfig", "vsc"));
        assert!(score("Google Chrome", "gc") > score("bgcolor tool", "gc"));
    }

    #[test]
    fn positions_count_characters_not_bytes() {
        // Same character lengths and positions: Cyrillic must not be penalised
        // for its two-byte UTF-8 encoding.
        assert_eq!(score("заметки 2024", "2024"), score("notes12 2024", "2024"));
        assert_eq!(
            score("заметки.txt", "заметки"),
            score("notes12.txt", "notes12")
        );
    }

    #[test]
    fn compact_subsequences_match_scattered_ones_do_not() {
        assert!(fuzzy_score("Firefox", "frfx").is_some());
        assert!(fuzzy_score("Neovim", "nvim").is_some());
        assert!(fuzzy_score("Google Chrome", "gco").is_some());
        assert_eq!(
            fuzzy_score(
                "Mastering_REST_APIs_Boosting_Your_Web_Development_Journey_with_Advanced.azw3",
                "invoice"
            ),
            None
        );
    }

    #[test]
    fn a_later_compact_match_is_found_after_an_early_stray_letter() {
        // The first `f` is a dead end; the compact match starts at "firefox".
        assert!(fuzzy_score("f bundle firefox", "frfx").is_some());
    }

    #[test]
    fn single_letters_and_empty_queries() {
        assert_eq!(fuzzy_score("firefox", ""), None);
        assert!(fuzzy_score("firefox", "f").is_some());
        assert_eq!(fuzzy_score("firefox", "z"), None);
    }
}

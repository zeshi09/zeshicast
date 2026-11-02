//! `zeshicast-gtk --dmenu`: lines on stdin → pick one in the palette → the
//! picked line on stdout (Q1.4). This module is the toolkit-free half: options,
//! input, filtering and what to print. The window lives in `ui::dmenu`.
//!
//! Exit codes follow dmenu/rofi: 0 with a line on stdout when something was
//! chosen, 1 when the user cancelled or there was nothing to choose from.

use std::io::{self, Read};

use crate::search::fuzzy_score;

/// Successful pick: the choice is on stdout.
pub const EXIT_CHOSEN: i32 = 0;
/// Cancelled, or nothing to pick from.
pub const EXIT_CANCELLED: i32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DmenuOptions {
    /// Placeholder shown in the empty input.
    pub prompt: Option<String>,
    /// Hide what is typed and print it instead of a list choice (askpass-
    /// style prompts). Input lines are optional in this mode.
    pub password: bool,
}

/// `Some` when `--dmenu` is present. Understands `--prompt TEXT`,
/// `--prompt=TEXT`, `-p TEXT` and `--password`.
pub fn parse_dmenu_args(args: &[String]) -> Option<DmenuOptions> {
    if !args.iter().any(|arg| arg == "--dmenu") {
        return None;
    }

    let mut options = DmenuOptions::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if let Some(prompt) = arg.strip_prefix("--prompt=") {
            options.prompt = Some(prompt.to_string());
        } else if arg == "--prompt" || arg == "-p" {
            options.prompt = iter.next().cloned();
        } else if arg == "--password" {
            options.password = true;
        }
    }
    Some(options)
}

/// Read the items: one per line, `\r\n` tolerated, empty lines dropped. A
/// line that is not UTF-8 is skipped with a warning rather than mangled,
/// because the chosen line is printed back verbatim for a script to use.
pub fn read_items(mut input: impl Read) -> io::Result<Vec<String>> {
    let mut bytes = Vec::new();
    input.read_to_end(&mut bytes)?;

    let mut items = Vec::new();
    for (number, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        match std::str::from_utf8(line) {
            Ok(text) => items.push(text.to_string()),
            Err(_) => log::warn!("dmenu: skipping line {} (not UTF-8)", number + 1),
        }
    }
    Ok(items)
}

/// Indices of `items` matching `query`, best first. An empty query keeps the
/// input order, like dmenu; ties keep it too.
pub fn filter_items(items: &[String], query: &str) -> Vec<usize> {
    if query.trim().is_empty() {
        return (0..items.len()).collect();
    }
    let mut scored: Vec<(usize, i32)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| fuzzy_score(item, query).map(|score| (index, score)))
        .collect();
    scored.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    scored.into_iter().map(|(index, _)| index).collect()
}

/// What Enter prints. A selected row wins; with no row (nothing matches)
/// the typed text is printed, as dmenu does, so a script can accept new
/// values. Password mode always prints the typed text. `None` means there is
/// nothing to print and the pick counts as cancelled.
pub fn enter_output(
    items: &[String],
    selected: Option<usize>,
    query: &str,
    options: &DmenuOptions,
) -> Option<String> {
    if options.password {
        return Some(query.to_string());
    }
    if let Some(item) = selected.and_then(|index| items.get(index)) {
        return Some(item.clone());
    }
    (!query.is_empty()).then(|| query.to_string())
}

/// Whether there is anything to show at all. A password prompt needs no
/// items; a list without items is cancelled at once (exit 1), like rofi.
pub fn has_input(items: &[String], options: &DmenuOptions) -> bool {
    options.password || !items.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_string()).collect()
    }

    fn items(list: &[&str]) -> Vec<String> {
        list.iter().map(|item| (*item).to_string()).collect()
    }

    #[test]
    fn dmenu_flags_are_parsed() {
        assert_eq!(parse_dmenu_args(&args(&["zeshicast-gtk"])), None);
        assert_eq!(
            parse_dmenu_args(&args(&["zeshicast-gtk", "--dmenu"])),
            Some(DmenuOptions::default())
        );
        let options = parse_dmenu_args(&args(&[
            "zeshicast-gtk",
            "--dmenu",
            "-p",
            "Pick a branch",
            "--password",
        ]))
        .expect("dmenu mode");
        assert_eq!(options.prompt.as_deref(), Some("Pick a branch"));
        assert!(options.password);
        let options = parse_dmenu_args(&args(&["zeshicast-gtk", "--dmenu", "--prompt=Run"]))
            .expect("dmenu mode");
        assert_eq!(options.prompt.as_deref(), Some("Run"));
    }

    #[test]
    fn input_lines_are_split_trimmed_of_crlf_and_empty_lines_dropped() {
        let read = read_items(&b"alpha\r\n\nbeta\ngamma"[..]).expect("in-memory read");
        assert_eq!(read, items(&["alpha", "beta", "gamma"]));
        assert!(read_items(&b""[..]).expect("in-memory read").is_empty());
    }

    #[test]
    fn non_utf8_lines_are_skipped_not_mangled() {
        let read = read_items(&b"ok\n\xff\xfe broken\nalso ok\n"[..]).expect("in-memory read");
        assert_eq!(read, items(&["ok", "also ok"]));
    }

    #[test]
    fn empty_query_keeps_input_order_and_a_query_ranks_matches() {
        let list = items(&["zeta", "main", "feature/main-menu", "release"]);
        assert_eq!(filter_items(&list, ""), vec![0, 1, 2, 3]);
        assert_eq!(filter_items(&list, "main"), vec![1, 2]);
        assert!(filter_items(&list, "nothing-like-this").is_empty());
    }

    #[test]
    fn enter_prints_the_selection_or_the_typed_text() {
        let list = items(&["main", "develop"]);
        let plain = DmenuOptions::default();
        assert_eq!(
            enter_output(&list, Some(1), "dev", &plain).as_deref(),
            Some("develop")
        );
        // No matching row: the typed text, so scripts can take new values.
        assert_eq!(
            enter_output(&list, None, "hotfix", &plain).as_deref(),
            Some("hotfix")
        );
        assert_eq!(enter_output(&list, None, "", &plain), None);
        // A stale index never panics.
        assert_eq!(enter_output(&list, Some(9), "", &plain), None);
    }

    #[test]
    fn password_mode_prints_what_was_typed_and_needs_no_items() {
        let password = DmenuOptions {
            password: true,
            ..DmenuOptions::default()
        };
        assert_eq!(
            enter_output(&[], None, "s3cret", &password).as_deref(),
            Some("s3cret")
        );
        assert!(has_input(&[], &password));
        assert!(!has_input(&[], &DmenuOptions::default()));
    }
}

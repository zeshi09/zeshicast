//! What a clipboard change is worth keeping, decided from its MIME types
//! and the app it came from (Q6.4). Pure, so it is tested without a
//! compositor; `ui/clipboard_capture.rs` does the reading.

// Capture is the GUI daemon's; the headless build stores what it is given.
#![cfg_attr(not(feature = "gui"), allow(dead_code))]

/// Set by KeePassXC, KDE and others on a copied password
/// (`x-kde-passwordManagerHint: secret`): such a copy is never recorded.
pub(crate) const PASSWORD_HINT: &str = "x-kde-passwordManagerHint";

pub(crate) const HTML: &str = "text/html";
pub(crate) const URI_LIST: &str = "text/uri-list";
pub(crate) const PNG: &str = "image/png";

/// What to read for one clipboard change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Capture {
    /// Nothing: a password, an ignored app, or no type we keep.
    Skip(SkipReason),
    Image,
    /// Copied files: `text/uri-list`, shown and searched as their paths.
    Files,
    /// Rich text: the plain text plus its `text/html`.
    Html,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkipReason {
    Password,
    IgnoredApp,
    NoSupportedType,
}

fn is_text_type(mime: &str) -> bool {
    mime.starts_with("text/plain") || matches!(mime, "UTF8_STRING" | "STRING" | "TEXT")
}

/// Decide from the offered `types` and the focused app's id.
pub(crate) fn choose(types: &[String], app_id: Option<&str>, ignored_apps: &[String]) -> Capture {
    if types.iter().any(|mime| mime == PASSWORD_HINT) {
        return Capture::Skip(SkipReason::Password);
    }
    if app_id.is_some_and(|app| is_ignored_app(app, ignored_apps)) {
        return Capture::Skip(SkipReason::IgnoredApp);
    }
    let has = |wanted: &str| types.iter().any(|mime| mime == wanted);
    let has_text = types.iter().any(|mime| is_text_type(mime));
    if has(PNG) {
        Capture::Image
    } else if has(URI_LIST) {
        Capture::Files
    } else if has(HTML) && has_text {
        Capture::Html
    } else if has_text {
        Capture::Text
    } else {
        Capture::Skip(SkipReason::NoSupportedType)
    }
}

/// `clipboard_ignore_apps` entries match an app id exactly or by its last
/// dot-separated part, case-insensitively: `KeePassXC` covers
/// `org.keepassxc.KeePassXC`.
pub(crate) fn is_ignored_app(app_id: &str, ignored_apps: &[String]) -> bool {
    let app = app_id.trim().to_lowercase();
    if app.is_empty() {
        return false;
    }
    let last = app.rsplit('.').next().unwrap_or(&app).to_string();
    ignored_apps.iter().any(|entry| {
        let entry = entry.trim().to_lowercase();
        !entry.is_empty() && (entry == app || entry == last)
    })
}

/// The comma separated `clipboard_ignore_apps` preference.
pub(crate) fn parse_ignored_apps(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

/// Password managers ignored unless the user sets the list themselves.
pub(crate) const DEFAULT_IGNORED_APPS: &str =
    "KeePassXC, org.keepassxc.KeePassXC, Bitwarden, 1Password, com.bitwarden.desktop, Enpass";

/// Local paths of the `file://` URIs in a `text/uri-list`, decoded; other
/// URIs as they are. Comment lines (`#`) are skipped.
pub(crate) fn uri_list_paths(uri_list: &str) -> Vec<String> {
    uri_list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|uri| match uri.strip_prefix("file://") {
            // `file://host/path`: only the local form has an empty host.
            Some(rest) if rest.starts_with('/') => percent_decode(rest),
            _ => uri.to_string(),
        })
        .collect()
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    let hex = |byte: u8| (byte as char).to_digit(16);
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            out.push((high * 16 + low) as u8);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(list: &[&str]) -> Vec<String> {
        list.iter().map(|mime| mime.to_string()).collect()
    }

    #[test]
    fn the_richest_kept_format_is_chosen() {
        let none: [String; 0] = [];
        let pick = |list: &[&str]| choose(&types(list), Some("firefox"), &none);
        assert_eq!(pick(&["image/png", "text/plain"]), Capture::Image);
        assert_eq!(
            pick(&[
                "x-special/gnome-copied-files",
                "text/uri-list",
                "text/plain"
            ]),
            Capture::Files
        );
        assert_eq!(
            pick(&["text/html", "text/plain;charset=utf-8", "UTF8_STRING"]),
            Capture::Html
        );
        assert_eq!(
            pick(&["text/html"]),
            Capture::Skip(SkipReason::NoSupportedType)
        );
        assert_eq!(pick(&["UTF8_STRING", "STRING"]), Capture::Text);
        assert_eq!(
            pick(&["application/x-thing"]),
            Capture::Skip(SkipReason::NoSupportedType)
        );
    }

    #[test]
    fn passwords_and_ignored_apps_are_never_kept() {
        let ignored = parse_ignored_apps(DEFAULT_IGNORED_APPS);
        assert_eq!(
            choose(
                &types(&["text/plain", PASSWORD_HINT]),
                Some("anything"),
                &[]
            ),
            Capture::Skip(SkipReason::Password)
        );
        assert_eq!(
            choose(
                &types(&["text/plain"]),
                Some("org.keepassxc.KeePassXC"),
                &ignored
            ),
            Capture::Skip(SkipReason::IgnoredApp)
        );
        assert_eq!(
            choose(&types(&["text/plain"]), Some("Bitwarden"), &ignored),
            Capture::Skip(SkipReason::IgnoredApp)
        );
        assert!(is_ignored_app(
            "org.telegram.desktop",
            &parse_ignored_apps(" desktop , ")
        ));
        assert!(!is_ignored_app(
            "org.telegram.desktop",
            &parse_ignored_apps("telegram")
        ));
        assert_eq!(
            choose(&types(&["text/plain"]), None, &ignored),
            Capture::Text,
            "an unknown focused app is not a reason to drop a copy"
        );
    }

    #[test]
    fn a_uri_list_reads_as_local_paths() {
        assert_eq!(uri_list_paths("file:///tmp/%D1%84%"), vec!["/tmp/ф%"]);
        let list = "# copied\r\nfile:///home/me/My%20Report.pdf\r\nfile:///tmp/a%25b\r\n\
                    https://example.org/x\r\nfile://host/share/y\r\n";
        assert_eq!(
            uri_list_paths(list),
            vec![
                "/home/me/My Report.pdf",
                "/tmp/a%b",
                "https://example.org/x",
                "file://host/share/y"
            ]
        );
    }
}

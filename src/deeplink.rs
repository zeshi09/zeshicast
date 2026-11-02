//! `zeshicast://` deep links and command ids (Q1.5).
//!
//! Links can come from anywhere (a browser, a chat message), so a link never
//! runs anything by itself: `command/<id>` only opens the palette with that
//! command in the search, and the user presses Enter. Running directly is
//! reserved for `zeshicast-gtk --cmd run <id>`, which only the user's own
//! shell or compositor binding can issue.

/// What a link asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepLink {
    /// `zeshicast://view/<name>`
    View { name: String },
    /// `zeshicast://search?q=…`
    Search { query: String },
    /// `zeshicast://command/<id>`, where the id itself is `<kind>/<name>`
    /// (see [`command_id`]): `zeshicast://command/script/backup-home`.
    Command { id: String },
}

pub const SCHEME: &str = "zeshicast://";

/// `true` for an argument that should be parsed as a deep link.
pub fn is_deep_link(arg: &str) -> bool {
    arg.get(..SCHEME.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(SCHEME))
}

/// Parse a `zeshicast://` link. Anything unexpected is an error, never a
/// guess: an unknown host, an extra path segment, a bad escape, `..`.
pub fn parse_deep_link(link: &str) -> Result<DeepLink, String> {
    if !is_deep_link(link) {
        return Err(format!("not a {SCHEME} link"));
    }
    let rest = &link[SCHEME.len()..];
    let rest = rest.split_once('#').map_or(rest, |(before, _)| before);
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let segments: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    let q = query_param(query, "q")?;

    match segments.as_slice() {
        ["view", name] => Ok(DeepLink::View {
            name: checked_segment(name)?,
        }),
        ["search"] => Ok(DeepLink::Search {
            query: q.unwrap_or_default(),
        }),
        ["command", kind, name] => Ok(DeepLink::Command {
            id: format!("{}/{}", checked_segment(kind)?, checked_segment(name)?),
        }),
        _ => Err(format!("unsupported link: {SCHEME}{path}")),
    }
}

fn checked_segment(raw: &str) -> Result<String, String> {
    let value = percent_decode(raw)?;
    if value.is_empty() || value == "." || value == ".." || value.contains('/') {
        return Err(format!("invalid link segment: {raw:?}"));
    }
    Ok(value)
}

/// The value of `name` in an `a=1&b=2` query string, decoded.
fn query_param(query: &str, name: &str) -> Result<Option<String>, String> {
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key == name {
            return percent_decode(&value.replace('+', " ")).map(Some);
        }
    }
    Ok(None)
}

fn percent_decode(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes
                .get(index + 1..index + 3)
                .and_then(|pair| std::str::from_utf8(pair).ok())
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or_else(|| format!("bad percent escape in {text:?}"))?;
            out.push(hex);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| format!("link is not UTF-8: {text:?}"))
}

/// Stable, readable id of a command, script or other runnable entry:
/// `<kind>/<slug of the name>`, e.g. `command/docker-ps`, `script/backup-home`.
pub fn command_id(kind: &str, name: &str) -> String {
    format!("{}/{}", slug(kind), slug(name))
}

/// A ready-to-paste key binding that runs `id` (Q1.6), in the syntax of
/// `compositor` (`niri`, `hyprland`, `sway`), or a plain shell command when
/// the compositor is unknown. The key is a placeholder to change. Ids are
/// slugs, so they need no escaping inside the quotes.
pub fn hotkey_binding(compositor: Option<&str>, id: &str) -> String {
    match compositor {
        Some("niri") => {
            format!("Mod+Shift+X {{ spawn \"zeshicast-gtk\" \"--cmd\" \"run\" \"{id}\"; }}")
        }
        Some("hyprland") => format!("bind = SUPER SHIFT, X, exec, zeshicast-gtk --cmd run {id}"),
        Some("sway") => format!("bindsym $mod+Shift+x exec zeshicast-gtk --cmd run {id}"),
        _ => format!("zeshicast-gtk --cmd run {id}"),
    }
}

fn slug(text: &str) -> String {
    let mut slug = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_search_and_command_links_parse() {
        assert_eq!(
            parse_deep_link("zeshicast://view/clipboard"),
            Ok(DeepLink::View {
                name: "clipboard".to_string()
            })
        );
        assert_eq!(
            parse_deep_link("zeshicast://view/system%20monitor/"),
            Ok(DeepLink::View {
                name: "system monitor".to_string()
            })
        );
        assert_eq!(
            parse_deep_link("ZESHICAST://search?q=hello+world&x=1#frag"),
            Ok(DeepLink::Search {
                query: "hello world".to_string()
            })
        );
        assert_eq!(
            parse_deep_link("zeshicast://command/script/backup-home"),
            Ok(DeepLink::Command {
                id: "script/backup-home".to_string()
            })
        );
        // The id round-trips through `command_id`.
        assert_eq!(command_id("Script", "Backup home"), "script/backup-home");
    }

    #[test]
    fn foreign_hosts_traversal_and_garbage_are_rejected() {
        for link in [
            "https://evil.example/view/clipboard",
            "zeshicast://",
            "zeshicast://run/rm",
            "zeshicast://view",
            "zeshicast://view/a/b",
            "zeshicast://view/..",
            "zeshicast://view/%2e%2e",
            "zeshicast://command/",
            "zeshicast://command/docker-ps",
            "zeshicast://command/command%2Fdocker-ps/x",
            "zeshicast://search?q=%zz",
            "zeshicast://search?q=%ff",
        ] {
            assert!(parse_deep_link(link).is_err(), "{link} must be rejected");
        }
    }

    #[test]
    fn deep_link_arguments_are_recognised_case_insensitively() {
        assert!(is_deep_link("zeshicast://view/clipboard"));
        assert!(is_deep_link("Zeshicast://search"));
        assert!(!is_deep_link("--view"));
        assert!(!is_deep_link("zeshi"));
    }

    #[test]
    fn hotkey_bindings_match_each_compositor_syntax() {
        let id = "command/weather";
        assert_eq!(
            hotkey_binding(Some("niri"), id),
            r#"Mod+Shift+X { spawn "zeshicast-gtk" "--cmd" "run" "command/weather"; }"#
        );
        assert_eq!(
            hotkey_binding(Some("hyprland"), id),
            "bind = SUPER SHIFT, X, exec, zeshicast-gtk --cmd run command/weather"
        );
        assert_eq!(
            hotkey_binding(Some("sway"), id),
            "bindsym $mod+Shift+x exec zeshicast-gtk --cmd run command/weather"
        );
        assert_eq!(
            hotkey_binding(None, id),
            "zeshicast-gtk --cmd run command/weather"
        );
        assert_eq!(
            hotkey_binding(Some("kwin"), id),
            "zeshicast-gtk --cmd run command/weather"
        );
    }

    #[test]
    fn command_ids_are_readable_slugs() {
        assert_eq!(command_id("Command", "Docker PS"), "command/docker-ps");
        assert_eq!(
            command_id("Script", "  Backup: home!! "),
            "script/backup-home"
        );
        assert_eq!(
            command_id("Command", "Погода в Москве"),
            "command/погода-в-москве"
        );
    }
}

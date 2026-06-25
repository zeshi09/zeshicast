//! Minimal Markdown → Pango-markup conversion for the AI message view.
//!
//! GtkLabel renders Pango markup, not Markdown, so we translate the common
//! subset LLMs emit: fenced/inline code, **bold**, *italic*, `# headings`,
//! and `- ` bullets. The output is always valid (balanced) markup with literal
//! text escaped, so `Label::set_markup` never rejects it.

/// Convert a Markdown string to Pango markup safe for `Label::set_markup`.
pub(crate) fn to_pango_markup(input: &str) -> String {
    let mut segments: Vec<String> = Vec::new();
    let mut fence: Option<Vec<String>> = None;

    for line in input.lines() {
        let trimmed = line.trim_start();

        if trimmed.starts_with("```") {
            match fence.take() {
                Some(buf) => segments.push(format!("<tt>{}</tt>", escape(&buf.join("\n")))),
                None => fence = Some(Vec::new()),
            }
            continue;
        }
        if let Some(buf) = fence.as_mut() {
            buf.push(line.to_string());
            continue;
        }

        if trimmed.is_empty() {
            segments.push(String::new());
        } else if let Some(rest) = heading_body(trimmed) {
            segments.push(format!("<big><b>{}</b></big>", inline(rest)));
        } else if let Some(rest) = bullet_body(trimmed) {
            segments.push(format!("•  {}", inline(rest)));
        } else {
            segments.push(inline(line));
        }
    }

    // An unclosed fence (still streaming, or malformed) — render what we have.
    if let Some(buf) = fence.take() {
        segments.push(format!("<tt>{}</tt>", escape(&buf.join("\n"))));
    }

    segments.join("\n")
}

fn heading_body(line: &str) -> Option<&str> {
    let rest = line.trim_start_matches('#');
    let hashes = line.len() - rest.len();
    if (1..=6).contains(&hashes) && rest.starts_with(' ') {
        Some(rest.trim_start())
    } else {
        None
    }
}

fn bullet_body(line: &str) -> Option<&str> {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return Some(rest);
        }
    }
    None
}

/// Inline formatting on a single line: pull out `code` spans first (so their
/// contents are never treated as emphasis), then apply emphasis to the rest.
fn inline(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;

    while let Some(start) = rest.find('`') {
        let (before, after) = rest.split_at(start);
        out.push_str(&emphasis(before));
        let after = &after[1..];
        match after.find('`') {
            Some(end) => {
                let (code, tail) = after.split_at(end);
                out.push_str(&format!("<tt>{}</tt>", escape(code)));
                rest = &tail[1..];
            }
            None => {
                // No closing backtick — emit it literally.
                out.push_str(&escape("`"));
                rest = after;
            }
        }
    }
    out.push_str(&emphasis(rest));
    out
}

/// Escape literal text, then apply **bold** and *italic*. Underscores are left
/// alone so identifiers like `snake_case` aren't mangled.
fn emphasis(text: &str) -> String {
    // Private-use sentinel: shields any leftover (unbalanced) `**` from the
    // single-`*` italic pass, which would otherwise read the two stars as a pair.
    const STAR_GUARD: char = '\u{E000}';
    let escaped = escape(text);
    let bold = wrap_pairs(&escaped, "**", "<b>", "</b>");
    let guarded = bold.replace("**", &STAR_GUARD.to_string());
    let italic = wrap_pairs(&guarded, "*", "<i>", "</i>");
    italic.replace(STAR_GUARD, "**")
}

/// Replace an *even* number of `marker` occurrences with alternating open/close
/// tags, leaving any unbalanced trailing marker literal — so the result is
/// always valid markup.
fn wrap_pairs(text: &str, marker: &str, open: &str, close: &str) -> String {
    let total = text.matches(marker).count();
    if total < 2 {
        return text.to_string();
    }
    let usable = total - (total % 2);

    let mut out = String::new();
    let mut rest = text;
    let mut replaced = 0;
    while replaced < usable {
        let Some(pos) = rest.find(marker) else { break };
        out.push_str(&rest[..pos]);
        out.push_str(if replaced % 2 == 0 { open } else { close });
        rest = &rest[pos + marker.len()..];
        replaced += 1;
    }
    out.push_str(rest);
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_literal_markup() {
        assert_eq!(to_pango_markup("a < b & c"), "a &lt; b &amp; c");
    }

    #[test]
    fn bold_and_italic() {
        assert_eq!(to_pango_markup("**hi** and *there*"), "<b>hi</b> and <i>there</i>");
    }

    #[test]
    fn unbalanced_markers_stay_literal() {
        assert_eq!(to_pango_markup("a ** b"), "a ** b");
        assert_eq!(to_pango_markup("rate is 5 * x"), "rate is 5 * x");
    }

    #[test]
    fn underscores_are_left_alone() {
        assert_eq!(to_pango_markup("call my_func_name"), "call my_func_name");
    }

    #[test]
    fn inline_code_is_monospace_and_escaped() {
        assert_eq!(to_pango_markup("use `a<b>`"), "use <tt>a&lt;b&gt;</tt>");
        // Emphasis markers inside code are literal.
        assert_eq!(to_pango_markup("`**x**`"), "<tt>**x**</tt>");
    }

    #[test]
    fn fenced_code_block() {
        let md = "before\n```\nlet x = 1 < 2;\n```\nafter";
        assert_eq!(
            to_pango_markup(md),
            "before\n<tt>let x = 1 &lt; 2;</tt>\nafter"
        );
    }

    #[test]
    fn headings_and_bullets() {
        assert_eq!(to_pango_markup("## Title"), "<big><b>Title</b></big>");
        assert_eq!(to_pango_markup("- item one"), "•  item one");
    }
}

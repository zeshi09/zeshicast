use std::borrow::Cow;
use std::collections::HashMap;
use std::time::SystemTime;

use chrono::{DateTime, Local};

use crate::search::calculator::{Calculator, format_number};

#[derive(Debug, Clone)]
pub(crate) struct PlaceholderContext<'a> {
    pub(crate) query: String,
    pub(crate) clipboard: String,
    pub(crate) args: HashMap<String, String>,
    /// Borrowed from the launcher preferences on the per-keystroke search path
    /// so building a context never clones the whole map; owned where the map is
    /// already available by value (form submission, merged command prefs).
    pub(crate) preferences: Cow<'a, HashMap<String, String>>,
    pub(crate) now: SystemTime,
}

impl<'a> PlaceholderContext<'a> {
    pub(crate) fn new(query: &str, clipboard: Option<&String>) -> Self {
        Self {
            query: query.to_string(),
            clipboard: clipboard.cloned().unwrap_or_default(),
            args: HashMap::new(),
            preferences: Cow::Owned(HashMap::new()),
            now: SystemTime::now(),
        }
    }

    /// Attach launcher preferences by reference: the context borrows them for
    /// the duration of the search instead of cloning the map.
    pub(crate) fn with_preferences(mut self, preferences: &'a HashMap<String, String>) -> Self {
        self.preferences = Cow::Borrowed(preferences);
        self
    }
}

/// Expand placeholders verbatim. Use for non-shell contexts (URLs, snippets,
/// environment values).
pub(crate) fn expand_placeholders(template: &str, context: &PlaceholderContext<'_>) -> String {
    expand(template, context, false)
}

/// Expand placeholders for a string that will be passed to `sh -c`. Substituted
/// values (query, clipboard, arg, pref, …) are POSIX shell-quoted so untrusted
/// input (e.g. clipboard containing `$(rm -rf ~)` or `; reboot`) cannot break
/// out into command execution. Command authors therefore must NOT add their own
/// quotes around placeholders — the quoting is supplied here.
pub(crate) fn expand_placeholders_shell(
    template: &str,
    context: &PlaceholderContext<'_>,
) -> String {
    expand(template, context, true)
}

fn expand(template: &str, context: &PlaceholderContext<'_>, shell_escape: bool) -> String {
    let mut output = String::new();
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        let (before, after_start) = rest.split_at(start);
        output.push_str(before);

        let after_start = &after_start[2..];
        let Some(end) = after_start.find("}}") else {
            output.push_str("{{");
            output.push_str(after_start);
            return output;
        };

        let (placeholder, after_end) = after_start.split_at(end);

        // Where the substituted value lands decides how it must be escaped. A
        // template author may wrap a placeholder in their own quotes; the three
        // quoting contexts (unquoted, inside `'…'`, inside `"…"`) are safe
        // because the value is escaped for the context it is inserted into.
        // One scan per placeholder reports both the quoting context and the
        // here-document body the text ends inside of. Closed bodies are skipped
        // whole, so a quote inside one cannot misclassify a later placeholder.
        let (shell_context, heredoc) = if shell_escape {
            let scan = scan_shell(&output);
            (scan.context, scan.open_heredoc)
        } else {
            (ShellContext::Unquoted, None)
        };

        // A here-document body is not a quoting context: quotes are literal
        // there while `$`, backticks and backslashes keep working unless the
        // delimiter was quoted. Checked before the comment rule, because a `#`
        // inside a body is just text (P1.3b).
        if shell_escape && let Some(heredoc) = heredoc {
            match render_placeholder(placeholder.trim(), context) {
                Some(value) if heredoc_can_hold(&heredoc, &value) => {
                    output.push_str(&heredoc_escape(&value, heredoc.quoted));
                }
                // Unknown placeholder, or a value that contains the delimiter
                // line: no escaping can keep the body intact, so the template is
                // emitted unchanged rather than silently truncating it.
                _ => output.push_str(&format!("{{{{{}}}}}", placeholder.trim())),
            }
            rest = &after_end[2..];
            continue;
        }

        // A placeholder inside a shell comment is never code: no escaping can
        // make a substituted value safe there (a newline in the value ends the
        // comment and whatever follows becomes a command), so it is emitted
        // literally instead.
        if shell_context == ShellContext::Comment {
            output.push_str(&format!("{{{{{}}}}}", placeholder.trim()));
            rest = &after_end[2..];
            continue;
        }

        match render_placeholder(placeholder.trim(), context) {
            Some(value) if shell_escape => {
                // An odd run of trailing backslashes means the author's last
                // backslash escapes whatever comes next — which is now the
                // first byte of our escape sequence, not the value. Emit one
                // more backslash so the author's stays self-contained.
                if shell_context != ShellContext::Single && trailing_backslash_run(&output) % 2 == 1
                {
                    output.push('\\');
                }
                output.push_str(&escape_for_context(&value, shell_context));
            }
            Some(value) => output.push_str(&value),
            // Unknown placeholder: emit it literally, unquoted.
            None => output.push_str(&format!("{{{{{}}}}}", placeholder.trim())),
        }
        rest = &after_end[2..];
    }

    output.push_str(rest);
    output
}

/// POSIX single-quote escaping: wrap in `'…'`, turning embedded `'` into `'\''`.
fn shell_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

/// Which shell quoting context the text ends in, following POSIX lexing rules:
/// inside single quotes every character is literal (a `"` does NOT open a
/// double-quoted segment); inside double quotes a single quote is literal and
/// does NOT open a single-quoted segment; a backslash escapes the following
/// character outside single quotes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellContext {
    /// Outside any quotes: values need full `'…'` wrapping.
    Unquoted,
    /// Inside an open `'…'` run: everything is literal except `'`.
    Single,
    /// Inside an open `"…"` run: `$`, backtick, `\` and `"` stay special.
    Double,
    /// Inside a `#` comment: the rest of the line is not shell code at all.
    Comment,
}

/// Count the run of backslashes at the very end of `text`.
fn trailing_backslash_run(text: &str) -> usize {
    text.chars().rev().take_while(|ch| *ch == '\\').count()
}

/// `#` starts a comment only at the start of a word (POSIX). Being precise here
/// matters: an over-eager match would silently skip legitimate substitutions.
fn starts_comment(previous: Option<char>) -> bool {
    match previous {
        None => true,
        Some(ch) => ch.is_whitespace() || matches!(ch, ';' | '|' | '&' | '(' | ')' | '<' | '>'),
    }
}

/// A here-document whose body the scanned text ends inside of.
struct OpenHeredoc {
    delimiter: String,
    /// The delimiter was quoted (`<<'EOF'`), so nothing expands in the body.
    quoted: bool,
    /// `<<-EOF`: leading tabs are stripped from the terminating line.
    strip_tabs: bool,
}

/// What one pass over shell text ends in.
struct ShellScan {
    context: ShellContext,
    open_heredoc: Option<OpenHeredoc>,
}

/// One pass over shell text that follows the same lexing rules for quotes,
/// comments, escapes and here-document bodies (P1.3b).
///
/// A here-document body is not a quoting context: quotes are literal there while
/// `$`, backticks and backslashes keep working unless the delimiter was quoted.
/// Escaping for it therefore means something different from the three quoting
/// contexts, so an unclosed body is reported in
/// [`ShellScan::open_heredoc`] instead of a context.
///
/// Closed bodies are skipped whole, so a quote inside one is literal and cannot
/// misclassify a later placeholder. Deliberately conservative: when the scan
/// cannot pair bodies with their operators (more than one here-document on the
/// same line) it reports an *expanding* body, because escaping there is safe
/// and not escaping is not.
fn scan_shell(text: &str) -> ShellScan {
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0usize;
    let mut single = false;
    let mut double = false;
    let mut comment = false;
    let mut open_heredoc = None;

    while index < chars.len() {
        let ch = chars[index];

        if comment {
            if ch == '\n' {
                comment = false;
            }
            index += 1;
            continue;
        }
        if single {
            if ch == '\'' {
                single = false;
            }
            index += 1;
            continue;
        }
        if ch == '\\' {
            // A backslash escapes the next character, so a `<<` after one is not
            // an operator.
            index += 2;
            continue;
        }
        if ch == '\'' && !double {
            single = true;
            index += 1;
            continue;
        }
        if ch == '"' {
            double = !double;
            index += 1;
            continue;
        }
        if ch == '#' && !double && starts_comment(index.checked_sub(1).map(|i| chars[i])) {
            comment = true;
            index += 1;
            continue;
        }

        if ch == '<' && chars.get(index + 1) == Some(&'<') && chars.get(index + 2) != Some(&'<') {
            let operator = index + 2;
            let strip_tabs = chars.get(operator) == Some(&'-');
            let start = if strip_tabs { operator + 1 } else { operator };

            let Some((delimiter, quoted, after_delimiter)) = parse_heredoc_delimiter(&chars, start)
            else {
                index += 2;
                continue;
            };

            let line_rest: String = chars[after_delimiter..]
                .iter()
                .take_while(|c| **c != '\n')
                .collect();
            let multiple = line_rest.contains("<<");

            let Some(line_break) = chars[after_delimiter..].iter().position(|c| *c == '\n') else {
                // The body has not started: the cursor is still on the command
                // line, where normal quoting rules apply.
                index += 1;
                continue;
            };
            let body_start = after_delimiter + line_break + 1;

            match heredoc_body_end(&chars, body_start, &delimiter, strip_tabs) {
                Some(after_body) => {
                    index = after_body;
                    continue;
                }
                None => {
                    open_heredoc = Some(OpenHeredoc {
                        delimiter,
                        quoted: quoted && !multiple,
                        strip_tabs,
                    });
                    break;
                }
            }
        }

        index += 1;
    }

    let context = if comment {
        ShellContext::Comment
    } else if single {
        ShellContext::Single
    } else if double {
        ShellContext::Double
    } else {
        ShellContext::Unquoted
    };
    ShellScan {
        context,
        open_heredoc,
    }
}

/// The here-document body `text` ends inside of, if any (P1.3b).
#[cfg(test)]
fn open_heredoc(text: &str) -> Option<OpenHeredoc> {
    scan_shell(text).open_heredoc
}

/// Read a here-document delimiter, honouring a quoted form and backslash
/// escapes. Returns the delimiter, whether it was quoted, and where it ended.
fn parse_heredoc_delimiter(chars: &[char], start: usize) -> Option<(String, bool, usize)> {
    let mut index = start;
    while matches!(chars.get(index), Some(c) if c.is_whitespace()) {
        index += 1;
    }

    let mut delimiter = String::new();
    let mut quoted = false;

    match chars.get(index) {
        Some('\'') | Some('"') => {
            let quote = chars[index];
            quoted = true;
            index += 1;
            while let Some(&ch) = chars.get(index) {
                index += 1;
                if ch == quote {
                    break;
                }
                delimiter.push(ch);
            }
        }
        Some(_) => {
            while let Some(&ch) = chars.get(index) {
                if ch.is_whitespace() || matches!(ch, ';' | '|' | '&' | '<' | '>' | '(' | ')') {
                    break;
                }
                index += 1;
                if ch == '\\' {
                    if let Some(&escaped) = chars.get(index) {
                        delimiter.push(escaped);
                        index += 1;
                    }
                    continue;
                }
                delimiter.push(ch);
            }
        }
        None => return None,
    }

    if delimiter.is_empty() {
        return None;
    }
    Some((delimiter, quoted, index))
}

/// Where the here-document body ends, if it does: the index just past the line
/// that holds nothing but the delimiter.
fn heredoc_body_end(
    chars: &[char],
    body_start: usize,
    delimiter: &str,
    strip_tabs: bool,
) -> Option<usize> {
    let mut index = body_start;

    while index <= chars.len() {
        let line_end = chars[index..]
            .iter()
            .position(|c| *c == '\n')
            .map(|offset| index + offset)
            .unwrap_or(chars.len());
        let line: String = chars[index..line_end].iter().collect();
        let candidate = if strip_tabs {
            line.trim_start_matches('\t')
        } else {
            line.as_str()
        };

        if candidate == delimiter {
            return Some((line_end + 1).min(chars.len()));
        }
        if line_end >= chars.len() {
            return None;
        }
        index = line_end + 1;
    }

    None
}

/// Escape `value` for a here-document body: a backslash escapes `\`, `$` and a
/// backtick there. Quotes are literal in a body, so they are left alone.
fn heredoc_escape(value: &str, quoted_delimiter: bool) -> String {
    if quoted_delimiter {
        // Nothing expands in such a body, so the value goes in as it is.
        return value.to_string();
    }

    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(ch, '\\' | '$' | '`') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

/// Whether a value can be inserted into this body at all.
///
/// A value containing a line equal to the delimiter would end the here-document
/// and turn the rest of it into commands, and no escaping prevents that.
fn heredoc_can_hold(heredoc: &OpenHeredoc, value: &str) -> bool {
    !value.lines().any(|line| {
        let line = if heredoc.strip_tabs {
            line.trim_start_matches('\t')
        } else {
            line
        };
        line == heredoc.delimiter
    })
}

/// The quoting context `text` ends in. Test-only: expansion uses [`scan_shell`]
/// directly so one scan answers both questions.
#[cfg(test)]
fn shell_context(text: &str) -> ShellContext {
    scan_shell(text).context
}

/// Escape `value` for the context it is about to be inserted into.
fn escape_for_context(value: &str, context: ShellContext) -> String {
    match context {
        ShellContext::Unquoted => shell_quote(value),
        ShellContext::Double => double_quote_escape(value),
        // Already inside an open single-quoted run: only an embedded apostrophe
        // needs the POSIX `'\''` dance. Wrapping the value in quotes here would
        // close the author's run early.
        ShellContext::Single => value.replace('\'', "'\\''"),
        // Never reached: `expand` emits the placeholder literally when it sits
        // inside a comment instead of substituting a value there.
        ShellContext::Comment => value.to_string(),
    }
}

/// Escape a value for interpolation inside an open double-quoted string in a
/// POSIX shell: backslash-escape `\`, `"`, `$` and `` ` `` so the substituted
/// text is treated literally (no command substitution, no parameter expansion,
/// no premature end of the quoted string). Unlike [`shell_quote`] the value is
/// NOT wrapped in additional quotes — the author's own `"…"` remain in place.
fn double_quote_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(ch, '\\' | '"' | '$' | '`') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Thin wrapper kept for the scanner regression tests.
#[cfg(test)]
fn is_inside_unclosed_double_quotes(text: &str) -> bool {
    shell_context(text) == ShellContext::Double
}

/// Resolve a placeholder to its value, or `None` if the name is unknown.
fn render_placeholder(placeholder: &str, context: &PlaceholderContext<'_>) -> Option<String> {
    let (name, argument) = placeholder
        .split_once(':')
        .map(|(name, argument)| (name.trim(), Some(argument.trim())))
        .unwrap_or((placeholder, None));

    let value = match name {
        "query" => context.query.clone(),
        "clipboard" => context.clipboard.clone(),
        "arg" => argument
            .and_then(|name| context.args.get(name))
            .cloned()
            .unwrap_or_default(),
        "pref" => argument
            .and_then(|name| context.preferences.get(name))
            .cloned()
            .unwrap_or_default(),
        "date" => format_local_time(context.now, argument.unwrap_or("%Y-%m-%d")),
        "time" => format_local_time(context.now, argument.unwrap_or("%H:%M:%S")),
        "datetime" | "timestamp" => {
            format_local_time(context.now, argument.unwrap_or("%Y-%m-%d %H:%M:%S"))
        }
        "calc" => argument
            .and_then(|expr| Calculator::new(expr).parse().ok())
            .map(format_number)
            .unwrap_or_default(),
        "uuid" => generate_simple_uuid(),
        _ => return None,
    };
    Some(value)
}

fn generate_simple_uuid() -> String {
    use std::time::UNIX_EPOCH;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p1 = (nanos & 0xFFFFFFFF) as u32;
    let p2 = ((nanos >> 32) & 0xFFFF) as u16;
    let p3 = (((nanos >> 48) & 0x0FFF) | 0x4000) as u16;
    let p4 = (((nanos >> 60) & 0x3FFF) | 0x8000) as u16;
    let p5 = ((nanos >> 72) & 0xFFFFFFFFFFFF) as u64;
    format!("{p1:08x}-{p2:04x}-{p3:04x}-{p4:04x}-{p5:012x}")
}

pub(crate) fn format_local_time(time: SystemTime, format: &str) -> String {
    DateTime::<Local>::from(time).format(format).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redundant_single_quotes_in_shell_placeholders() {
        let context = PlaceholderContext::new("foo bar", None);
        let result = expand_placeholders_shell("echo '{{query}}'", &context);
        assert_eq!(result, "echo 'foo bar'");
    }

    #[test]
    fn single_quote_wrapper_still_collapses() {
        let context = PlaceholderContext::new("foo bar", None);
        let result = expand_placeholders_shell("echo '{{query}}'", &context);
        assert_eq!(result, "echo 'foo bar'");
    }

    /// B-2: a placeholder inside an *already open* single-quoted run must be
    /// inserted literally (only an apostrophe is escaped), so command
    /// substitution in the value never reaches the shell.
    #[test]
    fn placeholder_inside_open_single_quote_run_is_literal() {
        let ctx = PlaceholderContext::new("", Some(&"$(touch /tmp/z)".to_string()));
        assert_eq!(
            expand_placeholders_shell("xdotool type 'clip: {{clipboard}}'", &ctx),
            "xdotool type 'clip: $(touch /tmp/z)'"
        );
    }

    /// The author's closing quote must survive: the value is inserted into the
    /// open run instead of consuming the template's tail.
    #[test]
    fn apostrophe_value_in_single_quote_run_round_trips() {
        let ctx = PlaceholderContext::new("", Some(&"it's".to_string()));
        let expanded = expand_placeholders_shell("printf '%s' '{{clipboard}}'", &ctx);
        assert_eq!(expanded, "printf '%s' 'it'\\''s'");
    }

    #[test]
    fn shell_context_scanner_reports_all_three_states() {
        assert_eq!(shell_context("echo "), ShellContext::Unquoted);
        assert_eq!(shell_context("echo \"closed\" "), ShellContext::Unquoted);
        assert_eq!(shell_context("echo 'open"), ShellContext::Single);
        assert_eq!(shell_context("echo 'it\"s open"), ShellContext::Single);
        assert_eq!(shell_context("echo \"open"), ShellContext::Double);
        assert_eq!(shell_context("echo \"it's open"), ShellContext::Double);
        assert_eq!(shell_context("echo ok # note "), ShellContext::Comment);
        assert_eq!(
            shell_context("echo ok # note\necho "),
            ShellContext::Unquoted
        );
        // `#` is only a comment at the start of a word.
        assert_eq!(shell_context("echo a#b "), ShellContext::Unquoted);
        assert_eq!(
            shell_context("echo \"#not a comment\" "),
            ShellContext::Unquoted
        );
    }

    /// A template that puts a placeholder inside a `#` comment cannot be made
    /// safe by escaping (a newline in the value ends the comment), so the
    /// placeholder is emitted literally and the value is never substituted.
    #[test]
    fn placeholder_inside_shell_comment_is_not_substituted() {
        let marker = std::env::temp_dir().join(format!("zeshicast-comment-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let ctx = PlaceholderContext::new("", Some(&format!("\ntouch {}\n#", marker.display())));

        let expanded = expand_placeholders_shell("echo ok # {{clipboard}}", &ctx);
        assert_eq!(expanded, "echo ok # {{clipboard}}");

        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(&expanded)
            .output()
            .expect("sh is available");
        assert!(!marker.exists(), "value escaped the comment: {expanded:?}");
    }

    /// An author-written backslash right before a placeholder must not eat the
    /// first byte of our escape sequence: otherwise `"C:\{{x}}"` becomes
    /// `"C:\\$(...)"` where `\\` is a literal backslash and `$(` is live
    /// command substitution again.
    #[test]
    fn shell_expansion_is_safe_after_a_dangling_author_backslash() {
        let marker =
            std::env::temp_dir().join(format!("zeshicast-backslash-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let payload = format!("$(touch {})", marker.display());
        let ctx = PlaceholderContext::new("", Some(&payload));

        let in_double_quotes =
            expand_placeholders_shell("printf '%s\\n' \"C:\\{{clipboard}}\"", &ctx);
        let unquoted = expand_placeholders_shell("printf '%s\\n' \\{{clipboard}}", &ctx);

        for expanded in [&in_double_quotes, &unquoted] {
            let _ = std::process::Command::new("sh")
                .arg("-c")
                .arg(expanded)
                .output()
                .expect("sh is available");
        }
        assert!(
            !marker.exists(),
            "author backslash escaped the value's quoting: {in_double_quotes:?} / {unquoted:?}"
        );
    }

    /// Round-trip guard for B-2: whatever the template context, the expanded
    /// text must reach `sh -c` as exactly one word equal to the original value
    /// — no command substitution, no extra words, no injection.
    #[test]
    fn shell_expansion_round_trips_evil_values_in_every_context() {
        let evil = [
            "$(touch /tmp/zeshicast-pwned)",
            "`touch /tmp/zeshicast-pwned`",
            "; rm -rf /",
            "it's",
            "\"quoted\"",
            "back\\slash",
            "line\nbreak",
            "a'b\"c$d`e\\f",
        ];
        let templates = [
            "printf '%s' {{clipboard}}",
            "printf '%s' '{{clipboard}}'",
            "printf '%s' \"{{clipboard}}\"",
        ];

        for value in evil {
            for template in templates {
                let ctx = PlaceholderContext::new("", Some(&value.to_string()));
                let expanded = expand_placeholders_shell(template, &ctx);
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&expanded)
                    .output()
                    .expect("sh is available");
                assert_eq!(
                    String::from_utf8_lossy(&output.stdout),
                    value,
                    "value {value:?} escaped into {expanded:?}"
                );
            }
        }
    }

    /// Template authors must not quote placeholders themselves, but if they
    /// wrap one in double quotes the value is interpolated inside that quoted
    /// string. There, single-quote escaping would be inert (the quotes become
    /// literal) while `$()`, backticks and `$VAR` would still be expanded by
    /// the shell. Instead, the value gets double-quote-safe escaping: `\`,
    /// `"`, `$` and `` ` `` are backslash-escaped and no extra quoting is
    /// added — the author's own `"…"` remain in place.
    #[test]
    fn test_double_quoted_placeholder_is_escaped_for_double_quote_context() {
        let context = PlaceholderContext::new("$(reboot)", None);
        let result = expand_placeholders_shell("echo \"{{query}}\"", &context);
        assert_eq!(result, "echo \"\\$(reboot)\"");
    }

    #[test]
    fn test_double_quoted_placeholder_escapes_all_specials() {
        let context = PlaceholderContext::new("a\"b$c`d\\e", None);
        let result = expand_placeholders_shell("cmd \"{{query}}\"", &context);
        assert_eq!(result, "cmd \"a\\\"b\\$c\\`d\\\\e\"");
    }

    /// Regression: placeholders without author-supplied quotes still get the
    /// regular single-quote wrapping.
    #[test]
    fn test_unquoted_placeholder_still_single_quoted() {
        let context = PlaceholderContext::new("$(reboot)", None);
        let result = expand_placeholders_shell("echo {{query}}", &context);
        assert_eq!(result, "echo '$(reboot)'");
    }

    /// Regression: explicit single-quote wrappers keep their collapsing
    /// behaviour.
    #[test]
    fn test_single_quoted_placeholder_with_specials_still_collapsed() {
        let context = PlaceholderContext::new("$(reboot)", None);
        let result = expand_placeholders_shell("echo '{{query}}'", &context);
        assert_eq!(result, "echo '$(reboot)'");
    }

    /// Regression (validator FINDING 1): a single quote in an already
    /// substituted value must NOT flip the scanner into single-quote state when
    /// the insertion point is inside double quotes (' is literal there). Before
    /// the fix, `notify-send "{{query}}" {{clipboard}}` with query=`it's` made
    /// the scanner lose track of the closing `"`, so {{clipboard}} was treated
    /// as double-quoted and inserted without single-quote wrapping, letting
    /// `hello; touch /tmp/pwned` execute under `sh -c`.
    #[test]
    fn test_apostrophe_in_double_quoted_value_does_not_poison_later_placeholders() {
        let context = PlaceholderContext::new("it's", Some(&"hello; touch /tmp/pwned".to_string()));
        let result = expand_placeholders_shell("notify-send \"{{query}}\" {{clipboard}}", &context);
        assert_eq!(result, "notify-send \"it's\" 'hello; touch /tmp/pwned'");
    }

    /// Regression (validator FINDING 2): a nested `'{{query}}'` wrapper inside
    /// unclosed double quotes must not collapse into single-quote wrapping —
    /// inside double quotes those single quotes are literal, so `'$(reboot)'
    /// would still execute. Instead, the raw value is double-quote-escaped and
    /// the author's single quotes remain around it as literal characters.
    #[test]
    fn test_nested_single_quote_wrapper_inside_double_quotes_uses_dq_escape() {
        let context = PlaceholderContext::new("$(reboot)", None);
        let result = expand_placeholders_shell("cmd \"'{{query}}'\"", &context);
        assert_eq!(result, "cmd \"'\\$(reboot)'\"");
    }

    /// Same contract for a later placeholder: the first value closes its double
    /// quote normally, then a nested single-quoted placeholder inside a fresh
    /// double-quoted segment stays escaped too.
    #[test]
    fn test_nested_wrapper_in_second_quoted_segment() {
        let context = PlaceholderContext::new("$(reboot)", None);
        let result = expand_placeholders_shell("cmd \"{{query}} x '{{query}}' y\"", &context);
        assert_eq!(result, "cmd \"\\$(reboot) x '\\$(reboot)' y\"");
    }

    /// Regression: an apostrophe inside a substituted value must be escaped
    /// via the POSIX `'\''` sequence so it cannot terminate the quoted token
    /// early and smuggle in extra shell commands (`sh -c` contract).
    #[test]
    fn test_apostrophe_in_value_uses_posix_quote_escape() {
        let context = PlaceholderContext::new("", Some(&"a'b".to_string()));
        let result = expand_placeholders_shell("echo {{clipboard}}", &context);
        assert_eq!(result, "echo 'a'\\''b'");
    }

    #[test]
    fn test_inside_unclosed_double_quotes_scanner() {
        assert!(is_inside_unclosed_double_quotes("echo \"foo"));
        assert!(!is_inside_unclosed_double_quotes("echo \"foo\" bar"));
        // Double quotes inside single-quoted segments are literal.
        assert!(!is_inside_unclosed_double_quotes("echo '\"' foo"));
        // Single quotes inside double-quoted segments are literal.
        assert!(is_inside_unclosed_double_quotes("echo \"it's me"));
        // Backslash escapes the next character, so it cannot flip state.
        assert!(is_inside_unclosed_double_quotes("echo \"a\\\" b"));
        // An escaped double quote (shell text `\"`) does not close the quote.
        assert!(!is_inside_unclosed_double_quotes("echo \\\" foo"));
        // Regression (FINDING 1): an apostrophe inside double quotes does not
        // open a single-quoted segment, so a following `"` really closes it.
        assert!(!is_inside_unclosed_double_quotes("notify-send \"it's\" "));
    }

    #[test]
    fn test_uuid_placeholder_expansion() {
        let context = PlaceholderContext::new("", None);
        let result = expand_placeholders("id={{uuid}}", &context);
        assert!(result.starts_with("id="));
        assert_eq!(result.len(), 3 + 36);
        assert_eq!(&result[11..12], "-");
        assert_eq!(&result[16..17], "-");
    }

    #[test]
    fn a_placeholder_in_a_heredoc_body_is_escaped_for_that_body() {
        let ctx = PlaceholderContext::new("", Some(&"$(touch /tmp/pwned)".to_string()));
        let expanded = expand_placeholders_shell("cat <<EOF\n{{clipboard}}\nEOF\n", &ctx);

        assert_eq!(expanded, "cat <<EOF\n\\$(touch /tmp/pwned)\nEOF\n");
    }

    #[test]
    fn backslashes_dollars_and_backticks_are_escaped_in_a_body() {
        let ctx = PlaceholderContext::new("", Some(&"a\\b $HOME `id`".to_string()));
        let expanded = expand_placeholders_shell("cat <<-EOF\n\t{{clipboard}}\n\tEOF\n", &ctx);

        assert_eq!(expanded, "cat <<-EOF\n\ta\\\\b \\$HOME \\`id\\`\n\tEOF\n");
    }

    #[test]
    fn a_quoted_delimiter_keeps_the_body_literal() {
        let ctx = PlaceholderContext::new("", Some(&"$(touch /tmp/pwned)".to_string()));
        // `<<'EOF'`: nothing expands in such a body, so quoting the value would
        // change it.
        let expanded = expand_placeholders_shell("cat <<'EOF'\n{{clipboard}}\nEOF\n", &ctx);

        assert_eq!(expanded, "cat <<'EOF'\n$(touch /tmp/pwned)\nEOF\n");
    }

    #[test]
    fn a_value_that_would_end_the_body_is_not_substituted() {
        let ctx = PlaceholderContext::new("", Some(&"ok\nEOF\nrm -rf /".to_string()));
        let expanded = expand_placeholders_shell("cat <<EOF\n{{clipboard}}\nEOF\n", &ctx);

        assert_eq!(
            expanded, "cat <<EOF\n{{clipboard}}\nEOF\n",
            "a value that ends the here-document must not be inserted"
        );
    }

    #[test]
    fn a_placeholder_on_the_command_line_is_not_a_body() {
        let ctx = PlaceholderContext::new("", Some(&"$(x)".to_string()));
        let expanded = expand_placeholders_shell("cat <<EOF | tee {{clipboard}}", &ctx);

        assert_eq!(expanded, "cat <<EOF | tee '$(x)'");
    }

    #[test]
    fn two_heredocs_on_one_line_are_treated_as_expanding() {
        // Bodies arrive in order and this scan does not pair them with their
        // operators, so it takes the safe direction: escape.
        let ctx = PlaceholderContext::new("", Some(&"$(x)".to_string()));
        let expanded = expand_placeholders_shell("cat <<'A' <<B\n{{clipboard}}\n", &ctx);

        assert!(expanded.contains("\\$(x)"), "got {expanded}");
    }

    #[test]
    fn open_heredoc_reports_the_body_it_ends_inside() {
        assert!(open_heredoc("cat <<EOF\n").is_some(), "body is open");
        assert!(open_heredoc("cat <<EOF\nbody\nEOF\n").is_none(), "closed");
        assert!(
            open_heredoc("cat <<EOF").is_none(),
            "still on the command line"
        );
        // `<< b` really is a here-document with the delimiter `b`; an operator
        // with nothing after it is not.
        let spaced = open_heredoc("cat << b\n").expect("an unquoted delimiter");
        assert_eq!(spaced.delimiter, "b");
        assert!(!spaced.quoted);
        assert!(open_heredoc("echo a <<\n").is_none(), "no delimiter word");
        assert!(
            open_heredoc("echo '<<EOF'\n").is_none(),
            "quoted, not an operator"
        );

        let quoted = open_heredoc("cat <<'EOF'\n").expect("open body");
        assert!(quoted.quoted, "a quoted delimiter means a literal body");
        let plain = open_heredoc("cat <<EOF\n").expect("open body");
        assert!(!plain.quoted);
        assert_eq!(plain.delimiter, "EOF");

        let stripped = open_heredoc("cat <<-EOF\n").expect("open body");
        assert!(stripped.strip_tabs);
    }

    /// P1: an apostrophe inside a *closed* here-document body is literal and
    /// must not flip the scanner into single-quote state for a later
    /// placeholder. Before the fix, the value after the body was inserted
    /// unquoted, so `$(…)` from the clipboard ran under `sh -c`.
    #[test]
    fn apostrophe_in_a_closed_heredoc_body_does_not_unquote_a_later_value() {
        let ctx = PlaceholderContext::new("", Some(&"$(touch /tmp/z)".to_string()));
        let expanded = expand_placeholders_shell("cat <<EOF\nit's\nEOF\necho {{clipboard}}", &ctx);
        assert_eq!(expanded, "cat <<EOF\nit's\nEOF\necho '$(touch /tmp/z)'");
    }

    /// P1, end to end: the expanded template must not create the marker file.
    #[test]
    fn heredoc_body_quote_does_not_let_a_value_execute() {
        let marker = std::env::temp_dir().join(format!("zeshicast-hd-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let ctx = PlaceholderContext::new("", Some(&format!("$(touch {})", marker.display())));
        let expanded = expand_placeholders_shell("cat <<EOF\nit's\nEOF\necho {{clipboard}}", &ctx);

        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(&expanded)
            .output();
        assert!(!marker.exists(), "value executed: {expanded}");
    }

    /// P2: author-written quotes next to a placeholder are literal parts of the
    /// template. The old collapse heuristic could pop a *closing* quote and
    /// leave the value unquoted. Both templates must keep the value literal.
    #[test]
    fn author_quotes_beside_a_placeholder_keep_the_value_quoted() {
        let ctx = PlaceholderContext::new("", Some(&"$(x)".to_string()));
        assert_eq!(
            expand_placeholders_shell("echo 'x'{{clipboard}}'y'", &ctx),
            "echo 'x''$(x)''y'"
        );
        assert_eq!(
            expand_placeholders_shell("printf ''{{clipboard}}''", &ctx),
            "printf '''$(x)'''"
        );
    }

    /// P2, end to end: neither template may let the value execute.
    #[test]
    fn quotes_beside_a_placeholder_do_not_let_a_value_execute() {
        let marker = std::env::temp_dir().join(format!("zeshicast-q-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let value = format!("$(touch {})", marker.display());
        let ctx = PlaceholderContext::new("", Some(&value));

        for template in ["echo 'x'{{clipboard}}'y'", "printf ''{{clipboard}}''"] {
            let expanded = expand_placeholders_shell(template, &ctx);
            let _ = std::process::Command::new("sh")
                .arg("-c")
                .arg(&expanded)
                .output();
            assert!(
                !marker.exists(),
                "value executed from {template}: {expanded}"
            );
        }
    }

    /// P2: in non-shell expansion the author's quotes are literal characters
    /// and must survive round-trip (the old heuristic dropped the closing one).
    #[test]
    fn non_shell_expansion_keeps_literal_quotes() {
        let ctx = PlaceholderContext::new("v", None);
        assert_eq!(expand_placeholders("x'{{query}}'y", &ctx), "x'v'y");
    }
}

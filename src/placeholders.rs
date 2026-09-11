use std::borrow::Cow;
use std::collections::HashMap;
use std::time::SystemTime;

use chrono::{DateTime, Local};

use crate::{Calculator, format_number};

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
        // template author may wrap a placeholder in their own quotes; all three
        // contexts (unquoted, inside `'…'`, inside `"…"`) are safe because the
        // value is escaped for the context it is inserted into.
        let shell_context = if shell_escape {
            shell_context(&output)
        } else {
            ShellContext::Unquoted
        };

        // Collapsing an author-written `'{{x}}'` wrapper is only correct in the
        // unquoted context: inside an open single-quoted run the closing quote
        // belongs to that run, and "collapsing" it would let the author's
        // trailing quote swallow the rest of the template.
        let mut trailing_quote_to_skip = 0;
        let mut is_wrapped_in_single_quotes = false;
        if shell_context == ShellContext::Unquoted
            && output.ends_with('\'')
            && after_end.len() >= 3
            && after_end[2..].starts_with('\'')
        {
            output.pop();
            trailing_quote_to_skip = 1;
            is_wrapped_in_single_quotes = true;
        }

        match render_placeholder(placeholder.trim(), context) {
            Some(value) if shell_escape => {
                output.push_str(&escape_for_context(&value, shell_context));
            }
            Some(value) => {
                if is_wrapped_in_single_quotes {
                    output.push('\'');
                }
                output.push_str(&value);
            }
            // Unknown placeholder: emit it literally, unquoted.
            None => {
                if is_wrapped_in_single_quotes {
                    output.push('\'');
                }
                output.push_str(&format!("{{{{{}}}}}", placeholder.trim()));
            }
        }
        rest = &after_end[2 + trailing_quote_to_skip..];
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
}

fn shell_context(text: &str) -> ShellContext {
    let mut single = false;
    let mut double = false;
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if single {
            if ch == '\'' {
                single = false;
            }
        } else if ch == '\\' {
            // Backslash escapes the next character (unquoted and inside double
            // quotes), so neither character affects quoting state.
            chars.next();
        } else if ch == '\'' && !double {
            single = true;
        } else if ch == '"' {
            double = !double;
        }
    }
    if single {
        ShellContext::Single
    } else if double {
        ShellContext::Double
    } else {
        ShellContext::Unquoted
    }
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
        let result = expand_placeholders_shell(
            "notify-send \"{{query}}\" {{clipboard}}",
            &context,
        );
        assert_eq!(
            result,
            "notify-send \"it's\" 'hello; touch /tmp/pwned'"
        );
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
        assert!(!is_inside_unclosed_double_quotes(
            "notify-send \"it's\" "
        ));
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
}

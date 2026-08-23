use std::collections::HashMap;
use std::time::SystemTime;

use chrono::{DateTime, Local};

use crate::{Calculator, format_number};

#[derive(Debug, Clone)]
pub(crate) struct PlaceholderContext {
    pub(crate) query: String,
    pub(crate) clipboard: String,
    pub(crate) args: HashMap<String, String>,
    pub(crate) preferences: HashMap<String, String>,
    pub(crate) now: SystemTime,
}

impl PlaceholderContext {
    pub(crate) fn new(query: &str, clipboard: Option<&String>) -> Self {
        Self {
            query: query.to_string(),
            clipboard: clipboard.cloned().unwrap_or_default(),
            args: HashMap::new(),
            preferences: HashMap::new(),
            now: SystemTime::now(),
        }
    }

    pub(crate) fn with_preferences(mut self, preferences: HashMap<String, String>) -> Self {
        self.preferences = preferences;
        self
    }
}

/// Expand placeholders verbatim. Use for non-shell contexts (URLs, snippets,
/// environment values).
pub(crate) fn expand_placeholders(template: &str, context: &PlaceholderContext) -> String {
    expand(template, context, false)
}

/// Expand placeholders for a string that will be passed to `sh -c`. Substituted
/// values (query, clipboard, arg, pref, …) are POSIX shell-quoted so untrusted
/// input (e.g. clipboard containing `$(rm -rf ~)` or `; reboot`) cannot break
/// out into command execution. Command authors therefore must NOT add their own
/// quotes around placeholders — the quoting is supplied here.
pub(crate) fn expand_placeholders_shell(template: &str, context: &PlaceholderContext) -> String {
    expand(template, context, true)
}

fn expand(template: &str, context: &PlaceholderContext, shell_escape: bool) -> String {
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

        // A template author may place the placeholder inside their own double
        // quotes (e.g. `cmd "{{query}}"`). There, injected single quotes are
        // literal, while `$()`, backticks and `$VAR` would still be expanded by
        // the shell — so single-quote wrapping is not enough and the value must
        // be escaped for the double-quote context instead. This is decided
        // independently of the single-quote wrapper below: a nested
        // `'{{query}}'` inside unclosed double quotes needs the double-quote
        // treatment too (the author's single quotes stay literal there), so the
        // wrapper collapse is skipped in that context.
        let inside_double_quotes =
            shell_escape && is_inside_unclosed_double_quotes(&output);

        let mut trailing_quote_to_skip = 0;
        let mut is_wrapped_in_single_quotes = false;
        if shell_escape
            && !inside_double_quotes
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
                if inside_double_quotes {
                    output.push_str(&double_quote_escape(&value));
                } else {
                    output.push_str(&shell_quote(&value));
                }
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

/// Report whether the next character appended to `text` would land inside
/// unclosed double quotes, applying POSIX shell lexing rules: inside single
/// quotes every character is literal; inside double quotes a single quote is
/// literal (it does NOT open a single-quoted segment); a backslash escapes the
/// following character outside single quotes. This lets `expand()` detect
/// templates like `cmd "{{query}}"` where a placeholder sits inside the
/// author's own quotes.
fn is_inside_unclosed_double_quotes(text: &str) -> bool {
    let mut inside_single_quotes = false;
    let mut inside_double_quotes = false;
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if inside_single_quotes {
            if ch == '\'' {
                inside_single_quotes = false;
            }
        } else if ch == '\\' {
            // Backslash escapes the next character (both unquoted and inside
            // double quotes), so neither character affects quoting state.
            chars.next();
        } else if ch == '\'' && !inside_double_quotes {
            // Outside double quotes a single quote opens a literal segment;
            // inside double quotes it is just an ordinary character.
            inside_single_quotes = true;
        } else if ch == '"' {
            inside_double_quotes = !inside_double_quotes;
        }
    }
    inside_double_quotes
}

/// Resolve a placeholder to its value, or `None` if the name is unknown.
fn render_placeholder(placeholder: &str, context: &PlaceholderContext) -> Option<String> {
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
        _ => return None,
    };
    Some(value)
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
}

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::action::{Action, ActionKind, ProcessCommand};
use crate::search::fuzzy_score;

#[derive(Debug, Clone)]
pub(crate) struct AppEntry {
    pub(crate) name: String,
    /// `Exec` with its `%`-codes stripped, for display.
    exec: String,
    /// `Exec` exactly as written, for the argv parser: stripping `%`-codes for
    /// display also destroys the quoting, and the quoting is what keeps an
    /// argument one argument.
    exec_raw: String,
    exec_name: String,
    comment: Option<String>,
    icon_name: String,
    /// Where this entry was read from, for the `%k` field code.
    desktop_path: Option<String>,
}

pub(crate) fn load_apps(home: &Path) -> Vec<AppEntry> {
    let xdg_data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));

    let xdg_data_dirs_raw = env::var_os("XDG_DATA_DIRS")
        .unwrap_or_else(|| std::ffi::OsString::from("/usr/local/share:/usr/share"));
    let mut dirs = vec![xdg_data_home.join("applications")];
    dirs.extend(
        env::split_paths(&xdg_data_dirs_raw)
            .map(|dir| dir.join("applications"))
            .collect::<Vec<_>>(),
    );

    let flatpak_user = home.join(".local/share/flatpak/exports/share/applications");
    if flatpak_user.exists() {
        dirs.push(flatpak_user);
    }

    let flatpak_system = PathBuf::from("/var/lib/flatpak/exports/share/applications");
    if flatpak_system.exists() {
        dirs.push(flatpak_system);
    }

    let mut seen = HashSet::new();
    let mut apps = Vec::new();

    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
                continue;
            }
            if let Some(app) = parse_desktop_file(&path) {
                let key = app.name.to_lowercase();
                if seen.insert(key) {
                    apps.push(app);
                }
            }
        }
    }

    apps
}

fn parse_desktop_file(path: &Path) -> Option<AppEntry> {
    let mut entry = parse_desktop_content(&fs::read_to_string(path).ok()?)?;
    entry.desktop_path = Some(path.display().to_string());
    Some(entry)
}

pub(crate) fn parse_desktop_content(content: &str) -> Option<AppEntry> {
    let mut name = None;
    let mut exec = None;
    let mut exec_raw = None;
    let mut comment = None;
    let mut icon = None;
    let mut no_display = false;
    let mut hidden = false;
    // Only the `[Desktop Entry]` group describes the app itself. Later
    // `[Desktop Action …]` groups carry their own Name=/Exec= which must not
    // overwrite the app's (otherwise e.g. LibreWolf becomes "Profile Manager").
    let mut in_desktop_entry = false;

    for line in content.lines().map(str::trim) {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop_entry {
            continue;
        }
        if let Some(value) = line.strip_prefix("Name=") {
            name = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("Exec=") {
            exec = Some(clean_desktop_exec(value));
            exec_raw = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("Comment=") {
            comment = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("Icon=") {
            icon = Some(value.to_string());
        } else if line == "NoDisplay=true" {
            no_display = true;
        } else if line == "Hidden=true" {
            hidden = true;
        }
    }

    if no_display || hidden {
        return None;
    }

    let icon_name = icon.unwrap_or_else(|| "application-x-executable-symbolic".to_string());
    let exec_name = exec
        .as_deref()
        .and_then(|e| e.split_whitespace().next())
        .map(|bin| {
            Path::new(bin)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(bin)
                .to_string()
        })
        .unwrap_or_default();

    Some(AppEntry {
        name: name?,
        exec: exec?,
        exec_raw: exec_raw?,
        exec_name,
        comment,
        icon_name,
        desktop_path: None,
    })
}

/// Program and arguments from a `.desktop` `Exec` value (P4.2).
///
/// Implements the Desktop Entry Specification's rules for quoting, backslash
/// escaping and field codes, because the result is handed to `execve` directly.
/// Field codes that would name files (`%f`, `%F`, `%u`, `%U`) are dropped: the
/// palette launches an application, it has no file arguments to pass. Codes the
/// specification marks deprecated are dropped as well.
///
/// `env VAR=value app` needs no special treatment: `env` is a real program, and
/// with a direct exec it receives `VAR=value` as its own argument and applies it
/// to the child itself.
pub(crate) fn desktop_exec_argv(
    exec: &str,
    name: &str,
    icon: Option<&str>,
    desktop_path: Option<&str>,
) -> Option<(String, Vec<String>)> {
    let tokens = split_desktop_exec(exec);

    let mut argv: Vec<String> = Vec::new();
    for token in tokens {
        match token.as_str() {
            // %i expands to two arguments, and only when an icon is known.
            "%i" => {
                if let Some(icon) = icon {
                    argv.push("--icon".to_string());
                    argv.push(icon.to_string());
                }
            }
            "%c" => argv.push(name.to_string()),
            "%k" => {
                if let Some(path) = desktop_path {
                    argv.push(path.to_string());
                }
            }
            // No file arguments, and the deprecated codes.
            "%f" | "%F" | "%u" | "%U" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => {}
            other => {
                let (expanded, removed_code) = expand_field_codes(other, name, desktop_path);
                // A token that was nothing but a dropped code disappears; one
                // that was explicitly quoted empty stays an empty argument.
                if !expanded.is_empty() || !removed_code {
                    argv.push(expanded);
                }
            }
        }
    }

    let program = argv.first()?.clone();
    argv.remove(0);
    Some((program, argv))
}

/// Split an `Exec` value into arguments, honouring double quotes and the
/// backslash escapes the specification defines.
fn split_desktop_exec(exec: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut token = String::new();
    let mut in_token = false;
    let mut quoted = false;
    let mut chars = exec.chars();

    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                // "\\" is a backslash, "\"" a quote and so on: the escaped
                // character stands for itself.
                if let Some(escaped) = chars.next() {
                    token.push(escaped);
                }
                in_token = true;
            }
            '"' => {
                quoted = !quoted;
                // `""` is one empty argument, not no argument.
                in_token = true;
            }
            c if c.is_whitespace() && !quoted => {
                if in_token {
                    args.push(std::mem::take(&mut token));
                    in_token = false;
                }
            }
            c => {
                token.push(c);
                in_token = true;
            }
        }
    }

    if in_token {
        args.push(token);
    }
    args
}

/// Expand `%`-codes inside one argument. Returns the expansion and whether a
/// code was removed, so a token that was only a code can be dropped.
fn expand_field_codes(token: &str, name: &str, desktop_path: Option<&str>) -> (String, bool) {
    let mut out = String::with_capacity(token.len());
    let mut removed = false;
    let mut chars = token.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('c') => out.push_str(name),
            Some('k') => {
                if let Some(path) = desktop_path {
                    out.push_str(path);
                } else {
                    removed = true;
                }
            }
            // Every other code, including the documented ones: dropped.
            Some(_) => removed = true,
            // A trailing `%` is not a code.
            None => out.push('%'),
        }
    }

    (out, removed)
}

pub(crate) fn clean_desktop_exec(exec: &str) -> String {
    exec.split_whitespace()
        .filter(|part| !part.starts_with('%'))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn search_apps(apps: &[AppEntry], query: &str) -> Vec<Action> {
    apps.iter()
        .filter_map(|app| {
            let haystack = {
                let mut h = app.name.clone();
                if !app.exec_name.is_empty() && app.exec_name != app.name {
                    h.push(' ');
                    h.push_str(&app.exec_name);
                }
                if let Some(c) = &app.comment {
                    h.push(' ');
                    h.push_str(c);
                }
                h
            };
            let score = fuzzy_score(&haystack, query)?;
            Some(app_action(app, score + 100))
        })
        .collect()
}

pub(crate) fn app_action(app: &AppEntry, score: i32) -> Action {
    // P4.2: the app's own argv, not its `Exec` line handed to a shell. `sh -c`
    // made every quoted argument and every `%`-code a place where a `.desktop`
    // file's contents could become a second command.
    let kind = match desktop_exec_argv(
        &app.exec_raw,
        &app.name,
        Some(app.icon_name.as_str()),
        app.desktop_path.as_deref(),
    ) {
        Some((program, args)) => ActionKind::Command(ProcessCommand::new(program, args)),
        None => ActionKind::None,
    };

    Action::new("App", &app.name, kind, score)
        .with_subtitle(
            app.comment
                .as_deref()
                .filter(|comment| !comment.trim().is_empty())
                .unwrap_or(&app.exec),
        )
        .with_icon(&app.icon_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_actions_do_not_override_main_entry() {
        // Mirrors librewolf.desktop: a [Desktop Entry] followed by action groups
        // whose Name=/Exec= must not leak into the app itself.
        let app = parse_desktop_content(
            "[Desktop Entry]\n\
             Name=LibreWolf\n\
             Exec=librewolf --name librewolf %U\n\
             Type=Application\n\
             \n\
             [Desktop Action new-private-window]\n\
             Exec=librewolf --private-window %U\n\
             Name=New Private Window\n\
             \n\
             [Desktop Action profile-manager-window]\n\
             Exec=librewolf --ProfileManager\n\
             Name=Profile Manager\n",
        )
        .expect("desktop entry parses");

        assert_eq!(app.name, "LibreWolf");
        assert_eq!(app.exec, "librewolf --name librewolf");
        assert!(fuzzy_score("LibreWolf librewolf", "librewolf").is_some());
    }

    fn app(name: &str, exec: &str) -> AppEntry {
        AppEntry {
            name: name.to_string(),
            exec: clean_desktop_exec(exec),
            exec_raw: exec.to_string(),
            exec_name: "app".to_string(),
            comment: None,
            icon_name: "icon".to_string(),
            desktop_path: Some("/usr/share/applications/app.desktop".to_string()),
        }
    }

    #[test]
    fn desktop_exec_with_quotes_splits_like_the_spec() {
        let (program, args) = desktop_exec_argv(
            "app \"an argument with spaces\" --flag \"\" --quoted=\"a b\"",
            "App",
            None,
            None,
        )
        .expect("an executable");

        assert_eq!(program, "app");
        assert_eq!(
            args,
            vec![
                "an argument with spaces".to_string(),
                // An explicitly empty argument stays an argument.
                "--flag".to_string(),
                String::new(),
                "--quoted=a b".to_string(),
            ]
        );
    }

    #[test]
    fn backslash_escapes_are_undone_before_exec() {
        // The value as it appears in a .desktop file: one argument containing
        // backslashes, written with `\\` escapes.
        let (_, args) = desktop_exec_argv(r#"app "C:\\Program Files\\""#, "App", None, None)
            .expect("an executable");
        assert_eq!(args[0], r"C:\Program Files\");

        // And a quote inside an argument, escaped with a backslash, is a quote
        // and not the end of the argument.
        let (_, args) =
            desktop_exec_argv(r#"app say\"hi\""#, "App", None, None).expect("an executable");
        assert_eq!(args[0], "say\"hi\"");
    }

    #[test]
    fn field_codes_are_expanded_or_dropped() {
        let (program, args) = desktop_exec_argv(
            "app %U --name %c --desktop %k %i %% done",
            "My App",
            Some("my-icon"),
            Some("/usr/share/applications/my.desktop"),
        )
        .expect("an executable");

        assert_eq!(program, "app");
        assert_eq!(
            args,
            vec![
                // %U: no file arguments to pass, so it disappears entirely.
                "--name".to_string(),
                "My App".to_string(),
                "--desktop".to_string(),
                "/usr/share/applications/my.desktop".to_string(),
                "--icon".to_string(),
                "my-icon".to_string(),
                "%".to_string(),
                "done".to_string(),
            ]
        );
    }

    #[test]
    fn env_prefixed_exec_keeps_env_as_the_program() {
        // The `env VAR=value cmd` form needs no rewriting: with a direct exec,
        // `env` gets its own argument and applies it to the child itself.
        let (program, args) =
            desktop_exec_argv("env FOO=bar app --flag", "App", None, None).expect("env exists");

        assert_eq!(program, "env");
        assert_eq!(
            args,
            vec![
                "FOO=bar".to_string(),
                "app".to_string(),
                "--flag".to_string()
            ]
        );
    }

    #[test]
    fn an_app_action_carries_argv_instead_of_a_shell_line() {
        // The regression: this used to be `ActionKind::Launch(exec)`, which ran
        // the whole `Exec` value through `sh -c`.
        let action = app_action(&app("App", "app --flag %U"), 100);

        match action.kind {
            ActionKind::Command(command) => {
                assert_eq!(command.program, "app");
                assert_eq!(command.args, vec!["--flag".to_string()]);
            }
            other => panic!("expected an argv command, got {other:?}"),
        }
    }

    #[test]
    fn an_exec_without_a_program_does_not_become_a_shell_command() {
        // `Exec=%U` names no program at all: there is nothing to run, and
        // inventing a shell line here is exactly what this change removed.
        let action = app_action(&app("App", "%U"), 100);
        assert!(matches!(action.kind, ActionKind::None));
    }
}

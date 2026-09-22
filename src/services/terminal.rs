use std::env;
use std::path::Path;

pub const SUPPORTED_TERMINALS: &[&str] = &[
    "ghostty",
    "foot",
    "kitty",
    "alacritty",
    "wezterm",
    "gnome-terminal",
    "konsole",
    "xterm",
];

/// Detect the default or best available terminal emulator.
pub fn detect_terminal(preferred: Option<&str>) -> Option<String> {
    if let Some(pref) = preferred
        && !pref.trim().is_empty()
        && which(pref).is_some()
    {
        return Some(pref.to_string());
    }

    if let Ok(env_term) = env::var("TERMINAL")
        && !env_term.trim().is_empty()
        && which(&env_term).is_some()
    {
        return Some(env_term);
    }

    for &term in SUPPORTED_TERMINALS {
        if which(term).is_some() {
            return Some(term.to_string());
        }
    }

    None
}

/// Helper to check if a binary exists in $PATH.
pub fn which(binary: &str) -> Option<String> {
    if binary.contains('/') {
        let path = Path::new(binary);
        if path.is_file() {
            return Some(binary.to_string());
        }
        return None;
    }

    if let Ok(path_var) = env::var("PATH") {
        for dir in env::split_paths(&path_var) {
            let full_path = dir.join(binary);
            if full_path.is_file() {
                return Some(full_path.to_string_lossy().to_string());
            }
        }
    }
    None
}

/// Builds the interactive shell execution arguments for a given terminal.
pub fn build_terminal_command(
    terminal: &str,
    shell_command: &str,
    hold_open: bool,
) -> (String, Vec<String>) {
    let script = if hold_open {
        format!(
            "{}; printf '\\n\\x1b[90m[Process completed. Press Enter to exit]\\x1b[0m '; read -r _",
            shell_command
        )
    } else {
        shell_command.to_string()
    };

    let term_name = Path::new(terminal)
        .file_name()
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|| terminal.into());

    let args: Vec<String> = match term_name.as_ref() {
        "ghostty" => vec!["-e".into(), "bash".into(), "-c".into(), script],
        "foot" => vec!["-e".into(), "bash".into(), "-c".into(), script],
        "kitty" => vec!["--".into(), "bash".into(), "-c".into(), script],
        "alacritty" => vec!["-e".into(), "bash".into(), "-c".into(), script],
        "wezterm" => vec![
            "start".into(),
            "--".into(),
            "bash".into(),
            "-c".into(),
            script,
        ],
        "gnome-terminal" => vec!["--".into(), "bash".into(), "-c".into(), script],
        "konsole" => vec!["-e".into(), "bash".into(), "-c".into(), script],
        _ => vec!["-e".into(), "bash".into(), "-c".into(), script],
    };

    (terminal.to_string(), args)
}

/// Builds the terminal spawn for `shell_command` as a typed process command.
///
/// Deliberately does **not** spawn: the caller must route the command through
/// the execution gateway (`crate::action::execute`) so the capability ceiling and the
/// confirmation policy are enforced before anything starts.
pub fn terminal_process_command(
    shell_command: &str,
    terminal_override: Option<&str>,
    hold_open: bool,
) -> crate::action::ProcessCommand {
    let term = detect_terminal(terminal_override)
        .or_else(|| terminal_override.map(ToString::to_string))
        .unwrap_or_else(|| "xterm".to_string());
    let (bin, args) = build_terminal_command(&term, shell_command, hold_open);
    crate::action::ProcessCommand::new(bin, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_terminal_command_ghostty() {
        let (bin, args) = build_terminal_command("ghostty", "htop", true);
        assert_eq!(bin, "ghostty");
        assert_eq!(args[0], "-e");
        assert_eq!(args[1], "bash");
        assert_eq!(args[2], "-c");
        assert!(args[3].contains("htop"));
        assert!(args[3].contains("Process completed"));
    }

    #[test]
    fn build_terminal_command_kitty() {
        let (bin, args) = build_terminal_command("/usr/bin/kitty", "ls -la", false);
        assert_eq!(bin, "/usr/bin/kitty");
        assert_eq!(args[0], "--");
        assert_eq!(args[1], "bash");
        assert_eq!(args[2], "-c");
        assert_eq!(args[3], "ls -la");
    }

    #[test]
    fn build_terminal_command_wezterm() {
        let (bin, args) = build_terminal_command("wezterm", "cargo build", false);
        assert_eq!(bin, "wezterm");
        assert_eq!(args[0], "start");
        assert_eq!(args[1], "--");
        assert_eq!(args[2], "bash");
        assert_eq!(args[3], "-c");
        assert_eq!(args[4], "cargo build");
    }

    #[test]
    fn which_finds_sh() {
        assert!(which("sh").is_some());
    }

    #[test]
    fn the_action_string_stays_one_argument_for_the_terminal_shell() {
        // The terminal needs a shell, so this one path really is `bash -c`. What
        // matters is that the action arrives as a single argument: nothing of it
        // is spliced into a larger line, and no quoting of ours can turn part of
        // it into a second command.
        let command = terminal_process_command("echo hi; rm -rf /tmp/x", Some("foot"), true);

        assert_eq!(command.program, "foot");
        assert_eq!(command.args[0], "-e");
        assert_eq!(command.args[1], "bash");
        assert_eq!(command.args[2], "-c");
        assert!(
            command.args[3].starts_with("echo hi; rm -rf /tmp/x"),
            "the action must be the whole script, not a fragment of one: {}",
            command.args[3]
        );
        assert_eq!(command.args.len(), 4, "no other argument may be added");
    }
}

use std::collections::HashMap;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalEmulator {
    pub id: String,
    pub name: String,
    pub binary: String,
    pub exec_arg: String,
    pub available: bool,
}

pub const KNOWN_TERMINALS: &[(&str, &str, &str, &str)] = &[
    ("ghostty", "Ghostty", "ghostty", "-e"),
    ("foot", "Foot", "foot", "-e"),
    ("kitty", "Kitty", "kitty", "-e"),
    ("alacritty", "Alacritty", "alacritty", "-e"),
    ("wezterm", "WezTerm", "wezterm", "start --"),
    ("rio", "Rio", "rio", "-e"),
    ("gnome-terminal", "GNOME Terminal", "gnome-terminal", "--"),
    ("konsole", "Konsole", "konsole", "-e"),
    ("xfce4-terminal", "XFCE Terminal", "xfce4-terminal", "-e"),
    ("urxvt", "rxvt-unicode", "urxvt", "-e"),
    ("xterm", "XTerm", "xterm", "-e"),
];

pub fn detected_terminals() -> Vec<TerminalEmulator> {
    KNOWN_TERMINALS
        .iter()
        .map(|&(id, name, binary, exec_arg)| {
            let available = is_binary_available(binary);
            TerminalEmulator {
                id: id.to_string(),
                name: name.to_string(),
                binary: binary.to_string(),
                exec_arg: exec_arg.to_string(),
                available,
            }
        })
        .collect()
}

pub fn resolve_default_terminal(preferences: &HashMap<String, String>) -> TerminalEmulator {
    // 1. Check user preference
    if let Some(pref) = preferences.get("default_terminal").filter(|s| !s.trim().is_empty()) {
        let pref = pref.trim();
        if let Some(term) = detected_terminals()
            .into_iter()
            .find(|t| t.id.eq_ignore_ascii_case(pref) || t.binary.eq_ignore_ascii_case(pref))
        {
            return term;
        }
        let available = is_binary_available(pref);
        return TerminalEmulator {
            id: pref.to_string(),
            name: pref.to_string(),
            binary: pref.to_string(),
            exec_arg: "-e".to_string(),
            available,
        };
    }

    // 2. Check $TERMINAL
    if let Ok(env_term) = std::env::var("TERMINAL") {
        let env_term = env_term.trim();
        if !env_term.is_empty() {
            if let Some(term) = detected_terminals().into_iter().find(|t| {
                t.id.eq_ignore_ascii_case(env_term) || t.binary.eq_ignore_ascii_case(env_term)
            }) {
                return term;
            }
            let available = is_binary_available(env_term);
            return TerminalEmulator {
                id: env_term.to_string(),
                name: env_term.to_string(),
                binary: env_term.to_string(),
                exec_arg: "-e".to_string(),
                available,
            };
        }
    }

    // 3. Pick first available from modern list
    let all = detected_terminals();
    if let Some(term) = all.into_iter().find(|t| t.available) {
        return term;
    }

    // 4. Default fallback
    TerminalEmulator {
        id: "xterm".to_string(),
        name: "XTerm".to_string(),
        binary: "xterm".to_string(),
        exec_arg: "-e".to_string(),
        available: false,
    }
}

pub fn spawn_in_terminal(
    command_str: &str,
    preferences: &HashMap<String, String>,
) -> std::io::Result<()> {
    let term = resolve_default_terminal(preferences);
    let mut cmd = Command::new(&term.binary);

    let exec_parts: Vec<&str> = term.exec_arg.split_whitespace().collect();
    for part in exec_parts {
        cmd.arg(part);
    }

    cmd.arg("bash")
        .arg("-c")
        .arg(format!("{command_str}; echo; read -p 'Press Enter to close...'"));
    cmd.spawn()?;
    Ok(())
}

fn is_binary_available(binary: &str) -> bool {
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths).find_map(|p| {
                let full = p.join(binary);
                if full.is_file() { Some(full) } else { None }
            })
        })
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_terminal_preference_override() {
        let mut preferences = HashMap::new();
        preferences.insert("default_terminal".to_string(), "kitty".to_string());
        let term = resolve_default_terminal(&preferences);
        assert_eq!(term.id, "kitty");
        assert_eq!(term.binary, "kitty");
        assert_eq!(term.exec_arg, "-e");
    }

    #[test]
    fn test_resolve_terminal_custom_binary() {
        let mut preferences = HashMap::new();
        preferences.insert("default_terminal".to_string(), "my-custom-term".to_string());
        let term = resolve_default_terminal(&preferences);
        assert_eq!(term.id, "my-custom-term");
        assert_eq!(term.binary, "my-custom-term");
    }
}

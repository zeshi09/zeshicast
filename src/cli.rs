use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum CliCommand {
    Help,
    Export {
        dest: PathBuf,
        /// None when --include-secrets was not passed on the CLI; then the
        /// `export_include_secrets` preference decides.
        include_secrets: Option<bool>,
        /// None when --include-history was not passed on the CLI; then the
        /// `export_include_history` preference decides (default: excluded).
        include_history: Option<bool>,
    },
    Import {
        src: PathBuf,
    },
    /// `--extension-lint <dir> [--json]` (Q5.3).
    ExtensionLint {
        dir: PathBuf,
        json: bool,
        /// `--allow a,b`: the capabilities the manifest may ask for (Q5.6).
        allow: Option<Vec<String>>,
    },
    /// `--extension-install <id> [--registry <file|https-url>] [--root <dir>]
    /// [--yes]` (Q5.6). `root` replaces `~/.config/zeshicast`.
    ExtensionInstall {
        id: String,
        registry: Option<String>,
        root: Option<PathBuf>,
        yes: bool,
    },
    Query(String),
    Repl,
}

pub fn parse_cli_args<I, T>(args: I) -> CliCommand
where
    I: IntoIterator<Item = T>,
    T: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    if args.is_empty() {
        return CliCommand::Repl;
    }

    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        return CliCommand::Help;
    }

    // P4.3: `--export`/`--import` act only as the *first* argument. Searching
    // the whole list turned a query that merely contains the word into an
    // export: `zeshicast shell --export x` exported instead of searching.
    match args.first().map(String::as_str) {
        Some("--export") => {
            let dest = args
                .get(1)
                .filter(|a| !a.starts_with('-'))
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("zeshicast-config.tar.gz"));
            CliCommand::Export {
                dest,
                include_secrets: parse_bool_flag(&args, "--include-secrets"),
                include_history: parse_bool_flag(&args, "--include-history"),
            }
        }
        Some("--import") => match args.get(1).map(PathBuf::from) {
            Some(src) => CliCommand::Import { src },
            None => CliCommand::Help,
        },
        Some("--extension-lint") => {
            let allow_at = args.iter().position(|arg| arg == "--allow");
            let allow = allow_at.map(|index| {
                args.get(index + 1)
                    .map(|list| {
                        list.split(',')
                            .map(str::trim)
                            .filter(|name| !name.is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default()
            });
            let dir = args
                .iter()
                .enumerate()
                .skip(1)
                .find(|(index, arg)| !arg.starts_with("--") && Some(index - 1) != allow_at)
                .map(|(_, arg)| arg);
            match dir {
                Some(dir) => CliCommand::ExtensionLint {
                    dir: PathBuf::from(dir),
                    json: args[1..].iter().any(|arg| arg == "--json"),
                    allow,
                },
                None => CliCommand::Help,
            }
        }
        Some("--extension-install") => {
            let value_of = |flag: &str| {
                args.iter()
                    .position(|arg| arg == flag)
                    .and_then(|index| args.get(index + 1))
                    .filter(|value| !value.starts_with("--"))
                    .cloned()
            };
            let mut rest = args[1..].iter().enumerate();
            let id = rest.find_map(|(index, arg)| {
                let after_flag =
                    index > 0 && matches!(args[index].as_str(), "--registry" | "--root");
                (!arg.starts_with("--") && !after_flag).then(|| arg.clone())
            });
            match id {
                Some(id) => CliCommand::ExtensionInstall {
                    id,
                    registry: value_of("--registry"),
                    root: value_of("--root").map(PathBuf::from),
                    yes: args.iter().any(|arg| arg == "--yes"),
                },
                None => CliCommand::Help,
            }
        }
        _ => CliCommand::Query(args.join(" ")),
    }
}

/// Parse a tri-state boolean flag into an explicit value:
/// - `None`: flag absent (fall back to the matching preference)
/// - `Some(true)` / `Some(false)`: explicit CLI override
fn parse_bool_flag(args: &[String], flag: &str) -> Option<bool> {
    let with_equals = format!("{flag}=");
    for (index, arg) in args.iter().enumerate() {
        if arg == flag {
            // Only consume a following value when it is explicitly true/false;
            // otherwise this is the bare boolean form of the flag.
            return match args.get(index + 1).map(String::as_str) {
                Some("true") => Some(true),
                Some("false") => Some(false),
                _ => Some(true),
            };
        }
        if let Some(value) = arg.strip_prefix(&with_equals) {
            return Some(matches!(
                value.to_ascii_lowercase().as_str(),
                "true" | "1" | "yes"
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_lint_takes_a_directory_and_an_optional_json_flag() {
        assert_eq!(
            parse_cli_args(vec!["--extension-lint", "ext"]),
            CliCommand::ExtensionLint {
                dir: PathBuf::from("ext"),
                json: false,
                allow: None,
            }
        );
        assert_eq!(
            parse_cli_args(vec!["--extension-lint", "--json", "ext"]),
            CliCommand::ExtensionLint {
                dir: PathBuf::from("ext"),
                json: true,
                allow: None,
            }
        );
        assert_eq!(
            parse_cli_args(vec!["--extension-lint", "--allow", "shell,open_url", "ext"]),
            CliCommand::ExtensionLint {
                dir: PathBuf::from("ext"),
                json: false,
                allow: Some(vec!["shell".to_string(), "open_url".to_string()]),
            }
        );
        assert_eq!(
            parse_cli_args(vec!["--extension-lint", "ext", "--allow", ""]),
            CliCommand::ExtensionLint {
                dir: PathBuf::from("ext"),
                json: false,
                allow: Some(Vec::new()),
            }
        );
        assert_eq!(parse_cli_args(vec!["--extension-lint"]), CliCommand::Help);
        assert_eq!(
            parse_cli_args(vec![
                "--extension-install",
                "--registry",
                "reg.toml",
                "you.hello",
                "--root",
                "/tmp/sandbox",
                "--yes"
            ]),
            CliCommand::ExtensionInstall {
                id: "you.hello".to_string(),
                registry: Some("reg.toml".to_string()),
                root: Some(PathBuf::from("/tmp/sandbox")),
                yes: true,
            }
        );
        assert_eq!(
            parse_cli_args(vec!["--extension-install", "you.hello"]),
            CliCommand::ExtensionInstall {
                id: "you.hello".to_string(),
                registry: None,
                root: None,
                yes: false,
            }
        );
        assert_eq!(
            parse_cli_args(vec!["--extension-install"]),
            CliCommand::Help
        );
        // Only as the first argument, like --export.
        assert_eq!(
            parse_cli_args(vec!["find", "--extension-lint"]),
            CliCommand::Query("find --extension-lint".to_string())
        );
    }

    #[test]
    fn test_parse_cli_args() {
        assert_eq!(parse_cli_args(Vec::<String>::new()), CliCommand::Repl);
        assert_eq!(parse_cli_args(vec!["--help"]), CliCommand::Help);
        assert_eq!(
            parse_cli_args(vec!["--export", "out.tar.gz"]),
            CliCommand::Export {
                dest: PathBuf::from("out.tar.gz"),
                include_secrets: None,
                include_history: None,
            }
        );
        assert_eq!(
            parse_cli_args(vec!["--export", "out.tar.gz", "--include-secrets"]),
            CliCommand::Export {
                dest: PathBuf::from("out.tar.gz"),
                include_secrets: Some(true),
                include_history: None,
            }
        );
        assert_eq!(
            parse_cli_args(vec!["--export", "out.tar.gz", "--include-secrets", "false"]),
            CliCommand::Export {
                dest: PathBuf::from("out.tar.gz"),
                include_secrets: Some(false),
                include_history: None,
            }
        );
        assert_eq!(
            parse_cli_args(vec!["--export", "--include-secrets=false"]),
            CliCommand::Export {
                dest: PathBuf::from("zeshicast-config.tar.gz"),
                include_secrets: Some(false),
                include_history: None,
            }
        );
        assert_eq!(
            parse_cli_args(vec!["--import", "in.tar.gz"]),
            CliCommand::Import {
                src: PathBuf::from("in.tar.gz")
            }
        );
        assert_eq!(
            parse_cli_args(vec!["firefox", "search"]),
            CliCommand::Query("firefox search".to_string())
        );
    }

    #[test]
    fn query_with_export_substring_is_a_query() {
        // `--export` was searched for anywhere in the argument list, so this
        // exported instead of searching for the words (P4.3).
        assert_eq!(
            parse_cli_args(vec!["shell", "--export", "x"]),
            CliCommand::Query("shell --export x".to_string())
        );
        assert_eq!(
            parse_cli_args(vec!["notes", "--import", "file"]),
            CliCommand::Query("notes --import file".to_string())
        );
    }
}

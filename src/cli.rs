use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum CliCommand {
    Help,
    Export {
        dest: PathBuf,
        /// None when --include-secrets was not passed on the CLI; then the
        /// `export_include_secrets` preference decides.
        include_secrets: Option<bool>,
    },
    Import {
        src: PathBuf,
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

    if let Some(pos) = args.iter().position(|a| a == "--export") {
        let dest = args
            .get(pos + 1)
            .filter(|a| !a.starts_with('-'))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("zeshicast-config.tar.gz"));
        let include_secrets = parse_include_secrets_flag(&args);
        return CliCommand::Export {
            dest,
            include_secrets,
        };
    }

    if let Some(pos) = args.iter().position(|a| a == "--import") {
        if let Some(src) = args.get(pos + 1).map(PathBuf::from) {
            return CliCommand::Import { src };
        } else {
            return CliCommand::Help;
        }
    }

    CliCommand::Query(args.join(" "))
}

/// Parse the `--include-secrets` flag into an explicit tri-state:
/// - `None`: flag absent (fall back to the `export_include_secrets` preference)
/// - `Some(true)` / `Some(false)`: explicit CLI override
fn parse_include_secrets_flag(args: &[String]) -> Option<bool> {
    for (index, arg) in args.iter().enumerate() {
        if arg == "--include-secrets" {
            // Only consume a following value when it is explicitly true/false;
            // otherwise this is the bare boolean form of the flag.
            return match args.get(index + 1).map(String::as_str) {
                Some("true") => Some(true),
                Some("false") => Some(false),
                _ => Some(true),
            };
        }
        if let Some(value) = arg.strip_prefix("--include-secrets=") {
            return Some(matches!(value.to_ascii_lowercase().as_str(), "true" | "1" | "yes"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cli_args() {
        assert_eq!(parse_cli_args(Vec::<String>::new()), CliCommand::Repl);
        assert_eq!(parse_cli_args(vec!["--help"]), CliCommand::Help);
        assert_eq!(
            parse_cli_args(vec!["--export", "out.tar.gz"]),
            CliCommand::Export {
                dest: PathBuf::from("out.tar.gz"),
                include_secrets: None,
            }
        );
        assert_eq!(
            parse_cli_args(vec!["--export", "out.tar.gz", "--include-secrets"]),
            CliCommand::Export {
                dest: PathBuf::from("out.tar.gz"),
                include_secrets: Some(true),
            }
        );
        assert_eq!(
            parse_cli_args(vec![
                "--export",
                "out.tar.gz",
                "--include-secrets",
                "false"
            ]),
            CliCommand::Export {
                dest: PathBuf::from("out.tar.gz"),
                include_secrets: Some(false),
            }
        );
        assert_eq!(
            parse_cli_args(vec!["--export", "--include-secrets=false"]),
            CliCommand::Export {
                dest: PathBuf::from("zeshicast-config.tar.gz"),
                include_secrets: Some(false),
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
}

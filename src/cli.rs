use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum CliCommand {
    Help,
    Export {
        dest: PathBuf,
        include_secrets: bool,
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
        let include_secrets = args.iter().any(|arg| arg == "--include-secrets");
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
                include_secrets: false,
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

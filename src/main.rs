use std::env;
use std::io::{self, Write};
use std::path::PathBuf;

use zeshicast::cli::{CliCommand, parse_cli_args};
use zeshicast::{Action, ExecutionDecision, SecondaryActionKind, Zeshicast};

fn main() {
    zeshicast::logging::init();
    let args: Vec<String> = env::args().skip(1).collect();
    std::process::exit(run_cli(parse_cli_args(args)));
}

/// Run one CLI invocation and return its exit code (P4.3).
///
/// Non-interactive commands report failure through the exit code: a scripted
/// `--export` that did not export anything used to exit 0, so `&&`-chains and
/// `set -e` scripts carried on as if it had worked.
fn run_cli(command: CliCommand) -> i32 {
    match command {
        CliCommand::Help => {
            print_help();
            0
        }
        CliCommand::Export {
            dest,
            include_secrets,
            include_history,
        } => {
            let home = env::var("HOME").map(PathBuf::from).unwrap_or_default();
            let config_dir = home.join(".config/zeshicast");
            // Explicit flags win; otherwise fall back to the
            // `export_include_secrets` / `export_include_history` preferences.
            let preferences = zeshicast::load_global_preferences(&config_dir);
            let include_secrets = zeshicast::resolve_include_secrets(include_secrets, &preferences);
            let include_history = zeshicast::resolve_include_history(include_history, &preferences);
            if !include_history {
                eprintln!(
                    "note: clipboard and usage history are not exported (pass --include-history to include them)"
                );
            }
            match zeshicast::export_config_with_options(
                &config_dir,
                &dest,
                include_secrets,
                include_history,
            ) {
                Ok(()) => println!("exported to {}", dest.display()),
                Err(err) => {
                    eprintln!("export failed: {err}");
                    return 1;
                }
            }
            0
        }
        CliCommand::Import { src } => {
            let home = env::var("HOME").map(PathBuf::from).unwrap_or_default();
            let config_dir = home.join(".config/zeshicast");
            match zeshicast::import_config(&src, &config_dir) {
                Ok(()) => println!("imported from {}", src.display()),
                Err(err) => {
                    eprintln!("import failed: {err}");
                    return 1;
                }
            }
            0
        }
        CliCommand::Query(query) => {
            let app = Zeshicast::load();
            run_once(&app, &query);
            0
        }
        CliCommand::Repl => {
            let mut app = Zeshicast::load();
            run_repl(&mut app);
            0
        }
    }
}

/// Describe an execution decision the CLI could not carry out, if any.
///
/// Returns whether the note belongs on stderr: a refusal is a failure, a pending
/// confirmation is not. `None` means the action ran.
fn decision_note(decision: &ExecutionDecision) -> Option<(bool, String)> {
    match decision {
        ExecutionDecision::RunNow => None,
        ExecutionDecision::NeedsConfirmation(risk) => Some((
            false,
            format!(
                "confirmation required ({}); use the GTK UI to run this action.",
                risk.label()
            ),
        )),
        ExecutionDecision::Denied(reason) => Some((true, format!("blocked: {reason}"))),
    }
}

/// Print the note for a decision the CLI could not act on (P4.3).
///
/// `run_action`'s result used to be dropped, so an action that needed a
/// confirmation -- or was blocked outright -- looked exactly like one that ran.
fn report_execution_decision(decision: &ExecutionDecision) {
    if let Some((to_stderr, note)) = decision_note(decision) {
        if to_stderr {
            eprintln!("{note}");
        } else {
            println!("{note}");
        }
    }
}

fn run_repl(app: &mut Zeshicast) {
    println!("zeshicast - Raycast-like launcher for Linux");
    println!("Type a query, ':help' for commands, ':quit' to exit.");

    loop {
        print!("\n> ");
        flush_stdout();

        let mut query = String::new();
        match io::stdin().read_line(&mut query) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => {
                eprintln!("failed to read input: {error}");
                break;
            }
        }

        let query = query.trim();
        if query.is_empty() {
            continue;
        }
        if query == ":quit" || query == ":q" {
            break;
        }
        if query == ":help" {
            print_help();
            continue;
        }
        if query == ":reload" {
            app.reload();
            println!("reloaded apps, quicklinks, snippets, commands, aliases and file index");
            continue;
        }

        let actions = app.search(query);
        if actions.is_empty() {
            println!("No results.");
            continue;
        }

        print_actions(&actions);
        print!("Run number, or press Enter to skip: ");
        flush_stdout();

        let mut choice = String::new();
        if io::stdin().read_line(&mut choice).is_err() {
            continue;
        }
        let choice = choice.trim();
        if choice.is_empty() {
            continue;
        }

        match choice.parse::<usize>() {
            Ok(number) if number > 0 && number <= actions.len() => {
                run_action_menu(app, &actions[number - 1])
            }
            _ => println!("Invalid choice."),
        }
    }
}

fn run_once(app: &Zeshicast, query: &str) {
    let actions = app.search(query);
    if actions.is_empty() {
        println!("No results.");
        return;
    }
    print_actions(&actions);
}

fn run_action_menu(app: &mut Zeshicast, action: &Action) {
    println!("\n{}", action.title);
    let secondary_actions = app.available_secondary_actions(action);
    for (index, secondary) in secondary_actions.iter().enumerate() {
        println!("{:>2}. {}", index + 1, secondary.title);
    }
    println!("{:>2}. Set Alias", secondary_actions.len() + 1);
    print!("Action: ");
    flush_stdout();

    let mut choice = String::new();
    if io::stdin().read_line(&mut choice).is_err() {
        return;
    }

    let choice = choice.trim();
    if choice.is_empty() {
        report_execution_decision(&app.run_action(action));
        return;
    }

    match choice.parse::<usize>() {
        Ok(number) if number > 0 && number <= secondary_actions.len() => {
            let secondary = secondary_actions[number - 1].kind;
            match app.run_secondary_action(action, secondary) {
                Err(error) => eprintln!("failed to run action: {error}"),
                Ok(decision) => {
                    report_execution_decision(&decision);
                    if matches!(secondary, SecondaryActionKind::Pin) {
                        println!("pinned");
                    } else if matches!(secondary, SecondaryActionKind::Unpin) {
                        println!("unpinned");
                    }
                }
            }
            // M-9: the CLI also launches processes it never waits for.
            zeshicast::reap_finished_children();
        }
        Ok(number) if number == secondary_actions.len() + 1 => prompt_alias(app, action),
        _ => println!("Invalid action."),
    }
}

fn prompt_alias(app: &mut Zeshicast, action: &Action) {
    print!("Alias: ");
    flush_stdout();

    let mut alias = String::new();
    if io::stdin().read_line(&mut alias).is_err() {
        return;
    }

    match app.set_alias_for_action(alias.trim(), action) {
        Ok(alias) => println!("saved alias: {alias}"),
        Err(error) => eprintln!("failed to save alias: {error}"),
    }
}

fn print_help() {
    println!(
        "\
Usage:
  zeshicast                 Start interactive command palette
  zeshicast <query>         Print matching actions
  zeshicast --export [file] Export config to tar.gz without API keys unless
                            the export_include_secrets preference or the flag says otherwise
  zeshicast --export [file] --include-secrets[=true|false]
                            Override the export_include_secrets preference for this export
  zeshicast --export [file] --include-history[=true|false]
                            Also export clipboard/usage history (zeshicast.db,
                            calc_history.json); excluded by default
  zeshicast --import <file> Import config from tar.gz

Queries:
  firefox                   Search installed .desktop applications (reads XDG_DATA_DIRS)
  file invoice              Search files under $HOME and open via xdg-open
  calc (12 + 8) / 5         Calculate an expression
  shell systemctl status    Run a shell command
  system lock               Search built-in system actions
  proc firefox              Search processes and build kill actions
  audio vol                 Audio actions: volume up/down, mute, mic mute, brightness
  media next                MPRIS playback controls over D-Bus
  notify dnd                Notification history and DND (built-in D-Bus server)
  net wifi                  Network actions: toggle wifi, network settings
  niri screenshot           Niri compositor actions: screenshot, workspaces, windows
  hypr fullscreen           Hyprland compositor actions: screenshot, workspaces, windows
  sway reload               Sway compositor actions: screenshot, workspaces, windows
  ai explain monads         Ask local AI through Ollama; response copied to clipboard
  trans hello in ru         Translate text via LibreTranslate; result copied to clipboard
  translate hello in de     Same as trans, with explicit language suffix
  docs                      Search custom command tags/descriptions
  gh rust gtk               Run a custom command by keyword, passing \"rust gtk\" as {{{{query}}}}

Config:
  ~/.config/zeshicast/quicklinks.txt   lines: Name | tag1,tag2 = https://example.com?q={{{{query}}}}
  ~/.config/zeshicast/snippets.txt     lines: Name | tag1,tag2 = text to copy
  ~/.config/zeshicast/commands/*.toml  custom command TOMLs
  ~/.config/zeshicast/extensions/*/extension.toml  local extension manifests
  ~/.config/zeshicast/preferences.toml global extension preferences
  ~/.config/zeshicast/zeshicast.db     SQLite clipboard and usage history
  ~/.cache/zeshicast/clipboard/        cached clipboard image PNGs
  ~/.config/zeshicast/aliases.txt      lines: ff = Firefox
  ~/.config/zeshicast/pins.txt         lines: App:Firefox or Firefox

AI / Translate preferences (in preferences.toml):
  ai_endpoint    = \"http://localhost:11434/v1\"
  ai_model       = \"gemma4:e4b\"
  ai_api_key     = \"\"
  translate_endpoint = \"https://libretranslate.com\"
  translate_api_key  = \"\"
  translate_target   = \"en\"

Placeholders:
  {{{{query}}}} {{{{arg:name}}}} {{{{pref:name}}}} {{{{clipboard}}}} {{{{date}}}} {{{{time}}}} {{{{datetime}}}} {{{{date:%d.%m.%Y}}}} {{{{calc:2 + 2}}}}

Command TOML:
  name = \"Deploy\"
  mode = \"shell\" # or \"json\" for stdout result lists
  keyword = \"deploy\"
  argument_hint = \"<env> <service>\"
  command = \"deploy --env {{{{arg:env}}}} --service '{{{{arg:service}}}}'\"
  arguments = [
    {{ name = \"env\", type = \"enum\", required = true, options = [\"dev\", \"prod\"] }},
    {{ name = \"service\", type = \"text\", required = true }}
  ]
  [preferences]
  workspace = \"~/Code\"
  [env]
  DEPLOY_TOKEN = \"{{{{pref:deploy_token}}}}\"
  permissions = [\"shell\"]   # enforced: \"shell\", \"network\", \"filesystem\", \"clipboard_write\"
"
    );
}

fn print_actions(actions: &[Action]) {
    for (index, action) in actions.iter().enumerate() {
        if action.subtitle.is_empty() {
            println!("{:>2}. {:<10} {}", index + 1, action.category, action.title);
        } else {
            println!(
                "{:>2}. {:<10} {} - {}",
                index + 1,
                action.category,
                action.title,
                action.subtitle
            );
        }
    }
}

fn flush_stdout() {
    if let Err(error) = io::stdout().flush() {
        eprintln!("failed to flush stdout: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeshicast::ActionRisk;

    #[test]
    fn a_pending_confirmation_is_reported_but_is_not_a_failure() {
        let (to_stderr, note) =
            decision_note(&ExecutionDecision::NeedsConfirmation(ActionRisk::Shell))
                .expect("a note");

        assert!(!to_stderr, "waiting for the GTK UI is not an error");
        assert!(note.contains("confirmation required"), "got {note}");
    }

    #[test]
    fn a_denial_is_reported_on_stderr() {
        let (to_stderr, note) =
            decision_note(&ExecutionDecision::Denied("no shell".to_string())).expect("a note");

        assert!(to_stderr, "a refusal is a failure and belongs on stderr");
        assert!(note.contains("no shell"), "got {note}");
    }

    #[test]
    fn a_run_now_action_needs_no_note() {
        assert!(decision_note(&ExecutionDecision::RunNow).is_none());
    }

    #[test]
    fn export_failure_exits_nonzero() {
        // `/dev/null/x` is not a directory, so the archive cannot be written --
        // and it cannot succeed just because the tests happen to run as root.
        let code = run_cli(CliCommand::Export {
            dest: PathBuf::from("/dev/null/zeshicast-test.tar.gz"),
            include_secrets: Some(false),
            include_history: Some(false),
        });

        assert_ne!(code, 0, "a failed export must not report success");
    }
}

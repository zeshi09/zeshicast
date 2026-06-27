use std::env;
use std::io::{self, Write};
use std::path::PathBuf;

use zeshicast::{Action, SecondaryActionKind, Zeshicast};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return;
    }

    if let Some(pos) = args.iter().position(|a| a == "--export") {
        let dest = args
            .get(pos + 1)
            .filter(|a| !a.starts_with('-'))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("zeshicast-config.tar.gz"));
        let home = env::var("HOME").map(PathBuf::from).unwrap_or_default();
        let config_dir = home.join(".config/zeshicast");
        let include_secrets = args.iter().any(|arg| arg == "--include-secrets");
        match zeshicast::export_config_with_options(&config_dir, &dest, include_secrets) {
            Ok(()) => println!("exported to {}", dest.display()),
            Err(err) => eprintln!("export failed: {err}"),
        }
        return;
    }

    if let Some(pos) = args.iter().position(|a| a == "--import") {
        let Some(src) = args.get(pos + 1).map(PathBuf::from) else {
            eprintln!("usage: zeshicast --import <file.tar.gz>");
            return;
        };
        let home = env::var("HOME").map(PathBuf::from).unwrap_or_default();
        let config_dir = home.join(".config/zeshicast");
        match zeshicast::import_config(&src, &config_dir) {
            Ok(()) => println!("imported from {}", src.display()),
            Err(err) => eprintln!("import failed: {err}"),
        }
        return;
    }

    let mut app = Zeshicast::load();

    if args.is_empty() {
        run_repl(&mut app);
    } else {
        run_once(&app, &args.join(" "));
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
        app.run_action(action);
        return;
    }

    match choice.parse::<usize>() {
        Ok(number) if number > 0 && number <= secondary_actions.len() => {
            let secondary = secondary_actions[number - 1].kind;
            if let Err(error) = app.run_secondary_action(action, secondary) {
                eprintln!("failed to run action: {error}");
            } else if matches!(secondary, SecondaryActionKind::Pin) {
                println!("pinned");
            } else if matches!(secondary, SecondaryActionKind::Unpin) {
                println!("unpinned");
            }
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
  zeshicast --export [file] Export config to tar.gz without API keys by default
  zeshicast --export [file] --include-secrets
                            Export config including API keys and secret-like preferences
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

use std::cell::RefCell;
use std::rc::Rc;

use gtk::Application;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;

const APP_ID: &str = "dev.zeshi.Zeshicast";

// GLib's `glib-unix.h` signal sources are not exposed by the `glib` crate
// (0.22 binds neither `g_unix_signal_add` nor `unix_signal_add_local`), so bind
// the one entry point we need straight to libglib-2.0, which gtk4 always links.
// `g_unix_signal_add*` installs a self-pipe based source on the default main
// context: the handler runs as an ordinary main-loop callback on the main
// thread, so the Rust closure has no async-signal-safety constraints.
type GSourceFunc = Option<unsafe extern "C" fn(*mut std::ffi::c_void) -> i32>;

unsafe extern "C" {
    fn g_unix_signal_add_full(
        priority: i32,
        signum: i32,
        handler: GSourceFunc,
        user_data: *mut std::ffi::c_void,
        notify: Option<unsafe extern "C" fn(*mut std::ffi::c_void)>,
    ) -> u32;
}

/// GLib's `G_PRIORITY_DEFAULT`.
const G_PRIORITY_DEFAULT: i32 = 0;
/// GLib's `G_SOURCE_CONTINUE`: keep the signal source registered.
const G_SOURCE_CONTINUE: i32 = 1;
/// POSIX SIGINT (Ctrl-C).
const SIGINT: i32 = 2;
/// POSIX SIGTERM (`systemctl --user stop`, plain `kill`).
const SIGTERM: i32 = 15;

/// Run `handler` on the default main context every time `signum` is delivered.
/// The source lives until the process exits; the boxed closure is released by
/// GLib through `destroy_notify` when the source dies with its context.
fn install_signal_handler<F: FnMut() + 'static>(signum: i32, handler: F) {
    let data = Box::into_raw(Box::new(handler)).cast::<std::ffi::c_void>();
    // SAFETY: `data` is a valid `Box<F>` handed to GLib ownership; it is freed
    // exactly once by `destroy_notify::<F>` when the source is destroyed.
    unsafe {
        g_unix_signal_add_full(
            G_PRIORITY_DEFAULT,
            signum,
            Some(trampoline::<F>),
            data,
            Some(destroy_notify::<F>),
        );
    }
}

unsafe extern "C" fn trampoline<F: FnMut()>(data: *mut std::ffi::c_void) -> i32 {
    // SAFETY: `data` was created by `install_signal_handler` and is only ever
    // touched by this source, which never re-enters itself.
    unsafe {
        let handler = &mut *data.cast::<F>();
        handler();
    }
    G_SOURCE_CONTINUE
}

unsafe extern "C" fn destroy_notify<F>(data: *mut std::ffi::c_void) {
    // SAFETY: `data` came from `Box::into_raw` in `install_signal_handler` and
    // GLib calls this exactly once when the signal source is destroyed.
    unsafe { drop(Box::from_raw(data.cast::<F>())) };
}

/// Route SIGTERM/SIGINT into the same clean path as `zeshicast-gtk --quit`:
/// `app.quit()` stops the GTK main loop, the D-Bus name is released when the
/// application shuts down, and the background watchers (wl-paste --watch,
/// niri event-stream) end via their existing liveness logic — their channel
/// receivers die with the main loop, so each reconnect probe fails and the
/// watcher threads return instead of racing an abruptly killed process.
fn install_termination_handlers(app: &Application) {
    for signum in [SIGINT, SIGTERM] {
        let app = app.clone();
        install_signal_handler(signum, move || app.quit());
    }
}

fn main() -> glib::ExitCode {
    // Logging first (P4.4): the GUI process is the daemon in `--daemon` mode, so
    // its diagnostics belong in the journal with levels, not on stdout.
    zeshicast::logging::init();
    zeshicast::trace::mark_process_start();

    // GTK4's Vulkan/NGL renderer randomly clips the tops of glyphs on some
    // GPU/driver setups, so cairo (software) is the default. The `ui_renderer`
    // preference can opt into GL (Q2.3); GSK_RENDERER still wins over both.
    if std::env::var_os("GSK_RENDERER").is_none() {
        let renderer = zeshicast::ui::preferred_renderer();
        // Safe: runs at the very start of main, before any threads or GTK init.
        unsafe { std::env::set_var("GSK_RENDERER", renderer) };
    }

    // Q1.4: `--dmenu` is a separate, short-lived picker process. It is
    // handled before the unique application exists, so it never forwards to
    // (or wakes) a running daemon and keeps its own stdin/stdout.
    let args: Vec<String> = std::env::args().collect();
    // Printed here, in the invoking process: inside `connect_command_line` a
    // running daemon handled it, so the text went to the daemon's journal
    // and the terminal stayed empty.
    if wants_help(&args) {
        println!("{}", help_text());
        return glib::ExitCode::SUCCESS;
    }
    // Q1.5: `--cmd` and `zeshicast://` links are validated here, in the
    // invoking process, so a mistake is reported on this terminal with exit
    // 1. Only valid requests are forwarded to the (possibly running) daemon.
    match parse_cmd(&args) {
        Some(Err(error)) => {
            eprintln!("zeshicast-gtk: {error}");
            return glib::ExitCode::FAILURE;
        }
        Some(Ok(CmdRequest::List)) => {
            for line in zeshicast::ui::command_list_lines() {
                println!("{line}");
            }
            return glib::ExitCode::SUCCESS;
        }
        Some(Ok(CmdRequest::Run(id))) => {
            if let Err(error) = zeshicast::ui::check_command_id(&id) {
                eprintln!("zeshicast-gtk: {error}");
                return glib::ExitCode::FAILURE;
            }
        }
        None => {}
    }
    if let Some(link) = deep_link_arg(&args) {
        let checked = zeshicast::deeplink::parse_deep_link(link).and_then(|link| match link {
            zeshicast::deeplink::DeepLink::Command { id } => zeshicast::ui::check_command_id(&id),
            _ => Ok(()),
        });
        if let Err(error) = checked {
            eprintln!("zeshicast-gtk: {error}");
            return glib::ExitCode::FAILURE;
        }
    }

    if let Some(options) = zeshicast::dmenu::parse_dmenu_args(&args) {
        let items = match zeshicast::dmenu::read_items(std::io::stdin().lock()) {
            Ok(items) => items,
            Err(error) => {
                eprintln!("zeshicast-gtk --dmenu: cannot read stdin: {error}");
                return glib::ExitCode::FAILURE;
            }
        };
        return match zeshicast::ui::run_dmenu(options, items) {
            zeshicast::dmenu::EXIT_CHOSEN => glib::ExitCode::SUCCESS,
            _ => glib::ExitCode::FAILURE,
        };
    }

    let state = Rc::new(RefCell::new(None::<zeshicast::ui::GuiState>));
    let hold = Rc::new(RefCell::new(None::<gio::ApplicationHoldGuard>));

    let app = Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    app.connect_startup(|_| {
        zeshicast::ui::install_css();
        // M-9: reap finished fire-and-forget children (launched apps, shell
        // commands, wtype) instead of letting them pile up as <defunct>.
        zeshicast::ui::install_child_reaper();
    });
    {
        let state = Rc::clone(&state);
        let hold = Rc::clone(&hold);
        app.connect_command_line(move |app, command_line| {
            let args: Vec<String> = command_line
                .arguments()
                .into_iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect();

            if args.iter().any(|arg| arg == "--quit") {
                app.quit();
                return glib::ExitCode::SUCCESS;
            }

            let daemon = args.iter().any(|arg| arg == "--daemon");
            if !daemon {
                // Q2.1: closed by the first frame after the window maps.
                zeshicast::trace::begin("show");
            }
            let toggle = wants_toggle(&args);
            let view = parse_view(&args);
            // A palette built by this very call was just presented by
            // `ensure_ui`; toggling it would hide it again at once.
            let existed = state.borrow().is_some();
            zeshicast::ui::ensure_ui(
                app,
                &state,
                &hold,
                daemon,
                zeshicast::ui::configure_launcher,
            );

            if !daemon && let Some(state) = state.borrow().as_ref() {
                // Already validated by the invoking process; an error here
                // means the configuration changed in between.
                let outcome = if let Some(Ok(CmdRequest::Run(id))) = parse_cmd(&args) {
                    zeshicast::ui::run_command_by_id(state, &id)
                } else if let Some(link) = deep_link_arg(&args) {
                    zeshicast::deeplink::parse_deep_link(link)
                        .and_then(|link| zeshicast::ui::open_deep_link(state, &link))
                } else {
                    if toggle && existed {
                        zeshicast::ui::toggle_launcher(state, &hold, view.as_deref());
                    } else {
                        zeshicast::ui::present_launcher_view(state, view.as_deref());
                    }
                    Ok(())
                };
                if let Err(error) = outcome {
                    log::warn!("command line request failed: {error}");
                }
            }

            glib::ExitCode::SUCCESS
        });
    }

    // P2-4 graceful shutdown: systemd stop / Ctrl-C must not kill us between
    // writes; turn the signals into the ordinary quit path instead.
    install_termination_handlers(&app);
    app.connect_shutdown(|_| zeshicast::trace::summary());

    app.run()
}

fn wants_help(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--help" || arg == "-h")
}

/// `--cmd list` / `--cmd run <id>` (Q1.5).
#[derive(Debug, PartialEq, Eq)]
enum CmdRequest {
    List,
    Run(String),
}

fn parse_cmd(args: &[String]) -> Option<Result<CmdRequest, String>> {
    let position = args.iter().position(|arg| arg == "--cmd")?;
    let usage = "usage: --cmd list | --cmd run <id>";
    Some(match args.get(position + 1).map(String::as_str) {
        Some("list") => Ok(CmdRequest::List),
        Some("run") => match args.get(position + 2) {
            Some(id) if !id.trim().is_empty() && !id.starts_with("--") => {
                Ok(CmdRequest::Run(id.trim().to_string()))
            }
            _ => Err(format!("missing command id; {usage}")),
        },
        _ => Err(usage.to_string()),
    })
}

/// The first `zeshicast://` argument (`%u` from the desktop file).
fn deep_link_arg(args: &[String]) -> Option<&str> {
    args.iter()
        .map(String::as_str)
        .find(|arg| zeshicast::deeplink::is_deep_link(arg))
}

/// `--toggle`: hide the palette when it is shown, show it when hidden (Q1.2).
fn wants_toggle(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--toggle")
}

/// Resolve a requested start-up view from the command line.
///
/// Accepts both `--view <name>` / `--view=<name>` and per-view convenience
/// flags (`--clipboard`, `--dashboard`, …). Returns the canonical view name
/// understood by `present_launcher_view`.
fn parse_view(args: &[String]) -> Option<String> {
    fn canonical(name: &str) -> Option<&'static str> {
        match name.trim().to_ascii_lowercase().as_str() {
            "clipboard" | "clip" => Some("clipboard"),
            "dashboard" | "dash" => Some("dashboard"),
            "network" | "net" | "wifi" => Some("network"),
            "media" | "player" | "mpris" => Some("media"),
            "audio" | "volume" | "vol" => Some("audio"),
            "ai" | "ai-chat" | "chat" => Some("ai"),
            "system" | "system-monitor" | "sysmon" | "monitor" => Some("system"),
            "notifications" | "notify" | "notifs" => Some("notifications"),
            "emoji" | "emojis" => Some("emoji"),
            "fonts" | "font" => Some("fonts"),
            _ => None,
        }
    }

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if let Some(value) = arg.strip_prefix("--view=") {
            if let Some(view) = canonical(value) {
                return Some(view.to_string());
            }
        } else if arg == "--view" {
            if let Some(value) = iter.next()
                && let Some(view) = canonical(value)
            {
                return Some(view.to_string());
            }
        } else if let Some(flag) = arg.strip_prefix("--")
            && let Some(view) = canonical(flag)
        {
            return Some(view.to_string());
        }
    }
    None
}

fn help_text() -> &'static str {
    "\
Usage:
  zeshicast-gtk            Show the launcher window
  zeshicast-gtk --daemon   Start hidden, keep the index warm, record clipboard history
  zeshicast-gtk --toggle   Show the launcher, or hide it if it is already shown
                           (bind this to your launcher hotkey)
  zeshicast-gtk --quit     Stop the running daemon

Commands and links:
  zeshicast-gtk --cmd list         Print `id<TAB>name` for commands and scripts
  zeshicast-gtk --cmd run <id>     Run one as if picked and confirmed with Enter
                                   (risky ones still ask; forms still open)
  zeshicast-gtk zeshicast://view/<name>
  zeshicast-gtk zeshicast://search?q=<text>
  zeshicast-gtk zeshicast://command/<id>
                                   Links only open the palette; a command link
                                   puts the command in the search, Enter runs it

Pick a line from stdin (dmenu/rofi style; prints the choice, exit 1 on Esc):
  zeshicast-gtk --dmenu [--prompt TEXT | -p TEXT] [--password]
                           e.g.  git branch --format='%(refname:short)' |
                                 zeshicast-gtk --dmenu -p Branch

Open a specific view directly (works against a running --daemon too):
  --view <name>            clipboard | dashboard | network | media | audio
                           ai | system | notifications | emoji | fonts
  --clipboard, --dashboard, --network, --media, --audio,
  --ai, --system, --notifications, --emoji, --fonts
                           Convenience flags equivalent to --view <name>

In the window:
  Enter                   Run selected result (opens form panel for commands with missing args)
  Ctrl+Enter              Copy selected result value
  Ctrl+K                  Open searchable action panel (pin, alias, secondary actions)
  Ctrl+H                  Open clipboard history view
  Delete                  In clipboard history: delete selected item
  Ctrl+Delete             In clipboard history: clear history
  Ctrl+S                  Open snippet manager
  Delete                  In snippet manager: delete selected snippet
  Ctrl+D                  Open dashboard
  Ctrl+I                  Open local AI chat
  Ctrl+M                  Open media status
  Ctrl+O                  Open audio mixer (output/input devices and volumes)
  Ctrl+N                  Open network status
  Ctrl+B                  Open extension browser (list all custom commands)
  Ctrl+,                  Open preferences editor (AI endpoint, model, translate settings)
  Esc                     Hide in daemon mode, otherwise quit
  Up/Down                 Move selection

  ai <text>               Ask local AI through Ollama; response copied to clipboard
  trans <text> in <lang>  Translate via LibreTranslate — result copied to clipboard
  shell <cmd>             Run an arbitrary shell command
  system / sys            System actions (lock, suspend, reboot, power off)
  audio / vol / volume    Audio/brightness actions
  media / player / mpris   MPRIS playback controls over D-Bus
  notify / dnd            Notification history and DND (built-in D-Bus server)
  net / wifi / network    Network actions
  niri                    Niri compositor actions
  clip / clipboard        Search clipboard history
  file / find             Search indexed files
  proc / process          Search and kill processes
"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Linux SIGUSR1 — unused by the daemon, safe to raise in a test.
    const SIGUSR1: i32 = 10;

    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }

    /// End-to-end check of the FFI plumbing behind the SIGTERM/SIGINT quit
    /// handlers: a delivered signal must run our Rust closure as an ordinary
    /// callback on the default main context (the SIGINT/SIGTERM path itself
    /// cannot be raised in unit tests without killing the test harness).
    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_string()).collect()
    }

    #[test]
    fn toggle_flag_is_recognised_alongside_a_view() {
        assert!(wants_toggle(&args(&["zeshicast-gtk", "--toggle"])));
        assert!(wants_toggle(&args(&[
            "zeshicast-gtk",
            "--toggle",
            "--clipboard"
        ])));
        assert_eq!(
            parse_view(&args(&["zeshicast-gtk", "--toggle", "--clipboard"])).as_deref(),
            Some("clipboard")
        );
        assert!(!wants_toggle(&args(&["zeshicast-gtk"])));
        assert!(!wants_toggle(&args(&["zeshicast-gtk", "--daemon"])));
        assert!(help_text().contains("--toggle"));
    }

    #[test]
    fn cmd_requests_are_parsed_and_mistakes_explained() {
        assert_eq!(parse_cmd(&args(&["zeshicast-gtk"])), None);
        assert_eq!(
            parse_cmd(&args(&["zeshicast-gtk", "--cmd", "list"])),
            Some(Ok(CmdRequest::List))
        );
        assert_eq!(
            parse_cmd(&args(&[
                "zeshicast-gtk",
                "--cmd",
                "run",
                "command/docker-ps"
            ])),
            Some(Ok(CmdRequest::Run("command/docker-ps".to_string())))
        );
        for bad in [
            &["zeshicast-gtk", "--cmd"][..],
            &["zeshicast-gtk", "--cmd", "launch", "x"],
            &["zeshicast-gtk", "--cmd", "run"],
            &["zeshicast-gtk", "--cmd", "run", "--toggle"],
        ] {
            assert!(matches!(parse_cmd(&args(bad)), Some(Err(_))), "{bad:?}");
        }
    }

    #[test]
    fn the_first_deep_link_argument_is_found() {
        assert_eq!(
            deep_link_arg(&args(&["zeshicast-gtk", "zeshicast://view/clipboard"])),
            Some("zeshicast://view/clipboard")
        );
        assert_eq!(deep_link_arg(&args(&["zeshicast-gtk", "--toggle"])), None);
    }

    #[test]
    fn help_is_recognised_in_both_spellings() {
        assert!(wants_help(&args(&["zeshicast-gtk", "--help"])));
        assert!(wants_help(&args(&["zeshicast-gtk", "-h"])));
        assert!(!wants_help(&args(&["zeshicast-gtk", "--toggle"])));
        assert!(help_text().contains("--dmenu"));
    }

    #[test]
    fn toggle_hides_a_visible_palette_and_shows_a_hidden_one() {
        use zeshicast::ui::{LauncherToggle, launcher_toggle_action};

        assert_eq!(launcher_toggle_action(true), LauncherToggle::Hide);
        assert_eq!(launcher_toggle_action(false), LauncherToggle::Show);
    }

    #[test]
    fn unix_signal_handler_runs_on_the_default_main_context() {
        let handled = Arc::new(AtomicBool::new(false));

        let main_loop = glib::MainLoop::new(None, false);
        {
            let main_loop = main_loop.clone();
            let handled = Arc::clone(&handled);
            install_signal_handler(SIGUSR1, move || {
                handled.store(true, Ordering::Relaxed);
                main_loop.quit();
            });
        }

        // Fail-safe: stop even if the signal never dispatches.
        {
            let main_loop = main_loop.clone();
            glib::timeout_add_local(std::time::Duration::from_secs(5), move || {
                main_loop.quit();
                glib::ControlFlow::Break
            });
        }

        // SAFETY: plain libc kill with valid arguments.
        assert_eq!(
            unsafe { kill(std::process::id() as i32, SIGUSR1) },
            0,
            "failed to raise the test signal"
        );

        main_loop.run();

        assert!(
            handled.load(Ordering::Relaxed),
            "signal handler must have run before the main loop stopped"
        );
    }
}

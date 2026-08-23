use std::cell::RefCell;
use std::rc::Rc;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow};

const APP_ID: &str = "dev.zeshi.Zeshicast";

// GTK layer shell enum values from the C headers
#[cfg(feature = "layer-shell")]
const GTK_LAYER_SHELL_LAYER_OVERLAY: u32 = 3;
#[cfg(feature = "layer-shell")]
const GTK_LAYER_SHELL_KEYBOARD_MODE_EXCLUSIVE: u32 = 1;

#[cfg(feature = "layer-shell")]
unsafe extern "C" {
    fn gtk_layer_is_supported() -> i32;
    fn gtk_layer_init_for_window(window: *mut std::ffi::c_void);
    fn gtk_layer_set_layer(window: *mut std::ffi::c_void, layer: u32);
    fn gtk_layer_set_keyboard_mode(window: *mut std::ffi::c_void, mode: u32);
}

#[cfg(feature = "layer-shell")]
fn configure_layer_shell(window: &ApplicationWindow) {
    let ptr = window.as_ptr() as *mut std::ffi::c_void;
    unsafe {
        if gtk_layer_is_supported() == 0 {
            return;
        }
        gtk_layer_init_for_window(ptr);
        gtk_layer_set_layer(ptr, GTK_LAYER_SHELL_LAYER_OVERLAY);
        gtk_layer_set_keyboard_mode(ptr, GTK_LAYER_SHELL_KEYBOARD_MODE_EXCLUSIVE);
    }
}

#[cfg(not(feature = "layer-shell"))]
fn configure_layer_shell(_window: &ApplicationWindow) {}

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
    // GTK4's Vulkan/NGL renderer randomly clips the tops of glyphs on some
    // GPU/driver setups; the cairo (software) renderer is glitch-free and plenty
    // fast for a launcher. Default to it, but let the user override.
    if std::env::var_os("GSK_RENDERER").is_none() {
        // Safe: runs at the very start of main, before any threads or GTK init.
        unsafe { std::env::set_var("GSK_RENDERER", "cairo") };
    }

    let state = Rc::new(RefCell::new(None::<zeshicast::ui::GuiState>));
    let hold = Rc::new(RefCell::new(None::<gio::ApplicationHoldGuard>));

    let app = Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    app.connect_startup(|_| zeshicast::ui::install_css());
    {
        let state = Rc::clone(&state);
        let hold = Rc::clone(&hold);
        app.connect_command_line(move |app, command_line| {
            let args: Vec<String> = command_line
                .arguments()
                .into_iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect();

            if args.iter().any(|arg| arg == "--help" || arg == "-h") {
                println!("{}", help_text());
                return glib::ExitCode::SUCCESS;
            }

            if args.iter().any(|arg| arg == "--quit") {
                app.quit();
                return glib::ExitCode::SUCCESS;
            }

            let daemon = args.iter().any(|arg| arg == "--daemon");
            let view = parse_view(&args);
            zeshicast::ui::ensure_ui(app, &state, &hold, daemon, configure_layer_shell);

            if !daemon && let Some(state) = state.borrow().as_ref() {
                zeshicast::ui::present_launcher_view(state, view.as_deref());
            }

            glib::ExitCode::SUCCESS
        });
    }

    // P2-4 graceful shutdown: systemd stop / Ctrl-C must not kill us between
    // writes; turn the signals into the ordinary quit path instead.
    install_termination_handlers(&app);

    app.run()
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
  zeshicast-gtk --quit     Stop the running daemon

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

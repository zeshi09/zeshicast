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

            if !daemon {
                if let Some(state) = state.borrow().as_ref() {
                    zeshicast::ui::present_launcher_view(state, view.as_deref());
                }
            }

            glib::ExitCode::SUCCESS
        });
    }

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
            if let Some(value) = iter.next() {
                if let Some(view) = canonical(value) {
                    return Some(view.to_string());
                }
            }
        } else if let Some(flag) = arg.strip_prefix("--") {
            if let Some(view) = canonical(flag) {
                return Some(view.to_string());
            }
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

Prefix searches:
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

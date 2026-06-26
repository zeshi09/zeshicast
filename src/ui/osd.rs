//! Lightweight on-screen-display popups rendered as their own focus-less
//! layer-shell surface, independent of the launcher window. Currently used for
//! the keyboard-layout pill; the same surface can back notification toasts.

use std::cell::RefCell;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use gtk::{Align, Application, Box as GtkBox, Label, Orientation, Revealer, Window};

/// How long the pill stays fully shown before it fades out.
const VISIBLE_MS: u64 = 850;
/// Fade in/out duration (also the GtkRevealer crossfade time).
const FADE_MS: u32 = 200;

struct Osd {
    window: Window,
    label: Label,
    revealer: Revealer,
    /// Bumped on every show so stale dismiss timers from earlier shows no-op.
    generation: u64,
}

thread_local! {
    static LAYOUT_OSD: RefCell<Option<Osd>> = const { RefCell::new(None) };
}

/// Flash a centered pill with the given keyboard-layout code (e.g. "RU").
/// Reuses a single surface, so rapid layout switches just refresh the text and
/// restart the timer instead of stacking windows.
pub fn show_layout_osd(app: &Application, code: &str) {
    let code = code.trim();
    if code.is_empty() {
        return;
    }

    let generation = LAYOUT_OSD.with(|cell| {
        let mut slot = cell.borrow_mut();
        let osd = slot.get_or_insert_with(|| build_osd(app));
        osd.generation = osd.generation.wrapping_add(1);
        osd.label.set_text(&code.to_uppercase());
        osd.window.set_visible(true);
        osd.revealer.set_reveal_child(true);
        osd.generation
    });

    // Fade out after the dwell time, then hide once the crossfade finished —
    // but only if no newer show has happened in the meantime.
    glib::timeout_add_local_once(Duration::from_millis(VISIBLE_MS), move || {
        let still_current = LAYOUT_OSD.with(|cell| {
            if let Some(osd) = cell.borrow().as_ref()
                && osd.generation == generation
            {
                osd.revealer.set_reveal_child(false);
                return true;
            }
            false
        });
        if !still_current {
            return;
        }
        glib::timeout_add_local_once(Duration::from_millis(FADE_MS as u64 + 40), move || {
            LAYOUT_OSD.with(|cell| {
                if let Some(osd) = cell.borrow().as_ref()
                    && osd.generation == generation
                {
                    osd.window.set_visible(false);
                }
            });
        });
    });
}

fn build_osd(app: &Application) -> Osd {
    let window = Window::builder()
        .application(app)
        .decorated(false)
        .resizable(false)
        .build();
    window.add_css_class("osd-window");
    configure_layer_shell(&window);

    let label = Label::new(None);
    label.add_css_class("osd-pill-label");

    let pill = GtkBox::new(Orientation::Horizontal, 0);
    pill.add_css_class("osd-pill");
    pill.set_halign(Align::Center);
    pill.set_valign(Align::Center);
    pill.append(&label);

    let revealer = Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::Crossfade)
        .transition_duration(FADE_MS)
        .reveal_child(false)
        .child(&pill)
        .build();

    window.set_child(Some(&revealer));

    Osd {
        window,
        label,
        revealer,
        generation: 0,
    }
}

// ── Layer-shell: overlay, centered, never grabs the keyboard ─────────────────

#[cfg(feature = "layer-shell")]
fn configure_layer_shell(window: &Window) {
    const LAYER_OVERLAY: u32 = 3;
    const KEYBOARD_MODE_NONE: u32 = 0;

    unsafe extern "C" {
        fn gtk_layer_is_supported() -> i32;
        fn gtk_layer_init_for_window(window: *mut std::ffi::c_void);
        fn gtk_layer_set_layer(window: *mut std::ffi::c_void, layer: u32);
        fn gtk_layer_set_keyboard_mode(window: *mut std::ffi::c_void, mode: u32);
        fn gtk_layer_set_namespace(window: *mut std::ffi::c_void, name_space: *const i8);
    }

    let ptr = window.as_ptr() as *mut std::ffi::c_void;
    unsafe {
        if gtk_layer_is_supported() == 0 {
            return;
        }
        gtk_layer_init_for_window(ptr);
        gtk_layer_set_layer(ptr, LAYER_OVERLAY);
        // No anchors set → the compositor centers the surface.
        gtk_layer_set_keyboard_mode(ptr, KEYBOARD_MODE_NONE);
        let ns = c"zeshicast-osd";
        gtk_layer_set_namespace(ptr, ns.as_ptr());
    }
}

#[cfg(not(feature = "layer-shell"))]
fn configure_layer_shell(_window: &Window) {}

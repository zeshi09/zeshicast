//! Lightweight on-screen-display popups rendered as their own focus-less
//! layer-shell surface, independent of the launcher window. Used for
//! the keyboard-layout pill and notification toasts.

use std::cell::RefCell;
use std::time::Duration;

use gtk::{gio, glib};
use gtk::prelude::*;
use gtk::{Align, Application, Box as GtkBox, Button, Label, Orientation, Revealer, Window};

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

// ── Notification OSD (toast in top-right corner) ─────────────────────────────

struct NotificationOsd {
    window: Window,
    revealer: Revealer,
    icon_box: GtkBox,
    app_label: Label,
    summary_label: Label,
    body_label: Label,
    generation: u64,
    is_hovered: std::rc::Rc<std::cell::Cell<bool>>,
}

thread_local! {
    static NOTIFICATION_OSD: RefCell<Option<NotificationOsd>> = const { RefCell::new(None) };
}

/// Show a notification toast anchored in the top-right corner.
/// Reuses a single surface to prevent stacking windows on high notification volume.
pub fn show_notification_osd(
    app: Option<&Application>,
    app_name: &str,
    summary: &str,
    body: &str,
    app_icon: &str,
    expire_timeout_ms: i32,
) {
    let default_app = gio::Application::default().and_then(|a| a.downcast::<Application>().ok());
    let app = app.or(default_app.as_ref());

    let dwell_ms = match expire_timeout_ms {
        ms if ms > 0 => (ms as u64).clamp(1500, 30_000),
        0 => 8_000,
        _ => 4_500,
    };

    let generation = NOTIFICATION_OSD.with(|cell| {
        let mut slot = cell.borrow_mut();
        let osd = slot.get_or_insert_with(|| build_notification_osd(app));
        osd.generation = osd.generation.wrapping_add(1);

        let app_display = if app_name.trim().is_empty() {
            "Notification"
        } else {
            app_name.trim()
        };
        osd.app_label.set_text(app_display);

        let summary_trimmed = summary.trim();
        osd.summary_label.set_text(summary_trimmed);
        osd.summary_label.set_visible(!summary_trimmed.is_empty());

        let body_trimmed = body.trim();
        osd.body_label.set_text(body_trimmed);
        osd.body_label.set_visible(!body_trimmed.is_empty());

        update_icon(&osd.icon_box, app_name, app_icon);

        osd.window.set_visible(true);
        osd.revealer.set_reveal_child(true);
        osd.generation
    });

    schedule_dismiss(generation, dwell_ms);
}

/// Dismiss the currently displayed notification toast with a smooth crossfade.
pub fn dismiss_notification_osd() {
    let generation = NOTIFICATION_OSD.with(|cell| {
        if let Some(osd) = cell.borrow_mut().as_mut() {
            if !osd.revealer.reveals_child() {
                return 0;
            }
            osd.generation = osd.generation.wrapping_add(1);
            osd.revealer.set_reveal_child(false);
            osd.is_hovered.set(false);
            osd.generation
        } else {
            0
        }
    });

    if generation > 0 {
        glib::timeout_add_local_once(Duration::from_millis(FADE_MS as u64 + 40), move || {
            NOTIFICATION_OSD.with(|cell| {
                if let Some(osd) = cell.borrow().as_ref()
                    && osd.generation == generation
                {
                    osd.window.set_visible(false);
                }
            });
        });
    }
}

fn schedule_dismiss(generation: u64, delay_ms: u64) {
    glib::timeout_add_local_once(Duration::from_millis(delay_ms), move || {
        let (still_current, hovered) = NOTIFICATION_OSD.with(|cell| {
            if let Some(osd) = cell.borrow().as_ref()
                && osd.generation == generation
            {
                (true, osd.is_hovered.get())
            } else {
                (false, false)
            }
        });

        if !still_current || hovered {
            return;
        }

        NOTIFICATION_OSD.with(|cell| {
            if let Some(osd) = cell.borrow().as_ref()
                && osd.generation == generation
            {
                osd.revealer.set_reveal_child(false);
            }
        });

        glib::timeout_add_local_once(Duration::from_millis(FADE_MS as u64 + 40), move || {
            NOTIFICATION_OSD.with(|cell| {
                if let Some(osd) = cell.borrow().as_ref()
                    && osd.generation == generation
                {
                    osd.window.set_visible(false);
                }
            });
        });
    });
}

fn update_icon(icon_box: &GtkBox, app_name: &str, app_icon: &str) {
    while let Some(child) = icon_box.first_child() {
        icon_box.remove(&child);
    }

    let icon_trimmed = app_icon.trim();
    if !icon_trimmed.is_empty() {
        if icon_trimmed.starts_with('/') || icon_trimmed.starts_with("file://") {
            let path_str = icon_trimmed.strip_prefix("file://").unwrap_or(icon_trimmed);
            let path = std::path::Path::new(path_str);
            if path.exists() {
                let img = gtk::Image::from_file(path);
                img.set_pixel_size(32);
                icon_box.append(&img);
                return;
            }
        }
        let display = gtk::gdk::Display::default();
        let has_theme_icon = display
            .as_ref()
            .map(|d| gtk::IconTheme::for_display(d).has_icon(icon_trimmed))
            .unwrap_or(false);
        if has_theme_icon {
            let img = gtk::Image::from_icon_name(icon_trimmed);
            img.set_pixel_size(32);
            icon_box.append(&img);
            return;
        }
    }

    let name = if app_name.trim().is_empty() {
        "App"
    } else {
        app_name.trim()
    };
    let letter = crate::ui::letter_icon(name, 32);
    icon_box.append(&letter);
}

fn build_notification_osd(app: Option<&Application>) -> NotificationOsd {
    let mut builder = Window::builder()
        .decorated(false)
        .resizable(false);
    if let Some(app) = app {
        builder = builder.application(app);
    }
    let window = builder.build();
    window.set_default_size(1, 1);
    window.add_css_class("osd-window");
    configure_notification_layer_shell(&window);

    let card = GtkBox::new(Orientation::Horizontal, 12);
    card.add_css_class("osd-notification");
    card.set_halign(Align::End);
    card.set_valign(Align::Start);

    let icon_box = GtkBox::new(Orientation::Vertical, 0);
    icon_box.set_valign(Align::Start);
    icon_box.set_size_request(32, 32);
    card.append(&icon_box);

    let text_col = GtkBox::new(Orientation::Vertical, 2);
    text_col.set_hexpand(true);
    text_col.set_valign(Align::Center);

    let header_row = GtkBox::new(Orientation::Horizontal, 6);
    let app_label = Label::new(None);
    app_label.add_css_class("osd-notification-app");
    app_label.set_xalign(0.0);
    app_label.set_hexpand(true);
    app_label.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let close_btn = Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("Dismiss notification")
        .has_frame(false)
        .valign(Align::Center)
        .halign(Align::End)
        .build();
    close_btn.add_css_class("osd-notification-close");
    close_btn.connect_clicked(|_| {
        dismiss_notification_osd();
    });

    header_row.append(&app_label);
    header_row.append(&close_btn);
    text_col.append(&header_row);

    let summary_label = Label::new(None);
    summary_label.add_css_class("osd-notification-title");
    summary_label.set_xalign(0.0);
    summary_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    summary_label.set_max_width_chars(38);
    text_col.append(&summary_label);

    let body_label = Label::new(None);
    body_label.add_css_class("osd-notification-body");
    body_label.set_xalign(0.0);
    body_label.set_wrap(true);
    body_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    body_label.set_lines(4);
    body_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    body_label.set_max_width_chars(38);
    text_col.append(&body_label);

    card.append(&text_col);

    let click = gtk::GestureClick::new();
    click.connect_released(|_, _, _, _| {
        dismiss_notification_osd();
    });
    card.add_controller(click);

    let is_hovered = std::rc::Rc::new(std::cell::Cell::new(false));
    let motion = gtk::EventControllerMotion::new();
    {
        let is_hovered = is_hovered.clone();
        motion.connect_enter(move |_, _, _| {
            is_hovered.set(true);
        });
    }
    {
        let is_hovered = is_hovered.clone();
        motion.connect_leave(move |_| {
            is_hovered.set(false);
            let current_generation = NOTIFICATION_OSD.with(|cell| {
                cell.borrow().as_ref().map(|o| o.generation).unwrap_or(0)
            });
            if current_generation > 0 {
                schedule_dismiss(current_generation, 1800);
            }
        });
    }
    card.add_controller(motion);

    let revealer = Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::Crossfade)
        .transition_duration(FADE_MS)
        .reveal_child(false)
        .child(&card)
        .build();
    revealer.set_halign(Align::End);
    revealer.set_valign(Align::Start);

    window.set_child(Some(&revealer));

    NotificationOsd {
        window,
        revealer,
        icon_box,
        app_label,
        summary_label,
        body_label,
        generation: 0,
        is_hovered,
    }
}

// ── Layer-shell configuration ────────────────────────────────────────────────

#[cfg(feature = "layer-shell")]
unsafe extern "C" {
    fn gtk_layer_is_supported() -> i32;
    fn gtk_layer_init_for_window(window: *mut std::ffi::c_void);
    fn gtk_layer_set_layer(window: *mut std::ffi::c_void, layer: u32);
    fn gtk_layer_set_keyboard_mode(window: *mut std::ffi::c_void, mode: u32);
    fn gtk_layer_set_namespace(window: *mut std::ffi::c_void, name_space: *const i8);
    fn gtk_layer_set_anchor(window: *mut std::ffi::c_void, edge: u32, anchor_to_edge: i32);
    fn gtk_layer_set_margin(window: *mut std::ffi::c_void, edge: u32, margin_size: i32);
}

#[cfg(feature = "layer-shell")]
fn configure_layer_shell(window: &Window) {
    const LAYER_OVERLAY: u32 = 3;
    const KEYBOARD_MODE_NONE: u32 = 0;

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

#[cfg(feature = "layer-shell")]
fn configure_notification_layer_shell(window: &Window) {
    const LAYER_OVERLAY: u32 = 3;
    const KEYBOARD_MODE_NONE: u32 = 0;
    const EDGE_RIGHT: u32 = 1;
    const EDGE_TOP: u32 = 2;

    let ptr = window.as_ptr() as *mut std::ffi::c_void;
    unsafe {
        if gtk_layer_is_supported() == 0 {
            return;
        }
        gtk_layer_init_for_window(ptr);
        gtk_layer_set_layer(ptr, LAYER_OVERLAY);
        gtk_layer_set_keyboard_mode(ptr, KEYBOARD_MODE_NONE);
        let ns = c"zeshicast-notification";
        gtk_layer_set_namespace(ptr, ns.as_ptr());
        gtk_layer_set_anchor(ptr, EDGE_TOP, 1);
        gtk_layer_set_anchor(ptr, EDGE_RIGHT, 1);
        gtk_layer_set_margin(ptr, EDGE_TOP, 8);
        gtk_layer_set_margin(ptr, EDGE_RIGHT, 8);
    }
}

#[cfg(not(feature = "layer-shell"))]
fn configure_notification_layer_shell(_window: &Window) {}

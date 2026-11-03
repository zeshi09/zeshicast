//! Lightweight on-screen-display popups rendered as their own focus-less
//! layer-shell surface, independent of the launcher window. Used for
//! the keyboard-layout pill, volume/brightness levels and notification toasts.

use std::cell::RefCell;
use std::time::Duration;

use gtk::prelude::*;
use gtk::{Align, Application, Box as GtkBox, Button, Label, Orientation, Revealer, Window};
use gtk::{gio, glib};

/// How long the pill stays fully shown before it fades out.
const VISIBLE_MS: u64 = 850;
/// Fade in/out duration (also the GtkRevealer crossfade time).
const FADE_MS: u32 = 200;

/// Refuse to load an icon file larger than this. The path is confined to the
/// icon directories, but a large file there would still freeze decode (N-7).
const MAX_ICON_BYTES: u64 = 512 * 1024;

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
    super::layer_shell::configure_osd(&window);

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

// ── Level OSD (volume, microphone, brightness) ───────────────────────────────

/// What a level OSD shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelKind {
    Volume,
    Microphone,
    Brightness,
}

/// One level to show: `percent` may exceed 100 (PipeWire allows boost).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    pub kind: LevelKind,
    pub percent: u16,
    pub muted: bool,
}

impl Level {
    /// The symbolic icon for this level, as the GNOME OSD picks them.
    pub(crate) fn icon_name(&self) -> &'static str {
        match self.kind {
            LevelKind::Volume => match (self.muted, self.percent) {
                (true, _) | (_, 0) => "audio-volume-muted-symbolic",
                (_, 1..=33) => "audio-volume-low-symbolic",
                (_, 34..=66) => "audio-volume-medium-symbolic",
                (_, 67..=100) => "audio-volume-high-symbolic",
                _ => "audio-volume-overamplified-symbolic",
            },
            LevelKind::Microphone => {
                if self.muted {
                    "microphone-sensitivity-muted-symbolic"
                } else {
                    "audio-input-microphone-symbolic"
                }
            }
            LevelKind::Brightness => "display-brightness-symbolic",
        }
    }

    /// The bar's fill, 0.0–1.0; a muted level shows empty.
    pub(crate) fn fraction(&self) -> f64 {
        if self.muted {
            0.0
        } else {
            (f64::from(self.percent) / 100.0).clamp(0.0, 1.0)
        }
    }

    pub(crate) fn text(&self) -> String {
        if self.muted {
            "Muted".to_string()
        } else {
            format!("{}%", self.percent)
        }
    }
}

/// How long a level stays after its last change.
const LEVEL_VISIBLE_MS: u64 = 1200;
/// How long the bar takes to glide to a new value.
const LEVEL_GLIDE_MS: f64 = 140.0;

struct LevelOsd {
    window: Window,
    pill: GtkBox,
    icon: gtk::Image,
    value: Label,
    /// (shown, target) fraction; the tick callback eases shown → target.
    bar: std::rc::Rc<std::cell::Cell<(f64, f64)>>,
    area: gtk::DrawingArea,
    generation: u64,
}

thread_local! {
    static LEVEL_OSD: RefCell<Option<LevelOsd>> = const { RefCell::new(None) };
}

/// Show (or update) the level OSD at the bottom center. A burst of changes
/// moves one OSD's bar instead of stacking windows; it hides
/// [`LEVEL_VISIBLE_MS`] after the last one. Shown under Do Not Disturb too:
/// it answers a key press, it is not a notification.
pub fn show_level_osd(app: Option<&Application>, level: Level) {
    let Some(app) = app
        .cloned()
        .or_else(|| gio::Application::default().and_then(|a| a.downcast::<Application>().ok()))
    else {
        return;
    };
    let generation = LEVEL_OSD.with(|cell| {
        let mut slot = cell.borrow_mut();
        let osd = slot.get_or_insert_with(|| build_level_osd(&app));
        osd.generation = osd.generation.wrapping_add(1);
        osd.icon.set_icon_name(Some(level.icon_name()));
        osd.value.set_text(&level.text());
        let (shown, _) = osd.bar.get();
        let first_show = !osd.window.is_visible();
        // A fresh OSD starts at its value; a visible one glides there.
        osd.bar.set((
            if first_show { level.fraction() } else { shown },
            level.fraction(),
        ));
        osd.area.queue_draw();
        if level.muted {
            osd.pill.add_css_class("muted");
        } else {
            osd.pill.remove_css_class("muted");
        }
        osd.window.set_visible(true);
        // Next frame, so the CSS transition runs from the hidden state.
        let pill = osd.pill.clone();
        glib::idle_add_local_once(move || pill.add_css_class("shown"));
        osd.generation
    });

    glib::timeout_add_local_once(Duration::from_millis(LEVEL_VISIBLE_MS), move || {
        let still_current = LEVEL_OSD.with(|cell| {
            if let Some(osd) = cell.borrow().as_ref()
                && osd.generation == generation
            {
                osd.pill.remove_css_class("shown");
                return true;
            }
            false
        });
        if !still_current {
            return;
        }
        glib::timeout_add_local_once(Duration::from_millis(FADE_MS as u64 + 60), move || {
            LEVEL_OSD.with(|cell| {
                if let Some(osd) = cell.borrow().as_ref()
                    && osd.generation == generation
                {
                    osd.window.set_visible(false);
                }
            });
        });
    });
}

/// `--volume …` / `--mic-mute` (O2): change the level with `wpctl` and show
/// the result at once, without waiting for the poller.
pub fn change_volume(app: &Application, request: crate::services::audio::VolumeRequest) {
    use crate::services::audio::{DEFAULT_VOLUME_MAX, VolumeRequest, apply_volume_request};
    let max = crate::config::load_global_preferences(
        &crate::config::home_dir().join(".config/zeshicast"),
    )
    .get("volume_max")
    .and_then(|value| value.trim().parse::<u8>().ok())
    .unwrap_or(DEFAULT_VOLUME_MAX);
    let Some(device) = apply_volume_request(request, max) else {
        log::warn!("wpctl could not apply {request:?}");
        return;
    };
    let kind = match request {
        VolumeRequest::ToggleMicMute => LevelKind::Microphone,
        _ => LevelKind::Volume,
    };
    show_level_osd(
        Some(app),
        Level {
            kind,
            percent: u16::from(device.volume_percent),
            muted: device.muted,
        },
    );
    crate::services::poll_cache::refresh_now();
}

fn build_level_osd(app: &Application) -> LevelOsd {
    let window = Window::builder()
        .application(app)
        .decorated(false)
        .resizable(false)
        .build();
    window.add_css_class("osd-window");
    super::layer_shell::configure_level_osd(&window);

    let icon = gtk::Image::from_icon_name("audio-volume-medium-symbolic");
    icon.set_pixel_size(22);
    icon.add_css_class("osd-level-icon");

    let bar = std::rc::Rc::new(std::cell::Cell::new((0.0_f64, 0.0_f64)));
    let area = gtk::DrawingArea::new();
    area.set_content_width(220);
    area.set_content_height(8);
    area.set_valign(Align::Center);
    area.add_css_class("osd-level-bar");
    {
        let bar = std::rc::Rc::clone(&bar);
        area.set_draw_func(move |area, cr, width, height| {
            let (shown, _) = bar.get();
            let (w, h) = (f64::from(width), f64::from(height));
            let radius = h / 2.0;
            let rounded = |cr: &gtk::cairo::Context, width: f64| {
                cr.new_sub_path();
                cr.arc(
                    width - radius,
                    radius,
                    radius,
                    -std::f64::consts::FRAC_PI_2,
                    std::f64::consts::FRAC_PI_2,
                );
                cr.arc(
                    radius,
                    radius,
                    radius,
                    std::f64::consts::FRAC_PI_2,
                    3.0 * std::f64::consts::FRAC_PI_2,
                );
                cr.close_path();
            };
            // `WidgetExt::color` needs GTK 4.10; the style context gives the
            // same CSS colour (accent, dimmed when muted) on 4.6.
            #[allow(deprecated)]
            let color = area.style_context().color();
            cr.set_source_rgba(
                f64::from(color.red()),
                f64::from(color.green()),
                f64::from(color.blue()),
                0.18,
            );
            rounded(cr, w);
            let _ = cr.fill();
            if shown > 0.0 {
                cr.set_source_rgba(
                    f64::from(color.red()),
                    f64::from(color.green()),
                    f64::from(color.blue()),
                    0.92,
                );
                rounded(cr, (w * shown).max(h));
                let _ = cr.fill();
            }
        });
    }
    {
        // Ease the shown fraction toward the target, one frame at a time.
        let bar = std::rc::Rc::clone(&bar);
        let last = std::cell::Cell::new(None::<i64>);
        area.add_tick_callback(move |area, clock| {
            let now = clock.frame_time();
            let elapsed_ms = last
                .replace(Some(now))
                .map_or(16.0, |before| (now - before) as f64 / 1000.0);
            let (shown, target) = bar.get();
            if (target - shown).abs() > 0.001 {
                let step = (elapsed_ms / LEVEL_GLIDE_MS).clamp(0.0, 1.0);
                bar.set((shown + (target - shown) * step.max(0.18), target));
                area.queue_draw();
            } else if shown != target {
                bar.set((target, target));
                area.queue_draw();
            }
            glib::ControlFlow::Continue
        });
    }

    let value = Label::new(None);
    value.add_css_class("osd-level-value");
    value.set_width_chars(5);
    value.set_xalign(1.0);

    let pill = GtkBox::new(Orientation::Horizontal, 14);
    pill.add_css_class("osd-level");
    pill.set_halign(Align::Center);
    pill.set_valign(Align::End);
    pill.append(&icon);
    pill.append(&area);
    pill.append(&value);

    window.set_child(Some(&pill));

    LevelOsd {
        window,
        pill,
        icon,
        value,
        bar,
        area,
        generation: 0,
    }
}

// ── Toast OSD (transient operational feedback) ──────────────────────────────

struct ToastOsd {
    window: Window,
    label: Label,
    revealer: Revealer,
    generation: u64,
}

thread_local! {
    static TOAST_OSD: RefCell<Option<ToastOsd>> = const { RefCell::new(None) };
}

pub fn show_toast_osd(app: Option<&Application>, message: &str) {
    let message = message.trim();
    if message.is_empty() {
        return;
    }
    let Some(app) = app
        .cloned()
        .or_else(|| gio::Application::default().and_then(|a| a.downcast::<Application>().ok()))
    else {
        return;
    };

    let generation = TOAST_OSD.with(|cell| {
        let mut slot = cell.borrow_mut();
        let osd = slot.get_or_insert_with(|| build_toast_osd(&app));
        osd.generation = osd.generation.wrapping_add(1);
        osd.label.set_text(message);
        osd.window.set_visible(true);
        osd.revealer.set_reveal_child(true);
        osd.generation
    });

    glib::timeout_add_local_once(Duration::from_millis(1200), move || {
        let still_current = TOAST_OSD.with(|cell| {
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
            TOAST_OSD.with(|cell| {
                if let Some(osd) = cell.borrow().as_ref()
                    && osd.generation == generation
                {
                    osd.window.set_visible(false);
                }
            });
        });
    });
}

fn build_toast_osd(app: &Application) -> ToastOsd {
    let window = Window::builder()
        .application(app)
        .decorated(false)
        .resizable(false)
        .build();
    window.add_css_class("osd-window");
    super::layer_shell::configure_osd(&window);

    let label = Label::new(None);
    label.add_css_class("osd-toast-label");

    let pill = GtkBox::new(Orientation::Horizontal, 0);
    pill.add_css_class("osd-toast-pill");
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

    ToastOsd {
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

    // freedesktop: 0 means "the notification never expires on its own" (the
    // server would be free to choose a timeout); a negative value means the
    // server decides. Used to treat 0 as 8 s, which silently dismissed
    // notifications the sender asked to keep.
    let dwell_ms = match expire_timeout_ms {
        ms if ms > 0 => Some((ms as u64).clamp(1500, 30_000)),
        0 => None,
        _ => Some(4_500),
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

        // The toast's own counter, after this show() bumped it.
        let toast_generation = osd.generation;
        update_icon(&osd.icon_box, app_name, app_icon, move || {
            is_current_notification_generation(toast_generation)
        });

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

fn schedule_dismiss(generation: u64, delay_ms: Option<u64>) {
    let Some(delay_ms) = delay_ms else {
        return;
    };
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

/// Icon directories an `app_icon` *path* may live in (M-14).
fn icon_search_dirs() -> Vec<std::path::PathBuf> {
    let home = crate::config::home_dir();
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    let mut dirs = vec![data_home.join("icons"), home.join(".icons")];
    if let Some(data_dirs) = std::env::var_os("XDG_DATA_DIRS") {
        for dir in data_dirs.to_string_lossy().split(':') {
            if !dir.is_empty() {
                dirs.push(std::path::PathBuf::from(dir).join("icons"));
            }
        }
    }
    dirs
}

/// Validate an `app_icon` value that names a *file* (M-14).
///
/// An untrusted notification is free to send any `app_icon`: a path outside the
/// icon directories (or a symlink, directory or FIFO) must not make the main
/// loop read an arbitrary file — a FIFO would block it forever.
fn safe_icon_path(app_icon: &str) -> Option<std::path::PathBuf> {
    safe_icon_path_in(&icon_search_dirs(), app_icon)
}

/// The testable core of [`safe_icon_path`]: `icon_dirs` replaces the search path.
fn safe_icon_path_in(
    icon_dirs: &[std::path::PathBuf],
    app_icon: &str,
) -> Option<std::path::PathBuf> {
    let trimmed = app_icon.trim();
    let raw = trimmed.strip_prefix("file://").unwrap_or(trimmed);
    if !raw.starts_with('/') {
        return None;
    }
    // `canonicalize` resolves symlinks, so the check below is about the real
    // target, not the link the sender provided.
    let canonical = std::path::Path::new(raw).canonicalize().ok()?;
    if !std::fs::metadata(&canonical).ok()?.is_file() {
        return None;
    }
    icon_dirs
        .iter()
        .any(|dir| canonical.starts_with(dir))
        .then_some(canonical)
}

/// Whether the notification toast still shows the generation that asked for an
/// icon. `false` when there is no toast at all: nothing should be applied to a
/// surface that has already been dismissed (P2.5b).
fn is_current_notification_generation(generation: u64) -> bool {
    NOTIFICATION_OSD.with(|cell| {
        cell.borrow()
            .as_ref()
            .is_some_and(|osd| osd.generation == generation)
    })
}

/// Read and decode an icon without blocking the main loop (P2.5b).
///
/// `gtk::Image::from_file` reads and decodes synchronously, and this runs on the
/// path a notification's `app_icon` takes -- an attacker-chosen file inside an
/// icon directory (what `safe_icon_path` allows) could therefore freeze the whole
/// UI for as long as reading and decoding it takes. The generation check keeps a
/// slow read from overwriting the icon of a notification that has replaced it.
fn load_icon_async(
    icon_box: &GtkBox,
    path: std::path::PathBuf,
    is_current: impl Fn() -> bool + 'static,
) {
    if std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0) > MAX_ICON_BYTES {
        return;
    }
    let icon_box = icon_box.clone();
    gio::File::for_path(path).load_bytes_async(gio::Cancellable::NONE, move |result| {
        if !is_current() {
            return;
        }
        let Ok((bytes, _etag)) = result else {
            return;
        };
        let Ok(texture) = gtk::gdk::Texture::from_bytes(&bytes) else {
            return;
        };

        let img = gtk::Image::from_paintable(Some(&texture));
        img.set_pixel_size(32);
        while let Some(child) = icon_box.first_child() {
            icon_box.remove(&child);
        }
        icon_box.append(&img);
    });
}

fn update_icon(
    icon_box: &GtkBox,
    app_name: &str,
    app_icon: &str,
    is_current: impl Fn() -> bool + 'static,
) {
    while let Some(child) = icon_box.first_child() {
        icon_box.remove(&child);
    }

    let icon_trimmed = app_icon.trim();
    if !icon_trimmed.is_empty() {
        if let Some(path) = safe_icon_path(icon_trimmed) {
            // The fallback below is drawn now and replaced when the bytes arrive.
            load_icon_async(icon_box, path, is_current);
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
    let mut builder = Window::builder().decorated(false).resizable(false);
    if let Some(app) = app {
        builder = builder.application(app);
    }
    let window = builder.build();
    window.set_default_size(1, 1);
    window.add_css_class("osd-window");
    super::layer_shell::configure_notification(&window);

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
            let current_generation = NOTIFICATION_OSD
                .with(|cell| cell.borrow().as_ref().map(|o| o.generation).unwrap_or(0));
            if current_generation > 0 {
                schedule_dismiss(current_generation, Some(1800));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(percent: u16, muted: bool) -> Level {
        Level {
            kind: LevelKind::Volume,
            percent,
            muted,
        }
    }

    #[test]
    fn a_level_picks_its_icon_fill_and_text() {
        assert_eq!(volume(0, false).icon_name(), "audio-volume-muted-symbolic");
        assert_eq!(volume(20, false).icon_name(), "audio-volume-low-symbolic");
        assert_eq!(
            volume(50, false).icon_name(),
            "audio-volume-medium-symbolic"
        );
        assert_eq!(volume(100, false).icon_name(), "audio-volume-high-symbolic");
        assert_eq!(
            volume(130, false).icon_name(),
            "audio-volume-overamplified-symbolic"
        );
        assert_eq!(volume(70, true).icon_name(), "audio-volume-muted-symbolic");
        assert_eq!(
            volume(130, false).fraction(),
            1.0,
            "boost fills the bar, no more"
        );
        assert_eq!(volume(70, true).fraction(), 0.0);
        assert_eq!(volume(70, true).text(), "Muted");
        assert_eq!(volume(45, false).text(), "45%");
        let mic = Level {
            kind: LevelKind::Microphone,
            percent: 80,
            muted: true,
        };
        assert_eq!(mic.icon_name(), "microphone-sensitivity-muted-symbolic");
        let light = Level {
            kind: LevelKind::Brightness,
            percent: 30,
            muted: false,
        };
        assert_eq!(light.icon_name(), "display-brightness-symbolic");
    }

    fn icon_fixture_dir(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "zeshicast-icons-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn app_icon_outside_icon_dirs_is_ignored() {
        let root = icon_fixture_dir("outside");
        let icons = root.join("icons");
        std::fs::create_dir_all(&icons).unwrap();
        let inside = icons.join("app.png");
        std::fs::write(&inside, b"not really a png").unwrap();
        let outside = root.join("secret.png");
        std::fs::write(&outside, b"secret").unwrap();
        let link = icons.join("link.png");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let a_directory = icons.join("a-directory");
        std::fs::create_dir_all(&a_directory).unwrap();

        let dirs = vec![icons];
        assert_eq!(
            safe_icon_path_in(&dirs, &inside.to_string_lossy()),
            Some(inside.canonicalize().unwrap()),
            "a regular file inside the icon dirs is accepted"
        );
        assert!(
            safe_icon_path_in(&dirs, &format!("file://{}", inside.display())).is_some(),
            "the file:// form works too"
        );

        // Theme names fall through to the icon theme, everything else is ignored.
        assert!(safe_icon_path_in(&dirs, "firefox").is_none());
        assert!(safe_icon_path_in(&dirs, &outside.to_string_lossy()).is_none());
        assert!(
            safe_icon_path_in(&dirs, &link.to_string_lossy()).is_none(),
            "a symlink is resolved and then rejected"
        );
        assert!(safe_icon_path_in(&dirs, &a_directory.to_string_lossy()).is_none());
        assert!(safe_icon_path_in(&dirs, "/etc/hostname").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
    #[test]
    fn an_icon_load_is_refused_when_its_toast_is_gone() {
        // No OSD has been built in this process (building one needs a display),
        // so the fail-closed direction is what can be asserted here: a load whose
        // generation no longer matches must not touch the surface.
        assert!(!is_current_notification_generation(0));
        assert!(!is_current_notification_generation(7));
    }
}

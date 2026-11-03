use chrono::Local;
use gtk::glib;
use gtk::prelude::*;
use gtk::{Box as GtkBox, Label, Orientation};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use crate::services::audio::AudioSnapshot;
use crate::services::battery::BatterySnapshot;
use crate::services::media::MediaSnapshot;
use crate::services::network::NetworkSnapshot;

#[derive(Clone)]
pub struct StatusStrip {
    root: GtkBox,
    clock: Label,
    date: Label,
    network: Chip,
    battery: Chip,
    audio: Chip,
    media: Chip,
    layout: Chip,
    /// The chips enabled through the `status_items` preference.
    items: Rc<RefCell<HashSet<String>>>,
}

/// Whether a chip should be on screen. `has_data` is what the latest snapshot
/// says; the preference wins, so a snapshot can never resurrect a chip the user
/// turned off (M-8).
fn chip_visible(enabled: &HashSet<String>, name: &str, has_data: bool) -> bool {
    has_data && enabled.contains(name)
}

impl StatusStrip {
    pub fn new() -> Self {
        let root = GtkBox::new(Orientation::Horizontal, 6);
        root.add_css_class("status-strip");

        // Left side: clock + date
        let left = GtkBox::new(Orientation::Horizontal, 6);
        left.set_valign(gtk::Align::Center);

        let clock = Label::new(None);
        clock.add_css_class("status-time");
        clock.set_xalign(0.0);
        clock.set_valign(gtk::Align::Center);

        let date = Label::new(None);
        date.add_css_class("status-date");
        date.set_xalign(0.0);
        date.set_valign(gtk::Align::Center);

        left.append(&clock);
        left.append(&date);

        // Spacer
        let spacer = GtkBox::new(Orientation::Horizontal, 0);
        spacer.set_hexpand(true);

        // Right side: status chips
        let right = GtkBox::new(Orientation::Horizontal, 4);
        right.set_valign(gtk::Align::Center);

        let network = status_chip();
        let battery = status_chip();
        let audio = status_chip();
        let media = status_chip();
        let layout = status_chip();

        right.append(&network.root);
        right.append(&battery.root);
        right.append(&audio.root);
        right.append(&media.root);
        right.append(&layout.root);

        root.append(&left);
        root.append(&spacer);
        root.append(&right);

        let strip = Self {
            root,
            clock,
            date,
            network,
            battery,
            audio,
            media,
            layout,
            items: Rc::new(RefCell::new(default_items())),
        };
        strip.refresh();
        strip.start_clock();
        strip
    }

    pub fn widget(&self) -> &GtkBox {
        &self.root
    }

    pub fn set_items(&self, items: &[String]) {
        self.items.replace(items.iter().cloned().collect());
        for (widget, name) in [
            (self.clock.upcast_ref::<gtk::Widget>(), "clock"),
            (self.date.upcast_ref(), "date"),
            (self.network.root.upcast_ref(), "network"),
            (self.battery.root.upcast_ref(), "battery"),
            (self.audio.root.upcast_ref(), "audio"),
            (self.media.root.upcast_ref(), "media"),
            (self.layout.root.upcast_ref(), "layout"),
        ] {
            widget.set_visible(self.items.borrow().contains(name));
        }
    }

    /// Hide the clock and date where the view shows them in large (the
    /// dashboard); elsewhere they follow `status_items`.
    pub fn show_clock(&self, shown: bool) {
        let items = self.items.borrow();
        self.clock.set_visible(shown && items.contains("clock"));
        self.date.set_visible(shown && items.contains("date"));
    }

    fn show_chip(&self, chip: &Chip, name: &str, has_data: bool) {
        chip.root
            .set_visible(chip_visible(&self.items.borrow(), name, has_data));
    }

    fn refresh(&self) {
        let now = Local::now();
        self.clock.set_text(&now.format("%H:%M").to_string());
        self.date.set_text(&now.format("%a %d %b").to_string());
    }

    /// The time is right in the first frame after showing (L1): redraw on
    /// map, then on each minute boundary rather than on a free-running
    /// second that skips while hidden.
    fn start_clock(&self) {
        {
            let strip = self.clone();
            self.root.connect_map(move |_| strip.refresh());
        }
        schedule_minute_tick(self.clone());
    }

    pub fn set_network_snapshot(&self, snapshot: &NetworkSnapshot) {
        let iface = snapshot
            .interfaces
            .iter()
            .find(|i| i.name != "lo" && i.state == "up")
            .or_else(|| snapshot.interfaces.iter().find(|i| i.name != "lo"));
        let Some(iface) = iface else {
            self.show_chip(&self.network, "network", false);
            return;
        };
        let connected = iface.state == "up";
        let label = if iface.is_wireless {
            "Wi-Fi"
        } else {
            "Ethernet"
        };
        let icon = if iface.is_wireless {
            "network-wireless-symbolic"
        } else {
            "network-wired-symbolic"
        };
        self.network.set(icon, label);
        self.show_chip(&self.network, "network", true);
        if connected {
            self.network.root.add_css_class("active");
        } else {
            self.network.root.remove_css_class("active");
        }
    }

    pub fn set_battery_snapshot(&self, snapshot: &BatterySnapshot) {
        let Some(battery) = snapshot.primary() else {
            self.show_chip(&self.battery, "battery", false);
            return;
        };
        let capacity = battery
            .capacity_percent
            .map(|v| format!("{v}%"))
            .unwrap_or_else(|| "?".to_string());
        let charging = battery.status.as_deref() == Some("Charging");
        let icon = if charging {
            "battery-good-charging-symbolic"
        } else {
            "battery-good-symbolic"
        };
        self.battery.set(icon, &capacity);
        self.show_chip(&self.battery, "battery", true);
        if charging {
            self.battery.root.add_css_class("active");
        } else {
            self.battery.root.remove_css_class("active");
        }
    }

    pub fn set_audio_snapshot(&self, snapshot: &AudioSnapshot) {
        let Some(output) = &snapshot.output else {
            self.show_chip(&self.audio, "audio", false);
            return;
        };
        let (icon, text) = if output.muted {
            ("audio-volume-muted-symbolic", "Muted".to_string())
        } else {
            (
                "audio-volume-high-symbolic",
                format!("{}%", output.volume_percent),
            )
        };
        self.audio.set(icon, &text);
        self.show_chip(&self.audio, "audio", true);
        if !output.muted {
            self.audio.root.add_css_class("active");
        } else {
            self.audio.root.remove_css_class("active");
        }
    }

    pub fn set_media_snapshot(&self, snapshot: &MediaSnapshot) {
        if snapshot.is_active() {
            let title = snapshot
                .title
                .as_deref()
                .or(snapshot.artist.as_deref())
                .unwrap_or("Media");
            let playing = snapshot.status.as_deref() == Some("Playing");
            let icon = if playing {
                "media-playback-start-symbolic"
            } else {
                "media-playback-pause-symbolic"
            };
            // Truncate by chars, not bytes — byte slicing panics on multi-byte
            // UTF-8 (e.g. Cyrillic track titles).
            let short_title = if title.chars().count() > 18 {
                let head: String = title.chars().take(16).collect();
                format!("{head}…")
            } else {
                title.to_string()
            };
            self.media.set(icon, &short_title);
            self.show_chip(&self.media, "media", true);
            if playing {
                self.media.root.add_css_class("active");
            } else {
                self.media.root.remove_css_class("active");
            }
        } else {
            self.show_chip(&self.media, "media", false);
        }
    }

    pub fn set_keyboard_layout(&self, code: Option<&str>) {
        let Some(code) = code.filter(|c| !c.is_empty()) else {
            self.show_chip(&self.layout, "layout", false);
            return;
        };
        self.layout
            .set("input-keyboard-symbolic", &code.to_uppercase());
        self.show_chip(&self.layout, "layout", true);
        self.layout.root.add_css_class("active");
    }
}

impl Default for StatusStrip {
    fn default() -> Self {
        Self::new()
    }
}

fn default_items() -> HashSet<String> {
    [
        "clock", "date", "network", "battery", "audio", "media", "layout",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// One chip: a theme icon and its text. Unicode glyphs (✻ ♦ ♩ ⌨) used to
/// stand in for the icons and did not say what the chip was about.
#[derive(Clone)]
struct Chip {
    root: GtkBox,
    icon: gtk::Image,
    text: Label,
}

impl Chip {
    fn set(&self, icon: &str, text: &str) {
        self.icon.set_icon_name(Some(icon));
        self.text.set_text(text);
    }
}

fn status_chip() -> Chip {
    let root = GtkBox::new(Orientation::Horizontal, 5);
    root.add_css_class("status-chip");
    root.set_valign(gtk::Align::Center);
    root.set_visible(false);
    let icon = gtk::Image::new();
    icon.set_pixel_size(12);
    let text = Label::new(None);
    root.append(&icon);
    root.append(&text);
    Chip { root, icon, text }
}

/// Milliseconds from `now` to the next minute boundary, plus a little so
/// the tick lands after it, not just before.
fn ms_until_next_minute(now: chrono::DateTime<Local>) -> u64 {
    use chrono::Timelike;
    let into_minute =
        u64::from(now.second()) * 1000 + u64::from(now.timestamp_subsec_millis().min(999));
    60_000 - into_minute + 50
}

fn schedule_minute_tick(strip: StatusStrip) {
    let delay = ms_until_next_minute(Local::now());
    glib::timeout_add_local_once(std::time::Duration::from_millis(delay), move || {
        if !crate::ui::hidden() {
            strip.refresh();
        }
        schedule_minute_tick(strip);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_ticks_just_after_each_minute() {
        use chrono::TimeZone;
        let at = |s: u32, ms: u32| {
            Local.with_ymd_and_hms(2026, 11, 3, 12, 30, s).unwrap()
                + chrono::Duration::milliseconds(i64::from(ms))
        };
        assert_eq!(ms_until_next_minute(at(0, 0)), 60_050);
        assert_eq!(ms_until_next_minute(at(59, 900)), 150);
        assert_eq!(ms_until_next_minute(at(30, 0)), 30_050);
    }

    #[test]
    fn a_disabled_chip_stays_hidden_when_a_snapshot_arrives() {
        let enabled = ["clock", "date"]
            .into_iter()
            .map(String::from)
            .collect::<HashSet<_>>();

        assert!(chip_visible(&enabled, "clock", true));
        assert!(!chip_visible(&enabled, "clock", false), "no data, no chip");
        assert!(
            !chip_visible(&enabled, "network", true),
            "arriving data must not override the status_items preference"
        );
    }

    #[test]
    fn default_items_show_every_chip() {
        let enabled = default_items();
        for name in [
            "clock", "date", "network", "battery", "audio", "media", "layout",
        ] {
            assert!(
                chip_visible(&enabled, name, true),
                "{name} is on by default"
            );
        }
    }
}

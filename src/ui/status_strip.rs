use chrono::Local;
use gtk::glib;
use gtk::prelude::*;
use gtk::{Box as GtkBox, Label, Orientation};
use std::collections::HashSet;

use crate::{AudioSnapshot, BatterySnapshot, MediaSnapshot, NetworkSnapshot};

#[derive(Clone)]
pub struct StatusStrip {
    root: GtkBox,
    clock: Label,
    date: Label,
    network: Label,
    battery: Label,
    audio: Label,
    media: Label,
    layout: Label,
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

        right.append(&network);
        right.append(&battery);
        right.append(&audio);
        right.append(&media);
        right.append(&layout);

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
        };
        strip.refresh();
        strip.start_clock();
        strip
    }

    pub fn widget(&self) -> &GtkBox {
        &self.root
    }

    pub fn set_items(&self, items: &[String]) {
        let enabled = items.iter().map(String::as_str).collect::<HashSet<_>>();
        self.clock.set_visible(enabled.contains("clock"));
        self.date.set_visible(enabled.contains("date"));
        self.network.set_visible(enabled.contains("network"));
        self.battery.set_visible(enabled.contains("battery"));
        self.audio.set_visible(enabled.contains("audio"));
        self.media.set_visible(enabled.contains("media"));
        self.layout.set_visible(enabled.contains("layout"));
    }

    fn refresh(&self) {
        let now = Local::now();
        self.clock.set_text(&now.format("%H:%M").to_string());
        self.date.set_text(&now.format("%a %d %b").to_string());
    }

    fn start_clock(&self) {
        let strip = self.clone();
        glib::timeout_add_seconds_local(1, move || {
            strip.refresh();
            glib::ControlFlow::Continue
        });
    }

    pub fn set_network_snapshot(&self, snapshot: &NetworkSnapshot) {
        let iface = snapshot
            .interfaces
            .iter()
            .find(|i| i.name != "lo" && i.state == "up")
            .or_else(|| snapshot.interfaces.iter().find(|i| i.name != "lo"));
        let Some(iface) = iface else {
            self.network.set_visible(false);
            return;
        };
        let connected = iface.state == "up";
        let label = if iface.is_wireless {
            "Wi-Fi"
        } else {
            "Ethernet"
        };
        let text = format!("✻  {label}");
        self.network.set_text(&text);
        self.network.set_visible(true);
        if connected {
            self.network.add_css_class("active");
        } else {
            self.network.remove_css_class("active");
        }
    }

    pub fn set_battery_snapshot(&self, snapshot: &BatterySnapshot) {
        let Some(battery) = snapshot.primary() else {
            self.battery.set_visible(false);
            return;
        };
        let capacity = battery
            .capacity_percent
            .map(|v| format!("{v}%"))
            .unwrap_or_else(|| "?".to_string());
        let charging = battery.status.as_deref() == Some("Charging");
        let icon = if charging { "⚡" } else { "♦" };
        self.battery.set_text(&format!("{icon}  {capacity}"));
        self.battery.set_visible(true);
        if charging {
            self.battery.add_css_class("active");
        } else {
            self.battery.remove_css_class("active");
        }
    }

    pub fn set_audio_snapshot(&self, snapshot: &AudioSnapshot) {
        let Some(output) = &snapshot.output else {
            self.audio.set_visible(false);
            return;
        };
        let text = if output.muted {
            "♩  Muted".to_string()
        } else {
            format!("♩  {}%", output.volume_percent)
        };
        self.audio.set_text(&text);
        self.audio.set_visible(true);
        if !output.muted {
            self.audio.add_css_class("active");
        } else {
            self.audio.remove_css_class("active");
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
            let icon = if playing { "▶" } else { "⏸" };
            // Truncate by chars, not bytes — byte slicing panics on multi-byte
            // UTF-8 (e.g. Cyrillic track titles).
            let short_title = if title.chars().count() > 18 {
                let head: String = title.chars().take(16).collect();
                format!("{head}…")
            } else {
                title.to_string()
            };
            self.media.set_text(&format!("{icon}  {short_title}"));
            self.media.set_visible(true);
            if playing {
                self.media.add_css_class("active");
            } else {
                self.media.remove_css_class("active");
            }
        } else {
            self.media.set_visible(false);
        }
    }

    pub fn set_keyboard_layout(&self, code: Option<&str>) {
        let Some(code) = code.filter(|c| !c.is_empty()) else {
            self.layout.set_visible(false);
            return;
        };
        self.layout.set_text(&format!("⌨  {}", code.to_uppercase()));
        self.layout.set_visible(true);
        self.layout.add_css_class("active");
    }
}

impl Default for StatusStrip {
    fn default() -> Self {
        Self::new()
    }
}

fn status_chip() -> Label {
    let label = Label::new(None);
    label.add_css_class("status-chip");
    label.set_valign(gtk::Align::Center);
    label.set_visible(false);
    label
}

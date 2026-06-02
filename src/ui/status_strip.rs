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
}

impl StatusStrip {
    pub fn new() -> Self {
        let root = GtkBox::new(Orientation::Horizontal, 8);
        root.add_css_class("status-strip");

        let clock = Label::new(None);
        clock.add_css_class("status-clock");
        clock.set_xalign(0.0);

        let date = Label::new(None);
        date.add_css_class("status-date");
        date.set_xalign(0.0);

        let network = status_label();
        let battery = status_label();
        let audio = status_label();
        let media = status_label();

        root.append(&clock);
        root.append(&date);
        root.append(&network);
        root.append(&battery);
        root.append(&audio);
        root.append(&media);

        let strip = Self {
            root,
            clock,
            date,
            network,
            battery,
            audio,
            media,
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
    }

    fn refresh(&self) {
        let now = Local::now();
        self.clock.set_text(&now.format("%H:%M:%S").to_string());
        self.date.set_text(&now.format("%a, %d %b").to_string());
    }

    fn start_clock(&self) {
        let strip = self.clone();
        glib::timeout_add_seconds_local(1, move || {
            strip.refresh();
            glib::ControlFlow::Continue
        });
    }

    pub fn set_network_snapshot(&self, snapshot: &NetworkSnapshot) {
        let summary = snapshot
            .interfaces
            .iter()
            .find(|interface| interface.name != "lo" && interface.state == "up")
            .or_else(|| {
                snapshot
                    .interfaces
                    .iter()
                    .find(|interface| interface.name != "lo")
            })
            .map(|interface| {
                let icon = if interface.is_wireless {
                    "Wi-Fi"
                } else {
                    "Net"
                };
                format!("{icon} {}", interface.state)
            })
            .unwrap_or("Net unavailable".to_string());
        self.network.set_text(&summary);
    }

    pub fn set_battery_snapshot(&self, snapshot: &BatterySnapshot) {
        let Some(battery) = snapshot.primary() else {
            self.battery.set_text("");
            return;
        };

        let capacity = battery
            .capacity_percent
            .map(|value| format!("{value}%"))
            .unwrap_or("Battery".to_string());
        let status = battery.status.as_deref().unwrap_or("");
        self.battery
            .set_text(&format!("Battery {capacity} {status}").trim().to_string());
    }

    pub fn set_audio_snapshot(&self, snapshot: &AudioSnapshot) {
        let Some(output) = &snapshot.output else {
            self.audio.set_text("");
            return;
        };
        let mute = if output.muted { " muted" } else { "" };
        self.audio
            .set_text(&format!("Vol {}%{mute}", output.volume_percent));
    }

    pub fn set_media_snapshot(&self, snapshot: &MediaSnapshot) {
        if snapshot.is_active() {
            let title = snapshot
                .title
                .as_deref()
                .or(snapshot.artist.as_deref())
                .unwrap_or("Media");
            let status = snapshot.status.as_deref().unwrap_or("Playing");
            self.media.set_text(&format!("{status} {title}"));
        } else {
            self.media.set_text("");
        }
    }
}

fn status_label() -> Label {
    let label = Label::new(None);
    label.add_css_class("status-date");
    label.set_xalign(0.0);
    label
}

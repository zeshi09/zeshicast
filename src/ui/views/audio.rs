use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{Button, Image, Box as GtkBox, Label, ListBox, Orientation, ProgressBar};

use crate::{AudioDeviceOption, AudioDeviceSnapshot, AudioSnapshot, AudioStreamSnapshot};

#[derive(Clone)]
pub struct AudioView {
    pub root: GtkBox,
    pub output_name: Label,
    pub output_volume: Label,
    pub output_bar: ProgressBar,
    pub input_name: Label,
    pub input_volume: Label,
    pub input_bar: ProgressBar,
    pub streams_list: ListBox,
    pub mute_output: Button,
    pub mute_input: Button,
    pub output_devices: ListBox,
    pub input_devices: ListBox,
    pub output_scale: gtk::Scale,
    pub input_scale: gtk::Scale,
    /// Set while we push real volumes into the scales so their value-changed
    /// handlers don't fire `wpctl set-volume` back at the device.
    suppress_volume_cb: Rc<Cell<bool>>,
}


pub fn audio_view(snapshot: &AudioSnapshot) -> AudioView {
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);

    // ── Output section ───────────────────────────────────────────────────────
    root.append(&crate::ui::section_header("Output"));

    let suppress_volume_cb = Rc::new(Cell::new(false));

    let output_devices = ListBox::new();
    output_devices.add_css_class("results-list");
    output_devices.set_activate_on_single_click(true);
    // Populated from the real device list in set_audio_snapshot.
    root.append(&output_devices);

    let output_name = Label::new(Some("Built-in Speakers"));
    output_name.add_css_class("result-subtitle");
    output_name.set_visible(false); // used for data binding

    // Volume row: mute btn + GtkScale + value
    let vol_row = GtkBox::new(Orientation::Horizontal, 10);
    vol_row.set_margin_start(14);
    vol_row.set_margin_end(14);
    vol_row.set_margin_top(4);
    vol_row.set_margin_bottom(8);

    let mute_output = Button::with_label("🔊");
    mute_output.add_css_class("action-bar-btn");
    mute_output.set_tooltip_text(Some("Toggle mute"));
    mute_output.set_valign(gtk::Align::Center);

    let output_bar_scale = gtk::Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
    output_bar_scale.set_draw_value(false);
    output_bar_scale.set_hexpand(true);
    output_bar_scale.add_css_class("audio-volume-bar");
    {
        let suppress = Rc::clone(&suppress_volume_cb);
        output_bar_scale.connect_value_changed(move |scale| {
            if suppress.get() {
                return;
            }
            set_default_volume("@DEFAULT_AUDIO_SINK@", scale.value());
        });
    }

    let output_volume = Label::new(Some("65%"));
    output_volume.add_css_class("audio-volume-value");
    output_volume.set_width_chars(5);
    output_volume.set_xalign(1.0);

    vol_row.append(&mute_output);
    vol_row.append(&output_bar_scale);
    vol_row.append(&output_volume);
    root.append(&vol_row);

    // ── Input section ────────────────────────────────────────────────────────
    root.append(&crate::ui::section_header("Input"));

    let input_devices = ListBox::new();
    input_devices.add_css_class("results-list");
    input_devices.set_activate_on_single_click(true);
    // Populated from the real device list in set_audio_snapshot.
    root.append(&input_devices);

    let input_name = Label::new(Some("Built-in Microphone"));
    input_name.add_css_class("result-subtitle");
    input_name.set_visible(false);

    let in_vol_row = GtkBox::new(Orientation::Horizontal, 10);
    in_vol_row.set_margin_start(14);
    in_vol_row.set_margin_end(14);
    in_vol_row.set_margin_top(4);

    let mute_input = Button::with_label("🎙");
    mute_input.add_css_class("action-bar-btn");
    mute_input.set_valign(gtk::Align::Center);

    let input_bar_scale = gtk::Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
    input_bar_scale.set_draw_value(false);
    input_bar_scale.set_hexpand(true);
    input_bar_scale.add_css_class("audio-volume-bar");
    {
        let suppress = Rc::clone(&suppress_volume_cb);
        input_bar_scale.connect_value_changed(move |scale| {
            if suppress.get() {
                return;
            }
            set_default_volume("@DEFAULT_AUDIO_SOURCE@", scale.value());
        });
    }

    let input_volume = Label::new(Some("80%"));
    input_volume.add_css_class("audio-volume-value");
    input_volume.set_width_chars(5);
    input_volume.set_xalign(1.0);

    in_vol_row.append(&mute_input);
    in_vol_row.append(&input_bar_scale);
    in_vol_row.append(&input_volume);
    root.append(&in_vol_row);

    // ── App streams ─────────────────────────────────────────────────────────
    // PipeWire per-application streams are intentionally not shown (they clutter
    // the view and aren't part of the target design). Kept for struct/data compat.
    let streams_list = crate::ui::results_list();
    streams_list.set_visible(false);

    // Compat fields: output_bar / input_bar as ProgressBar for snapshot update
    let output_bar = ProgressBar::new();
    output_bar.set_visible(false);
    let input_bar = ProgressBar::new();
    input_bar.set_visible(false);

    let view = AudioView {
        root,
        output_name,
        output_volume,
        output_bar,
        input_name,
        input_volume,
        input_bar,
        streams_list,
        mute_output,
        mute_input,
        output_devices,
        input_devices,
        output_scale: output_bar_scale,
        input_scale: input_bar_scale,
        suppress_volume_cb,
    };
    set_audio_snapshot(&view, snapshot);
    view
}


pub fn set_audio_snapshot(view: &AudioView, snapshot: &AudioSnapshot) {
    set_audio_device(
        &view.output_name,
        &view.output_volume,
        &view.output_bar,
        snapshot.output.as_ref(),
        "Output device unavailable",
    );
    set_audio_device(
        &view.input_name,
        &view.input_volume,
        &view.input_bar,
        snapshot.input.as_ref(),
        "Input device unavailable",
    );

    // Real device lists (click a row to make it the default device).
    populate_audio_device_list(&view.output_devices, &snapshot.output_devices, "Sinks");
    populate_audio_device_list(&view.input_devices, &snapshot.input_devices, "Sources");

    // Reflect real volumes on the sliders without re-triggering set-volume.
    view.suppress_volume_cb.set(true);
    if let Some(output) = snapshot.output.as_ref() {
        view.output_scale.set_value(output.volume_percent as f64);
    }
    if let Some(input) = snapshot.input.as_ref() {
        view.input_scale.set_value(input.volume_percent as f64);
    }
    view.suppress_volume_cb.set(false);

    set_audio_stream_rows(&view.streams_list, &snapshot.streams);
}

/// `wpctl set-volume <target> <percent>%` — clamped to a sane 0–150 range.
fn set_default_volume(target: &str, percent: f64) {
    let pct = percent.round().clamp(0.0, 150.0) as u32;
    let _ = std::process::Command::new("wpctl")
        .args(["set-volume", target, &format!("{pct}%")])
        .status();
}

/// Fill a device ListBox from real devices; clicking a row sets it as the
/// system default (`wpctl set-default <id>`) and repopulates in place.
fn populate_audio_device_list(
    list: &ListBox,
    devices: &[AudioDeviceOption],
    section: &'static str,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if devices.is_empty() {
        list.append(&crate::ui::secondary_action_row(
            "audio-card-symbolic",
            "No devices found",
        ));
        return;
    }

    for device in devices {
        let row = audio_device_row(&device.name, device.is_default);
        if let Some(id) = device.id {
            let gesture = gtk::GestureClick::new();
            let list = list.clone();
            gesture.connect_released(move |_, _, _, _| {
                let _ = std::process::Command::new("wpctl")
                    .args(["set-default", &id.to_string()])
                    .status();
                let snapshot = crate::audio_snapshot();
                let devices = if section == "Sinks" {
                    snapshot.output_devices
                } else {
                    snapshot.input_devices
                };
                populate_audio_device_list(&list, &devices, section);
            });
            row.add_controller(gesture);
        }
        list.append(&row);
    }
}


fn audio_device_row(name: &str, active: bool) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_start(14);
    layout.set_margin_end(14);
    layout.set_valign(gtk::Align::Center);

    // Radio dot
    let dot = GtkBox::new(Orientation::Vertical, 0);
    dot.set_width_request(7);
    dot.set_height_request(7);
    dot.set_valign(gtk::Align::Center);
    dot.add_css_class(if active {
        "radio-dot-active"
    } else {
        "radio-dot-inactive"
    });
    layout.append(&dot);

    let label = Label::new(Some(name));
    label.add_css_class(if active {
        "result-title"
    } else {
        "result-subtitle"
    });
    label.set_xalign(0.0);
    label.set_hexpand(true);
    layout.append(&label);

    if active {
        let default_lbl = Label::new(Some("default"));
        default_lbl.add_css_class("clipboard-time");
        layout.append(&default_lbl);
    }

    row.set_child(Some(&layout));
    row
}


fn set_audio_device(
    name: &Label,
    volume: &Label,
    bar: &ProgressBar,
    device: Option<&AudioDeviceSnapshot>,
    empty: &str,
) {
    let Some(device) = device else {
        name.set_text(empty);
        volume.set_text("--");
        bar.set_fraction(0.0);
        return;
    };

    name.set_text(device.name.as_deref().unwrap_or("Default device"));
    let muted = if device.muted { " muted" } else { "" };
    volume.set_text(&format!("{}%{muted}", device.volume_percent));
    bar.set_fraction((device.volume_percent as f64 / 100.0).clamp(0.0, 1.0));
}

fn set_audio_stream_rows(list: &ListBox, streams: &[AudioStreamSnapshot]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if streams.is_empty() {
        list.append(&crate::ui::secondary_action_row(
            "dialog-information-symbolic",
            "No active application streams",
        ));
        return;
    }

    for stream in streams {
        list.append(&audio_stream_row(stream));
    }
}

fn audio_stream_row(stream: &AudioStreamSnapshot) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(10);
    layout.set_margin_end(10);

    let icon = Image::from_icon_name("audio-volume-medium-symbolic");
    icon.set_pixel_size(20);
    icon.add_css_class("result-icon");
    layout.append(&icon);

    let text = GtkBox::new(Orientation::Vertical, 4);
    text.set_hexpand(true);

    let title = Label::new(Some(&stream.name));
    title.add_css_class("result-title");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&title);

    let subtitle = stream
        .id
        .map(|id| format!("stream {id}"))
        .unwrap_or("application stream".to_string());
    let subtitle = Label::new(Some(&subtitle));
    subtitle.add_css_class("result-subtitle");
    subtitle.set_xalign(0.0);
    text.append(&subtitle);

    let bar = ProgressBar::new();
    bar.add_css_class("audio-volume-bar");
    bar.set_show_text(false);
    bar.set_fraction(
        stream
            .volume_percent
            .map(|value| value as f64 / 100.0)
            .unwrap_or_default()
            .clamp(0.0, 1.0),
    );
    text.append(&bar);

    let volume = stream
        .volume_percent
        .map(|value| format!("{value}%"))
        .unwrap_or("--".to_string());
    let volume = Label::new(Some(if stream.muted { "muted" } else { &volume }));
    volume.add_css_class("audio-volume-value");

    layout.append(&text);
    layout.append(&volume);
    row.set_child(Some(&layout));
    row
}


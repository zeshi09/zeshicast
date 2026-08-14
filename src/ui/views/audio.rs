use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, Image, Label, ListBox, Orientation, ProgressBar};
use crate::{AudioDeviceSnapshot, AudioSnapshot, AudioStreamSnapshot};
use super::dashboard::{dashboard_card_value, dashboard_grid, dashboard_plain_card};

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
}

pub fn audio_view(snapshot: &AudioSnapshot) -> AudioView {
    let root = crate::ui::panel_root(10, 12);
    root.set_vexpand(true);

    let header = crate::ui::panel_title("Audio");
    root.append(&header);

    let device_grid = dashboard_grid();
    let output_card = dashboard_plain_card("Output Volume", "audio-volume-high-symbolic");
    let input_card = dashboard_plain_card("Input Volume", "audio-input-microphone-symbolic");

    let (output_name, output_volume, output_bar, mute_output) =
        audio_device_controls(&output_card, "audio-volume-muted-symbolic");
    let (input_name, input_volume, input_bar, mute_input) =
        audio_device_controls(&input_card, "microphone-sensitivity-muted-symbolic");

    device_grid.attach(&output_card, 0, 0, 1, 1);
    device_grid.attach(&input_card, 1, 0, 1, 1);
    root.append(&device_grid);

    let streams_card =
        dashboard_plain_card("Application Volumes", "multimedia-volume-control-symbolic");
    streams_card.set_vexpand(true);

    let streams_list = crate::ui::results_list();
    let streams_scroller = crate::ui::scrollable_list(&streams_list);
    streams_card.append(&streams_scroller);
    root.append(&streams_card);

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
    set_audio_stream_rows(&view.streams_list, &snapshot.streams);
}


fn audio_device_controls(
    card: &GtkBox,
    mute_icon_name: &str,
) -> (Label, Label, ProgressBar, Button) {
    let name = dashboard_card_value();
    card.append(&name);

    let controls = GtkBox::new(Orientation::Horizontal, 10);
    controls.set_hexpand(true);

    let bar = ProgressBar::new();
    bar.add_css_class("audio-volume-bar");
    bar.set_show_text(false);
    bar.set_hexpand(true);
    controls.append(&bar);

    let volume = Label::new(None);
    volume.add_css_class("audio-volume-value");
    controls.append(&volume);

    let mute = Button::builder()
        .icon_name(mute_icon_name)
        .tooltip_text("Toggle mute")
        .build();
    mute.add_css_class("dashboard-button");
    controls.append(&mute);

    card.append(&controls);
    (name, volume, bar, mute)
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

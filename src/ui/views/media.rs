use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, Label};
use crate::MediaSnapshot;
use super::dashboard::{
    dashboard_card_actions, dashboard_plain_card, dashboard_subtitle_label,
    dashboard_value_label,
};

#[derive(Clone)]
pub struct MediaView {
    pub root: GtkBox,
    pub player: Label,
    pub status: Label,
    pub title: Label,
    pub previous: Button,
    pub play_pause: Button,
    pub next: Button,
}

pub fn media_view(snapshot: &MediaSnapshot) -> MediaView {
    let root = crate::ui::panel_root(10, 12);
    root.set_vexpand(true);

    let header = crate::ui::panel_title("Media");
    root.append(&header);

    let media_card = dashboard_plain_card("Now Playing", "media-playback-start-symbolic");
    let title = dashboard_value_label();
    title.add_css_class("media-title");
    let player = dashboard_subtitle_label();
    let status = dashboard_subtitle_label();
    media_card.append(&title);
    media_card.append(&player);
    media_card.append(&status);

    let buttons = dashboard_card_actions();
    buttons.set_halign(gtk::Align::End);
    let previous = Button::builder()
        .icon_name("media-skip-backward-symbolic")
        .tooltip_text("Previous")
        .build();
    previous.add_css_class("dashboard-button");
    let play_pause = Button::builder()
        .icon_name("media-playback-start-symbolic")
        .tooltip_text("Play or pause")
        .build();
    play_pause.add_css_class("dashboard-button");
    let next = Button::builder()
        .icon_name("media-skip-forward-symbolic")
        .tooltip_text("Next")
        .build();
    next.add_css_class("dashboard-button");
    buttons.append(&previous);
    buttons.append(&play_pause);
    buttons.append(&next);
    media_card.append(&buttons);
    root.append(&media_card);

    let view = MediaView {
        root,
        player,
        status,
        title,
        previous,
        play_pause,
        next,
    };
    set_media_snapshot(&view, snapshot);
    view
}

pub fn set_media_snapshot(view: &MediaView, snapshot: &MediaSnapshot) {
    if snapshot.is_active() {
        let title = match (&snapshot.artist, &snapshot.title) {
            (Some(artist), Some(title)) => format!("{artist} - {title}"),
            (_, Some(title)) => title.clone(),
            (Some(artist), _) => artist.clone(),
            _ => "Unknown track".to_string(),
        };
        view.title.set_text(&title);
        view.player.set_text(
            &snapshot
                .player
                .as_deref()
                .unwrap_or("Unknown player")
                .to_string(),
        );
        view.status
            .set_text(snapshot.status.as_deref().unwrap_or("Unknown status"));
    } else {
        view.title.set_text("No active player");
        view.player
            .set_text("Install playerctl for MPRIS media status");
        view.status.set_text("");
    }
}

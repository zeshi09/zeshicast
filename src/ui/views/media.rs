use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{Button, Image, Box as GtkBox, Label, Orientation};

use crate::MediaSnapshot;

#[derive(Clone)]
pub struct MediaView {
    pub root: GtkBox,
    pub player: Label,
    pub status: Label,
    pub title: Label,
    pub previous: Button,
    pub play_pause: Button,
    pub next: Button,
    pub scrubber: gtk::Scale,
    pub time_pos: Label,
    pub time_total: Label,
    pub art_picture: Image,
    pub art_icon: Label,
    /// Last art URL we loaded, so we don't refetch on every refresh tick.
    art_url: Rc<RefCell<Option<String>>>,
}


pub fn media_view(snapshot: &MediaSnapshot) -> MediaView {
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);
    root.set_margin_top(20);
    root.set_margin_bottom(18);
    root.set_margin_start(14);
    root.set_margin_end(14);

    // ── Album art + track info ───────────────────────────────────────────────
    let info_row = GtkBox::new(Orientation::Horizontal, 16);
    info_row.set_margin_bottom(20);

    let art = GtkBox::new(Orientation::Vertical, 0);
    art.set_width_request(96);
    art.set_height_request(96);
    art.add_css_class("media-art");
    art.set_valign(gtk::Align::Start);
    art.set_halign(gtk::Align::Start);
    // Block the inner icon's expand (used to centre the glyph) from propagating
    // out and stretching the art box / info row.
    art.set_hexpand(false);
    art.set_vexpand(false);
    let art_icon = Label::new(Some("♪"));
    art_icon.set_vexpand(true);
    art_icon.set_hexpand(true);
    art_icon.set_valign(gtk::Align::Center);
    art_icon.set_halign(gtk::Align::Center);
    art_icon.add_css_class("media-art-icon");
    art.append(&art_icon);

    // Real album art (shown instead of the ♪ glyph once loaded). A fixed-size
    // Image scales the (large) cover texture down to 96px.
    let art_picture = Image::new();
    art_picture.set_pixel_size(96);
    art_picture.add_css_class("media-art-image");
    art_picture.set_visible(false);
    art.append(&art_picture);
    info_row.append(&art);

    let track_info = GtkBox::new(Orientation::Vertical, 4);
    track_info.set_valign(gtk::Align::Center);
    track_info.set_hexpand(true);

    let title = Label::new(Some("No active player"));
    title.add_css_class("media-title");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);

    // Artist line.
    let player = Label::new(None);
    player.add_css_class("media-artist");
    player.set_xalign(0.0);
    player.set_ellipsize(gtk::pango::EllipsizeMode::End);

    // "album · player" meta line.
    let status = Label::new(None);
    status.add_css_class("media-meta");
    status.set_xalign(0.0);
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);

    track_info.append(&title);
    track_info.append(&player);
    track_info.append(&status);
    info_row.append(&track_info);
    root.append(&info_row);

    // ── Scrubber (GtkScale) ──────────────────────────────────────────────────
    let scrubber = gtk::Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
    scrubber.set_draw_value(false);
    scrubber.set_hexpand(true);
    scrubber.set_margin_bottom(4);
    root.append(&scrubber);

    // Time labels
    let time_row = GtkBox::new(Orientation::Horizontal, 0);
    let time_pos = Label::new(Some("0:00"));
    time_pos.add_css_class("clipboard-time");
    time_pos.set_xalign(0.0);
    let time_spacer = GtkBox::new(Orientation::Horizontal, 0);
    time_spacer.set_hexpand(true);
    let time_total = Label::new(Some("0:00"));
    time_total.add_css_class("clipboard-time");
    time_total.set_xalign(1.0);
    time_row.append(&time_pos);
    time_row.append(&time_spacer);
    time_row.append(&time_total);
    root.append(&time_row);

    // ── Playback controls ────────────────────────────────────────────────────
    let controls = GtkBox::new(Orientation::Horizontal, 10);
    controls.set_halign(gtk::Align::Center);
    controls.set_margin_top(14);

    let previous = media_ctrl_btn("media-skip-backward-symbolic", "Previous", "media-btn-skip");
    let seek_back = media_ctrl_btn(
        "media-seek-backward-symbolic",
        "Seek back 10s",
        "media-btn-seek",
    );
    let play_pause = media_play_btn("media-playback-start-symbolic");
    let seek_fwd = media_ctrl_btn(
        "media-seek-forward-symbolic",
        "Seek forward 10s",
        "media-btn-seek",
    );
    let next = media_ctrl_btn("media-skip-forward-symbolic", "Next", "media-btn-skip");

    // Wire MPRIS controls (direct D-Bus, no playerctl).
    previous.connect_clicked(|_| crate::media_control(crate::MediaControl::Previous));
    next.connect_clicked(|_| crate::media_control(crate::MediaControl::Next));
    play_pause.connect_clicked(|_| crate::media_control(crate::MediaControl::PlayPause));
    seek_back.connect_clicked(|_| crate::media_control(crate::MediaControl::SeekBy(-10_000_000)));
    seek_fwd.connect_clicked(|_| crate::media_control(crate::MediaControl::SeekBy(10_000_000)));
    scrubber.connect_change_value(|scale, _, val| {
        // Relative seek by the delta between the dragged value and the current one.
        let offset = ((val - scale.value()) * 1_000_000.0).round() as i64;
        crate::media_control(crate::MediaControl::SeekBy(offset));
        glib::Propagation::Proceed
    });

    controls.append(&previous);
    controls.append(&seek_back);
    controls.append(&play_pause);
    controls.append(&seek_fwd);
    controls.append(&next);
    root.append(&controls);

    // Absorbs the remaining height so the player stays pinned to the top
    // (matching the mockup) instead of centring/floating in the page.
    let spacer = GtkBox::new(Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    root.append(&spacer);

    let view = MediaView {
        root,
        player,
        status,
        title,
        previous,
        play_pause,
        next,
        scrubber,
        time_pos,
        time_total,
        art_picture,
        art_icon,
        art_url: Rc::new(RefCell::new(None)),
    };
    set_media_snapshot(&view, snapshot);
    view
}

fn media_ctrl_btn(icon_name: &str, tooltip: &str, css_class: &str) -> Button {
    // Symbolic icons (not emoji glyphs) so buttons stay crisp and recolour via
    // CSS. A square size_request keeps them perfectly round (border-radius 50%)
    // — otherwise a wide icon + button padding makes them oval.
    let btn = Button::from_icon_name(icon_name);
    btn.add_css_class(css_class);
    btn.set_tooltip_text(Some(tooltip));
    let size = if css_class == "media-btn-seek" {
        36
    } else {
        32
    };
    btn.set_size_request(size, size);
    btn.set_halign(gtk::Align::Center);
    btn.set_valign(gtk::Align::Center);
    btn
}

fn media_play_btn(_icon: &str) -> Button {
    let btn = Button::from_icon_name("media-playback-pause-symbolic");
    btn.add_css_class("media-btn-primary");
    btn.set_size_request(48, 48);
    btn.set_halign(gtk::Align::Center);
    btn.set_valign(gtk::Align::Center);
    btn
}

/// Swap the album art when the URL changes. `file://` loads synchronously;
/// `http(s)://` is fetched on a background thread (cached by URL so we don't
/// refetch on every refresh tick). Falls back to the ♪ glyph when absent.
fn update_media_art(view: &MediaView, art_url: Option<&str>) {
    if view.art_url.borrow().as_deref() == art_url {
        return; // unchanged — nothing to do
    }
    *view.art_url.borrow_mut() = art_url.map(str::to_string);

    let show_placeholder = |view: &MediaView| {
        view.art_picture.clear();
        view.art_picture.set_visible(false);
        view.art_icon.set_visible(true);
    };

    let Some(url) = art_url else {
        show_placeholder(view);
        return;
    };

    let set_texture = |view: &MediaView, texture: &gtk::gdk::Texture| {
        view.art_picture.set_property("paintable", texture);
        view.art_picture.set_visible(true);
        view.art_icon.set_visible(false);
    };

    if url.starts_with("file://") {
        match gtk::gdk::Texture::from_file(&gtk::gio::File::for_uri(url)) {
            Ok(texture) => set_texture(view, &texture),
            Err(_) => show_placeholder(view),
        }
        return;
    }

    if !(url.starts_with("http://") || url.starts_with("https://")) {
        show_placeholder(view);
        return;
    }

    // Remote art: fetch off-thread, deliver bytes back to the UI thread.
    let (tx, rx) = std::sync::mpsc::channel::<Option<Vec<u8>>>();
    let fetch_url = url.to_string();
    std::thread::spawn(move || {
        let bytes = ureq::get(&fetch_url).call().ok().and_then(|resp| {
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf)
                .ok()
                .map(|_| buf)
        });
        let _ = tx.send(bytes);
    });

    let picture = view.art_picture.clone();
    let icon = view.art_icon.clone();
    let expected = url.to_string();
    let art_url = Rc::clone(&view.art_url);
    glib::timeout_add_local(std::time::Duration::from_millis(40), move || {
        match rx.try_recv() {
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Ok(Some(bytes)) => {
                // Ignore if the track already moved on while we were fetching.
                if art_url.borrow().as_deref() == Some(expected.as_str()) {
                    let glib_bytes = glib::Bytes::from_owned(bytes);
                    if let Ok(texture) = gtk::gdk::Texture::from_bytes(&glib_bytes) {
                        picture.set_property("paintable", &texture);
                        picture.set_visible(true);
                        icon.set_visible(false);
                    }
                }
                glib::ControlFlow::Break
            }
            _ => glib::ControlFlow::Break,
        }
    });
}

fn fmt_secs(s: f64) -> String {
    let total = s as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

pub fn set_media_snapshot(view: &MediaView, snapshot: &MediaSnapshot) {
    if snapshot.is_active() {
        // Title (bold) · artist · "album · player" — matches the mockup.
        view.title
            .set_text(snapshot.title.as_deref().unwrap_or("Unknown track"));
        view.player
            .set_text(snapshot.artist.as_deref().unwrap_or(""));

        let player = snapshot.player.as_deref().unwrap_or("");
        let meta = match snapshot.album.as_deref() {
            Some(album) if !album.is_empty() && !player.is_empty() => {
                format!("{album}  ·  {player}")
            }
            Some(album) if !album.is_empty() => album.to_string(),
            _ => player.to_string(),
        };
        view.status.set_text(&meta);

        update_media_art(view, snapshot.art_url.as_deref());

        let is_playing = snapshot.status.as_deref() == Some("Playing");
        view.play_pause.set_icon_name(if is_playing {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        });

        if let Some(len) = snapshot.length_secs {
            view.scrubber.set_range(0.0, len);
            view.time_total.set_text(&fmt_secs(len));
        }
        if let Some(pos) = snapshot.position_secs {
            view.scrubber.set_value(pos);
            view.time_pos.set_text(&fmt_secs(pos));
        }
        view.scrubber.set_sensitive(snapshot.length_secs.is_some());
    } else {
        view.title.set_text("No active player");
        view.player
            .set_text("Start a media player to see MPRIS status");
        view.status.set_text("");
        update_media_art(view, None);
        view.play_pause
            .set_icon_name("media-playback-start-symbolic");
        view.scrubber.set_sensitive(false);
        view.time_pos.set_text("0:00");
        view.time_total.set_text("0:00");
    }
}


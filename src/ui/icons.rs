use gtk::Label;
use gtk::prelude::*;

/// Return the FA6 Solid unicode codepoint for a GTK symbolic icon name, or a fallback.
pub fn fa_glyph(icon_name: &str) -> &'static str {
    match icon_name {
        // Apps / launch
        "system-run-symbolic" | "application-x-executable-symbolic" => "\u{f144}", // circle-play
        "utilities-terminal-symbolic" => "\u{f120}",                               // terminal
        "applications-engineering-symbolic" => "\u{f121}",                         // code

        // Files / folders
        "folder-symbolic" => "\u{f07b}",
        "folder-open-symbolic" => "\u{f07c}",
        "text-x-script-symbolic" | "document-edit-symbolic" => "\u{f15c}", // file-lines

        // Media
        "media-playback-start-symbolic" => "\u{f04b}", // play
        "media-playback-pause-symbolic" => "\u{f04c}", // pause
        "media-playback-stop-symbolic" => "\u{f04d}",  // stop
        "media-skip-backward-symbolic" => "\u{f04a}",  // backward-step
        "media-skip-forward-symbolic" => "\u{f04e}",   // forward-step

        // Audio
        "audio-volume-high-symbolic" => "\u{f028}", // volume-high
        "audio-volume-medium-symbolic" => "\u{f027}", // volume-low
        "audio-volume-muted-symbolic" => "\u{f6a9}", // volume-xmark
        "audio-input-microphone-symbolic" => "\u{f130}", // microphone
        "microphone-sensitivity-muted-symbolic" => "\u{f131}", // microphone-slash
        "multimedia-volume-control-symbolic" => "\u{f9c2}", // sliders

        // Network
        "network-wireless-symbolic" => "\u{f1eb}", // wifi
        "network-wired-symbolic" => "\u{f796}",    // network-wired
        "nm-device-wireless" => "\u{f1eb}",
        "network-vpn-symbolic" => "\u{f023}", // lock

        // System
        "utilities-system-monitor-symbolic" => "\u{f201}", // chart-line
        "view-dashboard-symbolic" => "\u{f3fd}",           // gauge
        "drive-harddisk-symbolic" => "\u{f0a0}",           // hdd
        "media-flash-symbolic" => "\u{f538}",              // memory
        "battery-good-symbolic" | "battery-full-symbolic" => "\u{f240}", // battery-full
        "battery-low-symbolic" => "\u{f243}",              // battery-quarter
        "appointment-soon-symbolic" | "clock-symbolic" => "\u{f017}", // clock
        "weather-clear-symbolic" => "\u{f185}",            // sun
        "process-stop-symbolic" => "\u{f05e}",             // ban

        // Notifications
        "preferences-system-notifications-symbolic" => "\u{f0f3}", // bell
        "notifications-disabled-symbolic" => "\u{f1f6}",           // bell-slash

        // Actions / edit
        "edit-copy-symbolic" => "\u{f0c5}",      // copy
        "edit-paste-symbolic" => "\u{f0ea}",     // paste
        "edit-delete-symbolic" => "\u{f1f8}",    // trash
        "edit-clear-symbolic" => "\u{f12d}",     // eraser
        "view-pin-symbolic" => "\u{f08d}",       // thumbtack
        "insert-link-symbolic" => "\u{f0c1}",    // link
        "input-keyboard-symbolic" => "\u{f11c}", // keyboard

        // Search / AI
        "system-search-symbolic" | "search-symbolic" => "\u{f002}", // magnifying-glass
        "face-smile-symbolic" => "\u{f118}",                        // face-smile
        "accessories-calculator-symbolic" => "\u{f1ec}",            // calculator

        // URL / web
        "emblem-web-symbolic" | "emblem-shared-symbolic" => "\u{f0c1}", // link

        // Clipboard
        "insert-text-symbolic" => "\u{f031}", // font (text)

        // Power
        "system-lock-screen-symbolic" => "\u{f023}", // lock
        "system-shutdown-symbolic" => "\u{f011}",    // power-off
        "system-suspend-symbolic" => "\u{f186}",     // moon

        // Settings / misc
        "view-list-symbolic" => "\u{f0ca}",            // list-ul
        "document-open-recent-symbolic" => "\u{f1da}", // clock-rotate-left

        _ => "\u{f111}", // circle — generic fallback
    }
}

/// Build a Font Awesome icon label.
/// Falls back gracefully if FA font is not installed — shows the GTK image instead.
pub fn fa_icon(icon_name: &str, size_px: u32) -> Label {
    let glyph = fa_glyph(icon_name);
    let label = Label::new(Some(glyph));
    label.add_css_class("fa-icon");
    label.set_width_request(size_px as i32);
    label.set_xalign(0.5);
    label.set_yalign(0.5);
    label.set_valign(gtk::Align::Center);
    label
}

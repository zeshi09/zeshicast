use gtk::{CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION, gdk};

use crate::{home_dir, load_preferences};

pub fn install_css() {
    super::fonts::ensure_fonts();
    let preferences = load_preferences(&home_dir().join(".config/zeshicast/preferences.toml"));
    let font_family = css_font_family(
        preferences
            .get("ui_font_family")
            .map(String::as_str)
            .unwrap_or("Outfit, Inter, Noto Sans, sans-serif"),
    );
    let font_size = preferences
        .get("ui_font_size")
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| (12..=22).contains(value))
        .unwrap_or(15);
    let subtitle_size = font_size.saturating_sub(3).max(11);
    let search_size = font_size + 2;
    let panel_title_size = font_size - 2;
    let dashboard_clock_size = 64u32;

    let density = preferences
        .get("ui_density")
        .map(String::as_str)
        .unwrap_or("compact");
    let row_height: u32 = if density == "compact" { 44 } else { 52 };

    let theme = preferences
        .get("ui_theme")
        .map(String::as_str)
        .unwrap_or("system");
    apply_gtk_theme(theme);

    let css = include_str!("../../resources/style.css")
        .replace("__FONT_FAMILY__", &font_family)
        .replace("__FONT_SIZE__", &font_size.to_string())
        .replace("__SUBTITLE_SIZE__", &subtitle_size.to_string())
        .replace("__SEARCH_SIZE__", &search_size.to_string())
        .replace("__PANEL_TITLE_SIZE__", &panel_title_size.to_string())
        .replace(
            "__DASHBOARD_CLOCK_SIZE__",
            &dashboard_clock_size.to_string(),
        )
        .replace("__ROW_HEIGHT__", &row_height.to_string());

    let provider = CssProvider::new();
    provider.load_from_data(&css);

    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn apply_gtk_theme(theme: &str) {
    if let Some(settings) = gtk::Settings::default() {
        match theme {
            "dark" => settings.set_gtk_application_prefer_dark_theme(true),
            "light" => settings.set_gtk_application_prefer_dark_theme(false),
            _ => {}
        }
    }
}

fn css_font_family(value: &str) -> String {
    let filtered = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '-' | '_' | ','))
        .collect::<String>();
    let filtered = filtered.trim();
    if filtered.is_empty() {
        "Outfit, Inter, Noto Sans, sans-serif".to_string()
    } else {
        filtered.to_string()
    }
}

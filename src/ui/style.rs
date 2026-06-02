use gtk::{CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION, gdk};

use crate::{home_dir, load_preferences};

pub fn install_css() {
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
    let panel_title_size = font_size + 1;
    let dashboard_clock_size = font_size + 11;

    let css = "
        .launcher-window {
          background: alpha(@window_bg_color, 0.985);
          border: 1px solid alpha(@window_fg_color, 0.12);
          border-radius: 12px;
          box-shadow: 0 22px 56px alpha(black, 0.36);
          font-family: __FONT_FAMILY__;
        }

        .action-panel {
          background: alpha(@window_bg_color, 0.99);
          border: 1px solid alpha(@accent_color, 0.35);
          border-radius: 12px;
          font-family: __FONT_FAMILY__;
        }

        .action-panel-title {
          font-size: __PANEL_TITLE_SIZE__px;
          font-weight: 600;
          min-height: 24px;
        }

        .search-shell {
          padding: 0 14px;
          border-bottom: 1px solid alpha(@window_fg_color, 0.08);
        }

        .search-entry {
          min-height: 60px;
          font-size: __SEARCH_SIZE__px;
          border-radius: 0;
          padding: 0 4px;
          background: transparent;
          border: none;
          box-shadow: none;
        }

        .results-list {
          background: transparent;
          padding: 4px 0;
        }

        .results-scroll {
          background: transparent;
          border: none;
          min-height: 260px;
        }

        .result-row {
          border-radius: 0;
          min-height: 52px;
        }

        .result-row:selected {
          background: alpha(@window_fg_color, 0.085);
          color: @window_fg_color;
        }

        .result-row:hover {
          background: alpha(@window_fg_color, 0.055);
        }

        .section-header-row {
          min-height: 28px;
        }

        .section-header {
          color: alpha(@window_fg_color, 0.58);
          font-size: __SUBTITLE_SIZE__px;
          font-weight: 600;
          min-height: 18px;
        }

        .category-pill {
          color: alpha(@window_fg_color, 0.7);
          font-size: __SUBTITLE_SIZE__px;
          padding: 2px 6px;
          border-radius: 6px;
          background: alpha(@window_fg_color, 0.075);
          min-height: 18px;
        }

        .result-title {
          font-size: __FONT_SIZE__px;
          font-weight: 500;
          min-height: 22px;
        }

        .result-subtitle {
          color: alpha(@window_fg_color, 0.52);
          font-size: __SUBTITLE_SIZE__px;
          min-height: 18px;
        }

        .result-icon {
          color: alpha(@window_fg_color, 0.8);
        }

        .action-bar {
          padding: 7px 12px;
          border-top: 1px solid alpha(@window_fg_color, 0.08);
        }

        .action-button {
          min-width: 38px;
          min-height: 34px;
          border-radius: 7px;
        }

        .footer-action {
          min-height: 30px;
          padding: 0 10px;
          border-radius: 7px;
          font-size: __SUBTITLE_SIZE__px;
          background: transparent;
          border: 1px solid alpha(@window_fg_color, 0.10);
        }

        .footer-action:hover {
          background: alpha(@window_fg_color, 0.06);
        }

        .status-strip {
          padding: 7px 14px 10px 14px;
          border-top: 1px solid alpha(@window_fg_color, 0.08);
        }

        .status-clock {
          font-size: __SUBTITLE_SIZE__px;
          font-weight: 600;
        }

        .status-date {
          color: alpha(@window_fg_color, 0.58);
          font-size: __SUBTITLE_SIZE__px;
        }

        .dashboard-clock {
          font-size: __DASHBOARD_CLOCK_SIZE__px;
          font-weight: 700;
        }

        .dashboard-header {
          padding-bottom: 2px;
        }

        .dashboard-header-stat {
          padding: 7px 10px;
          border-radius: 8px;
          background: alpha(@window_fg_color, 0.055);
          border: 1px solid alpha(@window_fg_color, 0.08);
        }

        .dashboard-card {
          min-height: 72px;
          padding: 10px;
          border-radius: 8px;
          background: alpha(@window_fg_color, 0.045);
          border: 1px solid alpha(@window_fg_color, 0.075);
        }

        .dashboard-card-title {
          color: alpha(@window_fg_color, 0.72);
          font-size: __SUBTITLE_SIZE__px;
          font-weight: 600;
          min-height: 18px;
        }

        .dashboard-card-value {
          color: @window_fg_color;
          font-size: __SUBTITLE_SIZE__px;
          min-height: 18px;
        }

        .dashboard-card-actions {
          padding-top: 2px;
        }

        .dashboard-button {
          min-height: 26px;
          padding: 0 8px;
          border-radius: 7px;
          font-size: __SUBTITLE_SIZE__px;
        }

        .dashboard-metric-bar trough {
          min-height: 6px;
          border-radius: 4px;
          background: alpha(@window_fg_color, 0.10);
        }

        .dashboard-metric-bar progress {
          min-height: 6px;
          border-radius: 4px;
          background: @accent_color;
        }

        .metric-graph {
          min-height: 36px;
          border-radius: 6px;
          background: alpha(@window_fg_color, 0.035);
        }

        .audio-volume-bar trough {
          min-height: 10px;
          border-radius: 6px;
          background: alpha(@window_fg_color, 0.12);
        }

        .audio-volume-bar progress {
          min-height: 10px;
          border-radius: 6px;
          background: @accent_color;
        }

        .audio-volume-value {
          font-weight: 700;
          min-width: 42px;
        }

        .resource-graphs {
          padding: 4px 0;
        }

        .resource-bar trough,
        .process-memory-bar trough {
          min-height: 8px;
          border-radius: 5px;
          background: alpha(@window_fg_color, 0.10);
        }

        .resource-bar progress,
        .process-memory-bar progress {
          min-height: 8px;
          border-radius: 5px;
          background: @accent_color;
        }

        .process-memory-bar trough,
        .process-memory-bar progress {
          min-height: 4px;
        }

        .action-section-row {
          min-height: 24px;
        }

        .action-section-label {
          color: alpha(@window_fg_color, 0.50);
          font-size: __SUBTITLE_SIZE__px;
          font-weight: 600;
          min-height: 14px;
          padding-top: 6px;
          padding-bottom: 2px;
        }

        .result-row.danger {
          color: #ff6b5f;
        }

        .result-row.danger .result-title {
          color: #ff6b5f;
        }

        .result-row.danger .result-icon {
          color: #ff6b5f;
        }

        .pref-sidebar {
          background: alpha(@window_fg_color, 0.04);
          border-right: 1px solid alpha(@window_fg_color, 0.08);
          min-width: 160px;
        }

        .pref-sidebar-row {
          min-height: 36px;
          border-radius: 0;
        }

        .pref-sidebar-row:selected {
          background: alpha(@accent_color, 0.18);
          color: @window_fg_color;
        }

        .pref-sidebar-label {
          font-size: __FONT_SIZE__px;
          font-weight: 500;
        }

        .pref-content {
          padding: 12px 14px;
        }

        .pref-field-row {
          padding: 5px 0;
        }

        .pref-field-label {
          font-size: __SUBTITLE_SIZE__px;
          color: alpha(@window_fg_color, 0.85);
        }

        .clipboard-ago {
          color: alpha(@window_fg_color, 0.45);
          font-size: __SUBTITLE_SIZE__px;
          min-width: 60px;
        }

        .ai-context-chip {
          background: alpha(@accent_color, 0.14);
          border: 1px solid alpha(@accent_color, 0.3);
          border-radius: 8px;
          padding: 2px 8px;
          font-size: __SUBTITLE_SIZE__px;
          color: @accent_color;
        }

        .ai-model-chip {
          background: alpha(@window_fg_color, 0.07);
          border: 1px solid alpha(@window_fg_color, 0.12);
          border-radius: 8px;
          padding: 2px 8px;
          font-size: __SUBTITLE_SIZE__px;
          color: alpha(@window_fg_color, 0.65);
        }
        "
    .replace("__FONT_FAMILY__", &font_family)
    .replace("__FONT_SIZE__", &font_size.to_string())
    .replace("__SUBTITLE_SIZE__", &subtitle_size.to_string())
    .replace("__SEARCH_SIZE__", &search_size.to_string())
    .replace("__PANEL_TITLE_SIZE__", &panel_title_size.to_string())
    .replace(
        "__DASHBOARD_CLOCK_SIZE__",
        &dashboard_clock_size.to_string(),
    );

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

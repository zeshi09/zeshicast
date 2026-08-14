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
    let dashboard_clock_size = font_size + 18;

    let density = preferences
        .get("ui_density")
        .map(String::as_str)
        .unwrap_or("comfortable");
    let row_height: u32 = if density == "compact" { 44 } else { 52 };

    let theme = preferences
        .get("ui_theme")
        .map(String::as_str)
        .unwrap_or("system");
    apply_gtk_theme(theme);

    let css = "
        @define-color z_bg #0d0f14;
        @define-color z_surface_0 #13161f;
        @define-color z_surface_1 #181c27;
        @define-color z_surface_2 #222736;
        @define-color z_border rgba(255, 255, 255, 0.08);
        @define-color z_border_subtle rgba(255, 255, 255, 0.05);
        @define-color z_border_hover rgba(255, 255, 255, 0.18);
        @define-color z_text_primary #f0f2f8;
        @define-color z_text_secondary #8b93a7;
        @define-color z_text_tertiary #555d72;
        @define-color z_accent #ff4a5a;
        @define-color z_accent_glow rgba(255, 74, 90, 0.22);
        @define-color z_success #34d399;
        @define-color z_warning #fbbf24;
        @define-color z_danger #f87171;

        entry,
        entry:focus,
        entry:focus-visible {
          outline: none;
          box-shadow: none;
        }

        .launcher-window {
          background: alpha(@z_bg, 0.985);
          border: 1px solid @z_border;
          border-radius: 12px;
          box-shadow: 0 24px 64px rgba(0, 0, 0, 0.55), 0 2px 8px rgba(0, 0, 0, 0.35);
          font-family: __FONT_FAMILY__;
        }

        .action-panel {
          background: alpha(@z_bg, 0.99);
          border: 1px solid @z_accent_glow;
          border-radius: 12px;
          box-shadow: 0 16px 40px rgba(0, 0, 0, 0.6);
          font-family: __FONT_FAMILY__;
        }

        .action-panel-title {
          font-size: __PANEL_TITLE_SIZE__px;
          font-weight: 600;
          color: @z_text_primary;
          min-height: 24px;
        }

        .search-shell {
          padding: 0 16px;
          border-bottom: 1px solid @z_border;
        }

        .search-entry {
          min-height: 58px;
          font-size: __SEARCH_SIZE__px;
          font-weight: 500;
          letter-spacing: -0.2px;
          color: @z_text_primary;
          border-radius: 0;
          padding: 0 4px;
          background: transparent;
          border: none;
          box-shadow: none;
          outline: none;
        }

        .search-entry placeholder {
          color: @z_text_tertiary;
          font-weight: 400;
        }

        .search-entry:focus {
          outline: none;
          box-shadow: none;
        }

        .results-list {
          background: transparent;
          padding: 6px 8px;
        }

        .results-scroll {
          background: transparent;
          border: none;
          min-height: 260px;
        }

        .result-row {
          border-radius: 8px;
          min-height: __ROW_HEIGHT__px;
          margin: 1px 0;
          transition: background 100ms ease;
        }

        .result-row:selected {
          background: alpha(@z_text_primary, 0.085);
          color: @z_text_primary;
          box-shadow: inset 3px 0 0 @z_accent;
        }

        .result-row:hover {
          background: alpha(@z_text_primary, 0.045);
        }

        .section-header-row {
          min-height: 28px;
        }

        .section-header {
          color: @z_text_tertiary;
          font-size: 11px;
          font-weight: 700;
          letter-spacing: 0.8px;
          text-transform: uppercase;
          min-height: 18px;
        }

        .category-pill {
          color: @z_text_secondary;
          font-size: 11px;
          font-weight: 700;
          letter-spacing: 0.5px;
          text-transform: uppercase;
          padding: 2px 7px;
          border-radius: 5px;
          background: @z_surface_1;
          border: 1px solid @z_border;
          min-height: 18px;
        }

        .result-title {
          font-size: __FONT_SIZE__px;
          font-weight: 600;
          color: @z_text_primary;
          min-height: 22px;
        }

        .result-subtitle {
          color: @z_text_secondary;
          font-size: __SUBTITLE_SIZE__px;
          font-weight: 400;
          min-height: 18px;
        }

        .result-icon {
          color: @z_text_primary;
        }

        .fa-icon {
          font-family: 'Font Awesome 6 Free', 'Font Awesome 6 Free Solid',
                       'FontAwesome', 'Font Awesome 5 Free';
          font-weight: 900;
          color: @z_text_secondary;
        }

        .action-bar {
          padding: 8px 14px;
          border-top: 1px solid @z_border;
          background: alpha(@z_surface_0, 0.6);
        }

        .action-button {
          min-width: 38px;
          min-height: 32px;
          border-radius: 6px;
        }

        .footer-action {
          min-height: 28px;
          padding: 0 10px;
          border-radius: 6px;
          font-size: 12px;
          font-weight: 500;
          color: @z_text_secondary;
          background: @z_surface_1;
          border: 1px solid @z_border;
          transition: all 120ms ease;
        }

        .footer-action:hover {
          background: @z_surface_2;
          color: @z_text_primary;
          border-color: @z_border_hover;
        }

        .status-strip {
          padding: 6px 16px 8px 16px;
          border-top: 1px solid @z_border;
        }

        .status-clock {
          font-size: __SUBTITLE_SIZE__px;
          font-weight: 600;
          color: @z_text_primary;
        }

        .status-date {
          color: @z_text_secondary;
          font-size: __SUBTITLE_SIZE__px;
        }

        .dashboard-clock {
          font-size: __DASHBOARD_CLOCK_SIZE__px;
          font-weight: 700;
          letter-spacing: -0.6px;
          color: @z_text_primary;
        }

        .dashboard-date {
          color: @z_text_secondary;
          font-size: __FONT_SIZE__px;
          font-weight: 400;
        }

        .dashboard-stat-chip {
          color: @z_text_secondary;
          font-size: 12px;
          font-weight: 500;
          padding: 3px 10px;
          border-radius: 16px;
          background: @z_surface_1;
          border: 1px solid @z_border;
        }

        .dashboard-header {
          padding-bottom: 2px;
        }

        .dashboard-header-stat {
          padding: 7px 10px;
          border-radius: 8px;
          background: @z_surface_0;
          border: 1px solid @z_border;
        }

        .dashboard-card {
          min-height: 86px;
          padding: 12px 14px;
          border-radius: 10px;
          background: @z_surface_0;
          border: 1px solid @z_border;
          transition: border-color 150ms ease;
        }

        .dashboard-card:hover {
          border-color: @z_border_hover;
        }

        .dashboard-card-title {
          color: @z_text_secondary;
          font-size: 11px;
          font-weight: 700;
          letter-spacing: 0.6px;
          text-transform: uppercase;
          min-height: 16px;
        }

        .dashboard-metric-value {
          color: @z_text_primary;
          font-size: __FONT_SIZE__px;
          font-weight: 700;
          min-height: 20px;
        }

        .dashboard-card-value {
          color: @z_text_primary;
          font-size: __SUBTITLE_SIZE__px;
          min-height: 16px;
        }

        .dashboard-card-actions {
          padding-top: 4px;
          gap: 6px;
        }

        .dashboard-button {
          min-height: 26px;
          padding: 0 10px;
          border-radius: 6px;
          font-size: 12px;
          font-weight: 500;
          color: @z_text_primary;
          background: @z_surface_1;
          border: 1px solid @z_border;
          transition: all 120ms ease;
        }

        .dashboard-button:hover {
          background: @z_surface_2;
          border-color: @z_border_hover;
        }

        .dashboard-metric-bar trough {
          min-height: 4px;
          border-radius: 2px;
          background: @z_surface_2;
        }

        .dashboard-metric-bar progress {
          min-height: 4px;
          border-radius: 2px;
          background: @z_accent;
        }

        .metric-graph {
          min-height: 52px;
          border-radius: 6px;
          margin-top: 6px;
        }

        .audio-volume-bar trough {
          min-height: 10px;
          border-radius: 6px;
          background: @z_surface_2;
        }

        .audio-volume-bar progress {
          min-height: 10px;
          border-radius: 6px;
          background: @z_accent;
        }

        .audio-volume-value {
          font-weight: 700;
          font-family: 'JetBrains Mono', 'Fira Code', monospace;
          color: @z_text_primary;
          min-width: 42px;
        }

        .resource-graphs {
          padding: 4px 0;
        }

        .resource-bar trough,
        .process-memory-bar trough {
          min-height: 8px;
          border-radius: 5px;
          background: @z_surface_2;
        }

        .resource-bar progress,
        .process-memory-bar progress {
          min-height: 8px;
          border-radius: 5px;
          background: @z_accent;
        }

        .process-memory-bar trough,
        .process-memory-bar progress {
          min-height: 4px;
        }

        .action-section-row {
          min-height: 24px;
        }

        .action-section-label {
          color: @z_text_tertiary;
          font-size: 11px;
          font-weight: 700;
          letter-spacing: 0.8px;
          text-transform: uppercase;
          min-height: 14px;
          padding-top: 8px;
          padding-bottom: 2px;
        }

        .result-row.danger {
          color: @z_danger;
        }

        .result-row.danger .result-title {
          color: @z_danger;
        }

        .result-row.danger .result-icon {
          color: @z_danger;
        }

        .pref-sidebar {
          background: @z_surface_0;
          border-right: 1px solid @z_border;
          min-width: 160px;
        }

        .pref-sidebar-row {
          min-height: 36px;
          border-radius: 6px;
          margin: 2px 6px;
        }

        .pref-sidebar-row:selected {
          background: @z_accent_glow;
          color: @z_text_primary;
        }

        .pref-sidebar-label {
          font-size: __FONT_SIZE__px;
          font-weight: 500;
          color: @z_text_primary;
        }

        .pref-content {
          padding: 12px 16px;
        }

        .pref-field-row {
          padding: 6px 0;
        }

        .pref-field-label {
          font-size: __SUBTITLE_SIZE__px;
          color: @z_text_primary;
          font-weight: 500;
        }

        .clipboard-ago {
          color: @z_text_tertiary;
          font-size: 11px;
          font-family: 'JetBrains Mono', 'Fira Code', monospace;
          min-width: 60px;
        }

        .ai-context-chip {
          background: @z_accent_glow;
          border: 1px solid @z_accent;
          border-radius: 8px;
          padding: 2px 8px;
          font-size: 11px;
          font-weight: 600;
          color: @z_accent;
        }

        .ai-model-chip {
          background: @z_surface_1;
          border: 1px solid @z_border;
          border-radius: 8px;
          padding: 2px 8px;
          font-size: 11px;
          font-family: 'JetBrains Mono', 'Fira Code', monospace;
          color: @z_text_secondary;
        }
        "
    .replace("__FONT_FAMILY__", &font_family)
    .replace("__FONT_SIZE__", &font_size.to_string())
    .replace("__SUBTITLE_SIZE__", &subtitle_size.to_string())
    .replace("__SEARCH_SIZE__", &search_size.to_string())
    .replace("__PANEL_TITLE_SIZE__", &panel_title_size.to_string())
    .replace("__DASHBOARD_CLOCK_SIZE__", &dashboard_clock_size.to_string())
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

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
    let panel_title_size = font_size - 2;
    let dashboard_clock_size = font_size + 18;

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

    let css = "
/* ════════════════════════════════════════════════════════════
   ANIMATIONS
   ════════════════════════════════════════════════════════════ */

@keyframes launcher-open {
  from { opacity: 0; transform: scale(0.960) translateY(-4px); }
  to   { opacity: 1; transform: scale(1.000) translateY(0);    }
}

@keyframes row-enter {
  from { opacity: 0; transform: translateY(3px); }
  to   { opacity: 1; transform: translateY(0);   }
}

@keyframes panel-slide-in {
  from { opacity: 0; transform: translateX(16px); }
  to   { opacity: 1; transform: translateX(0);    }
}

@keyframes dashboard-enter {
  from { opacity: 0; transform: translateY(6px); }
  to   { opacity: 1; transform: translateY(0);   }
}

/* ════════════════════════════════════════════════════════════
   WINDOW
   ════════════════════════════════════════════════════════════ */

.launcher-window {
  background-color: alpha(@window_bg_color, 0.985);
  border-radius: 14px;
  font-family: __FONT_FAMILY__;
  box-shadow:
    0 0  0  1px alpha(@window_fg_color, 0.090),
    0 1px 0  0   alpha(@window_fg_color, 0.060),
    0 4px 12px   alpha(black, 0.250),
    0 12px 40px  alpha(black, 0.360),
    0 32px 96px  alpha(black, 0.540);
  animation: launcher-open 110ms cubic-bezier(0.34, 1.56, 0.64, 1) both;
}

.window-invisible {
  opacity: 0;
  transform: scale(0.96) translateY(-4px);
}

/* ════════════════════════════════════════════════════════════
   SEARCH BAR
   ════════════════════════════════════════════════════════════ */

.search-bar {
  min-height: 60px;
  padding: 0 14px;
  border-bottom: 1px solid alpha(@window_fg_color, 0.070);
}

/* alias for sub-views that still use search-shell */
.search-shell {
  padding: 0 14px;
  border-bottom: 1px solid alpha(@window_fg_color, 0.070);
}

.search-input,
entry.search-input,
entry.search-input:focus,
entry.search-input:focus-within {
  font-size: __SEARCH_SIZE__px;
  font-weight: 400;
  letter-spacing: -0.02em;
  color: alpha(@window_fg_color, 0.940);
  caret-color: @accent_color;
  background-color: transparent;
  border: none;
  box-shadow: none;
  outline: none;
  padding: 0 4px;
  min-height: 56px;
}

entry.search-input text {
  background-color: transparent;
}

entry.search-input undershoot,
entry.search-input overshoot {
  background: none;
}

/* sub-view entry fields (AI chat search, emoji search, etc.) */
entry.search-entry,
entry.search-entry:focus {
  font-size: __SUBTITLE_SIZE__px;
  background: transparent;
  border: none;
  box-shadow: none;
  outline: none;
  padding: 0 4px;
  min-height: 36px;
}

entry, entry:focus, entry:focus-visible {
  outline: none;
  box-shadow: none;
}

/* Mode badge (Calculator, File Search, SSH…) */
.mode-badge {
  background: alpha(@accent_color, 0.110);
  border: 1px solid alpha(@accent_color, 0.260);
  border-radius: 7px;
  padding: 2px 8px 2px 7px;
  color: @accent_color;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
}

/* Ctrl+K hint badge */
.ctrl-k-hint {
  background: alpha(@window_fg_color, 0.055);
  border: 1px solid alpha(@window_fg_color, 0.100);
  border-radius: 5px;
  padding: 2px 5px;
  font-size: 11px;
  font-family: monospace;
  color: alpha(@window_fg_color, 0.380);
  letter-spacing: 0.01em;
}

/* ════════════════════════════════════════════════════════════
   RESULTS LIST
   ════════════════════════════════════════════════════════════ */

.results-list {
  background: transparent;
  padding: 4px 0;
}

.results-scroll {
  background: transparent;
  border: none;
  min-height: 240px;
}

.section-header-row {
  min-height: 28px;
}

.section-header {
  padding: 10px 14px 3px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.075em;
  text-transform: uppercase;
  color: alpha(@window_fg_color, 0.300);
}

/* ════════════════════════════════════════════════════════════
   RESULT ROW
   ════════════════════════════════════════════════════════════ */

.result-row {
  min-height: __ROW_HEIGHT__px;
  padding: 0 14px;
  border-radius: 0;
  background: transparent;
  animation: row-enter 130ms ease both;
  transition: background 70ms ease;
}

.result-row:hover {
  background: alpha(@window_fg_color, 0.040);
}

.result-row:selected,
.result-row.selected {
  background-color: alpha(@window_fg_color, 0.085);
  box-shadow: inset 3px 0 0 @accent_color;
  color: @window_fg_color;
}

.result-row.danger .result-title {
  color: #FF6B5F;
}

.result-row.extension-disabled {
  opacity: 0.5;
}

.result-icon {
  min-width: 18px;
  min-height: 18px;
  color: alpha(@window_fg_color, 0.72);
}

.result-icon.app {
  border-radius: 5px;
  border: 1px solid alpha(@window_fg_color, 0.055);
}

.result-icon.command {
  border-radius: 50px;
  background: alpha(@window_fg_color, 0.055);
  border: 1px solid alpha(@window_fg_color, 0.085);
  color: alpha(@window_fg_color, 0.400);
}

.fa-icon {
  font-family: 'Font Awesome 6 Free', 'Font Awesome 6 Free Solid',
               'FontAwesome', 'Font Awesome 5 Free';
  font-weight: 900;
  color: alpha(@window_fg_color, 0.72);
}

.result-title {
  font-size: __FONT_SIZE__px;
  font-weight: 500;
  letter-spacing: -0.012em;
  color: alpha(@window_fg_color, 0.930);
  min-height: 22px;
}

.result-subtitle {
  font-size: __SUBTITLE_SIZE__px;
  color: alpha(@window_fg_color, 0.400);
  margin-top: 1px;
  min-height: 18px;
}

.category-pill {
  padding: 2px 7px;
  background: alpha(@window_fg_color, 0.055);
  border-radius: 6px;
  font-size: 11px;
  font-weight: 500;
  color: alpha(@window_fg_color, 0.300);
  letter-spacing: 0.010em;
  min-height: 18px;
}

.overflow-counter {
  font-size: 11px;
  color: alpha(@window_fg_color, 0.280);
  letter-spacing: 0.010em;
}

/* ════════════════════════════════════════════════════════════
   ACTION PANEL
   ════════════════════════════════════════════════════════════ */

.action-panel {
  background: alpha(@window_bg_color, 1.000);
  border: 1px solid alpha(@accent_color, 0.30);
  border-radius: 12px;
  font-family: __FONT_FAMILY__;
  animation: panel-slide-in 120ms cubic-bezier(0.25, 0.46, 0.45, 0.94) both;
}

.action-panel-header {
  min-height: 46px;
  padding: 0 14px;
  border-bottom: 1px solid alpha(@window_fg_color, 0.060);
}

.action-panel-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.060em;
  text-transform: uppercase;
  color: alpha(@window_fg_color, 0.280);
  min-height: 24px;
}

.action-panel-filter {
  font-size: __PANEL_TITLE_SIZE__px;
  color: alpha(@window_fg_color, 0.850);
  background: transparent;
  border: none;
  box-shadow: none;
  letter-spacing: -0.010em;
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

/* ── Action rows ── */
.action-row {
  min-height: 38px;
  padding: 0 14px;
  background: transparent;
  transition: background 70ms ease;
}

.action-row:hover {
  background: alpha(@window_fg_color, 0.048);
}

.action-row:selected,
.action-row.selected {
  background: alpha(@window_fg_color, 0.070);
  box-shadow: inset 3px 0 0 @accent_color;
}

.action-row-label {
  font-size: __PANEL_TITLE_SIZE__px;
  font-weight: 500;
  letter-spacing: -0.010em;
  color: alpha(@window_fg_color, 0.850);
}

.action-row-label.danger {
  color: #FF6B5F;
}

/* ── Hotkey badges ── */
.hotkey-badge {
  background: alpha(@window_fg_color, 0.055);
  border: 1px solid alpha(@window_fg_color, 0.100);
  border-radius: 5px;
  padding: 2px 5px;
  font-size: 11px;
  font-family: monospace;
  color: alpha(@window_fg_color, 0.380);
  letter-spacing: 0.010em;
}

/* ════════════════════════════════════════════════════════════
   ACTION BAR (footer)
   ════════════════════════════════════════════════════════════ */

.action-bar {
  min-height: 38px;
  padding: 0 12px;
  border-top: 1px solid alpha(@window_fg_color, 0.060);
}

.action-bar-btn {
  min-width: 26px;
  min-height: 26px;
  padding: 0;
  border-radius: 6px;
  background: transparent;
  border: 1px solid transparent;
  color: alpha(@window_fg_color, 0.280);
  font-size: 13px;
  transition: background 120ms ease, color 120ms ease, border-color 120ms ease;
}

.action-bar-btn:hover {
  background: alpha(@window_fg_color, 0.055);
  border-color: alpha(@window_fg_color, 0.080);
  color: alpha(@window_fg_color, 0.550);
}

/* backward compat */
.action-button {
  min-width: 26px;
  min-height: 26px;
  padding: 0;
  border-radius: 6px;
  background: transparent;
  border: 1px solid transparent;
  color: alpha(@window_fg_color, 0.280);
}

.action-bar-more {
  padding: 3px 8px;
  border-radius: 6px;
  background: transparent;
  border: 1px solid transparent;
  color: alpha(@window_fg_color, 0.300);
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.010em;
  transition: background 120ms ease, color 120ms ease, border-color 120ms ease;
}

.action-bar-more:hover {
  background: alpha(@window_fg_color, 0.055);
  border-color: alpha(@window_fg_color, 0.080);
  color: alpha(@window_fg_color, 0.550);
}

.result-counter {
  font-size: 11px;
  font-weight: 500;
  color: alpha(@window_fg_color, 0.38);
  letter-spacing: -0.01em;
}

/* backward compat */
.footer-action {
  padding: 3px 8px;
  border-radius: 6px;
  background: transparent;
  border: 1px solid alpha(@window_fg_color, 0.100);
  color: alpha(@window_fg_color, 0.300);
  font-size: __SUBTITLE_SIZE__px;
  min-height: 30px;
}

.footer-action:hover {
  background: alpha(@window_fg_color, 0.055);
}

/* ════════════════════════════════════════════════════════════
   STATUS STRIP
   ════════════════════════════════════════════════════════════ */

.status-strip {
  padding: 7px 14px 10px;
  border-top: 1px solid alpha(@window_fg_color, 0.080);
}

.status-time {
  font-size: 12px;
  font-weight: 600;
  color: alpha(@window_fg_color, 0.450);
  letter-spacing: -0.010em;
}

/* backward compat */
.status-clock {
  font-size: __SUBTITLE_SIZE__px;
  font-weight: 600;
  color: alpha(@window_fg_color, 0.450);
}

.status-date {
  font-size: 11px;
  color: alpha(@window_fg_color, 0.240);
  letter-spacing: 0.010em;
}

.status-chip {
  padding: 3px 8px;
  background-color: alpha(@window_fg_color, 0.08);
  border: 1px solid alpha(@window_fg_color, 0.12);
  border-radius: 6px;
  font-size: 11px;
  font-weight: 500;
  color: alpha(@window_fg_color, 0.55);
  transition: background-color 120ms ease;
}

.status-chip:hover {
  background-color: alpha(@window_fg_color, 0.12);
}

.status-chip.active {
  background-color: alpha(@accent_color, 0.15);
  border-color: alpha(@accent_color, 0.30);
  color: @accent_color;
}

/* ════════════════════════════════════════════════════════════
   DASHBOARD VIEW
   ════════════════════════════════════════════════════════════ */

.dashboard-view {
  padding: 16px 14px 14px;
  animation: dashboard-enter 200ms ease both;
}

.dashboard-clock {
  font-size: __DASHBOARD_CLOCK_SIZE__px;
  font-weight: 700;
  letter-spacing: -0.035em;
  color: alpha(@window_fg_color, 0.950);
}

.dashboard-clock-sep {
  color: alpha(@window_fg_color, 0.350);
}

.dashboard-date {
  font-size: 13px;
  color: alpha(@window_fg_color, 0.380);
  letter-spacing: 0.010em;
  margin-top: 4px;
}

/* ── Stat chips ── */
.stat-chip {
  padding: 4px 9px;
  background-color: alpha(@window_fg_color, 0.07);
  border: 1px solid alpha(@window_fg_color, 0.10);
  border-radius: 7px;
}

.stat-chip-label {
  font-size: 11px;
  color: alpha(@window_fg_color, 0.360);
  font-weight: 500;
}

.stat-chip-value {
  font-size: 12px;
  color: alpha(@window_fg_color, 0.780);
  font-weight: 600;
  letter-spacing: -0.010em;
}

/* backward compat */
.dashboard-stat-chip {
  padding: 4px 9px;
  background: alpha(@window_fg_color, 0.050);
  border: 1px solid alpha(@window_fg_color, 0.080);
  border-radius: 7px;
  color: alpha(@window_fg_color, 0.680);
  font-size: __SUBTITLE_SIZE__px;
}

/* ── Metric cards ── */
.metric-card {
  background-color: alpha(@window_fg_color, 0.055);
  border: 1px solid alpha(@window_fg_color, 0.090);
  border-radius: 10px;
  padding: 13px 14px;
  animation: dashboard-enter 200ms ease both;
  transition: background-color 140ms ease, border-color 140ms ease;
}

.metric-card:hover {
  background-color: alpha(@window_fg_color, 0.085);
  border-color: alpha(@window_fg_color, 0.130);
  transform: translateY(-1px);
}

.metric-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.065em;
  text-transform: uppercase;
  color: alpha(@window_fg_color, 0.300);
  margin-bottom: 6px;
}

.metric-value {
  font-size: 26px;
  font-weight: 700;
  letter-spacing: -0.025em;
  color: alpha(@window_fg_color, 0.930);
}

.metric-unit {
  font-size: 12px;
  font-weight: 500;
  color: alpha(@window_fg_color, 0.360);
}

/* ── Progress bars (new design) ── */
.progress-track {
  min-height: 3px;
  background: alpha(@window_fg_color, 0.070);
  border-radius: 2px;
  margin-top: 8px;
}

.progress-fill {
  min-height: 3px;
  border-radius: 2px;
  background: @accent_color;
  transition: min-width 600ms cubic-bezier(0.34, 1.10, 0.64, 1);
}

.progress-fill.warn  { background: #F5A623; }
.progress-fill.crit  { background: #FF6B5F; }

/* ── Progress bars (GTK ProgressBar widget compat) ── */
.dashboard-metric-bar trough {
  min-height: 3px;
  border-radius: 2px;
  background: alpha(@window_fg_color, 0.070);
}

.dashboard-metric-bar progress {
  min-height: 3px;
  border-radius: 2px;
  background: @accent_color;
  transition: background 300ms ease;
}

.dashboard-metric-bar.warning progress {
  background: #F5A623;
}

.dashboard-metric-bar.danger progress {
  background: #FF6B5F;
}

/* ── Control cards ── */
.control-card {
  background-color: alpha(@window_fg_color, 0.055);
  border: 1px solid alpha(@window_fg_color, 0.090);
  border-radius: 10px;
  padding: 12px 14px;
  animation: dashboard-enter 200ms ease both;
  transition: background-color 140ms ease, border-color 140ms ease;
}

.control-card:hover {
  background-color: alpha(@window_fg_color, 0.085);
  border-color: alpha(@window_fg_color, 0.130);
}

.control-card-icon {
  min-width: 26px;
  min-height: 26px;
  border-radius: 7px;
  background: alpha(@window_fg_color, 0.055);
  border: 1px solid alpha(@window_fg_color, 0.080);
  color: alpha(@window_fg_color, 0.450);
  font-size: 13px;
}

.control-card-icon.active {
  background: alpha(@accent_color, 0.130);
  border-color: alpha(@accent_color, 0.260);
  color: @accent_color;
}

.control-card-heading {
  font-size: 12px;
  font-weight: 600;
  color: alpha(@window_fg_color, 0.550);
  letter-spacing: 0.010em;
}

.control-card-value {
  font-size: 14px;
  font-weight: 600;
  letter-spacing: -0.010em;
  color: alpha(@window_fg_color, 0.880);
}

.control-card-sub {
  font-size: 11px;
  color: alpha(@window_fg_color, 0.360);
}

.control-card-actions {
  padding-top: 4px;
}

/* backward compat for old dashboard-card classes */
.dashboard-card {
  background: alpha(@window_fg_color, 0.038);
  border: 1px solid alpha(@window_fg_color, 0.070);
  border-radius: 10px;
  padding: 12px 14px;
  animation: dashboard-enter 200ms ease both;
  transition: background 140ms ease;
}

.dashboard-card-title {
  font-size: __SUBTITLE_SIZE__px;
  font-weight: 600;
  color: alpha(@window_fg_color, 0.550);
}

.dashboard-metric-value {
  font-size: __FONT_SIZE__px;
  font-weight: 700;
  color: @window_fg_color;
  min-height: 20px;
}

.dashboard-card-value {
  font-size: 14px;
  font-weight: 600;
  color: alpha(@window_fg_color, 0.880);
  min-height: 16px;
}

.dashboard-card-actions {
  padding-top: 4px;
}

.dashboard-header-stat {
  padding: 7px 10px;
  border-radius: 8px;
  background: alpha(@window_fg_color, 0.050);
  border: 1px solid alpha(@window_fg_color, 0.080);
}

.dashboard-button {
  min-height: 24px;
  padding: 0 8px;
  border-radius: 6px;
  background: alpha(@window_fg_color, 0.060);
  border: 1px solid alpha(@window_fg_color, 0.090);
  font-size: __SUBTITLE_SIZE__px;
  color: alpha(@window_fg_color, 0.700);
  transition: background 100ms ease;
}

.dashboard-button:hover {
  background: alpha(@window_fg_color, 0.100);
}

.metric-graph {
  min-height: 52px;
  border-radius: 6px;
  margin-top: 6px;
}

/* ════════════════════════════════════════════════════════════
   AI CHAT VIEW
   ════════════════════════════════════════════════════════════ */

.ai-model-bar {
  padding: 8px 14px;
  border-bottom: 1px solid alpha(@window_fg_color, 0.060);
}

.ai-model-btn {
  padding: 2px 8px;
  background: alpha(@window_fg_color, 0.040);
  border: 1px solid alpha(@window_fg_color, 0.070);
  border-radius: 5px;
  font-size: 11px;
  font-family: monospace;
  color: alpha(@window_fg_color, 0.380);
  transition: background 100ms ease, color 100ms ease, border-color 100ms ease;
}

.ai-model-btn.active {
  background: alpha(@accent_color, 0.130);
  border-color: alpha(@accent_color, 0.310);
  color: @accent_color;
}

/* backward compat */
.ai-model-chip {
  background: alpha(@window_fg_color, 0.040);
  border: 1px solid alpha(@window_fg_color, 0.070);
  border-radius: 5px;
  font-size: 11px;
  color: alpha(@window_fg_color, 0.380);
}

.ai-message-user {
  background-color: alpha(@accent_color, 0.18);
  border: 1px solid alpha(@accent_color, 0.30);
  border-radius: 10px;
  border-bottom-right-radius: 3px;
  padding: 8px 12px;
  font-size: 13px;
  color: alpha(@window_fg_color, 0.930);
  margin-start: 40px;
}

.ai-message-assistant {
  background-color: alpha(@window_fg_color, 0.07);
  border: 1px solid alpha(@window_fg_color, 0.11);
  border-radius: 10px;
  border-bottom-left-radius: 3px;
  padding: 8px 12px;
  font-size: 13px;
  color: alpha(@window_fg_color, 0.850);
  margin-end: 40px;
}

.ai-input-row {
  border-top: 1px solid alpha(@window_fg_color, 0.060);
  padding: 8px 14px;
}

.ai-send-btn {
  min-width: 26px;
  min-height: 26px;
  border-radius: 7px;
  border: none;
  background: alpha(@window_fg_color, 0.070);
  color: alpha(@window_fg_color, 0.250);
  font-size: 12px;
  transition: background 140ms ease, color 140ms ease;
}

.ai-send-btn.ready {
  background: @accent_color;
  color: white;
}

.ai-context-chip {
  background: alpha(@accent_color, 0.140);
  border: 1px solid alpha(@accent_color, 0.300);
  border-radius: 8px;
  padding: 2px 8px;
  font-size: __SUBTITLE_SIZE__px;
  color: @accent_color;
}

/* ════════════════════════════════════════════════════════════
   CLIPBOARD VIEW
   ════════════════════════════════════════════════════════════ */

.clipboard-row {
  min-height: 52px;
  padding: 0 14px;
  background: transparent;
  animation: row-enter 130ms ease both;
  transition: background 70ms ease;
}

.clipboard-row:hover {
  background: alpha(@window_fg_color, 0.040);
}

.clipboard-row:selected {
  background: alpha(@window_fg_color, 0.085);
  box-shadow: inset 3px 0 0 @accent_color;
}

.clipboard-text {
  font-size: 13px;
  color: alpha(@window_fg_color, 0.880);
}

.clipboard-text.code {
  font-family: monospace;
  font-size: 12px;
  letter-spacing: -0.010em;
}

.clipboard-time {
  font-size: 11px;
  color: alpha(@window_fg_color, 0.280);
}

/* backward compat */
.clipboard-ago {
  color: alpha(@window_fg_color, 0.280);
  font-size: __SUBTITLE_SIZE__px;
  min-width: 60px;
}

/* ════════════════════════════════════════════════════════════
   AUDIO VIEW
   ════════════════════════════════════════════════════════════ */

.audio-volume-bar trough {
  min-height: 6px;
  border-radius: 4px;
  background: alpha(@window_fg_color, 0.100);
}

.audio-volume-bar progress {
  min-height: 6px;
  border-radius: 4px;
  background: @accent_color;
}

.audio-volume-value {
  font-weight: 600;
  font-size: 13px;
  min-width: 42px;
}

/* ════════════════════════════════════════════════════════════
   SYSTEM MONITOR
   ════════════════════════════════════════════════════════════ */

.resource-graphs {
  padding: 4px 0;
}

.resource-bar trough,
.process-memory-bar trough {
  min-height: 6px;
  border-radius: 4px;
  background: alpha(@window_fg_color, 0.100);
}

.resource-bar progress,
.process-memory-bar progress {
  min-height: 6px;
  border-radius: 4px;
  background: @accent_color;
}

.process-memory-bar trough,
.process-memory-bar progress {
  min-height: 3px;
}

/* ════════════════════════════════════════════════════════════
   MEDIA VIEW
   ════════════════════════════════════════════════════════════ */

.media-title {
  font-size: __FONT_SIZE__px;
  font-weight: 600;
  letter-spacing: -0.015em;
  color: alpha(@window_fg_color, 0.930);
}

/* ════════════════════════════════════════════════════════════
   PREFERENCES
   ════════════════════════════════════════════════════════════ */

.pref-sidebar {
  background: alpha(@window_fg_color, 0.040);
  border-right: 1px solid alpha(@window_fg_color, 0.080);
  min-width: 138px;
}

.pref-sidebar-row {
  min-height: 34px;
  border-radius: 0;
}

.pref-sidebar-row:selected {
  background: alpha(@window_fg_color, 0.070);
  box-shadow: inset 3px 0 0 @accent_color;
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
  min-height: 36px;
  border-bottom: 1px solid alpha(@window_fg_color, 0.040);
}

.pref-field-label {
  font-size: __SUBTITLE_SIZE__px;
  color: alpha(@window_fg_color, 0.680);
}

/* ════════════════════════════════════════════════════════════
   SCALE (GtkScale — volume, scrubber)
   ════════════════════════════════════════════════════════════ */

scale trough {
  min-height: 4px;
  border-radius: 3px;
  background: alpha(@window_fg_color, 0.100);
}

scale trough highlight {
  min-height: 4px;
  border-radius: 3px;
  background: @accent_color;
}

scale slider {
  min-width: 14px;
  min-height: 14px;
  border-radius: 50%;
  background: white;
  box-shadow: 0 1px 3px alpha(black, 0.3);
}

scale.audio-volume-bar trough { min-height: 6px; border-radius: 4px; }
scale.audio-volume-bar trough highlight { min-height: 6px; border-radius: 4px; }

/* GtkSwitch styling */
switch {
  border-radius: 9px;
  background: alpha(@window_fg_color, 0.120);
  border: 1px solid alpha(@window_fg_color, 0.100);
  min-width: 34px;
  min-height: 18px;
}

switch:checked {
  background: @accent_color;
  border-color: @accent_color;
}

switch slider {
  border-radius: 50%;
  background: white;
  min-width: 14px;
  min-height: 14px;
  margin: 2px;
}

/* ════════════════════════════════════════════════════════════
   SCROLLBAR
   ════════════════════════════════════════════════════════════ */

scrollbar {
  background: transparent;
  min-width: 4px;
  min-height: 4px;
}

scrollbar slider {
  background: alpha(@window_fg_color, 0.100);
  border-radius: 2px;
  min-width: 4px;
  min-height: 24px;
}

scrollbar slider:hover {
  background: alpha(@window_fg_color, 0.180);
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

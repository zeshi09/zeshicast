mod action_panel_controller;
mod clipboard_capture;
mod fonts;
mod forms;
pub(crate) mod icons;
mod keybindings;
mod launcher;
mod launcher_helpers;
mod launcher_views;
mod markdown;
mod navigation;
mod notify_server;
mod osd;
mod panels;
mod preferences;
mod search_flow;
mod status_strip;
mod style;
mod views;
mod widgets;

pub use forms::show_form_panel;
pub use launcher::{GuiState, ensure_ui, present_launcher, present_launcher_view};
pub use navigation::{LauncherView, NavigationStack};
pub use osd::{dismiss_notification_osd, show_layout_osd, show_notification_osd};
pub use panels::{show_alias_panel, show_confirmation_panel, show_snippet_editor_panel};
pub use status_strip::StatusStrip;
pub use style::install_css;

/// Whether the palette window is visible (P3.3).
static UI_VISIBLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// `true` while the palette is hidden.
///
/// Recurring timers check this first: a hidden palette must not repaint views,
/// re-read clocks or fork anything. Deliberate exceptions: the clipboard monitor
/// (it has to notice copies while hidden), the child reaper (it only calls
/// `waitpid`), the keyboard-layout OSD drain (a separate layer-shell surface)
/// and the in-flight result pollers, which stop on their own.
pub(crate) fn hidden() -> bool {
    !UI_VISIBLE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Keeps [`hidden`] and the background pollers in step with the window (P3.3):
/// hiding the palette pauses `poll_cache`, showing it resumes the poller and the
/// timers on their next tick.
pub(crate) fn track_window_visibility(window: &gtk::ApplicationWindow) {
    use gtk::prelude::WidgetExt;

    let update = |window: &gtk::ApplicationWindow| {
        let visible = window.is_visible();
        UI_VISIBLE.store(visible, std::sync::atomic::Ordering::Relaxed);
        crate::services::poll_cache::set_active(visible);
    };
    update(window);
    window.connect_visible_notify(update);
}

/// Periodic housekeeping for the GTK main loop.
///
/// The launcher spawns applications and shell commands fire-and-forget; this
/// sweep reaps the ones that have exited so they do not stay `<defunct>` in the
/// process table for the lifetime of the daemon (M-9).
pub fn install_child_reaper() {
    gtk::glib::timeout_add_seconds_local(1, || {
        crate::reap_finished_children();
        gtk::glib::ControlFlow::Continue
    });
}
pub(crate) use views::row_process;
pub use views::{
    ActionPanelDisplayItem, ActionPanelView, AiChatView, AudioView, ClipboardHistoryView,
    DashboardView, EmojiPickerView, ExtensionBrowserView, FontBrowserView, MediaView, NetworkView,
    NotificationsView, PreferencesView, ScriptOutputView, SnippetManagerView, SystemMonitorView,
    WindowGridView, action_panel_view, ai_chat_view, audio_view, clipboard_history_view,
    dashboard_view, emoji_picker_view, extension_browser_view, font_browser_view, media_view,
    network_view, notifications_view, preferences_view, script_output_view, set_action_panel_items,
    set_action_panel_list, set_audio_snapshot, set_clipboard_detail, set_clipboard_history_items,
    set_dashboard_audio_snapshot, set_dashboard_battery_snapshot, set_dashboard_media_snapshot,
    set_dashboard_network_snapshot, set_dashboard_notification_snapshot, set_dashboard_snapshot,
    set_dashboard_thermal, set_media_snapshot, set_network_snapshot, set_notification_snapshot,
    set_script_output, set_snippet_items, set_system_monitor_snapshot,
    set_system_monitor_thermal_snapshot, snippet_manager_view, system_monitor_view,
    window_grid_view,
};
pub use widgets::{
    action_panel, control_card, letter_icon, metric_card, move_selection, panel_root, panel_title,
    result_row, results_list, scrollable_list, secondary_action_row, section_header,
};

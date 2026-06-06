mod forms;
pub(crate) mod icons;
mod launcher;
mod launcher_helpers;
mod launcher_views;
mod navigation;
mod panels;
mod preferences;
mod status_strip;
mod style;
mod views;
mod widgets;

pub use forms::show_form_panel;
pub use launcher::{GuiState, ensure_ui, present_launcher};
pub use navigation::{LauncherView, NavigationStack};
pub use panels::{show_alias_panel, show_extension_browser, show_preferences_editor};
pub use status_strip::StatusStrip;
pub use style::install_css;
pub use views::{
    ActionPanelDisplayItem, ActionPanelView, AiChatView, AudioView, ClipboardHistoryView,
    DashboardView, EmojiPickerView, ExtensionBrowserView, FontBrowserView, MediaView, NetworkView,
    NotificationsView, PreferencesView, ScriptOutputView, SnippetManagerView, SystemMonitorView,
    action_panel_view, ai_chat_view, audio_view, clipboard_history_view, dashboard_view,
    emoji_picker_view, extension_browser_view, font_browser_view, media_view, network_view,
    notifications_view, preferences_view, script_output_view, set_action_panel_items,
    set_action_panel_list, set_audio_snapshot, set_clipboard_detail, set_clipboard_history_items,
    set_dashboard_audio_snapshot, set_dashboard_battery_snapshot, set_dashboard_media_snapshot,
    set_dashboard_network_snapshot, set_dashboard_notification_snapshot, set_dashboard_snapshot,
    set_dashboard_thermal, set_media_snapshot, set_network_snapshot, set_notification_snapshot,
    set_script_output, set_snippet_items, set_system_monitor_snapshot,
    set_system_monitor_thermal_snapshot, snippet_manager_view, system_monitor_view,
};
pub use widgets::{
    action_panel, control_card, metric_card, move_selection, panel_root, panel_title, result_row,
    results_list, scrollable_list, secondary_action_row, section_header,
};

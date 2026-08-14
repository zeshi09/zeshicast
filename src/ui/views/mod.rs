mod action_panel;
mod ai_chat;
mod audio;
mod clipboard_history;
pub(crate) mod dashboard;
mod extension_browser;
mod media;
mod network;
mod notifications;
mod preferences;
mod script_output;
mod snippets;
pub(crate) mod system_monitor;
mod window_grid;

pub use action_panel::{
    ActionPanelDisplayItem, ActionPanelView, action_panel_view, set_action_panel_items,
    set_action_panel_list,
};
pub use ai_chat::{AiChatView, ai_chat_view};
pub use audio::{AudioView, audio_view, set_audio_snapshot};
pub use clipboard_history::{
    ClipboardHistoryView, clipboard_history_view, set_clipboard_detail,
    set_clipboard_history_items,
};
pub use dashboard::{
    DashboardView, dashboard_view, set_dashboard_audio_snapshot, set_dashboard_battery_snapshot,
    set_dashboard_media_snapshot, set_dashboard_network_snapshot,
    set_dashboard_notification_snapshot, set_dashboard_snapshot, set_dashboard_thermal,
};
pub use extension_browser::{ExtensionBrowserView, extension_browser_view};
pub use media::{MediaView, media_view, set_media_snapshot};
pub use network::{NetworkView, network_view, set_network_snapshot};
pub use notifications::{NotificationsView, notifications_view, set_notification_snapshot};
pub use preferences::{PreferencesView, preferences_view};
pub use script_output::{ScriptOutputView, script_output_view, set_script_output};
pub use snippets::{SnippetManagerView, snippet_manager_view, set_snippet_items};
#[allow(unused_imports)]
pub use system_monitor::MetricGraph;
pub use system_monitor::{
    SystemMonitorView, set_system_monitor_snapshot,
    set_system_monitor_thermal_snapshot, system_monitor_view,
};
pub use window_grid::{
    GridSnapTarget, WindowGridView, execute_close_focused_window, execute_column_resize,
    execute_grid_snap, window_grid_view,
};

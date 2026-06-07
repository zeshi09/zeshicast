use gtk::prelude::*;
use gtk::{Box as GtkBox, Entry, ListBox};

pub(super) fn show_font_browser_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    font_view: &crate::ui::FontBrowserView,
) {
    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::Fonts);
    font_view.search.grab_focus();
}

pub(super) fn show_emoji_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    emoji_view: &crate::ui::EmojiPickerView,
) {
    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::Emoji);
    emoji_view.search.grab_focus();
}

pub(super) fn show_dashboard_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    dashboard_view: &crate::ui::DashboardView,
) {
    crate::ui::set_dashboard_snapshot(dashboard_view, &crate::system_snapshot());
    crate::ui::set_dashboard_thermal(
        dashboard_view,
        crate::thermal_snapshot().hottest_zone().map(|z| z.temperature_c),
    );
    crate::ui::set_dashboard_network_snapshot(dashboard_view, &crate::network_snapshot());
    crate::ui::set_dashboard_battery_snapshot(dashboard_view, &crate::battery_snapshot());
    crate::ui::set_dashboard_audio_snapshot(dashboard_view, &crate::audio_snapshot());
    crate::ui::set_dashboard_media_snapshot(dashboard_view, &crate::media_snapshot());
    crate::ui::set_dashboard_notification_snapshot(dashboard_view, &crate::notification_snapshot());
    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::Dashboard);
}

pub(super) fn show_system_monitor_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    system_monitor_view: &crate::ui::SystemMonitorView,
) {
    crate::ui::set_system_monitor_snapshot(
        system_monitor_view,
        &crate::system_snapshot(),
        &crate::top_processes_by_memory(8),
    );
    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::SystemMonitor);
    if let Some(row) = system_monitor_view.list.row_at_index(0) {
        system_monitor_view.list.select_row(Some(&row));
    }
    system_monitor_view.list.grab_focus();
}

pub(super) fn show_ai_chat_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    ai_chat_view: &crate::ui::AiChatView,
) {
    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::AiChat);
    ai_chat_view.input.grab_focus();
}

pub(super) fn show_audio_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    audio_view: &crate::ui::AudioView,
) {
    crate::ui::set_audio_snapshot(audio_view, &crate::audio_snapshot());
    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::Audio);
    audio_view.streams_list.grab_focus();
}

pub(super) fn show_network_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    network_list: &ListBox,
) {
    crate::ui::set_network_snapshot(network_list, &crate::network_snapshot());
    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::Network);
    if let Some(row) = network_list.row_at_index(0) {
        network_list.select_row(Some(&row));
    }
    network_list.grab_focus();
}

pub(super) fn show_notifications_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    notifications_view: &crate::ui::NotificationsView,
) {
    crate::ui::set_notification_snapshot(notifications_view, &crate::notification_snapshot());
    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::Notifications);
}

pub(super) fn show_media_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    media_view: &crate::ui::MediaView,
) {
    crate::ui::set_media_snapshot(media_view, &crate::media_snapshot());
    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::Media);
}

pub(super) fn show_script_output_view(
    navigation: &crate::ui::NavigationStack,
    entry: &gtk::Entry,
    action_bar: &gtk::Box,
    view: &crate::ui::ScriptOutputView,
    script_title: &str,
    stdout: &str,
) {
    crate::ui::set_script_output(view, script_title, stdout);
    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::ScriptOutput);
}

pub(super) fn run_launcher_command(
    command: crate::LauncherCommand,
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    ai_chat_view: &crate::ui::AiChatView,
    audio_view: &crate::ui::AudioView,
    dashboard_view: &crate::ui::DashboardView,
    emoji_view: &crate::ui::EmojiPickerView,
    font_view: &crate::ui::FontBrowserView,
    system_monitor_view: &crate::ui::SystemMonitorView,
    media_view: &crate::ui::MediaView,
    network_list: &ListBox,
    notifications_view: &crate::ui::NotificationsView,
) {
    match command {
        crate::LauncherCommand::AiChat => {
            show_ai_chat_view(navigation, entry, action_bar, ai_chat_view)
        }
        crate::LauncherCommand::Audio => show_audio_view(navigation, entry, action_bar, audio_view),
        crate::LauncherCommand::Dashboard => {
            show_dashboard_view(navigation, entry, action_bar, dashboard_view)
        }
        crate::LauncherCommand::Emoji => {
            show_emoji_view(navigation, entry, action_bar, emoji_view)
        }
        crate::LauncherCommand::Fonts => {
            show_font_browser_view(navigation, entry, action_bar, font_view)
        }
        crate::LauncherCommand::SystemMonitor => {
            show_system_monitor_view(navigation, entry, action_bar, system_monitor_view)
        }
        crate::LauncherCommand::Media => show_media_view(navigation, entry, action_bar, media_view),
        crate::LauncherCommand::Network => {
            show_network_view(navigation, entry, action_bar, network_list)
        }
        crate::LauncherCommand::Notifications => {
            show_notifications_view(navigation, entry, action_bar, notifications_view)
        }
    }
}

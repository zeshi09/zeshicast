#![allow(clippy::too_many_arguments)]

use std::cell::RefCell;
use std::rc::Rc;
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{ApplicationWindow, Box as GtkBox, Entry, ListBox};
use crate::{
    Action, ClipboardSummary, SecondaryActionKind, SnippetSummary, Zeshicast,
};
use crate::ui::launcher_views::{
    show_ai_chat_view, show_audio_view, show_dashboard_view, show_emoji_view,
    show_font_browser_view, show_media_view, show_network_view, show_notifications_view,
    show_system_monitor_view,
};
use super::action_panel_controller::{
    ActionPanelItem, DisplayedActionPanelRow, run_action_panel_row, show_action_panel_view,
};
use super::launcher::{
    clear_clipboard_history_or_confirm, clipboard_item_action, copy_clipboard_row,
    copy_selected, copy_snippet_row, finish_interaction, refresh_clipboard_view,
    refresh_snippet_view, run_secondary_action_or_confirm, run_selected_with_views,
    show_clipboard_view, show_extension_view, show_preferences_view, show_root_view,
    show_snippet_view, terminate_selected_system_process_or_confirm,
};

/// Resolve a hardware keycode to its keyval in the primary (Latin) layout group,
/// independent of the currently active keyboard layout. Lets Ctrl-shortcuts
/// match on e.g. a Cyrillic layout where the produced keyval would be Cyrillic.
pub(crate) fn latin_keyval(keycode: u32) -> Option<gdk::Key> {
    gdk::Display::default()
        .and_then(|display| display.translate_key(keycode, gdk::ModifierType::empty(), 0))
        .map(|(keyval, _, _, _)| keyval)
}

pub(crate) fn handle_key(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    entry: &Entry,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    action_bar: &GtkBox,
    navigation: &crate::ui::NavigationStack,
    action_panel_view: &crate::ui::ActionPanelView,
    ai_chat_view: &crate::ui::AiChatView,
    audio_view: &crate::ui::AudioView,
    dashboard_view: &crate::ui::DashboardView,
    emoji_view: &crate::ui::EmojiPickerView,
    font_view: &crate::ui::FontBrowserView,
    system_monitor_view: &crate::ui::SystemMonitorView,
    media_view: &crate::ui::MediaView,
    network_list: &ListBox,
    notifications_view: &crate::ui::NotificationsView,
    current_action: &Rc<RefCell<Option<Action>>>,
    action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    filtered_action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    displayed_action_panel_rows: &Rc<RefCell<Vec<DisplayedActionPanelRow>>>,
    clipboard_view: &crate::ui::ClipboardHistoryView,
    clipboard_items: &Rc<RefCell<Vec<ClipboardSummary>>>,
    extension_list: &ListBox,
    snippet_list: &ListBox,
    snippet_items: &Rc<RefCell<Vec<SnippetSummary>>>,
    script_output_view: &crate::ui::ScriptOutputView,
    key: gdk::Key,
    state: gdk::ModifierType,
) -> glib::Propagation {
    if navigation.current() != crate::ui::LauncherView::Root {
        return handle_view_key(
            window,
            launcher,
            list,
            results,
            navigation,
            entry,
            action_bar,
            &action_panel_view.list,
            ai_chat_view,
            audio_view,
            dashboard_view,
            system_monitor_view,
            media_view,
            network_list,
            notifications_view,
            current_action,
            displayed_action_panel_rows,
            clipboard_view,
            clipboard_items,
            extension_list,
            snippet_list,
            snippet_items,
            key,
            state,
        );
    }

    match key {
        gdk::Key::Escape => {
            finish_interaction(window, hold);
            glib::Propagation::Stop
        }
        gdk::Key::Return | gdk::Key::KP_Enter => {
            if state.contains(gdk::ModifierType::CONTROL_MASK) {
                copy_selected(list, results);
            } else {
                run_selected_with_views(
                    window,
                    launcher,
                    hold,
                    entry,
                    list,
                    results,
                    navigation,
                    action_bar,
                    ai_chat_view,
                    audio_view,
                    dashboard_view,
                    emoji_view,
                    font_view,
                    system_monitor_view,
                    media_view,
                    network_list,
                    notifications_view,
                    script_output_view,
                );
            }
            glib::Propagation::Stop
        }
        gdk::Key::k if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_action_panel_view(
                navigation,
                entry,
                action_bar,
                action_panel_view,
                current_action,
                action_panel_items,
                filtered_action_panel_items,
                displayed_action_panel_rows,
                launcher,
                list,
                results,
            );
            glib::Propagation::Stop
        }
        gdk::Key::s if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_snippet_view(
                navigation,
                entry,
                action_bar,
                snippet_list,
                snippet_items,
                launcher,
            );
            glib::Propagation::Stop
        }
        gdk::Key::d if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_dashboard_view(navigation, entry, action_bar, dashboard_view);
            glib::Propagation::Stop
        }
        gdk::Key::t if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_system_monitor_view(navigation, entry, action_bar, system_monitor_view);
            glib::Propagation::Stop
        }
        gdk::Key::i if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_ai_chat_view(navigation, entry, action_bar, ai_chat_view);
            glib::Propagation::Stop
        }
        gdk::Key::m if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_media_view(navigation, entry, action_bar, media_view);
            glib::Propagation::Stop
        }
        gdk::Key::o if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_audio_view(navigation, entry, action_bar, audio_view);
            glib::Propagation::Stop
        }
        gdk::Key::n if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_network_view(navigation, entry, action_bar, network_list);
            glib::Propagation::Stop
        }
        gdk::Key::u if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_notifications_view(navigation, entry, action_bar, notifications_view);
            glib::Propagation::Stop
        }
        gdk::Key::h if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_clipboard_view(
                navigation,
                entry,
                action_bar,
                clipboard_view,
                clipboard_items,
                launcher,
            );
            glib::Propagation::Stop
        }
        gdk::Key::b if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_extension_view(navigation, entry, action_bar, extension_list);
            glib::Propagation::Stop
        }
        gdk::Key::e if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_emoji_view(navigation, entry, action_bar, emoji_view);
            glib::Propagation::Stop
        }
        gdk::Key::f if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_font_browser_view(navigation, entry, action_bar, font_view);
            glib::Propagation::Stop
        }
        gdk::Key::comma if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_preferences_view(navigation, entry, action_bar);
            glib::Propagation::Stop
        }
        gdk::Key::Down => {
            crate::ui::move_selection(list, 1);
            glib::Propagation::Stop
        }
        gdk::Key::Up => {
            crate::ui::move_selection(list, -1);
            glib::Propagation::Stop
        }
        _ => glib::Propagation::Proceed,
    }
}

fn handle_view_key(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    action_panel_list: &ListBox,
    ai_chat_view: &crate::ui::AiChatView,
    audio_view: &crate::ui::AudioView,
    dashboard_view: &crate::ui::DashboardView,
    system_monitor_view: &crate::ui::SystemMonitorView,
    media_view: &crate::ui::MediaView,
    network_list: &ListBox,
    notifications_view: &crate::ui::NotificationsView,
    current_action: &Rc<RefCell<Option<Action>>>,
    displayed_action_panel_rows: &Rc<RefCell<Vec<DisplayedActionPanelRow>>>,
    clipboard_view: &crate::ui::ClipboardHistoryView,
    clipboard_items: &Rc<RefCell<Vec<ClipboardSummary>>>,
    extension_list: &ListBox,
    snippet_list: &ListBox,
    snippet_items: &Rc<RefCell<Vec<SnippetSummary>>>,
    key: gdk::Key,
    state: gdk::ModifierType,
) -> glib::Propagation {
    match key {
        gdk::Key::Escape => {
            if navigation.pop().is_some() {
                entry.set_visible(true);
                action_bar.set_visible(true);
                entry.grab_focus();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
        gdk::Key::Return | gdk::Key::KP_Enter => match navigation.current() {
            crate::ui::LauncherView::Actions => {
                if let Some(row) = action_panel_list.selected_row() {
                    run_action_panel_row(
                        window,
                        launcher,
                        entry,
                        list,
                        results,
                        navigation,
                        action_bar,
                        current_action,
                        displayed_action_panel_rows,
                        row.index() as usize,
                    );
                }
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Clipboard => {
                if let Some(row) = clipboard_view.list.selected_row() {
                    copy_clipboard_row(&clipboard_view.list, row.index() as usize, clipboard_items);
                }
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Extensions => {
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Dashboard => {
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::SystemMonitor => {
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::AiChat => {
                if ai_chat_view.input.text().is_empty() {
                    show_root_view(navigation, entry, action_bar);
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            }
            crate::ui::LauncherView::Audio => {
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Media => {
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Network => {
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Notifications => {
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Snippets => {
                if let Some(row) = snippet_list.selected_row() {
                    copy_snippet_row(row.index() as usize, snippet_items);
                }
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        },
        gdk::Key::Down => match navigation.current() {
            crate::ui::LauncherView::Actions => {
                crate::ui::move_selection(action_panel_list, 1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Clipboard => {
                crate::ui::move_selection(&clipboard_view.list, 1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Extensions => {
                crate::ui::move_selection(extension_list, 1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Dashboard => {
                crate::ui::set_dashboard_snapshot(dashboard_view, &crate::system_snapshot());
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::SystemMonitor => {
                crate::ui::move_selection(&system_monitor_view.list, 1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Audio => {
                crate::ui::move_selection(&audio_view.streams_list, 1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Media => {
                crate::ui::set_media_snapshot(media_view, &crate::media_snapshot());
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Network => {
                crate::ui::move_selection(network_list, 1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Notifications => {
                crate::ui::set_notification_snapshot(
                    notifications_view,
                    &crate::notification_snapshot(),
                );
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Snippets => {
                crate::ui::move_selection(snippet_list, 1);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        },
        gdk::Key::Up => match navigation.current() {
            crate::ui::LauncherView::Actions => {
                crate::ui::move_selection(action_panel_list, -1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Clipboard => {
                crate::ui::move_selection(&clipboard_view.list, -1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Extensions => {
                crate::ui::move_selection(extension_list, -1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Dashboard => {
                crate::ui::set_dashboard_snapshot(dashboard_view, &crate::system_snapshot());
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::SystemMonitor => {
                crate::ui::move_selection(&system_monitor_view.list, -1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Audio => {
                crate::ui::move_selection(&audio_view.streams_list, -1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Media => {
                crate::ui::set_media_snapshot(media_view, &crate::media_snapshot());
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Network => {
                crate::ui::move_selection(network_list, -1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Notifications => {
                crate::ui::set_notification_snapshot(
                    notifications_view,
                    &crate::notification_snapshot(),
                );
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Snippets => {
                crate::ui::move_selection(snippet_list, -1);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        },
        gdk::Key::Delete if navigation.current() == crate::ui::LauncherView::Clipboard => {
            if state.contains(gdk::ModifierType::CONTROL_MASK) {
                let launcher_for_done = Rc::clone(launcher);
                let clipboard_view = clipboard_view.clone();
                let clipboard_items = Rc::clone(clipboard_items);
                clear_clipboard_history_or_confirm(window, launcher, move || {
                    refresh_clipboard_view(&launcher_for_done, &clipboard_view, &clipboard_items);
                });
            } else if let Some(row) = clipboard_view.list.selected_row()
                && let Some(item) = clipboard_items.borrow().get(row.index() as usize)
            {
                // The Delete hotkey must pass the same destructive-action
                // confirmation gate as the action panel entry.
                let action = clipboard_item_action(&item.value);
                let launcher_for_done = Rc::clone(launcher);
                let clipboard_view = clipboard_view.clone();
                let clipboard_items = Rc::clone(clipboard_items);
                run_secondary_action_or_confirm(
                    window,
                    launcher,
                    action,
                    SecondaryActionKind::DeleteClipboardItem,
                    move || {
                        refresh_clipboard_view(
                            &launcher_for_done,
                            &clipboard_view,
                            &clipboard_items,
                        );
                    },
                );
            }
            if !state.contains(gdk::ModifierType::CONTROL_MASK) {
                refresh_clipboard_view(launcher, clipboard_view, clipboard_items);
            }
            glib::Propagation::Stop
        }
        gdk::Key::Delete if navigation.current() == crate::ui::LauncherView::Snippets => {
            if let Some(row) = snippet_list.selected_row()
                && let Some(item) = snippet_items.borrow().get(row.index() as usize)
                && let Err(error) = launcher
                    .borrow_mut()
                    .delete_snippet(&item.name, &item.value)
            {
                eprintln!("failed to delete snippet: {error}");
            }
            refresh_snippet_view(launcher, snippet_list, snippet_items);
            glib::Propagation::Stop
        }
        gdk::Key::e | gdk::Key::E
            if navigation.current() == crate::ui::LauncherView::Snippets
                && state.contains(gdk::ModifierType::CONTROL_MASK) =>
        {
            if let Some(row) = snippet_list.selected_row()
                && let Some(item) = snippet_items.borrow().get(row.index() as usize)
            {
                let launcher_for_done = Rc::clone(launcher);
                let snippet_list = snippet_list.clone();
                let snippet_items = Rc::clone(snippet_items);
                crate::ui::show_snippet_editor_panel(
                    window,
                    launcher,
                    Some(item.id),
                    &item.name,
                    &item.prefix,
                    &item.value,
                    move || {
                        refresh_snippet_view(
                            &launcher_for_done,
                            &snippet_list,
                            &snippet_items,
                        );
                    },
                );
            }
            glib::Propagation::Stop
        }
        gdk::Key::Delete if navigation.current() == crate::ui::LauncherView::SystemMonitor => {
            let system_monitor_view = system_monitor_view.clone();
            terminate_selected_system_process_or_confirm(
                window,
                &system_monitor_view.clone(),
                move || {
                    crate::ui::set_system_monitor_snapshot(
                        &system_monitor_view,
                        &crate::system_snapshot(),
                        &crate::top_processes_by_memory(8),
                    );
                },
            );
            glib::Propagation::Stop
        }
        _ => glib::Propagation::Proceed,
    }
}

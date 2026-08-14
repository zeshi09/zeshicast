use std::cell::RefCell;
use std::rc::Rc;
use crate::ui::launcher_views::*;
use crate::{
    Action, ClipboardSummary,
    SnippetSummary, Zeshicast,
};
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{ApplicationWindow, Box as GtkBox, Entry, ListBox};
use super::actions::*;
use super::clipboard::*;


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
    system_monitor_view: &crate::ui::SystemMonitorView,
    media_view: &crate::ui::MediaView,
    network_list: &ListBox,
    notifications_view: &crate::ui::NotificationsView,
    window_grid_view: &crate::ui::WindowGridView,
    current_action: &Rc<RefCell<Option<Action>>>,
    action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    filtered_action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    clipboard_view: &crate::ui::ClipboardHistoryView,
    clipboard_items: &Rc<RefCell<Vec<ClipboardSummary>>>,
    extension_list: &ListBox,
    snippet_list: &ListBox,
    snippet_items: &Rc<RefCell<Vec<SnippetSummary>>>,
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
            window_grid_view,
            current_action,
            filtered_action_panel_items,
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
                    system_monitor_view,
                    media_view,
                    network_list,
                    notifications_view,
                    window_grid_view,
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

pub(crate) fn handle_view_key(
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
    window_grid_view: &crate::ui::WindowGridView,
    current_action: &Rc<RefCell<Option<Action>>>,
    filtered_action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    clipboard_view: &crate::ui::ClipboardHistoryView,
    clipboard_items: &Rc<RefCell<Vec<ClipboardSummary>>>,
    extension_list: &ListBox,
    snippet_list: &ListBox,
    snippet_items: &Rc<RefCell<Vec<SnippetSummary>>>,
    key: gdk::Key,
    state: gdk::ModifierType,
) -> glib::Propagation {
    if navigation.current() == crate::ui::LauncherView::WindowGrid {
        match key {
            gdk::Key::Escape | gdk::Key::Return | gdk::Key::KP_Enter => {
                show_root_view(navigation, entry, action_bar);
                return glib::Propagation::Stop;
            }
            gdk::Key::h | gdk::Key::Left => {
                *window_grid_view.selected_target.borrow_mut() =
                    Some(crate::ui::GridSnapTarget::LeftHalf);
                window_grid_view.preview.queue_draw();
                window_grid_view.status_label.set_text("Applied: Left Half");
                crate::ui::execute_grid_snap(crate::ui::GridSnapTarget::LeftHalf);
                return glib::Propagation::Stop;
            }
            gdk::Key::l | gdk::Key::Right => {
                *window_grid_view.selected_target.borrow_mut() =
                    Some(crate::ui::GridSnapTarget::RightHalf);
                window_grid_view.preview.queue_draw();
                window_grid_view.status_label.set_text("Applied: Right Half");
                crate::ui::execute_grid_snap(crate::ui::GridSnapTarget::RightHalf);
                return glib::Propagation::Stop;
            }
            gdk::Key::k | gdk::Key::Up => {
                *window_grid_view.selected_target.borrow_mut() =
                    Some(crate::ui::GridSnapTarget::TopHalf);
                window_grid_view.preview.queue_draw();
                window_grid_view.status_label.set_text("Applied: Top Half");
                crate::ui::execute_grid_snap(crate::ui::GridSnapTarget::TopHalf);
                return glib::Propagation::Stop;
            }
            gdk::Key::j | gdk::Key::Down => {
                *window_grid_view.selected_target.borrow_mut() =
                    Some(crate::ui::GridSnapTarget::BottomHalf);
                window_grid_view.preview.queue_draw();
                window_grid_view.status_label.set_text("Applied: Bottom Half");
                crate::ui::execute_grid_snap(crate::ui::GridSnapTarget::BottomHalf);
                return glib::Propagation::Stop;
            }
            gdk::Key::m | gdk::Key::f => {
                *window_grid_view.selected_target.borrow_mut() =
                    Some(crate::ui::GridSnapTarget::Fullscreen);
                window_grid_view.preview.queue_draw();
                window_grid_view.status_label.set_text("Applied: Fullscreen");
                crate::ui::execute_grid_snap(crate::ui::GridSnapTarget::Fullscreen);
                return glib::Propagation::Stop;
            }
            gdk::Key::c => {
                *window_grid_view.selected_target.borrow_mut() =
                    Some(crate::ui::GridSnapTarget::Center);
                window_grid_view.preview.queue_draw();
                window_grid_view.status_label.set_text("Applied: Center 70%");
                crate::ui::execute_grid_snap(crate::ui::GridSnapTarget::Center);
                return glib::Propagation::Stop;
            }
            gdk::Key::plus | gdk::Key::equal => {
                window_grid_view.status_label.set_text("Expanded Width +10%");
                crate::ui::execute_column_resize(10);
                return glib::Propagation::Stop;
            }
            gdk::Key::minus => {
                window_grid_view.status_label.set_text("Shrunk Width -10%");
                crate::ui::execute_column_resize(-10);
                return glib::Propagation::Stop;
            }
            _ => {}
        }
    }

    match key {
        gdk::Key::Escape => {
            if navigation.pop().is_some() {
                entry.set_visible(true);
                action_bar.set_visible(true);
                entry.grab_focus();
                glib::Propagation::Stop
            } else {
                finish_interaction(window, &Rc::new(RefCell::new(None)));
                glib::Propagation::Stop
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
                        filtered_action_panel_items,
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
            crate::ui::LauncherView::WindowGrid => {
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
                if let Err(error) = launcher.borrow_mut().clear_clipboard_history() {
                    eprintln!("failed to clear clipboard history: {error}");
                }
            } else if let Some(row) = clipboard_view.list.selected_row() {
                if let Some(item) = clipboard_items.borrow().get(row.index() as usize) {
                    if let Err(error) = launcher.borrow_mut().delete_clipboard_value(&item.value) {
                        eprintln!("failed to delete clipboard item: {error}");
                    }
                }
            }
            refresh_clipboard_view(launcher, clipboard_view, clipboard_items);
            glib::Propagation::Stop
        }
        gdk::Key::Delete if navigation.current() == crate::ui::LauncherView::Snippets => {
            if let Some(row) = snippet_list.selected_row() {
                if let Some(item) = snippet_items.borrow().get(row.index() as usize) {
                    if let Err(error) = launcher
                        .borrow_mut()
                        .delete_snippet(&item.name, &item.value)
                    {
                        eprintln!("failed to delete snippet: {error}");
                    }
                }
            }
            refresh_snippet_view(launcher, snippet_list, snippet_items);
            glib::Propagation::Stop
        }
        gdk::Key::Delete if navigation.current() == crate::ui::LauncherView::SystemMonitor => {
            terminate_selected_system_process(system_monitor_view);
            crate::ui::set_system_monitor_snapshot(
                system_monitor_view,
                &crate::system_snapshot(),
                &crate::top_processes_by_memory(8),
            );
            glib::Propagation::Stop
        }
        _ => glib::Propagation::Proceed,
    }
}
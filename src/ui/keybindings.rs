#![allow(clippy::too_many_arguments)]

use super::action_panel_controller::{
    ActionPanelItem, DisplayedActionPanelRow, run_action_panel_row, show_action_panel_view,
};
use super::launcher::{
    clear_clipboard_history_or_confirm, clipboard_item_action, copy_clipboard_row, copy_selected,
    copy_snippet_row, finish_interaction, refresh_clipboard_view, refresh_snippet_view,
    run_secondary_action_or_confirm, run_secondary_for_selected, run_selected_with_views,
    selected_action, show_clipboard_view, show_extension_view, show_preferences_view,
    show_root_view, show_snippet_view, terminate_selected_system_process_or_confirm,
    update_results,
};
use crate::action::{Action, SecondaryActionKind};
use crate::app::{SnippetSummary, Zeshicast};
use crate::services::clipboard_store::ClipboardSummary;
use crate::ui::launcher_views::{
    show_ai_chat_view, show_audio_view, show_dashboard_view, show_emoji_view,
    show_font_browser_view, show_media_view, show_network_view, show_notifications_view,
    show_system_monitor_view,
};
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{ApplicationWindow, Box as GtkBox, Entry, ListBox};
use std::cell::RefCell;
use std::rc::Rc;

/// Resolve a hardware keycode to its keyval in the primary (Latin) layout group,
/// independent of the currently active keyboard layout. Lets Ctrl-shortcuts
/// match on e.g. a Cyrillic layout where the produced keyval would be Cyrillic.
pub(crate) fn latin_keyval(keycode: u32) -> Option<gdk::Key> {
    gdk::Display::default()
        .and_then(|display| display.translate_key(keycode, gdk::ModifierType::empty(), 0))
        .map(|(keyval, _, _, _)| keyval)
}

/// Everything a key press can act on (P5.2).
///
/// These used to be 32 positional arguments on `handle_key`, re-listed in a
/// different order for `handle_view_key`. A field per widget keeps that
/// signature stable when a view is added, and the two functions take what they
/// need with `let LauncherUi { .. } = *ui;`. `run_selected_with_views` keeps its
/// own list: the "run" button lives inside the action bar, so its handler cannot
/// construct a context that contains the bar it is building.
#[derive(Clone, Copy)]
pub(crate) struct LauncherUi<'a> {
    pub(crate) window: &'a ApplicationWindow,
    pub(crate) launcher: &'a Rc<RefCell<Zeshicast>>,
    pub(crate) hold: &'a Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    pub(crate) entry: &'a Entry,
    pub(crate) list: &'a ListBox,
    pub(crate) results: &'a Rc<RefCell<Vec<Action>>>,
    pub(crate) action_bar: &'a GtkBox,
    pub(crate) navigation: &'a crate::ui::NavigationStack,
    pub(crate) action_panel_view: &'a crate::ui::ActionPanelView,
    pub(crate) ai_chat_view: &'a crate::ui::AiChatView,
    pub(crate) audio_view: &'a crate::ui::AudioView,
    pub(crate) dashboard_view: &'a crate::ui::DashboardView,
    pub(crate) emoji_view: &'a crate::ui::EmojiPickerView,
    pub(crate) font_view: &'a crate::ui::FontBrowserView,
    pub(crate) system_monitor_view: &'a crate::ui::SystemMonitorView,
    pub(crate) media_view: &'a crate::ui::MediaView,
    pub(crate) network_list: &'a ListBox,
    pub(crate) notifications_view: &'a crate::ui::NotificationsView,
    pub(crate) window_grid_view: &'a crate::ui::WindowGridView,
    pub(crate) current_action: &'a Rc<RefCell<Option<Action>>>,
    pub(crate) action_panel_items: &'a Rc<RefCell<Vec<ActionPanelItem>>>,
    pub(crate) filtered_action_panel_items: &'a Rc<RefCell<Vec<ActionPanelItem>>>,
    pub(crate) displayed_action_panel_rows: &'a Rc<RefCell<Vec<DisplayedActionPanelRow>>>,
    pub(crate) clipboard_view: &'a crate::ui::ClipboardHistoryView,
    pub(crate) clipboard_items: &'a Rc<RefCell<Vec<ClipboardSummary>>>,
    pub(crate) extension_list: &'a ListBox,
    pub(crate) snippet_list: &'a ListBox,
    pub(crate) snippet_items: &'a Rc<RefCell<Vec<SnippetSummary>>>,
    pub(crate) script_output_view: &'a crate::ui::ScriptOutputView,
}

pub(crate) fn handle_key(
    ui: &LauncherUi<'_>,
    key: gdk::Key,
    state: gdk::ModifierType,
) -> glib::Propagation {
    let LauncherUi {
        window,
        launcher,
        hold,
        entry,
        list,
        results,
        action_bar,
        navigation,
        action_panel_view,
        ai_chat_view,
        audio_view,
        dashboard_view,
        emoji_view,
        font_view,
        system_monitor_view,
        media_view,
        network_list,
        notifications_view,
        window_grid_view,
        current_action,
        action_panel_items,
        filtered_action_panel_items,
        displayed_action_panel_rows,
        clipboard_view,
        clipboard_items,
        extension_list,
        snippet_list,
        snippet_items,
        script_output_view,
    } = *ui;

    if navigation.current() != crate::ui::LauncherView::Root {
        return handle_view_key(ui, key, state);
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
        gdk::Key::f | gdk::Key::F if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            if state.contains(gdk::ModifierType::SHIFT_MASK) {
                run_secondary_for_selected(
                    launcher,
                    list,
                    results,
                    SecondaryActionKind::OpenParent,
                );
            } else {
                show_font_browser_view(navigation, entry, action_bar, font_view);
            }
            glib::Propagation::Stop
        }
        gdk::Key::p | gdk::Key::P if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            if let Some(action) = selected_action(list, results) {
                let is_pinned = launcher.borrow().is_pinned(&action);
                let kind = if is_pinned {
                    SecondaryActionKind::Unpin
                } else {
                    SecondaryActionKind::Pin
                };
                if let Err(error) = launcher.borrow_mut().run_secondary_action(&action, kind) {
                    eprintln!("failed to update pin: {error}");
                } else {
                    crate::ui::show_toast_osd(
                        None,
                        if is_pinned {
                            "✓ Unpinned"
                        } else {
                            "✓ Pinned"
                        },
                    );
                }
                update_results(
                    &launcher.borrow(),
                    results,
                    list,
                    entry.text().as_str(),
                    None,
                );
            }
            glib::Propagation::Stop
        }
        gdk::Key::comma if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_preferences_view(navigation, entry, action_bar);
            glib::Propagation::Stop
        }
        gdk::Key::Tab | gdk::Key::ISO_Left_Tab => {
            let is_backward =
                key == gdk::Key::ISO_Left_Tab || state.contains(gdk::ModifierType::SHIFT_MASK);
            cycle_root_focus(window, entry, list, action_bar, is_backward);
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
    ui: &LauncherUi<'_>,
    key: gdk::Key,
    state: gdk::ModifierType,
) -> glib::Propagation {
    let LauncherUi {
        window,
        launcher,
        list,
        results,
        navigation,
        entry,
        action_bar,
        action_panel_view,
        ai_chat_view,
        audio_view,
        dashboard_view,
        system_monitor_view,
        media_view,
        network_list,
        notifications_view,
        window_grid_view,
        current_action,
        displayed_action_panel_rows,
        clipboard_view,
        clipboard_items,
        extension_list,
        snippet_list,
        snippet_items,
        ..
    } = *ui;
    let action_panel_list = &action_panel_view.list;

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
                crate::ui::activate_focused_dashboard_card(dashboard_view);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::SystemMonitor => {
                let window = window.clone();
                let system_monitor_view = system_monitor_view.clone();
                let view_for_cb = system_monitor_view.clone();
                terminate_selected_system_process_or_confirm(
                    &window,
                    &system_monitor_view,
                    move || {
                        crate::ui::set_system_monitor_snapshot(
                            &view_for_cb,
                            &crate::services::poll_cache::cached_system_snapshot(),
                            &crate::services::poll_cache::cached_top_processes(),
                        );
                    },
                );
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
                crate::ui::activate_selected_audio_device(audio_view);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Media => {
                media_view.play_pause.emit_clicked();
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Network => {
                crate::ui::launcher::activate_selected_network_row(network_list);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Notifications => {
                if let Some(row) = notifications_view.history.selected_row() {
                    crate::ui::dismiss_notification_row(&row);
                }
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Snippets => {
                if let Some(row) = snippet_list.selected_row() {
                    copy_snippet_row(row.index() as usize, snippet_items);
                }
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::WindowGrid => {
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        },
        gdk::Key::space if navigation.current() == crate::ui::LauncherView::Media => {
            media_view.play_pause.emit_clicked();
            glib::Propagation::Stop
        }
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
                crate::ui::cycle_dashboard_card(dashboard_view, 1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::SystemMonitor => {
                crate::ui::move_selection(&system_monitor_view.list, 1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Audio => {
                crate::ui::move_selection(&audio_view.output_devices, 1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Media => {
                crate::ui::set_media_snapshot(
                    media_view,
                    &crate::services::poll_cache::cached_media_snapshot(),
                );
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Network => {
                crate::ui::move_selection(network_list, 1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Notifications => {
                crate::ui::move_selection(&notifications_view.history, 1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Snippets => {
                crate::ui::move_selection(snippet_list, 1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::WindowGrid => {
                use crate::services::compositor::{WindowSnapPosition, snap_window};
                let target_pos = WindowSnapPosition::BottomHalf;
                *window_grid_view.current_position.borrow_mut() = target_pos;
                window_grid_view.drawing_area.queue_draw();
                snap_window(target_pos);
                window_grid_view
                    .status_label
                    .set_text(&format!("Applied: {:?}", target_pos));
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
                crate::ui::cycle_dashboard_card(dashboard_view, -1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::SystemMonitor => {
                crate::ui::move_selection(&system_monitor_view.list, -1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Audio => {
                crate::ui::move_selection(&audio_view.output_devices, -1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Media => {
                crate::ui::set_media_snapshot(
                    media_view,
                    &crate::services::poll_cache::cached_media_snapshot(),
                );
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Network => {
                crate::ui::move_selection(network_list, -1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Notifications => {
                crate::ui::move_selection(&notifications_view.history, -1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Snippets => {
                crate::ui::move_selection(snippet_list, -1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::WindowGrid => {
                use crate::services::compositor::{WindowSnapPosition, snap_window};
                let target_pos = WindowSnapPosition::TopHalf;
                *window_grid_view.current_position.borrow_mut() = target_pos;
                window_grid_view.drawing_area.queue_draw();
                snap_window(target_pos);
                window_grid_view
                    .status_label
                    .set_text(&format!("Applied: {:?}", target_pos));
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
            {
                let launcher_for_del = Rc::clone(launcher);
                let snippet_list = snippet_list.clone();
                let snippet_items = Rc::clone(snippet_items);
                let name = item.name.clone();
                let value = item.value.clone();
                crate::ui::show_confirmation_panel(
                    window,
                    &format!("Delete snippet '{name}'?"),
                    "This action cannot be undone.",
                    "Delete",
                    move || {
                        if let Err(error) =
                            launcher_for_del.borrow_mut().delete_snippet(&name, &value)
                        {
                            eprintln!("failed to delete snippet: {error}");
                        }
                        refresh_snippet_view(&launcher_for_del, &snippet_list, &snippet_items);
                    },
                );
            }
            glib::Propagation::Stop
        }
        gdk::Key::Delete if navigation.current() == crate::ui::LauncherView::Notifications => {
            if let Some(row) = notifications_view.history.selected_row() {
                crate::ui::dismiss_notification_row(&row);
            }
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
                        refresh_snippet_view(&launcher_for_done, &snippet_list, &snippet_items);
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
                        &crate::services::poll_cache::cached_system_snapshot(),
                        &crate::services::poll_cache::cached_top_processes(),
                    );
                },
            );
            glib::Propagation::Stop
        }
        _ => {
            if navigation.current() == crate::ui::LauncherView::WindowGrid {
                use crate::services::compositor::{WindowSnapPosition, snap_window};
                let pos = match key {
                    gdk::Key::h | gdk::Key::H | gdk::Key::Left => {
                        Some(WindowSnapPosition::LeftHalf)
                    }
                    gdk::Key::l | gdk::Key::L | gdk::Key::Right => {
                        Some(WindowSnapPosition::RightHalf)
                    }
                    gdk::Key::k | gdk::Key::K | gdk::Key::Up => Some(WindowSnapPosition::TopHalf),
                    gdk::Key::j | gdk::Key::J | gdk::Key::Down => {
                        Some(WindowSnapPosition::BottomHalf)
                    }
                    gdk::Key::f | gdk::Key::F => Some(WindowSnapPosition::Fullscreen),
                    gdk::Key::c | gdk::Key::C => Some(WindowSnapPosition::Center),
                    gdk::Key::_1 => Some(WindowSnapPosition::FirstThird),
                    gdk::Key::_2 => Some(WindowSnapPosition::CenterThird),
                    gdk::Key::_3 => Some(WindowSnapPosition::RightThird),
                    gdk::Key::_4 => Some(WindowSnapPosition::LeftTwoThirds),
                    gdk::Key::_5 => Some(WindowSnapPosition::RightTwoThirds),
                    gdk::Key::u | gdk::Key::U => Some(WindowSnapPosition::TopLeftQuarter),
                    gdk::Key::i | gdk::Key::I => Some(WindowSnapPosition::TopRightQuarter),
                    gdk::Key::n | gdk::Key::N => Some(WindowSnapPosition::BottomLeftQuarter),
                    gdk::Key::m | gdk::Key::M => Some(WindowSnapPosition::BottomRightQuarter),
                    _ => None,
                };
                if let Some(target_pos) = pos {
                    *window_grid_view.current_position.borrow_mut() = target_pos;
                    window_grid_view.drawing_area.queue_draw();
                    snap_window(target_pos);
                    window_grid_view
                        .status_label
                        .set_text(&format!("Applied: {:?}", target_pos));
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        }
    }
}

fn cycle_root_focus(
    window: &ApplicationWindow,
    entry: &Entry,
    list: &ListBox,
    action_bar: &GtkBox,
    backward: bool,
) {
    let focused = gtk::prelude::RootExt::focus(window);
    let is_entry = entry.has_focus() || focused.as_ref().is_some_and(|w| w == entry);
    let is_list = list.has_focus()
        || focused.as_ref().is_some_and(|w| {
            w == list || w.ancestor(ListBox::static_type()).as_ref() == Some(list.upcast_ref())
        });
    let is_action_bar = action_bar.has_focus()
        || focused.as_ref().is_some_and(|w| {
            w == action_bar
                || w.ancestor(GtkBox::static_type()).as_ref() == Some(action_bar.upcast_ref())
        });

    if !backward {
        // Forward: Entry -> List (selected row) -> Action Bar buttons -> Entry
        if is_entry {
            if let Some(row) = list.selected_row().or_else(|| list.row_at_index(0)) {
                list.select_row(Some(&row));
                row.grab_focus();
            } else if let Some(first_btn) = find_first_focusable(action_bar) {
                first_btn.grab_focus();
            } else {
                entry.grab_focus();
            }
        } else if is_list {
            if let Some(first_btn) = find_first_focusable(action_bar) {
                first_btn.grab_focus();
            } else {
                entry.grab_focus();
            }
        } else if is_action_bar {
            if let Some(ref current) = focused {
                let direct_child = direct_child_of(current, action_bar);
                let next = direct_child.as_ref().and_then(find_next_focusable_sibling);
                if let Some(next_btn) = next {
                    next_btn.grab_focus();
                } else {
                    entry.grab_focus();
                }
            } else {
                entry.grab_focus();
            }
        } else {
            entry.grab_focus();
        }
    } else {
        // Backward: Entry -> Action Bar (last button) -> List (selected row) -> Entry
        if is_entry {
            if let Some(last_btn) = find_last_focusable(action_bar) {
                last_btn.grab_focus();
            } else if let Some(row) = list.selected_row().or_else(|| list.row_at_index(0)) {
                list.select_row(Some(&row));
                row.grab_focus();
            } else {
                entry.grab_focus();
            }
        } else if is_action_bar {
            if let Some(ref current) = focused {
                let direct_child = direct_child_of(current, action_bar);
                let prev = direct_child.as_ref().and_then(find_prev_focusable_sibling);
                if let Some(prev_btn) = prev {
                    prev_btn.grab_focus();
                } else if let Some(row) = list.selected_row().or_else(|| list.row_at_index(0)) {
                    list.select_row(Some(&row));
                    row.grab_focus();
                } else {
                    entry.grab_focus();
                }
            } else {
                entry.grab_focus();
            }
        } else {
            entry.grab_focus();
        }
    }
}

fn direct_child_of(widget: &gtk::Widget, container: &GtkBox) -> Option<gtk::Widget> {
    let mut curr = widget.clone();
    loop {
        let parent = curr.parent()?;
        if parent == *container {
            return Some(curr);
        }
        curr = parent;
    }
}

fn find_first_focusable(container: &GtkBox) -> Option<gtk::Widget> {
    let mut child = container.first_child();
    while let Some(c) = child {
        if c.is_focusable() && c.is_visible() {
            return Some(c);
        }
        child = c.next_sibling();
    }
    None
}

fn find_last_focusable(container: &GtkBox) -> Option<gtk::Widget> {
    let mut child = container.last_child();
    while let Some(c) = child {
        if c.is_focusable() && c.is_visible() {
            return Some(c);
        }
        child = c.prev_sibling();
    }
    None
}

fn find_next_focusable_sibling(widget: &gtk::Widget) -> Option<gtk::Widget> {
    let mut sibling = widget.next_sibling();
    while let Some(s) = sibling {
        if s.is_focusable() && s.is_visible() {
            return Some(s);
        }
        sibling = s.next_sibling();
    }
    None
}

fn find_prev_focusable_sibling(widget: &gtk::Widget) -> Option<gtk::Widget> {
    let mut sibling = widget.prev_sibling();
    while let Some(s) = sibling {
        if s.is_focusable() && s.is_visible() {
            return Some(s);
        }
        sibling = s.prev_sibling();
    }
    None
}

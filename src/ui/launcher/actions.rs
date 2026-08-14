use std::cell::RefCell;
use std::rc::Rc;
use crate::ui::launcher_views::*;
use crate::{
    Action, ActionKind, ActionPanelSection, SecondaryActionKind,
    SnippetSummary, Zeshicast, ui::ActionPanelDisplayItem,
};
use gtk::gio;
use gtk::prelude::*;
use gtk::{ApplicationWindow, Box as GtkBox, Entry, ListBox};
use super::root_list::*;

#[derive(Clone)]
pub(crate) struct ActionPanelItem {
    pub(crate) display: ActionPanelDisplayItem,
    pub(crate) section: ActionPanelSection,
    pub(crate) kind: ActionPanelItemKind,
}

#[derive(Clone, Copy)]
pub(crate) enum ActionPanelItemKind {
    Secondary(SecondaryActionKind),
    SetAlias,
}

#[derive(Clone, Copy)]
pub(crate) enum NetworkCopyValue {
    Ip,
    Mac,
}

#[derive(Clone, Copy)]
pub(crate) enum NetworkCommandValue {
    ConnectWifi,
    DisconnectInterface,
}


pub(crate) fn show_action_panel_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    action_panel_view: &crate::ui::ActionPanelView,
    current_action: &Rc<RefCell<Option<Action>>>,
    action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    filtered_action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    launcher: &Rc<RefCell<Zeshicast>>,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
) {
    let Some(action) = selected_action(list, results) else {
        return;
    };

    let mut items = launcher
        .borrow()
        .available_secondary_actions(&action)
        .into_iter()
        .map(|secondary| ActionPanelItem {
            display: ActionPanelDisplayItem {
                title: secondary.title,
                icon_name: secondary.icon_name,
                is_section_header: false,
                is_destructive: secondary.section.is_danger(),
            },
            section: secondary.section,
            kind: ActionPanelItemKind::Secondary(secondary.kind),
        })
        .collect::<Vec<_>>();
    items.push(ActionPanelItem {
        display: ActionPanelDisplayItem {
            title: "Set Alias".to_string(),
            icon_name: "insert-link-symbolic".to_string(),
            is_section_header: false,
            is_destructive: false,
        },
        section: ActionPanelSection::Manage,
        kind: ActionPanelItemKind::SetAlias,
    });

    *current_action.borrow_mut() = Some(action.clone());
    *action_panel_items.borrow_mut() = items.clone();
    *filtered_action_panel_items.borrow_mut() = items;
    action_panel_view.search.set_text("");
    let displays = action_panel_displays(&filtered_action_panel_items.borrow());
    crate::ui::set_action_panel_items(action_panel_view, &action, &displays);

    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::Actions);
    action_panel_view.search.grab_focus();
}

pub(crate) fn filter_action_panel_items(
    query: &str,
    action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    filtered_action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    action_panel_list: &ListBox,
) {
    let query = query.trim().to_lowercase();
    let filtered = action_panel_items
        .borrow()
        .iter()
        .filter(|item| query.is_empty() || item.display.title.to_lowercase().contains(&query))
        .cloned()
        .collect::<Vec<_>>();
    let displays = action_panel_displays(&filtered);
    *filtered_action_panel_items.borrow_mut() = filtered;
    crate::ui::set_action_panel_list(action_panel_list, &displays);
}

pub(crate) fn action_panel_displays(items: &[ActionPanelItem]) -> Vec<ActionPanelDisplayItem> {
    const SECTION_ORDER: &[ActionPanelSection] = &[
        ActionPanelSection::Primary,
        ActionPanelSection::Manage,
        ActionPanelSection::Clipboard,
        ActionPanelSection::Danger,
    ];

    let mut result = Vec::new();
    for &section in SECTION_ORDER {
        let section_items: Vec<&ActionPanelItem> = items
            .iter()
            .filter(|item| item.section == section)
            .collect();
        if section_items.is_empty() {
            continue;
        }
        result.push(ActionPanelDisplayItem {
            title: section.title().to_string(),
            icon_name: String::new(),
            is_section_header: true,
            is_destructive: false,
        });
        for item in section_items {
            result.push(item.display.clone());
        }
    }
    result
}

pub(crate) fn run_action_panel_row(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    entry: &Entry,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    navigation: &crate::ui::NavigationStack,
    action_bar: &GtkBox,
    current_action: &Rc<RefCell<Option<Action>>>,
    filtered_action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    index: usize,
) {
    let Some(action) = current_action.borrow().clone() else {
        return;
    };
    let Some(item) = filtered_action_panel_items.borrow().get(index).cloned() else {
        return;
    };

    match item.kind {
        ActionPanelItemKind::Secondary(kind) => {
            if let Err(error) = launcher.borrow_mut().run_secondary_action(&action, kind) {
                eprintln!("failed to run action: {error}");
            }
            update_results(&launcher.borrow(), results, list, entry.text().as_str());
        }
        ActionPanelItemKind::SetAlias => {
            crate::ui::show_alias_panel(window, launcher, &action);
        }
    }
    show_root_view(navigation, entry, action_bar);
}

pub(crate) fn terminate_selected_system_process(system_monitor_view: &crate::ui::SystemMonitorView) {
    let Some(row) = system_monitor_view.list.selected_row() else {
        return;
    };
    let Some(process) = crate::top_processes_by_memory(8)
        .get(row.index() as usize)
        .cloned()
    else {
        return;
    };

    crate::spawn_command("kill", &[&process.pid.to_string()]);
}

pub(crate) fn copy_selected_network_value(list: &ListBox, value: NetworkCopyValue) {
    let Some(row) = list.selected_row() else {
        return;
    };
    let Some(interface) = crate::network_snapshot()
        .interfaces
        .get(row.index() as usize)
        .cloned()
    else {
        return;
    };

    let value = match value {
        NetworkCopyValue::Ip => interface
            .ipv4_addresses
            .first()
            .or_else(|| interface.ipv6_addresses.first())
            .cloned(),
        NetworkCopyValue::Mac => interface.mac_address,
    };

    if let Some(value) = value {
        crate::copy_text(&value);
    }
}

pub(crate) fn run_selected_network_command(list: &ListBox, value: NetworkCommandValue) {
    let Some(row) = list.selected_row() else {
        return;
    };
    let snapshot = crate::network_snapshot();
    let index = row.index() as usize;

    match value {
        NetworkCommandValue::DisconnectInterface => {
            let Some(interface) = snapshot.interfaces.get(index) else {
                return;
            };
            crate::spawn_command("nmcli", &["device", "disconnect", interface.name.as_str()]);
        }
        NetworkCommandValue::ConnectWifi => {
            let wifi_offset = snapshot.interfaces.len()
                + usize::from(!snapshot.dns_servers.is_empty())
                + usize::from(!snapshot.wifi_networks.is_empty());
            let Some(network) = index
                .checked_sub(wifi_offset)
                .and_then(|index| snapshot.wifi_networks.get(index))
            else {
                return;
            };
            crate::spawn_command("nmcli", &["dev", "wifi", "connect", network.ssid.as_str()]);
        }
    }
}

pub(crate) fn show_snippet_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    snippet_list: &ListBox,
    snippet_items: &Rc<RefCell<Vec<SnippetSummary>>>,
    launcher: &Rc<RefCell<Zeshicast>>,
) {
    refresh_snippet_view(launcher, snippet_list, snippet_items);
    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::Snippets);
    if let Some(row) = snippet_list.row_at_index(0) {
        snippet_list.select_row(Some(&row));
    }
    snippet_list.grab_focus();
}

pub(crate) fn refresh_snippet_view(
    launcher: &Rc<RefCell<Zeshicast>>,
    snippet_list: &ListBox,
    snippet_items: &Rc<RefCell<Vec<SnippetSummary>>>,
) {
    let items = launcher.borrow().list_snippets();
    crate::ui::set_snippet_items(snippet_list, &items);
    *snippet_items.borrow_mut() = items;
}

pub(crate) fn copy_snippet_row(index: usize, snippet_items: &Rc<RefCell<Vec<SnippetSummary>>>) {
    if let Some(item) = snippet_items.borrow().get(index) {
        crate::copy_text(&item.value);
    }
}

pub(crate) fn show_preferences_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
) {
    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::Preferences);
}

pub(crate) fn show_extension_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    extension_list: &ListBox,
) {
    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::Extensions);
    if let Some(row) = extension_list.row_at_index(0) {
        extension_list.select_row(Some(&row));
    }
    extension_list.grab_focus();
}

pub(crate) fn show_root_view(navigation: &crate::ui::NavigationStack, entry: &Entry, action_bar: &GtkBox) {
    navigation.reset();
    entry.set_visible(true);
    action_bar.set_visible(true);
    entry.grab_focus();
}

pub(crate) fn show_form_for_action(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    entry: &Entry,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    action: Action,
) {
    let parent_window = window.clone();
    let finish_window = window.clone();
    let launcher = Rc::clone(launcher);
    let hold = Rc::clone(hold);
    let entry = entry.clone();
    let list = list.clone();
    let results = Rc::clone(results);

    crate::ui::show_form_panel(&parent_window, action, move |action, values| {
        launcher.borrow_mut().run_form_action(&action, values);
        update_results(&launcher.borrow(), &results, &list, entry.text().as_str());
        finish_interaction(&finish_window, &hold);
    });
}

pub(crate) fn run_selected_with_views(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    entry: &Entry,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    navigation: &crate::ui::NavigationStack,
    action_bar: &GtkBox,
    ai_chat_view: &crate::ui::AiChatView,
    audio_view: &crate::ui::AudioView,
    dashboard_view: &crate::ui::DashboardView,
    system_monitor_view: &crate::ui::SystemMonitorView,
    media_view: &crate::ui::MediaView,
    network_list: &ListBox,
    notifications_view: &crate::ui::NotificationsView,
    window_grid_view: &crate::ui::WindowGridView,
) {
    if let Some(action) = selected_action(list, results) {
        if let Some(command) = action.launcher_command() {
            run_launcher_command(
                command,
                navigation,
                entry,
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
        } else if action.form_data().is_some() {
            show_form_for_action(window, launcher, hold, entry, list, results, action);
        } else {
            launcher.borrow_mut().run_action(&action);
            finish_interaction(window, hold);
        }
    }
}

pub(crate) fn finish_interaction(
    window: &ApplicationWindow,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
) {
    if hold.borrow().is_some() {
        window.hide();
    } else {
        window.close();
    }
}

pub(crate) fn copy_selected(list: &ListBox, results: &Rc<RefCell<Vec<Action>>>) {
    if let Some(action) = selected_action(list, results) {
        action.copy_value();
    }
}

pub(crate) fn run_secondary_for_selected(
    launcher: &Rc<RefCell<Zeshicast>>,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    kind: SecondaryActionKind,
) {
    if let Some(action) = selected_action(list, results) {
        let available = launcher
            .borrow()
            .available_secondary_actions(&action)
            .into_iter()
            .any(|secondary| secondary.kind == kind);
        if !available {
            return;
        }

        if let Err(error) = launcher.borrow_mut().run_secondary_action(&action, kind) {
            eprintln!("failed to run secondary action: {error}");
        }
    }
}

pub(crate) fn selected_action(list: &ListBox, results: &Rc<RefCell<Vec<Action>>>) -> Option<Action> {
    let row = list.selected_row()?;
    let index = action_index_for_row(list, &row)?;
    results.borrow().get(index).cloned()
}

pub(crate) fn run_script_capture(action: &Action) -> Option<String> {
    let ActionKind::Shell(cmd) = &action.kind else {
        return None;
    };
    let path = std::path::Path::new(&cmd.command);
    if !path.exists() {
        return None;
    }
    let stdout = crate::search::scripts::run_script_stdout(path).ok()?;
    if stdout.trim().is_empty() {
        return None;
    }
    Some(stdout)
}
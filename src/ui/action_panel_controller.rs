#![allow(clippy::too_many_arguments)]

use std::cell::RefCell;
use std::rc::Rc;
use gtk::prelude::*;
use gtk::{ApplicationWindow, Box as GtkBox, Entry, ListBox};
use crate::{
    Action, ActionPanelSection, SecondaryActionKind, Zeshicast,
    ui::ActionPanelDisplayItem,
};
use super::launcher::{
    run_secondary_action_or_confirm, selected_action, show_root_view, update_results,
};

#[derive(Clone)]
pub(crate) struct ActionPanelItem {
    pub(crate) display: ActionPanelDisplayItem,
    pub(crate) section: ActionPanelSection,
    pub(crate) kind: ActionPanelItemKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActionPanelItemKind {
    Secondary(SecondaryActionKind),
    SetAlias,
}

#[derive(Clone)]
pub(crate) enum DisplayedActionPanelRow {
    Header(ActionPanelSection),
    Action(ActionPanelItem),
}

pub(crate) fn show_action_panel_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    action_panel_view: &crate::ui::ActionPanelView,
    current_action: &Rc<RefCell<Option<Action>>>,
    action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    filtered_action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    displayed_action_panel_rows: &Rc<RefCell<Vec<DisplayedActionPanelRow>>>,
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
    let rows = action_panel_display_rows(&filtered_action_panel_items.borrow());
    let displays = action_panel_display_items(&rows);
    *displayed_action_panel_rows.borrow_mut() = rows;
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
    displayed_action_panel_rows: &Rc<RefCell<Vec<DisplayedActionPanelRow>>>,
    action_panel_list: &ListBox,
) {
    let query = query.trim().to_lowercase();
    let filtered = action_panel_items
        .borrow()
        .iter()
        .filter(|item| query.is_empty() || item.display.title.to_lowercase().contains(&query))
        .cloned()
        .collect::<Vec<_>>();
    let rows = action_panel_display_rows(&filtered);
    let displays = action_panel_display_items(&rows);
    *filtered_action_panel_items.borrow_mut() = filtered;
    *displayed_action_panel_rows.borrow_mut() = rows;
    crate::ui::set_action_panel_list(action_panel_list, &displays);
}

pub(crate) fn action_panel_display_rows(items: &[ActionPanelItem]) -> Vec<DisplayedActionPanelRow> {
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
        result.push(DisplayedActionPanelRow::Header(section));
        for item in section_items {
            result.push(DisplayedActionPanelRow::Action(item.clone()));
        }
    }
    result
}

pub(crate) fn action_panel_display_items(rows: &[DisplayedActionPanelRow]) -> Vec<ActionPanelDisplayItem> {
    rows.iter()
        .map(|row| match row {
            DisplayedActionPanelRow::Header(section) => ActionPanelDisplayItem {
                title: section.title().to_string(),
                icon_name: String::new(),
                is_section_header: true,
                is_destructive: false,
            },
            DisplayedActionPanelRow::Action(item) => item.display.clone(),
        })
        .collect()
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
    displayed_action_panel_rows: &Rc<RefCell<Vec<DisplayedActionPanelRow>>>,
    index: usize,
) {
    let Some(action) = current_action.borrow().clone() else {
        return;
    };
    let Some(DisplayedActionPanelRow::Action(item)) =
        displayed_action_panel_rows.borrow().get(index).cloned()
    else {
        return;
    };

    match item.kind {
        ActionPanelItemKind::Secondary(kind) => {
            let entry = entry.clone();
            let list = list.clone();
            let results = Rc::clone(results);
            let navigation = navigation.clone();
            let action_bar = action_bar.clone();
            let launcher_for_done = Rc::clone(launcher);
            run_secondary_action_or_confirm(window, launcher, action, kind, move || {
                update_results(
                    &launcher_for_done.borrow(),
                    &results,
                    &list,
                    entry.text().as_str(),
                    None,
                );
                show_root_view(&navigation, &entry, &action_bar);
            });
        }
        ActionPanelItemKind::SetAlias => {
            crate::ui::show_alias_panel(window, launcher, &action);
            show_root_view(navigation, entry, action_bar);
        }
    }
}

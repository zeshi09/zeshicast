use std::cell::RefCell;
use std::rc::Rc;
use crate::{Action, Zeshicast};
use gtk::prelude::*;
use gtk::ListBox;


pub(crate) fn update_results(
    launcher: &Zeshicast,
    results: &Rc<RefCell<Vec<Action>>>,
    list: &ListBox,
    query: &str,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let actions = launcher.search(query);
    let displayed_actions = if query.trim().is_empty() {
        append_grouped_root_actions(launcher, list, actions)
    } else {
        for action in &actions {
            list.append(&crate::ui::result_row(action));
        }
        actions
    };

    *results.borrow_mut() = displayed_actions;
    select_first_action_row(list);
}

pub(crate) fn append_grouped_root_actions(
    launcher: &Zeshicast,
    list: &ListBox,
    actions: Vec<Action>,
) -> Vec<Action> {
    let recent_top: std::collections::HashSet<String> = launcher
        .recent_top_identities(8)
        .into_iter()
        .collect();

    let sections = ["Favourites", "Recent", "Command Center", "Applications", "Library"];
    let mut buckets = sections
        .iter()
        .map(|section| (*section, Vec::<Action>::new()))
        .collect::<Vec<_>>();

    for action in actions {
        let section = root_action_section(launcher, &action, &recent_top);
        if let Some((_, actions)) = buckets.iter_mut().find(|(name, _)| *name == section) {
            actions.push(action);
        }
    }

    let mut displayed_actions = Vec::new();
    for (section, actions) in buckets {
        if actions.is_empty() {
            continue;
        }
        list.append(&crate::ui::section_header(section));
        for action in actions {
            list.append(&crate::ui::result_row(&action));
            displayed_actions.push(action);
        }
    }
    displayed_actions
}

pub(crate) fn root_action_section(
    launcher: &Zeshicast,
    action: &Action,
    recent_top: &std::collections::HashSet<String>,
) -> &'static str {
    if launcher.is_pinned(action) {
        return "Favourites";
    }

    let identity = action.identity().to_lowercase();
    if recent_top.contains(&identity) {
        return "Recent";
    }

    match action.category.as_str() {
        "Zeshicast" | "System" | "Audio" | "Network" | "Media" | "Notifications" => {
            "Command Center"
        }
        "App" => "Applications",
        _ => "Library",
    }
}

pub(crate) fn action_for_row(
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    row: &gtk::ListBoxRow,
) -> Option<Action> {
    let index = action_index_for_row(list, row)?;
    results.borrow().get(index).cloned()
}

pub(crate) fn action_index_for_row(list: &ListBox, row: &gtk::ListBoxRow) -> Option<usize> {
    if !row.is_selectable() {
        return None;
    }

    let mut action_index = 0usize;
    for index in 0..=row.index() {
        let Some(candidate) = list.row_at_index(index) else {
            continue;
        };
        if !candidate.is_selectable() {
            continue;
        }
        if candidate == *row {
            return Some(action_index);
        }
        action_index += 1;
    }
    None
}

/// Run a Script action and return stdout if the script produces output (fullOutput / compact).
/// Returns None if the script should just be spawned without capturing output.

pub(crate) fn select_first_action_row(list: &ListBox) {
    let mut index = 0;
    while let Some(row) = list.row_at_index(index) {
        if row.is_selectable() {
            list.select_row(Some(&row));
            return;
        }
        index += 1;
    }
}
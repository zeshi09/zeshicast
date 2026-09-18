//! Turning actions into rows, and mapping a row back to the action it was
//! built from (P5.2, `ACTION_ROW_TAG`).

use super::*;

pub(crate) fn empty_state_fallback_actions(query: &str) -> Vec<Action> {
    let query_trimmed = query.trim();
    if query_trimmed.is_empty() {
        return Vec::new();
    }

    let display_query = if query_trimmed.chars().count() > 36 {
        let truncated: String = query_trimmed.chars().take(36).collect();
        format!("{truncated}…")
    } else {
        query_trimmed.to_string()
    };

    vec![
        Action::new(
            "AI Assistant",
            format!("Ask AI: \"{display_query}\""),
            ActionKind::Launcher(crate::action::LauncherCommand::AiChatWithPrompt(
                query_trimmed.to_string(),
            )),
            100,
        )
        .with_subtitle("Stream answer from local Ollama model")
        .with_icon("face-smile-symbolic"),
        Action::new(
            "Web Search",
            format!("Search Web for \"{display_query}\""),
            ActionKind::OpenUrl(format!(
                "https://www.google.com/search?q={}",
                crate::action::percent_encode(query_trimmed)
            )),
            90,
        )
        .with_subtitle("Search Google in default browser")
        .with_icon("system-search-symbolic"),
        Action::new(
            "Snippets",
            format!("Create Snippet with \"{display_query}\""),
            ActionKind::Launcher(crate::action::LauncherCommand::CreateSnippet(
                query_trimmed.to_string(),
            )),
            80,
        )
        .with_subtitle("Save text as a reusable snippet")
        .with_icon("document-edit-symbolic"),
    ]
}

pub(crate) fn update_results(
    launcher: &Zeshicast,
    results: &Rc<RefCell<Vec<Action>>>,
    list: &ListBox,
    query: &str,
    counter: Option<&Label>,
) {
    render_results(
        launcher,
        results,
        list,
        query,
        counter,
        launcher.search(query),
    );
}

/// The mode badge follows every keystroke immediately: it is cheap, and the
/// debounced search must not delay visible feedback (M-1).
pub(crate) fn update_mode_badge(mode_badge: &Label, query: &str) {
    if query.starts_with('=') {
        mode_badge.set_text("Calculator");
        mode_badge.set_visible(true);
    } else if query.starts_with("file ") || query.starts_with("find ") {
        mode_badge.set_text("File Search");
        mode_badge.set_visible(true);
    } else {
        mode_badge.set_visible(false);
    }
}

/// The rendering half of the search flow: everything that touches widgets.
///
/// Split out of [`update_results`] so the search itself can run later (debounced
/// and, eventually, off the main loop) while rendering happens on the main
/// thread (M-1).
pub(crate) fn render_results(
    launcher: &Zeshicast,
    results: &Rc<RefCell<Vec<Action>>>,
    list: &ListBox,
    query: &str,
    counter: Option<&Label>,
    actions: Vec<Action>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    // Calculator inline result. It is appended here (so it is the first row) and
    // prepended to `displayed_actions` below, which is what keeps `results[i]`
    // aligned with the row the user highlighted.
    let calc = calc_row_action(query);
    if let Some(action) = &calc {
        list.append(&crate::ui::result_row(action));
    }

    let displayed_actions = if query.trim().is_empty() {
        append_grouped_root_actions(launcher, list, actions)
    } else if !actions.is_empty() {
        let mut stagger = 0usize;
        for action in &actions {
            let row = crate::ui::result_row(action);
            if stagger < 5 {
                row.add_css_class(&format!("row-stagger-{stagger}"));
                stagger += 1;
            }
            list.append(&row);
        }
        actions
    } else if !query.starts_with('=') {
        let header_row = gtk::ListBoxRow::new();
        header_row.set_selectable(false);
        header_row.set_activatable(false);

        let header_box = GtkBox::new(Orientation::Vertical, 4);
        header_box.set_margin_top(16);
        header_box.set_margin_bottom(8);
        header_box.set_margin_start(16);
        header_box.set_margin_end(16);

        let title_lbl = Label::new(Some(&format!("No results for \"{query}\"")));
        title_lbl.add_css_class("no-results-label");
        title_lbl.set_halign(gtk::Align::Start);

        let subtitle_lbl = Label::new(Some("Quick actions"));
        subtitle_lbl.add_css_class("section-header");
        subtitle_lbl.set_halign(gtk::Align::Start);

        header_box.append(&title_lbl);
        header_box.append(&subtitle_lbl);
        header_row.set_child(Some(&header_box));
        list.append(&header_row);

        let fallbacks = empty_state_fallback_actions(query);
        for action in &fallbacks {
            list.append(&crate::ui::result_row(action));
        }
        fallbacks
    } else {
        Vec::new()
    };

    let displayed_actions = with_calc_row_first(calc, displayed_actions);

    let total = displayed_actions.len();
    *results.borrow_mut() = displayed_actions;
    tag_action_rows(list, results);
    select_first_action_row(list);

    // Update overflow counter: show total when > threshold
    if let Some(ctr) = counter {
        const OVERFLOW_THRESHOLD: usize = 6;
        if total > OVERFLOW_THRESHOLD {
            // Selection handler refines this to "N of M"; seed with first row.
            ctr.set_text(&format!("1 of {total}"));
            ctr.set_visible(true);
        } else {
            ctr.set_visible(false);
        }
    }
}

pub(crate) fn set_raw_result_actions(
    results: &Rc<RefCell<Vec<Action>>>,
    list: &ListBox,
    actions: Vec<Action>,
    empty_message: &str,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for action in &actions {
        list.append(&crate::ui::result_row(action));
    }

    if actions.is_empty() {
        let row = gtk::ListBoxRow::new();
        row.set_selectable(false);
        row.set_activatable(false);
        let lbl = Label::new(Some(empty_message));
        lbl.add_css_class("no-results-label");
        lbl.set_halign(gtk::Align::Center);
        lbl.set_hexpand(true);
        lbl.set_margin_top(30);
        lbl.set_margin_bottom(30);
        row.set_child(Some(&lbl));
        list.append(&row);
    }

    *results.borrow_mut() = actions;
    tag_action_rows(list, results);
    select_first_action_row(list);
}

/// The actions the list will show, calculator row first (B-4). It has to match
/// the order in which the rows are appended above.
pub(crate) fn with_calc_row_first(calc: Option<Action>, actions: Vec<Action>) -> Vec<Action> {
    match calc {
        Some(action) => std::iter::once(action).chain(actions).collect(),
        None => actions,
    }
}

/// The calculator row for a `=…` query, as a real action (B-4).
///
/// The row used to be a hand-built widget that was *not* part of
/// `results`, while `action_index_for_row` counts selectable rows: every action
/// below it was reachable one row too high, and highlighting the calculator row
/// ran somebody else's action. The old fake "evaluator" also just echoed the
/// expression back, so `=2+2` displayed `2+2` instead of `4`.
pub(crate) fn calc_row_action(query: &str) -> Option<Action> {
    let expr = query.strip_prefix('=')?.trim();
    crate::search::calculator::calc_action(expr)
}

fn append_grouped_root_actions(
    launcher: &Zeshicast,
    list: &ListBox,
    actions: Vec<Action>,
) -> Vec<Action> {
    let recent_top: std::collections::HashSet<String> =
        launcher.recent_top_identities(8).into_iter().collect();

    let sections = [
        "Favourites",
        "Recent",
        "Command Center",
        "Applications",
        "Library",
    ];
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

fn root_action_section(
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

pub(crate) fn selected_action(
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
) -> Option<Action> {
    let row = list.selected_row()?;
    let index = action_index_for_row(&row)?;
    results.borrow().get(index).cloned()
}

pub(crate) fn action_for_row(
    results: &Rc<RefCell<Vec<Action>>>,
    row: &gtk::ListBoxRow,
) -> Option<Action> {
    let index = action_index_for_row(row)?;
    results.borrow().get(index).cloned()
}

/// Prefix of the tag that ties a row to the action it displays (B-4).
const ACTION_ROW_TAG: &str = "zeshicast-action-row:";

/// Tie every selectable row to the action registered for it.
///
/// The row list and `results` are built by different code paths, and
/// `action_index_for_row` used to *count* selectable rows to guess which action a
/// highlighted row stood for. Whenever the two disagreed -- exactly the case with
/// the hand-built calculator row, which was in the list but not in `results` --
/// the guess was silently one off, and highlighting a row ran its neighbour's
/// action. Tagging removes the guess: a row reports the index it was registered
/// under, and a selectable row nobody registered (more rows than actions) has no
/// tag at all, so nothing runs instead of the wrong thing.
fn tag_action_rows(list: &ListBox, results: &Rc<RefCell<Vec<Action>>>) {
    let registered = results.borrow().len();
    let mut next = 0usize;
    let mut index = 0;
    while let Some(row) = list.row_at_index(index) {
        if row.is_selectable() {
            row.set_widget_name(&match next < registered {
                true => format!("{ACTION_ROW_TAG}{next}"),
                false => String::new(),
            });
            next += 1;
        }
        index += 1;
    }
}

/// Which action a highlighted row displays (B-4).
fn action_index_for_row(row: &gtk::ListBoxRow) -> Option<usize> {
    action_index_from_tag(&row.widget_name())
}

/// Read the action index back out of a row tag.
pub(crate) fn action_index_from_tag(tag: &str) -> Option<usize> {
    tag.strip_prefix(ACTION_ROW_TAG)?.parse().ok()
}

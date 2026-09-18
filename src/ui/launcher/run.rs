//! Running a selected action: confirmation, the JSON-command round trip, the
//! secondary actions and the form panel (P5.2).

use super::*;

pub(crate) fn show_form_for_action(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    entry: &Entry,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    navigation: &crate::ui::NavigationStack,
    action_bar: &GtkBox,
    script_output_view: &crate::ui::ScriptOutputView,
    action: Action,
) {
    // P1.2: confirm *before* showing the form, so the user does not fill in
    // arguments only to have the submission refused afterwards.
    if action.risk.requires_confirmation() {
        let title = action.risk.label().to_string();
        let detail = action_confirmation_detail(&action);
        let window = window.clone();
        let launcher = Rc::clone(launcher);
        let hold = Rc::clone(hold);
        let entry = entry.clone();
        let list = list.clone();
        let results = Rc::clone(results);
        let navigation = navigation.clone();
        let action_bar = action_bar.clone();
        let script_output_view = script_output_view.clone();
        // The panel borrows a clone: `window` itself is moved into the closure.
        let confirm_window = window.clone();
        crate::ui::show_confirmation_panel(
            &confirm_window,
            &title,
            &detail,
            "Confirm",
            move || {
                present_form_panel(
                    &window,
                    &launcher,
                    &hold,
                    &entry,
                    &list,
                    &results,
                    &navigation,
                    &action_bar,
                    &script_output_view,
                    action.clone(),
                );
            },
        );
        return;
    }

    present_form_panel(
        window,
        launcher,
        hold,
        entry,
        list,
        results,
        navigation,
        action_bar,
        script_output_view,
        action,
    );
}

fn present_form_panel(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    entry: &Entry,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    navigation: &crate::ui::NavigationStack,
    action_bar: &GtkBox,
    script_output_view: &crate::ui::ScriptOutputView,
    action: Action,
) {
    let parent_window = window.clone();
    let finish_window = window.clone();
    let launcher = Rc::clone(launcher);
    let hold = Rc::clone(hold);
    let entry = entry.clone();
    let list = list.clone();
    let results = Rc::clone(results);
    let navigation = navigation.clone();
    let action_bar = action_bar.clone();
    let script_output_view = script_output_view.clone();

    crate::ui::show_form_panel(&parent_window, action, move |action, values| {
        if action.category == "Script" || action.script_mode().is_some() {
            let args: Vec<String> = if let Some(form) = action.form_data() {
                form.fields
                    .iter()
                    .map(|f| {
                        let val = values.get(&f.name).cloned().unwrap_or_default();
                        if f.percent_encoded {
                            crate::action::percent_encode(&val)
                        } else {
                            val
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            };
            execute_script_action(
                &finish_window,
                &launcher,
                &hold,
                &navigation,
                &entry,
                &action_bar,
                &script_output_view,
                action,
                args,
            );
        } else {
            let decision = launcher
                .borrow_mut()
                .run_form_action_confirmed(&action, values);
            if let crate::action::ExecutionDecision::Denied(reason) = decision {
                eprintln!("form action denied: {reason}");
            }
            update_results(
                &launcher.borrow(),
                &results,
                &list,
                entry.text().as_str(),
                None,
            );
            finish_interaction(&finish_window, &hold);
        }
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
    emoji_view: &crate::ui::EmojiPickerView,
    font_view: &crate::ui::FontBrowserView,
    system_monitor_view: &crate::ui::SystemMonitorView,
    media_view: &crate::ui::MediaView,
    network_list: &ListBox,
    notifications_view: &crate::ui::NotificationsView,
    script_output_view: &crate::ui::ScriptOutputView,
    window_grid_view: &crate::ui::WindowGridView,
) {
    if let Some(action) = selected_action(list, results) {
        if let Some(command) = action.launcher_command() {
            match command {
                crate::action::LauncherCommand::CreateSnippet(content) => {
                    crate::ui::show_snippet_editor_panel(
                        window,
                        launcher,
                        None,
                        "",
                        "",
                        &content,
                        || {},
                    );
                }
                crate::action::LauncherCommand::AiChatWithPrompt(prompt) => {
                    run_launcher_command(
                        crate::action::LauncherCommand::AiChatWithPrompt(prompt),
                        navigation,
                        entry,
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
                        window_grid_view,
                    );
                    ask_ai_from_view(launcher, ai_chat_view);
                }
                other => {
                    run_launcher_command(
                        other,
                        navigation,
                        entry,
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
                        window_grid_view,
                    );
                }
            }
        } else if action.form_data().is_some() {
            show_form_for_action(
                window,
                launcher,
                hold,
                entry,
                list,
                results,
                navigation,
                action_bar,
                script_output_view,
                action,
            );
        } else if action.json_command_data().is_some() {
            run_json_command_action_or_confirm(window, entry, list, results, action);
        } else if action.category == "Script" || action.script_mode().is_some() {
            execute_script_action(
                window,
                launcher,
                hold,
                navigation,
                entry,
                action_bar,
                script_output_view,
                action,
                Vec::new(),
            );
        } else {
            run_action_or_confirm(window, launcher, hold, action);
        }
    }
}

pub(crate) fn run_json_command_action_or_confirm(
    window: &ApplicationWindow,
    entry: &Entry,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    action: Action,
) {
    if !action.risk.requires_confirmation() {
        run_json_command_action_async(entry, list, results, action);
        return;
    }

    let title = action.risk.label().to_string();
    let detail = action_confirmation_detail(&action);
    let entry = entry.clone();
    let list = list.clone();
    let results = Rc::clone(results);
    crate::ui::show_confirmation_panel(window, &title, &detail, "Confirm", move || {
        run_json_command_action_async(&entry, &list, &results, action.clone());
    });
}

fn run_json_command_action_async(
    entry: &Entry,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    action: Action,
) {
    let Some(command) = action.json_command_data().cloned() else {
        return;
    };
    let query = entry.text().to_string();
    set_raw_result_actions(
        results,
        list,
        vec![
            Action::new(
                &action.category,
                format!("Running {}", action.title),
                ActionKind::None,
                action.score,
            )
            .with_subtitle("Running JSON command…")
            .with_icon("view-refresh-symbolic"),
        ],
        "No JSON results",
    );

    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(crate::search::commands::run_json_command_actions(&command));
    });

    let entry = entry.clone();
    let list = list.clone();
    let results = Rc::clone(results);
    glib::timeout_add_local(
        std::time::Duration::from_millis(30),
        move || match receiver.try_recv() {
            Ok(actions) => {
                if entry.text().as_str() == query {
                    set_raw_result_actions(&results, &list, actions, "No JSON results");
                }
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        },
    );
}

pub(crate) fn run_action_or_confirm(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    action: Action,
) {
    if !action.risk.requires_confirmation() {
        finish_interaction(window, hold);
        if action.category == "Window" {
            let action_clone = action.clone();
            let launcher_clone = Rc::clone(launcher);
            gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
                launcher_clone.borrow_mut().run_action(&action_clone);
            });
        } else {
            launcher.borrow_mut().run_action(&action);
        }
        return;
    }

    let title = action.risk.label().to_string();
    let detail = action_confirmation_detail(&action);
    let launcher = Rc::clone(launcher);
    let hold = Rc::clone(hold);
    let finish_window = window.clone();
    crate::ui::show_confirmation_panel(window, &title, &detail, "Confirm", move || {
        finish_interaction(&finish_window, &hold);
        if action.category == "Window" {
            let action_clone = action.clone();
            let launcher_clone = Rc::clone(&launcher);
            gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
                launcher_clone
                    .borrow_mut()
                    .run_action_confirmed(&action_clone);
            });
        } else {
            launcher.borrow_mut().run_action_confirmed(&action);
        }
    });
}

pub(crate) fn action_confirmation_detail(action: &Action) -> String {
    let value = action.value();
    if value == action.title {
        format!("{}: {}", action.category, action.title)
    } else {
        format!("{}: {}\n{}", action.category, action.title, value)
    }
}

pub(crate) fn run_command_request<I, S>(program: &str, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let _ = crate::action::execute(
        crate::action::ExecutionRequest::Command(crate::action::ProcessCommand::new(
            program,
            args.into_iter()
                .map(|arg| arg.as_ref().to_string())
                .collect(),
        )),
        &crate::action::ExecutionTicket::confirmed(),
    );
}

pub(crate) fn run_shell_request(command: &str) {
    let _ = crate::action::execute(
        crate::action::ExecutionRequest::Shell {
            command: crate::action::ShellCommand::new(command),
        },
        &crate::action::ExecutionTicket::confirmed(),
    );
}

pub(crate) fn run_secondary_action_or_confirm<F>(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    action: Action,
    kind: SecondaryActionKind,
    on_done: F,
) where
    F: Fn() + 'static,
{
    let risk = secondary_action_risk(&action, kind);
    if !risk.requires_confirmation() {
        run_secondary_action(launcher, &action, kind);
        on_done();
        return;
    }

    let title = risk.label().to_string();
    let detail = secondary_action_confirmation_detail(&action, kind);
    let launcher = Rc::clone(launcher);
    crate::ui::show_confirmation_panel(window, &title, &detail, "Confirm", move || {
        run_secondary_action_confirmed(&launcher, &action, kind);
        on_done();
    });
}

pub(crate) fn run_secondary_action(
    launcher: &Rc<RefCell<Zeshicast>>,
    action: &Action,
    kind: SecondaryActionKind,
) {
    if let Err(error) = launcher.borrow_mut().run_secondary_action(action, kind) {
        eprintln!("failed to run secondary action: {error}");
    }
}

fn run_secondary_action_confirmed(
    launcher: &Rc<RefCell<Zeshicast>>,
    action: &Action,
    kind: SecondaryActionKind,
) {
    if let Err(error) = launcher
        .borrow_mut()
        .run_secondary_action_confirmed(action, kind)
    {
        eprintln!("failed to run secondary action: {error}");
    }
}

fn secondary_action_confirmation_detail(action: &Action, kind: SecondaryActionKind) -> String {
    match kind {
        SecondaryActionKind::DeleteClipboardItem => {
            format!("Delete clipboard item:\n{}", action.value())
        }
        SecondaryActionKind::ClearClipboardHistory => {
            "This clears all local clipboard history stored by Zeshicast.".to_string()
        }
        _ => action_confirmation_detail(action),
    }
}

pub(crate) fn clear_clipboard_history_or_confirm<F>(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    on_done: F,
) where
    F: Fn() + 'static,
{
    run_secondary_action_or_confirm(
        window,
        launcher,
        clipboard_item_action(""),
        SecondaryActionKind::ClearClipboardHistory,
        on_done,
    );
}

/// Builds a synthetic Clipboard-category action so clipboard secondary kinds
/// route through the shared confirmation panel and app-level choke point.
pub(crate) fn clipboard_item_action(value: &str) -> Action {
    Action::new(
        "Clipboard",
        value.to_string(),
        ActionKind::Copy(value.to_string()),
        0,
    )
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

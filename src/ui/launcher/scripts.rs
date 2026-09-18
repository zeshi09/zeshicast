//! Script actions: capturing their stdout and dispatching the run (P5.2).

use super::*;

/// Outcome of the capture path for a Script action. Distinguishes "the script
/// already ran but produced no stdout" (must never be spawned again — that was
/// the P0-3 double-execution bug) from "nothing was ever executed" (the caller
/// may fall back to a plain confirmed spawn).
pub(crate) enum ScriptCaptureOutcome {
    NotCapturable,
    RanWithoutOutput,
    Output(String),
}

/// Apply the outcome of an off-thread script capture run: show stdout when the
/// script produced any, close the window when it already ran silently (no
/// respawn), and fall back to `run_action_confirmed` only when nothing was
/// executed so far.
fn finish_script_run(
    outcome: ScriptCaptureOutcome,
    script_output_view: &crate::ui::ScriptOutputView,
    action: &Action,
    launcher: &Rc<RefCell<Zeshicast>>,
    window: &ApplicationWindow,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
) {
    match outcome {
        ScriptCaptureOutcome::Output(stdout) => {
            crate::ui::set_script_output(script_output_view, &action.title, &stdout);
        }
        ScriptCaptureOutcome::RanWithoutOutput => {
            // The script already executed once; spawning it again would run
            // it twice.
            finish_interaction(window, hold);
        }
        ScriptCaptureOutcome::NotCapturable => {
            launcher.borrow_mut().run_action_confirmed(action);
            finish_interaction(window, hold);
        }
    }
}

/// Run a Script action's executable synchronously (call from a worker thread)
/// and classify its result for the capture path.
#[allow(dead_code)]
pub(crate) fn run_script_capture(action: &Action) -> ScriptCaptureOutcome {
    run_script_capture_with_args(action, &[])
}

fn run_script_capture_with_args(action: &Action, args: &[String]) -> ScriptCaptureOutcome {
    let path_str = match &action.kind {
        ActionKind::Shell(cmd) => &cmd.command,
        ActionKind::Form(form) => match &form.command {
            ActionFormCommand::Argv { program, .. } => program,
            ActionFormCommand::Shell(cmd) => cmd,
        },
        ActionKind::Command(cmd) => &cmd.program,
        _ => return ScriptCaptureOutcome::NotCapturable,
    };
    let path = std::path::Path::new(path_str);
    if !path.exists() {
        return ScriptCaptureOutcome::NotCapturable;
    }
    match crate::search::scripts::run_script_stdout_with_args(path, args) {
        Ok(stdout) if !stdout.trim().is_empty() => ScriptCaptureOutcome::Output(stdout),
        // Executed with empty stdout (or the spawn failed after we attempted
        // it): the script was tried, so callers must never respawn it.
        _ => ScriptCaptureOutcome::RanWithoutOutput,
    }
}

pub(crate) fn execute_script_action(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    script_output_view: &crate::ui::ScriptOutputView,
    action: Action,
    args: Vec<String>,
) {
    let mode = action
        .script_mode()
        .unwrap_or(crate::ScriptMode::FullOutput);

    if args.is_empty() && action.risk.requires_confirmation() {
        let title = action.risk.label().to_string();
        let detail = action_confirmation_detail(&action);
        let confirm_window = window.clone();
        let launcher = Rc::clone(launcher);
        let hold = Rc::clone(hold);
        let navigation = navigation.clone();
        let entry = entry.clone();
        let action_bar = action_bar.clone();
        let script_output_view = script_output_view.clone();
        crate::ui::show_confirmation_panel(window, &title, &detail, "Confirm", move || {
            dispatch_script_run(
                &confirm_window,
                &launcher,
                &hold,
                &navigation,
                &entry,
                &action_bar,
                &script_output_view,
                &action,
                &args,
                mode,
            );
        });
    } else {
        dispatch_script_run(
            window,
            launcher,
            hold,
            navigation,
            entry,
            action_bar,
            script_output_view,
            &action,
            &args,
            mode,
        );
    }
}

fn dispatch_script_run(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    script_output_view: &crate::ui::ScriptOutputView,
    action: &Action,
    args: &[String],
    mode: crate::ScriptMode,
) {
    let _ = launcher.borrow_mut().record_recent(action);

    match mode {
        crate::ScriptMode::Silent => {
            let worker_action = action.clone();
            let worker_args = args.to_vec();
            std::thread::spawn(move || {
                let _ = run_script_capture_with_args(&worker_action, &worker_args);
            });
            finish_interaction(window, hold);
        }
        crate::ScriptMode::Compact => {
            let worker_action = action.clone();
            let worker_args = args.to_vec();
            let script_title = action.title.clone();
            std::thread::spawn(move || {
                let outcome = run_script_capture_with_args(&worker_action, &worker_args);
                if let ScriptCaptureOutcome::Output(stdout) = outcome {
                    let trimmed = stdout.trim();
                    if !trimmed.is_empty() {
                        let script_title = script_title.clone();
                        let text = trimmed.to_string();
                        glib::idle_add_once(move || {
                            crate::push_notification(&script_title, &text, "", 0);
                            if !crate::is_dnd_enabled() {
                                crate::ui::show_notification_osd(
                                    None,
                                    &script_title,
                                    &text,
                                    "",
                                    "",
                                    4500,
                                );
                            }
                        });
                    }
                }
            });
            finish_interaction(window, hold);
        }
        crate::ScriptMode::FullOutput | crate::ScriptMode::Inline => {
            let (sender, receiver) = std::sync::mpsc::channel();
            let worker_action = action.clone();
            let worker_args = args.to_vec();
            std::thread::spawn(move || {
                let _ = sender.send(run_script_capture_with_args(&worker_action, &worker_args));
            });
            show_script_output_view(
                navigation,
                entry,
                action_bar,
                script_output_view,
                &action.title,
                "Running…",
            );
            let script_output_view = script_output_view.clone();
            let action = action.clone();
            let launcher = Rc::clone(launcher);
            let window = window.clone();
            let hold = Rc::clone(hold);
            glib::timeout_add_local(std::time::Duration::from_millis(30), move || match receiver
                .try_recv()
            {
                Ok(outcome) => {
                    finish_script_run(
                        outcome,
                        &script_output_view,
                        &action,
                        &launcher,
                        &window,
                        &hold,
                    );
                    glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
            });
        }
    }
}

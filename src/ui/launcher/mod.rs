#![allow(clippy::too_many_arguments)]

use super::action_panel_controller::*;
use super::clipboard_capture::*;
use super::keybindings::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::action::{
    Action, ActionFormCommand, ActionKind, ActionRisk, SecondaryActionKind, secondary_action_risk,
};
use crate::app::{SnippetSummary, Zeshicast};
use crate::services::clipboard_store::{ClipboardKind, ClipboardSummary};
use crate::ui::launcher_helpers::{
    ai_snippet_name, ask_ai_from_view, preference_duration_ms, preference_enabled, preference_list,
};
use crate::ui::launcher_views::{
    run_launcher_command, show_ai_chat_view, show_audio_view, show_dashboard_view, show_emoji_view,
    show_font_browser_view, show_media_view, show_network_view, show_notifications_view,
    show_script_output_view, show_system_monitor_view, show_window_grid_view,
};
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, Entry, EventControllerKey, Label,
    ListBox, Orientation,
};

mod build;
mod results;
mod run;
mod scripts;
mod views;

/// The split modules (P5.2). Everything they define is re-exported at
/// `launcher::`, which is the path the rest of `ui` uses.
pub(crate) use self::{build::*, results::*, run::*, scripts::*, views::*};

pub type WindowConfigurator = fn(&ApplicationWindow);

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

#[derive(Clone, Copy)]
enum ClipboardFilter {
    All,
    Kind(ClipboardKind),
}

#[derive(Clone)]
pub struct GuiState {
    launcher: Rc<RefCell<Zeshicast>>,
    results: Rc<RefCell<Vec<Action>>>,
    window: ApplicationWindow,
    entry: Entry,
    list: ListBox,
    action_bar: GtkBox,
    navigation: crate::ui::NavigationStack,
    open_view: Rc<dyn Fn(&str) -> bool>,
}

pub fn ensure_ui(
    app: &Application,
    state: &Rc<RefCell<Option<GuiState>>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    daemon: bool,
    configure_window: WindowConfigurator,
) {
    if daemon && hold.borrow().is_none() {
        *hold.borrow_mut() = Some(app.hold());
    }

    if state.borrow().is_none() {
        let gui = build_ui(app, hold, configure_window);
        if daemon {
            gui.window.hide();
        } else {
            present_launcher(&gui);
        }
        *state.borrow_mut() = Some(gui);
    }
}

pub fn present_launcher(state: &GuiState) {
    present_launcher_view(state, None);
}

/// Present the window, optionally jumping straight to a named view
/// (e.g. "clipboard", "dashboard"). Falls back to the search view when
/// `view` is `None` or unrecognised.
pub fn present_launcher_view(state: &GuiState, view: Option<&str>) {
    state.entry.set_text("");
    show_root_view(&state.navigation, &state.entry, &state.action_bar);
    update_results(
        &state.launcher.borrow(),
        &state.results,
        &state.list,
        state.entry.text().as_str(),
        None,
    );
    let opened = view.map(|name| (state.open_view)(name)).unwrap_or(false);
    if !opened {
        state.entry.grab_focus();
    }
    state.window.present();
}

fn select_first_action_row(list: &ListBox) {
    let mut index = 0;
    while let Some(row) = list.row_at_index(index) {
        if row.is_selectable() {
            list.select_row(Some(&row));
            return;
        }
        index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActionPanelItem, ActionPanelItemKind, DisplayedActionPanelRow, ScriptCaptureOutcome,
        action_index_from_tag, action_panel_display_items, action_panel_display_rows,
        calc_row_action, decode_clipboard_text, empty_state_fallback_actions, read_png_from_stream,
        run_script_capture, secondary_action_risk, watch_clipboard_text_with, with_calc_row_first,
    };
    use crate::action::{
        Action, ActionKind, ActionPanelSection, ActionRisk, ExecutionDecision, ExecutionPolicy,
        SecondaryActionKind, ShellCommand,
    };
    use crate::app::Zeshicast;
    use crate::ui::ActionPanelDisplayItem;

    #[test]
    fn oversized_clipboard_record_is_truncated_without_killing_the_watcher() {
        // Regression guard for the read cap: an oversized record used to leave
        // the producer blocked writing into a pipe nobody drained while this
        // thread sat in child.wait() forever — a silently dead watcher. The
        // deadlock lives in std's pipe/wait interaction, so this needs a real
        // child process; pure-logic coverage of the truncation itself is
        // already provided by decode_clipboard_text tests.
        let (tx, rx) = std::sync::mpsc::channel();
        let mut first_spawn = true;
        watch_clipboard_text_with(tx, move || {
            // First spawn produces an oversized record with no NUL terminator;
            // the second spawn fails so the watcher thread ends and closes the
            // channel — proving the reconnect loop ran instead of hanging.
            if !std::mem::take(&mut first_spawn) {
                return Err(std::io::Error::other("test: stop reconnecting"));
            }
            std::process::Command::new("sh")
                .args(["-c", "yes | head -c 2000000"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut saw_truncated_record = false;
        let mut reached_reconnect_probe = false;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match rx.recv_timeout(remaining) {
                Ok(message) if message.is_empty() => {
                    reached_reconnect_probe = true;
                    break;
                }
                Ok(_) => saw_truncated_record = true,
                // Channel closed: watcher thread finished.
                Err(_) => break,
            }
        }

        assert!(
            saw_truncated_record,
            "truncated record must be processed as a normal entry"
        );
        assert!(
            reached_reconnect_probe,
            "watcher must reach its reconnect probe instead of hanging on wait()"
        );
    }

    #[test]
    fn script_activation_gates_execution_behind_confirmation() {
        // Row activation for Script actions must show the confirmation panel
        // before run_script_capture may spawn the script; the interactive
        // policy refuses to run an unconfirmed Shell-risk action.
        let action = Action::new(
            "Script",
            "Echo Test",
            ActionKind::Shell(ShellCommand::new("/bin/echo hello")),
            0,
        )
        .with_risk(ActionRisk::Shell);

        assert!(action.risk.requires_confirmation());
        assert!(action.execution_request().is_some());
        assert_eq!(
            ExecutionPolicy::interactive().decide(&action),
            ExecutionDecision::NeedsConfirmation(ActionRisk::Shell)
        );
        assert_eq!(
            ExecutionPolicy::confirmed().decide(&action),
            ExecutionDecision::RunNow
        );
    }

    /// Build a Shell-risk "Script" action whose command is a script path.
    fn script_path_action(path: &std::path::Path) -> Action {
        Action::new(
            "Script",
            "Capture Test",
            ActionKind::Shell(ShellCommand::new(path.to_string_lossy().into_owned())),
            0,
        )
        .with_risk(ActionRisk::Shell)
    }

    #[test]
    fn silent_script_run_is_not_confused_with_not_executed() {
        // P0-3 follow-up: a script that ran but printed nothing must be
        // reported as executed-without-output so the confirmation fallback
        // never spawns it a second time.
        let dir = std::env::temp_dir().join(format!(
            "zeshicast-capture-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let make_executable = |path: &std::path::Path| {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(path, perms).unwrap();
        };

        let silent = dir.join("silent.sh");
        std::fs::write(&silent, "#!/bin/sh\nexit 0\n").unwrap();
        make_executable(&silent);

        let loud = dir.join("loud.sh");
        std::fs::write(&loud, "#!/bin/sh\necho capture-test-output\n").unwrap();
        make_executable(&loud);

        // Executed but empty stdout: must NOT look like "not executed".
        assert!(matches!(
            run_script_capture(&script_path_action(&silent)),
            ScriptCaptureOutcome::RanWithoutOutput
        ));
        // Executed with stdout: captured output.
        assert!(matches!(
            run_script_capture(&script_path_action(&loud)),
            ScriptCaptureOutcome::Output(_)
        ));
        // Missing path: nothing was ever executed — caller may fall back to
        // a plain confirmed spawn.
        assert!(matches!(
            run_script_capture(&script_path_action(&dir.join("missing.sh"))),
            ScriptCaptureOutcome::NotCapturable
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clipboard_text_accepts_plain_and_multiline() {
        assert_eq!(decode_clipboard_text(b"hello").as_deref(), Some("hello"));
        assert_eq!(
            decode_clipboard_text(b"line one\nline two").as_deref(),
            Some("line one\nline two")
        );
    }

    #[test]
    fn clipboard_text_rejects_blank_and_binary() {
        assert!(decode_clipboard_text(b"   \n").is_none());
        // Invalid UTF-8 (e.g. an image fragment).
        assert!(decode_clipboard_text(&[0xff, 0xfe, 0x00]).is_none());
        // Valid UTF-8 but carrying binary control bytes.
        assert!(decode_clipboard_text(b"PNG\x01\x02data").is_none());
        // Valid UTF-8 carrying PNG chunk identifiers or magic headers
        assert!(decode_clipboard_text(b"IHDR").is_none());
        assert!(decode_clipboard_text(b"IEND").is_none());
        assert!(decode_clipboard_text(b"\x89PNG\r\n\x1a\n").is_none());
        assert!(decode_clipboard_text(b"\xff\xd8\xff\xe0").is_none());
        assert!(decode_clipboard_text(b"GIF89a").is_none());
    }

    #[test]
    fn png_stream_parser_reads_complete_png() {
        // Minimal valid 1x1 RGBA PNG (67 bytes)
        let png_1x1: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, // signature
            0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 31, 21, 99,
            52, // IHDR
            0, 0, 0, 10, 73, 68, 65, 84, 120, 156, 99, 0, 1, 0, 0, 5, 0, 1, 13, 10, 45,
            180, // IDAT
            0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130, // IEND
        ];

        // 1. Valid stream reads exact bytes
        let mut cursor = std::io::Cursor::new(png_1x1);
        let parsed = read_png_from_stream(&mut cursor).unwrap();
        assert_eq!(parsed.as_deref(), Some(png_1x1));

        // 2. Trailing data after IEND remains unconsumed in stream
        let mut with_trailing = png_1x1.to_vec();
        with_trailing.extend_from_slice(b"extra non-png data");
        let mut cursor = std::io::Cursor::new(with_trailing);
        let parsed = read_png_from_stream(&mut cursor).unwrap();
        assert_eq!(parsed.as_deref(), Some(png_1x1));
        let mut rest = Vec::new();
        std::io::Read::read_to_end(&mut cursor, &mut rest).unwrap();
        assert_eq!(rest, b"extra non-png data");

        // 3. Non-PNG stream returns Ok(None) without panicking
        let mut invalid = std::io::Cursor::new(b"not a png signature at all");
        assert!(read_png_from_stream(&mut invalid).unwrap().is_none());

        // 4. Empty stream returns Ok(None)
        let mut empty = std::io::Cursor::new(b"");
        assert!(read_png_from_stream(&mut empty).unwrap().is_none());
    }

    #[test]
    fn action_panel_row_index_ignores_section_headers() {
        let rows = action_panel_display_rows(&[
            action_panel_item(
                "Run",
                ActionPanelSection::Primary,
                ActionPanelItemKind::Secondary(SecondaryActionKind::Run),
            ),
            action_panel_item(
                "Copy Value",
                ActionPanelSection::Primary,
                ActionPanelItemKind::Secondary(SecondaryActionKind::CopyValue),
            ),
            action_panel_item(
                "Set Alias",
                ActionPanelSection::Manage,
                ActionPanelItemKind::SetAlias,
            ),
            action_panel_item(
                "Clear Clipboard History",
                ActionPanelSection::Danger,
                ActionPanelItemKind::Secondary(SecondaryActionKind::ClearClipboardHistory),
            ),
        ]);

        assert!(matches!(
            rows.first(),
            Some(DisplayedActionPanelRow::Header(ActionPanelSection::Primary))
        ));
        assert_row_kind(
            &rows,
            1,
            ActionPanelItemKind::Secondary(SecondaryActionKind::Run),
        );
        assert_row_kind(
            &rows,
            2,
            ActionPanelItemKind::Secondary(SecondaryActionKind::CopyValue),
        );
        assert!(matches!(
            rows.get(3),
            Some(DisplayedActionPanelRow::Header(ActionPanelSection::Manage))
        ));
        assert_row_kind(&rows, 4, ActionPanelItemKind::SetAlias);
        assert!(matches!(
            rows.get(5),
            Some(DisplayedActionPanelRow::Header(ActionPanelSection::Danger))
        ));
        assert_row_kind(
            &rows,
            6,
            ActionPanelItemKind::Secondary(SecondaryActionKind::ClearClipboardHistory),
        );
    }

    #[test]
    fn action_panel_filter_preserves_row_mapping() {
        let filtered = vec![action_panel_item(
            "Copy Value",
            ActionPanelSection::Primary,
            ActionPanelItemKind::Secondary(SecondaryActionKind::CopyValue),
        )];
        let rows = action_panel_display_rows(&filtered);
        let displays = action_panel_display_items(&rows);

        assert_eq!(displays.len(), 2);
        assert!(displays[0].is_section_header);
        assert_eq!(displays[0].title, "Primary");
        assert_eq!(displays[1].title, "Copy Value");
        assert_row_kind(
            &rows,
            1,
            ActionPanelItemKind::Secondary(SecondaryActionKind::CopyValue),
        );
    }

    #[test]
    fn clipboard_clear_secondary_action_is_marked_clipboard_clear() {
        let action = Action::new(
            "Clipboard",
            "Item",
            ActionKind::Copy("value".to_string()),
            0,
        );
        assert_eq!(
            secondary_action_risk(&action, SecondaryActionKind::ClearClipboardHistory),
            ActionRisk::ClipboardClear
        );
        assert!(
            secondary_action_risk(&action, SecondaryActionKind::ClearClipboardHistory)
                .requires_confirmation()
        );
    }

    #[test]
    fn secondary_run_inherits_action_risk() {
        let action =
            Action::new("Shell", "Power", ActionKind::None, 0).with_risk(ActionRisk::SystemPower);

        assert_eq!(
            secondary_action_risk(&action, SecondaryActionKind::Run),
            ActionRisk::SystemPower
        );
    }

    #[test]
    fn clipboard_delete_secondary_path_is_gated_behind_confirmation() {
        // The Clipboard view Delete hotkey routes through the same gate as the
        // action panel entry: an unconfirmed DeleteClipboardItem must be
        // refused without touching the history.
        let mut app = Zeshicast {
            apps: Vec::new(),
            quicklinks: Vec::new(),
            snippets: Vec::new(),
            commands: Vec::new(),
            scripts: Vec::new(),
            clipboard_history: vec!["secret".to_string()],
            clipboard_timestamps: std::collections::HashMap::new(),
            calc_history: Vec::new(),
            preferences: std::collections::HashMap::new(),
            preferences_writable: true,
            aliases: std::collections::HashMap::new(),
            pins: std::collections::HashSet::new(),
            recent: Vec::new(),
            frequencies: std::collections::HashMap::new(),
            extensions: Vec::new(),
            files: Vec::new(),
            config_dir: std::env::temp_dir().join("zeshicast-launcher-delete-gate-test"),
        };
        let hotkey_action = super::clipboard_item_action("secret");

        assert_eq!(
            secondary_action_risk(&hotkey_action, SecondaryActionKind::DeleteClipboardItem),
            ActionRisk::Destructive
        );
        assert_eq!(
            app.run_secondary_action(&hotkey_action, SecondaryActionKind::DeleteClipboardItem)
                .unwrap(),
            ExecutionDecision::NeedsConfirmation(ActionRisk::Destructive)
        );
        assert_eq!(app.clipboard_history, vec!["secret"]);
    }

    fn action_panel_item(
        title: &str,
        section: ActionPanelSection,
        kind: ActionPanelItemKind,
    ) -> ActionPanelItem {
        ActionPanelItem {
            display: ActionPanelDisplayItem {
                title: title.to_string(),
                icon_name: "system-run-symbolic".to_string(),
                is_section_header: false,
                is_destructive: section.is_danger(),
            },
            section,
            kind,
        }
    }

    fn assert_row_kind(
        rows: &[DisplayedActionPanelRow],
        index: usize,
        expected: ActionPanelItemKind,
    ) {
        match rows.get(index) {
            Some(DisplayedActionPanelRow::Action(item)) => assert_eq!(item.kind, expected),
            Some(DisplayedActionPanelRow::Header(section)) => {
                panic!("expected action row at {index}, got {section:?} header")
            }
            None => panic!("expected action row at {index}, got no row"),
        }
    }

    #[test]
    fn test_empty_state_fallback_actions() {
        assert!(empty_state_fallback_actions("").is_empty());
        assert!(empty_state_fallback_actions("   ").is_empty());

        let actions = empty_state_fallback_actions("rust borrow checker");
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0].title, "Ask AI: \"rust borrow checker\"");
        assert_eq!(
            actions[0].launcher_command(),
            Some(crate::action::LauncherCommand::AiChatWithPrompt(
                "rust borrow checker".to_string()
            ))
        );
        assert_eq!(actions[1].title, "Search Web for \"rust borrow checker\"");
        assert!(matches!(
            actions[1].kind,
            ActionKind::OpenUrl(ref url) if url.contains("rust") && url.contains("google.com")
        ));
        assert_eq!(
            actions[2].title,
            "Create Snippet with \"rust borrow checker\""
        );
        assert_eq!(
            actions[2].launcher_command(),
            Some(crate::action::LauncherCommand::CreateSnippet(
                "rust borrow checker".to_string()
            ))
        );

        let long_query =
            "this is a very long query that definitely exceeds 36 characters in total length";
        let long_actions = empty_state_fallback_actions(long_query);
        assert_eq!(long_actions.len(), 3);
        assert!(long_actions[0].title.contains('…'));
        assert_eq!(
            long_actions[0].launcher_command(),
            Some(crate::action::LauncherCommand::AiChatWithPrompt(
                long_query.to_string()
            ))
        );
    }
    #[test]
    fn calc_row_is_the_first_action() {
        let calc = calc_row_action("=2+2").expect("2+2 is a valid expression");
        let rows = with_calc_row_first(
            Some(calc),
            vec![Action::new("Other", "other", ActionKind::None, 1)],
        );

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].category, "Calculator");
        assert_eq!(rows[0].title, "2+2 = 4");
        assert_eq!(rows[1].title, "other");
    }

    #[test]
    fn calc_row_copies_the_result() {
        let action = calc_row_action("=2+2").expect("2+2 is a valid expression");
        assert!(
            matches!(&action.kind, ActionKind::Copy(value) if value == "4"),
            "the row must copy the result, got {:?}",
            action.kind
        );
    }

    #[test]
    fn a_query_without_a_calculator_row_is_left_alone() {
        assert!(
            calc_row_action("2+2").is_none(),
            "only `=` asks for the row"
        );
        let rows = with_calc_row_first(
            None,
            vec![Action::new("Other", "other", ActionKind::None, 1)],
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "other");
    }

    #[test]
    fn a_row_reports_the_action_that_was_registered_for_it() {
        // Rows are tagged in list order, and only selectable ones are tagged at
        // all: `[calculator, header, action, action]`.
        let tags = [
            "zeshicast-action-row:0",
            "",
            "zeshicast-action-row:1",
            "zeshicast-action-row:2",
        ];

        assert_eq!(action_index_from_tag(tags[0]), Some(0));
        assert_eq!(
            action_index_from_tag(tags[1]),
            None,
            "a header has no action"
        );
        assert_eq!(action_index_from_tag(tags[2]), Some(1));
        assert_eq!(action_index_from_tag(tags[3]), Some(2));
    }

    #[test]
    fn an_unregistered_row_never_runs_a_neighbour_action() {
        assert_eq!(action_index_from_tag(""), None, "a row nobody registered");
        assert_eq!(action_index_from_tag("zeshicast-action-row:x"), None);
        assert_eq!(
            action_index_from_tag("iface:eth0"),
            None,
            "an unrelated tag is not an action index"
        );
    }
}

#![allow(clippy::too_many_arguments)]

use std::cell::RefCell;
use std::rc::Rc;
use super::action_panel_controller::*;
use super::clipboard_capture::*;
use super::keybindings::*;

use crate::ui::launcher_helpers::{
    ai_snippet_name, ask_ai_from_view, preference_duration_ms, preference_enabled, preference_list,
};
use crate::ui::launcher_views::{
    run_launcher_command, show_ai_chat_view, show_audio_view, show_dashboard_view, show_emoji_view,
    show_font_browser_view, show_media_view, show_network_view, show_notifications_view,
    show_script_output_view, show_system_monitor_view, show_window_grid_view,
};
use crate::{
    Action, ActionFormCommand, ActionKind, ActionRisk, ClipboardKind, ClipboardSummary,
    SecondaryActionKind, SnippetSummary, Zeshicast,
};
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, Entry, EventControllerKey, Label,
    ListBox, Orientation,
};

pub type WindowConfigurator = fn(&ApplicationWindow);


#[derive(Clone, Copy)]
enum NetworkCopyValue {
    Ip,
    Mac,
}

#[derive(Clone, Copy)]
enum NetworkCommandValue {
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

fn build_ui(
    app: &Application,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    configure_window: WindowConfigurator,
) -> GuiState {
    // Defer the filesystem index so the window appears immediately instead of
    // waiting on a `$HOME` walk (up to 10k entries). A worker builds it and
    // swaps it in; until then file search simply returns no matches.
    let launcher = Rc::new(RefCell::new(Zeshicast::load_deferred_files()));
    {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(Zeshicast::build_file_index());
        });
        let launcher = Rc::clone(&launcher);
        glib::timeout_add_local(
            std::time::Duration::from_millis(100),
            move || match receiver.try_recv() {
                Ok(files) => {
                    launcher.borrow_mut().set_file_index(files);
                    glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
            },
        );
    }
    let results = Rc::new(RefCell::new(Vec::<Action>::new()));
    let current_action = Rc::new(RefCell::new(None::<Action>));
    let action_panel_items = Rc::new(RefCell::new(Vec::<ActionPanelItem>::new()));
    let filtered_action_panel_items = Rc::new(RefCell::new(Vec::<ActionPanelItem>::new()));
    let displayed_action_panel_rows = Rc::new(RefCell::new(Vec::<DisplayedActionPanelRow>::new()));
    let clipboard_items = Rc::new(RefCell::new(Vec::<ClipboardSummary>::new()));
    let snippet_items = Rc::new(RefCell::new(Vec::<SnippetSummary>::new()));
    install_clipboard_monitor(&launcher);
    install_clipboard_background_watcher(&launcher);
    if launcher.borrow().notifications_history_enabled() {
        super::notify_server::install_notification_server();
    }

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Zeshicast")
        .default_width(900)
        .default_height(760)
        .resizable(false)
        .decorated(false)
        .build();
    window.add_css_class("launcher-window");
    configure_window(&window);

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("launcher-frame");

    let entry = Entry::builder()
        .placeholder_text("Search for apps and commands…")
        .hexpand(true)
        .build();
    entry.add_css_class("search-input");

    let mode_badge = Label::new(None);
    mode_badge.add_css_class("mode-badge");
    mode_badge.set_visible(false);

    let ctrl_k_hint = Label::new(Some("⌃K"));
    ctrl_k_hint.add_css_class("ctrl-k-hint");
    ctrl_k_hint.set_valign(gtk::Align::Center);

    let back_btn = Button::new();
    back_btn.add_css_class("action-bar-more");
    back_btn.set_valign(gtk::Align::Center);
    back_btn.set_visible(false);

    let list = ListBox::new();
    list.add_css_class("results-list");
    list.set_vexpand(true);
    list.set_activate_on_single_click(false);

    let navigation = crate::ui::NavigationStack::new();
    let search_page = GtkBox::new(Orientation::Vertical, 0);
    search_page.set_vexpand(true);
    let results_scroller = crate::ui::scrollable_list(&list);
    search_page.append(&results_scroller);

    let extension_view = crate::ui::extension_browser_view(&launcher.borrow().list_commands());
    let action_panel_view = crate::ui::action_panel_view();
    let ai_chat_view = crate::ui::ai_chat_view();
    let audio_view = crate::ui::audio_view(&crate::AudioSnapshot::default());
    let dashboard_view = crate::ui::dashboard_view(&crate::SystemSnapshot::default());
    let system_monitor_view =
        crate::ui::system_monitor_view(&crate::SystemSnapshot::default(), &[]);
    let media_view = crate::ui::media_view(&crate::MediaSnapshot::default());
    let network_view = crate::ui::network_view(&crate::NetworkSnapshot::default());
    let notifications_view = crate::ui::notifications_view(&crate::NotificationSnapshot::default());
    let current_clipboard = launcher.borrow().list_clipboard_history();
    *clipboard_items.borrow_mut() = current_clipboard.clone();
    let clipboard_view = crate::ui::clipboard_history_view(&current_clipboard);
    let current_snippets = launcher.borrow().list_snippets();
    *snippet_items.borrow_mut() = current_snippets.clone();
    let snippet_view = crate::ui::snippet_manager_view(&current_snippets);
    let emoji_view = crate::ui::emoji_picker_view();
    let font_view = crate::ui::font_browser_view();
    let preferences_view = crate::ui::preferences_view(launcher.borrow().get_preferences());
    let script_output_view = crate::ui::script_output_view();
    let window_grid_view = crate::ui::window_grid_view();

    navigation.add_page(crate::ui::LauncherView::Root, &search_page);
    navigation.add_page(crate::ui::LauncherView::Actions, &action_panel_view.root);
    navigation.add_page(crate::ui::LauncherView::AiChat, &ai_chat_view.root);
    navigation.add_page(crate::ui::LauncherView::Audio, &audio_view.root);
    navigation.add_page(crate::ui::LauncherView::Clipboard, &clipboard_view.root);
    navigation.add_page(crate::ui::LauncherView::Dashboard, &dashboard_view.root);
    navigation.add_page(crate::ui::LauncherView::Emoji, &emoji_view.root);
    navigation.add_page(crate::ui::LauncherView::Fonts, &font_view.root);
    navigation.add_page(crate::ui::LauncherView::Extensions, &extension_view.root);
    navigation.add_page(crate::ui::LauncherView::Media, &media_view.root);
    navigation.add_page(crate::ui::LauncherView::Network, &network_view.root);
    navigation.add_page(
        crate::ui::LauncherView::Notifications,
        &notifications_view.root,
    );
    navigation.add_page(crate::ui::LauncherView::Preferences, &preferences_view.root);
    navigation.add_page(
        crate::ui::LauncherView::ScriptOutput,
        &script_output_view.root,
    );
    navigation.add_page(crate::ui::LauncherView::Snippets, &snippet_view.root);
    navigation.add_page(
        crate::ui::LauncherView::SystemMonitor,
        &system_monitor_view.root,
    );
    navigation.add_page(
        crate::ui::LauncherView::WindowGrid,
        &window_grid_view.root,
    );

    let (action_bar, result_counter) = action_bar(
        &window,
        &launcher,
        &entry,
        &list,
        &results,
        hold,
        &navigation,
        &action_panel_view,
        &current_action,
        &action_panel_items,
        &filtered_action_panel_items,
        &displayed_action_panel_rows,
        &ai_chat_view,
        &audio_view,
        &dashboard_view,
        &emoji_view,
        &font_view,
        &system_monitor_view,
        &media_view,
        &network_view.list,
        &notifications_view,
        &script_output_view,
        &window_grid_view,
    );

    let status_strip = crate::ui::StatusStrip::new();
    apply_status_strip_preferences(&status_strip, &launcher);
    status_strip.set_network_snapshot(&crate::NetworkSnapshot::default());
    status_strip.set_battery_snapshot(&crate::battery_snapshot());
    status_strip.set_audio_snapshot(&crate::AudioSnapshot::default());
    status_strip.set_media_snapshot(&crate::MediaSnapshot::default());

    let search_shell = GtkBox::new(Orientation::Horizontal, 8);
    search_shell.add_css_class("search-bar");
    search_shell.set_valign(gtk::Align::Center);
    search_shell.append(&back_btn);
    search_shell.append(&mode_badge);
    search_shell.append(&entry);
    search_shell.append(&ctrl_k_hint);

    root.append(&search_shell);
    root.append(navigation.widget());
    root.append(&action_bar);
    root.append(status_strip.widget());
    window.set_child(Some(&root));

    // Navigation view-change callback — single place managing search bar / back button
    {
        let entry_cb = entry.clone();
        let action_bar_cb = action_bar.clone();
        let back_btn_cb = back_btn.clone();
        let ctrl_k_hint_cb = ctrl_k_hint.clone();
        navigation.connect_view_changed(move |view| {
            let is_root = view == crate::ui::LauncherView::Root;
            entry_cb.set_visible(is_root);
            action_bar_cb.set_visible(is_root);
            ctrl_k_hint_cb.set_visible(is_root);
            back_btn_cb.set_visible(!is_root);
            if !is_root {
                back_btn_cb.set_label(&format!("‹  {}", view.back_label()));
            }
        });
    }

    // Back button restores root
    {
        let navigation = navigation.clone();
        let entry_back = entry.clone();
        back_btn.connect_clicked(move |_| {
            navigation.pop();
            entry_back.grab_focus();
        });
    }

    {
        let launcher = Rc::clone(&launcher);
        let results = Rc::clone(&results);
        let list = list.clone();
        let mode_badge = mode_badge.clone();
        let result_counter = result_counter.clone();
        entry.connect_changed(move |entry| {
            let query = entry.text();
            let q = query.as_str();
            if q.starts_with('=') {
                mode_badge.set_text("Calculator");
                mode_badge.set_visible(true);
            } else if q.starts_with("file ") || q.starts_with("find ") {
                mode_badge.set_text("File Search");
                mode_badge.set_visible(true);
            } else {
                mode_badge.set_visible(false);
            }
            update_results(
                &launcher.borrow(),
                &results,
                &list,
                q,
                Some(&result_counter),
            );
        });
    }

    // Footer counter follows selection: "8 of 24"
    {
        let results = Rc::clone(&results);
        let result_counter = result_counter.clone();
        list.connect_row_selected(move |_, row| {
            let total = results.borrow().len();
            const OVERFLOW_THRESHOLD: usize = 6;
            if total > OVERFLOW_THRESHOLD
                && let Some(row) = row
            {
                result_counter.set_text(&format!("{} of {}", row.index() + 1, total));
                result_counter.set_visible(true);
            }
        });
    }

    {
        let window = window.clone();
        let launcher = Rc::clone(&launcher);
        let hold = Rc::clone(hold);
        let entry = entry.clone();
        let list_ref = list.clone();
        let results = Rc::clone(&results);
        let action_bar = action_bar.clone();
        let navigation = navigation.clone();
        let ai_chat_view = ai_chat_view.clone();
        let audio_view = audio_view.clone();
        let dashboard_view = dashboard_view.clone();
        let emoji_view = emoji_view.clone();
        let font_view = font_view.clone();
        let system_monitor_view = system_monitor_view.clone();
        let media_view = media_view.clone();
        let network_list = network_view.list.clone();
        let notifications_view = notifications_view.clone();
        let script_output_view = script_output_view.clone();
        let window_grid_view = window_grid_view.clone();
        list.connect_row_activated(move |_, row| {
            if let Some(action) = action_for_row(&list_ref, &results, row) {
                if let Some(command) = action.launcher_command() {
                    match command {
                        crate::LauncherCommand::CreateSnippet(content) => {
                            crate::ui::show_snippet_editor_panel(
                                &window,
                                &launcher,
                                None,
                                "",
                                "",
                                &content,
                                || {},
                            );
                        }
                        crate::LauncherCommand::AiChatWithPrompt(prompt) => {
                            run_launcher_command(
                                crate::LauncherCommand::AiChatWithPrompt(prompt),
                                &navigation,
                                &entry,
                                &action_bar,
                                &ai_chat_view,
                                &audio_view,
                                &dashboard_view,
                                &emoji_view,
                                &font_view,
                                &system_monitor_view,
                                &media_view,
                                &network_list,
                                &notifications_view,
                                &window_grid_view,
                            );
                            ask_ai_from_view(&launcher, &ai_chat_view);
                        }
                        other => {
                            run_launcher_command(
                                other,
                                &navigation,
                                &entry,
                                &action_bar,
                                &ai_chat_view,
                                &audio_view,
                                &dashboard_view,
                                &emoji_view,
                                &font_view,
                                &system_monitor_view,
                                &media_view,
                                &network_list,
                                &notifications_view,
                                &window_grid_view,
                            );
                        }
                    }
                } else if action.form_data().is_some() {
                    show_form_for_action(
                        &window,
                        &launcher,
                        &hold,
                        &entry,
                        &list_ref,
                        &results,
                        &navigation,
                        &action_bar,
                        &script_output_view,
                        action,
                    );
                } else if action.json_command_data().is_some() {
                    run_json_command_action_or_confirm(
                        &window, &entry, &list_ref, &results, action,
                    );
                } else if action.category == "Script" || action.script_mode().is_some() {
                    execute_script_action(
                        &window,
                        &launcher,
                        &hold,
                        &navigation,
                        &entry,
                        &action_bar,
                        &script_output_view,
                        action,
                        Vec::new(),
                    );
                } else {
                    run_action_or_confirm(&window, &launcher, &hold, action);
                }
            }
        });
    }

    {
        let controller_window = window.clone();
        let launcher = Rc::clone(&launcher);
        let hold = Rc::clone(hold);
        let entry = entry.clone();
        let list = list.clone();
        let results = Rc::clone(&results);
        let action_bar = action_bar.clone();
        let navigation = navigation.clone();
        let action_panel_view = action_panel_view.clone();
        let ai_chat_view = ai_chat_view.clone();
        let audio_view = audio_view.clone();
        let dashboard_view = dashboard_view.clone();
        let emoji_view = emoji_view.clone();
        let font_view = font_view.clone();
        let system_monitor_view = system_monitor_view.clone();
        let media_view = media_view.clone();
        let network_list = network_view.list.clone();
        let notifications_view = notifications_view.clone();
        let window_grid_view = window_grid_view.clone();
        let current_action = Rc::clone(&current_action);
        let action_panel_items = Rc::clone(&action_panel_items);
        let filtered_action_panel_items = Rc::clone(&filtered_action_panel_items);
        let displayed_action_panel_rows = Rc::clone(&displayed_action_panel_rows);
        let clipboard_view = clipboard_view.clone();
        let extension_list = extension_view.list.clone();
        let clipboard_items = Rc::clone(&clipboard_items);
        let snippet_list = snippet_view.list.clone();
        let snippet_items = Rc::clone(&snippet_items);
        let script_output_view = script_output_view.clone();
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, keycode, state| {
            // Match shortcuts against the Latin-layout keyval so Ctrl+O etc. work
            // when the active keyboard layout is non-Latin (e.g. Cyrillic). The
            // real event still reaches the entry unchanged, so typing is intact.
            let key = latin_keyval(keycode).unwrap_or(key);
            handle_key(
                &controller_window,
                &launcher,
                &hold,
                &entry,
                &list,
                &results,
                &action_bar,
                &navigation,
                &action_panel_view,
                &ai_chat_view,
                &audio_view,
                &dashboard_view,
                &emoji_view,
                &font_view,
                &system_monitor_view,
                &media_view,
                &network_list,
                &notifications_view,
                &window_grid_view,
                &current_action,
                &action_panel_items,
                &filtered_action_panel_items,
                &displayed_action_panel_rows,
                &clipboard_view,
                &clipboard_items,
                &extension_list,
                &snippet_list,
                &snippet_items,
                &script_output_view,
                key,
                state,
            )
        });
        // Capture phase: intercept navigation keys (Return, Up/Down, Ctrl-shortcuts)
        // before the focused search Entry consumes them. Otherwise GtkText eats
        // Return whenever the entry has focus, so Enter only worked when focus
        // happened to sit on the result list. Unhandled keys return `Proceed`, so
        // typing still reaches the entry.
        key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        window.add_controller(key_controller);
    }

    {
        let launcher = Rc::clone(&launcher);
        let ai_chat_view = ai_chat_view.clone();
        ai_chat_view.input.clone().connect_activate(move |_| {
            ask_ai_from_view(&launcher, &ai_chat_view);
        });
    }

    {
        let launcher = Rc::clone(&launcher);
        let ai_chat_view = ai_chat_view.clone();
        ai_chat_view.ask.clone().connect_clicked(move |_| {
            ask_ai_from_view(&launcher, &ai_chat_view);
        });
    }

    // Fetch the installed Ollama models into the selector bar, and re-fetch on
    // demand via the refresh button.
    populate_ai_models(&launcher, &ai_chat_view);
    {
        let launcher = Rc::clone(&launcher);
        let ai_chat_view = ai_chat_view.clone();
        ai_chat_view
            .refresh_models
            .clone()
            .connect_clicked(move |_| {
                populate_ai_models(&launcher, &ai_chat_view);
            });
    }

    {
        let ai_chat_view = ai_chat_view.clone();
        ai_chat_view.copy.clone().connect_clicked(move |_| {
            let answer = ai_chat_view.output.text();
            if !answer.is_empty() {
                crate::copy_text(answer.as_str());
            }
        });
    }

    {
        let launcher = Rc::clone(&launcher);
        let ai_chat_view = ai_chat_view.clone();
        ai_chat_view
            .use_clipboard
            .clone()
            .connect_clicked(move |_| {
                if let Some(item) = launcher.borrow().list_clipboard_history().first() {
                    ai_chat_view.input.set_text(&format!(
                        "Use this clipboard content as context:\n{}\n\nQuestion: ",
                        item.value
                    ));
                    ai_chat_view.input.grab_focus();
                }
            });
    }

    {
        let launcher = Rc::clone(&launcher);
        let ai_chat_view = ai_chat_view.clone();
        ai_chat_view.save.clone().connect_clicked(move |_| {
            let prompt = ai_chat_view.input.text();
            let answer = ai_chat_view.output.text();
            if answer.is_empty() {
                return;
            }
            let name = ai_snippet_name(prompt.as_str());
            if let Err(error) = launcher.borrow_mut().add_snippet(&name, answer.as_str()) {
                ai_chat_view
                    .output
                    .set_text(&format!("Failed to save snippet: {error}"));
            }
        });
    }

    {
        let ai_chat_view = ai_chat_view.clone();
        ai_chat_view.clear.clone().connect_clicked(move |_| {
            ai_chat_view.history.borrow_mut().clear();
            while let Some(child) = ai_chat_view.messages_box.first_child() {
                ai_chat_view.messages_box.remove(&child);
            }
            ai_chat_view
                .output
                .set_text("Hi! Running on Ollama. Ask me anything.");
            ai_chat_view.messages_box.append(&ai_chat_view.output);
            ai_chat_view.input.set_text("");
            ai_chat_view.status.set_visible(false);
            ai_chat_view.input.grab_focus();
        });
    }

    // Media buttons (previous / play-pause / next) are wired to MPRIS over
    // D-Bus inside `media_view`; no duplicate playerctl handlers here.

    // Poll the subprocess-heavy network/audio snapshots on a background thread
    // so the per-second UI timers below never fork on the main loop.
    crate::start_poll_cache();

    // Flash a centered pill when the keyboard layout changes (works while the
    // launcher is hidden — it's a separate layer-shell surface). The watcher
    // pushes from niri's event stream; we drain it on the main loop.
    {
        let layout_rx = crate::layout_change_receiver();
        let app = app.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(120), move || {
            while let Ok(code) = layout_rx.try_recv() {
                crate::ui::osd::show_layout_osd(&app, &code);
            }
            glib::ControlFlow::Continue
        });
    }

    {
        let navigation = navigation.clone();
        let media_view = media_view.clone();
        let audio_view = audio_view.clone();
        let dashboard_view = dashboard_view.clone();
        let status_strip = status_strip.clone();
        let launcher = Rc::clone(&launcher);
        // Re-render the audio device list only when it actually changed, so the
        // 1s tick doesn't rebuild (and visibly flicker) the list every second.
        let last_audio = Rc::new(RefCell::new(crate::AudioSnapshot::default()));
        glib::timeout_add_seconds_local(1, move || {
            if preference_enabled(&launcher, "show_status_strip", true) {
                status_strip.set_network_snapshot(&crate::cached_network_snapshot());
                status_strip.set_battery_snapshot(&crate::battery_snapshot());
                status_strip.set_audio_snapshot(&crate::cached_audio_snapshot());
                status_strip.set_media_snapshot(&crate::media_snapshot());
                status_strip.set_keyboard_layout(crate::cached_keyboard_layout().as_deref());
            }
            if navigation.current() == crate::ui::LauncherView::Media {
                crate::ui::set_media_snapshot(&media_view, &crate::media_snapshot());
            } else if navigation.current() == crate::ui::LauncherView::Audio {
                let snapshot = crate::cached_audio_snapshot();
                if *last_audio.borrow() != snapshot {
                    *last_audio.borrow_mut() = snapshot.clone();
                    crate::ui::set_audio_snapshot(&audio_view, &snapshot);
                }
            } else if navigation.current() == crate::ui::LauncherView::Dashboard {
                crate::ui::set_dashboard_media_snapshot(&dashboard_view, &crate::media_snapshot());
            }
            glib::ControlFlow::Continue
        });
    }

    {
        let navigation = navigation.clone();
        let network_list = network_view.list.clone();
        let dashboard_view = dashboard_view.clone();
        let notifications_view = notifications_view.clone();
        // Only rebuild these lists when their data changed (no per-second flicker).
        let last_network = Rc::new(RefCell::new(crate::NetworkSnapshot::default()));
        let last_notifications = Rc::new(RefCell::new(crate::NotificationSnapshot::default()));
        glib::timeout_add_seconds_local(1, move || {
            if navigation.current() == crate::ui::LauncherView::Network {
                let snapshot = crate::cached_network_snapshot();
                if *last_network.borrow() != snapshot {
                    *last_network.borrow_mut() = snapshot.clone();
                    crate::ui::set_network_snapshot(&network_list, &snapshot);
                }
            } else if navigation.current() == crate::ui::LauncherView::Dashboard {
                crate::ui::set_dashboard_network_snapshot(
                    &dashboard_view,
                    &crate::cached_network_snapshot(),
                );
                crate::ui::set_dashboard_battery_snapshot(
                    &dashboard_view,
                    &crate::battery_snapshot(),
                );
                crate::ui::set_dashboard_audio_snapshot(
                    &dashboard_view,
                    &crate::cached_audio_snapshot(),
                );
                crate::ui::set_dashboard_notification_snapshot(
                    &dashboard_view,
                    &crate::notification_snapshot(),
                );
            } else if navigation.current() == crate::ui::LauncherView::Notifications {
                let snapshot = crate::notification_snapshot();
                if *last_notifications.borrow() != snapshot {
                    *last_notifications.borrow_mut() = snapshot.clone();
                    crate::ui::set_notification_snapshot(&notifications_view, &snapshot);
                }
            }
            glib::ControlFlow::Continue
        });
    }

    {
        let navigation = navigation.clone();
        let dashboard_view = dashboard_view.clone();
        let system_monitor_view = system_monitor_view.clone();
        let dashboard_poll_interval =
            preference_duration_ms(&launcher, "dashboard_poll_interval_ms", 1000);
        glib::timeout_add_local(dashboard_poll_interval, move || {
            if navigation.current() == crate::ui::LauncherView::Dashboard {
                crate::ui::set_dashboard_snapshot(&dashboard_view, &crate::system_snapshot());
                crate::ui::set_dashboard_thermal(
                    &dashboard_view,
                    crate::thermal_snapshot()
                        .hottest_zone()
                        .map(|z| z.temperature_c),
                );
            } else if navigation.current() == crate::ui::LauncherView::SystemMonitor {
                crate::ui::set_system_monitor_snapshot(
                    &system_monitor_view,
                    &crate::system_snapshot(),
                    &crate::top_processes_by_memory(8),
                );
            }
            glib::ControlFlow::Continue
        });
    }

    {
        let entry = entry.clone();
        let action_bar = action_bar.clone();
        let navigation = navigation.clone();
        let network_list = network_view.list.clone();
        dashboard_view
            .open_network
            .clone()
            .connect_clicked(move |_| {
                show_network_view(&navigation, &entry, &action_bar, &network_list);
            });
    }

    {
        let network_list = network_view.list.clone();
        network_view.connect_wifi.clone().connect_clicked(move |_| {
            run_selected_network_command(&network_list, NetworkCommandValue::ConnectWifi);
        });
    }

    {
        let network_list = network_view.list.clone();
        network_view.disconnect.clone().connect_clicked(move |_| {
            run_selected_network_command(&network_list, NetworkCommandValue::DisconnectInterface);
        });
    }

    {
        let network_list = network_view.list.clone();
        network_view.copy_ip.clone().connect_clicked(move |_| {
            copy_selected_network_value(&network_list, NetworkCopyValue::Ip);
        });
    }

    {
        let network_list = network_view.list.clone();
        network_view.copy_mac.clone().connect_clicked(move |_| {
            copy_selected_network_value(&network_list, NetworkCopyValue::Mac);
        });
    }

    {
        let window = window.clone();
        let system_monitor_view = system_monitor_view.clone();
        system_monitor_view.kill.clone().connect_clicked(move |_| {
            terminate_selected_system_process_or_confirm(&window, &system_monitor_view, || {});
        });
    }

    {
        dashboard_view
            .toggle_wifi
            .clone()
            .connect_clicked(move |_| {
                run_command_request("nmcli", ["radio", "wifi", "toggle"]);
            });
    }

    {
        dashboard_view
            .toggle_bluetooth
            .clone()
            .connect_clicked(move |_| {
                run_shell_request(
                    "if bluetoothctl show | grep -q 'Powered: yes'; then bluetoothctl power off; else bluetoothctl power on; fi",
                );
            });
    }

    {
        dashboard_view.toggle_dnd.clone().connect_clicked(move |_| {
            crate::toggle_dnd();
        });
    }

    {
        dashboard_view
            .toggle_mute
            .clone()
            .connect_clicked(move |_| {
                run_command_request("wpctl", ["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]);
            });
    }

    {
        dashboard_view.lock.clone().connect_clicked(move |_| {
            run_command_request("loginctl", ["lock-session"]);
        });
    }

    {
        let window = window.clone();
        dashboard_view.suspend.clone().connect_clicked(move |_| {
            let window = window.clone();
            crate::ui::show_confirmation_panel(
                &window,
                ActionRisk::SystemPower.label(),
                "Suspend this system.",
                "Confirm",
                move || run_command_request("systemctl", ["suspend"]),
            );
        });
    }

    {
        let entry = entry.clone();
        let action_bar = action_bar.clone();
        let navigation = navigation.clone();
        let audio_view = audio_view.clone();
        dashboard_view.open_audio.clone().connect_clicked(move |_| {
            show_audio_view(&navigation, &entry, &action_bar, &audio_view);
        });
    }

    {
        let audio_view = audio_view.clone();
        audio_view.mute_output.clone().connect_clicked(move |_| {
            run_command_request("wpctl", ["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]);
            crate::ui::set_audio_snapshot(&audio_view, &crate::audio_snapshot());
        });
    }

    {
        let audio_view = audio_view.clone();
        audio_view.mute_input.clone().connect_clicked(move |_| {
            run_command_request("wpctl", ["set-mute", "@DEFAULT_AUDIO_SOURCE@", "toggle"]);
            crate::ui::set_audio_snapshot(&audio_view, &crate::audio_snapshot());
        });
    }

    {
        let entry = entry.clone();
        let action_bar = action_bar.clone();
        let navigation = navigation.clone();
        let media_view = media_view.clone();
        dashboard_view.open_media.clone().connect_clicked(move |_| {
            show_media_view(&navigation, &entry, &action_bar, &media_view);
        });
    }

    {
        let entry = entry.clone();
        let action_bar = action_bar.clone();
        let navigation = navigation.clone();
        let ai_chat_view = ai_chat_view.clone();
        dashboard_view.open_ai.clone().connect_clicked(move |_| {
            show_ai_chat_view(&navigation, &entry, &action_bar, &ai_chat_view);
        });
    }

    {
        let entry = entry.clone();
        let action_bar = action_bar.clone();
        let navigation = navigation.clone();
        let system_monitor_view = system_monitor_view.clone();
        dashboard_view
            .open_system
            .clone()
            .connect_clicked(move |_| {
                show_system_monitor_view(&navigation, &entry, &action_bar, &system_monitor_view);
            });
    }

    {
        let entry = entry.clone();
        let action_bar = action_bar.clone();
        let navigation = navigation.clone();
        let notifications_view = notifications_view.clone();
        dashboard_view
            .open_notifications
            .clone()
            .connect_clicked(move |_| {
                show_notifications_view(&navigation, &entry, &action_bar, &notifications_view);
            });
    }

    {
        let notifications_view = notifications_view.clone();
        notifications_view
            .toggle_dnd
            .clone()
            .connect_clicked(move |_| {
                crate::toggle_dnd();
                crate::ui::set_notification_snapshot(
                    &notifications_view,
                    &crate::notification_snapshot(),
                );
            });
    }

    {
        let notifications_view = notifications_view.clone();
        notifications_view
            .close_all
            .clone()
            .connect_clicked(move |_| {
                crate::clear_notifications();
                crate::ui::set_notification_snapshot(
                    &notifications_view,
                    &crate::notification_snapshot(),
                );
            });
    }

    {
        let action_panel_list = action_panel_view.list.clone();
        let action_panel_items = Rc::clone(&action_panel_items);
        let filtered_action_panel_items = Rc::clone(&filtered_action_panel_items);
        let displayed_action_panel_rows = Rc::clone(&displayed_action_panel_rows);
        action_panel_view.search.connect_changed(move |search| {
            filter_action_panel_items(
                search.text().as_str(),
                &action_panel_items,
                &filtered_action_panel_items,
                &displayed_action_panel_rows,
                &action_panel_list,
            );
        });
    }

    {
        let window = window.clone();
        let launcher = Rc::clone(&launcher);
        let entry = entry.clone();
        let list = list.clone();
        let results = Rc::clone(&results);
        let action_bar = action_bar.clone();
        let navigation = navigation.clone();
        let current_action = Rc::clone(&current_action);
        let displayed_action_panel_rows = Rc::clone(&displayed_action_panel_rows);
        action_panel_view.list.connect_row_activated(move |_, row| {
            run_action_panel_row(
                &window,
                &launcher,
                &entry,
                &list,
                &results,
                &navigation,
                &action_bar,
                &current_action,
                &displayed_action_panel_rows,
                row.index() as usize,
            );
        });
    }

    {
        let entry = entry.clone();
        let action_bar = action_bar.clone();
        let navigation = navigation.clone();
        let clipboard_items = Rc::clone(&clipboard_items);
        clipboard_view
            .list
            .clone()
            .connect_row_activated(move |list, row| {
                copy_clipboard_row(list, row.index() as usize, &clipboard_items);
                show_root_view(&navigation, &entry, &action_bar);
            });
    }

    {
        let clipboard_view = clipboard_view.clone();
        let clipboard_list = clipboard_view.list.clone();
        let clipboard_items = Rc::clone(&clipboard_items);
        clipboard_list.connect_selected_rows_changed(move |list| {
            let item = list
                .selected_row()
                .and_then(|row| clipboard_items.borrow().get(row.index() as usize).cloned());
            crate::ui::set_clipboard_detail(&clipboard_view, item.as_ref());
        });
    }

    {
        let launcher = Rc::clone(&launcher);
        let clipboard_view = clipboard_view.clone();
        let clipboard_filter = clipboard_view.filter.clone();
        let clipboard_items = Rc::clone(&clipboard_items);
        clipboard_filter.connect_selected_notify(move |_| {
            refresh_clipboard_view(&launcher, &clipboard_view, &clipboard_items);
        });
    }

    {
        let entry = entry.clone();
        let action_bar = action_bar.clone();
        let navigation = navigation.clone();
        let snippet_items = Rc::clone(&snippet_items);
        snippet_view.list.connect_row_activated(move |_, row| {
            copy_snippet_row(row.index() as usize, &snippet_items);
            show_root_view(&navigation, &entry, &action_bar);
        });
    }

    {
        let entry = entry.clone();
        let action_bar = action_bar.clone();
        let navigation = navigation.clone();
        extension_view.list.connect_row_activated(move |_, _| {
            show_root_view(&navigation, &entry, &action_bar);
        });
    }

    // Auto-save: persist each preference as its field changes (no Save button).
    // Controls (Switch/Scale/Spin/toggle pills) all funnel their value into the
    // bound entry, so listening on the entry covers every control type. Initial
    // values are already set before this wiring, so no spurious save on open.
    for (key, field) in &preferences_view.fields {
        let launcher = Rc::clone(&launcher);
        let status_strip = status_strip.clone();
        let key = key.clone();
        field.connect_changed(move |field| {
            let value = field.text().to_string();
            if let Err(error) = launcher.borrow_mut().set_preference(key.clone(), value) {
                eprintln!("failed to save preference {key}: {error}");
            }
            apply_status_strip_preferences(&status_strip, &launcher);
        });
    }

    {
        let hold = Rc::clone(hold);
        window.connect_close_request(move |window| {
            if hold.borrow().is_some() {
                window.hide();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
    }

    update_results(&launcher.borrow(), &results, &list, "", None);

    // Dispatch used to open a specific view directly (CLI flags, IPC).
    // Returns false for an unknown view name.
    let open_view: Rc<dyn Fn(&str) -> bool> = {
        let navigation = navigation.clone();
        let entry = entry.clone();
        let action_bar = action_bar.clone();
        let launcher = Rc::clone(&launcher);
        let clipboard_items = Rc::clone(&clipboard_items);
        let dashboard_view = dashboard_view.clone();
        let clipboard_view = clipboard_view.clone();
        let network_view = network_view.clone();
        let media_view = media_view.clone();
        let audio_view = audio_view.clone();
        let ai_chat_view = ai_chat_view.clone();
        let system_monitor_view = system_monitor_view.clone();
        let notifications_view = notifications_view.clone();
        let emoji_view = emoji_view.clone();
        let font_view = font_view.clone();
        let window_grid_view = window_grid_view.clone();
        Rc::new(move |view: &str| {
            match view {
                "dashboard" => {
                    show_dashboard_view(&navigation, &entry, &action_bar, &dashboard_view)
                }
                "clipboard" => show_clipboard_view(
                    &navigation,
                    &entry,
                    &action_bar,
                    &clipboard_view,
                    &clipboard_items,
                    &launcher,
                ),
                "network" => {
                    show_network_view(&navigation, &entry, &action_bar, &network_view.list)
                }
                "media" => show_media_view(&navigation, &entry, &action_bar, &media_view),
                "audio" => show_audio_view(&navigation, &entry, &action_bar, &audio_view),
                "ai" => show_ai_chat_view(&navigation, &entry, &action_bar, &ai_chat_view),
                "system" => {
                    show_system_monitor_view(&navigation, &entry, &action_bar, &system_monitor_view)
                }
                "notifications" => {
                    show_notifications_view(&navigation, &entry, &action_bar, &notifications_view)
                }
                "emoji" => show_emoji_view(&navigation, &entry, &action_bar, &emoji_view),
                "fonts" => show_font_browser_view(&navigation, &entry, &action_bar, &font_view),
                "grid" | "window-grid" => show_window_grid_view(&navigation, &entry, &action_bar, &window_grid_view),
                _ => return false,
            }
            true
        })
    };

    GuiState {
        launcher,
        results,
        window,
        entry,
        list,
        action_bar,
        navigation,
        open_view,
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

/// Fetch the Ollama model list off the main thread and fill the AI model bar.
fn populate_ai_models(launcher: &Rc<RefCell<Zeshicast>>, view: &crate::ui::AiChatView) {
    let endpoint = {
        let app = launcher.borrow();
        let prefs = app.get_preferences();
        prefs
            .get("ollama_endpoint")
            .or_else(|| prefs.get("local_ai_endpoint"))
            .cloned()
            .unwrap_or_else(|| "http://localhost:11434".to_string())
    };

    let (tx, rx) = std::sync::mpsc::channel::<Vec<String>>();
    std::thread::spawn(move || {
        let _ = tx.send(crate::list_models(&endpoint));
    });

    let launcher = Rc::clone(launcher);
    let view = view.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(80), move || {
        match rx.try_recv() {
            Ok(models) => {
                fill_ai_model_bar(&launcher, &view, &models);
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        }
    });
}

/// Rebuild the model buttons; clicking one switches the active model live and
/// persists it to the `ollama_model` preference (no config editing needed).
fn fill_ai_model_bar(
    launcher: &Rc<RefCell<Zeshicast>>,
    view: &crate::ui::AiChatView,
    models: &[String],
) {
    let list = &view.model_list;
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if models.is_empty() {
        let note = gtk::Label::new(Some("No models — is Ollama running?"));
        note.add_css_class("result-subtitle");
        note.set_valign(gtk::Align::Center);
        list.append(&note);
        return;
    }

    let current = {
        let app = launcher.borrow();
        let prefs = app.get_preferences();
        prefs
            .get("ollama_model")
            .or_else(|| prefs.get("local_ai_model"))
            .or_else(|| prefs.get("ai_model"))
            .cloned()
            .unwrap_or_default()
    };
    // Fall back to the first model if the configured one isn't installed, and
    // persist it so the next query uses something real.
    let active = if models.contains(&current) {
        current
    } else {
        let first = models[0].clone();
        if let Err(error) = launcher
            .borrow_mut()
            .set_preference("ollama_model".to_string(), first.clone())
        {
            eprintln!("failed to set default model: {error}");
        }
        first
    };

    for model in models {
        let btn = Button::with_label(model);
        btn.add_css_class("ai-model-btn");
        if *model == active {
            btn.add_css_class("active");
        }
        let launcher = Rc::clone(launcher);
        let siblings = list.clone();
        let model = model.clone();
        btn.connect_clicked(move |btn| {
            let mut sibling = siblings.first_child();
            while let Some(widget) = sibling {
                widget.remove_css_class("active");
                sibling = widget.next_sibling();
            }
            btn.add_css_class("active");
            if let Err(error) = launcher
                .borrow_mut()
                .set_preference("ollama_model".to_string(), model.clone())
            {
                eprintln!("failed to switch model: {error}");
            }
        });
        list.append(&btn);
    }
}

fn empty_state_fallback_actions(query: &str) -> Vec<Action> {
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
            ActionKind::Launcher(crate::LauncherCommand::AiChatWithPrompt(
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
                crate::percent_encode(query_trimmed)
            )),
            90,
        )
        .with_subtitle("Search Google in default browser")
        .with_icon("system-search-symbolic"),
        Action::new(
            "Snippets",
            format!("Create Snippet with \"{display_query}\""),
            ActionKind::Launcher(crate::LauncherCommand::CreateSnippet(
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
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    // Calculator inline result
    if query.starts_with('=') {
        let expr = query.trim_start_matches('=').trim();
        list.append(&calc_result_row(expr));
    }

    let actions = launcher.search(query);
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

    let total = displayed_actions.len();
    *results.borrow_mut() = displayed_actions;
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

fn set_raw_result_actions(
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
    select_first_action_row(list);
}

fn calc_result_row(expr: &str) -> gtk::ListBoxRow {
    use gtk::prelude::*;
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");
    row.set_selectable(true);

    let layout = GtkBox::new(Orientation::Horizontal, 12);
    layout.set_margin_start(14);
    layout.set_margin_end(14);
    layout.set_valign(gtk::Align::Center);

    // Calculator icon badge
    let badge = Label::new(Some("="));
    badge.add_css_class("mode-badge");
    badge.set_valign(gtk::Align::Center);
    layout.append(&badge);

    let text_col = GtkBox::new(Orientation::Vertical, 2);
    text_col.set_hexpand(true);
    text_col.set_valign(gtk::Align::Center);

    let expr_lbl = Label::new(Some(if expr.is_empty() {
        "Enter expression…"
    } else {
        expr
    }));
    expr_lbl.add_css_class("result-subtitle");
    expr_lbl.set_xalign(0.0);
    text_col.append(&expr_lbl);

    // Evaluate
    let result_text = if expr.is_empty() {
        "0".to_string()
    } else {
        evaluate_expr(expr)
    };

    let result_lbl = Label::new(Some(&result_text));
    result_lbl.add_css_class("metric-value");
    result_lbl.set_xalign(0.0);
    text_col.append(&result_lbl);

    layout.append(&text_col);

    let hint = Label::new(Some("⌃C"));
    hint.add_css_class("ctrl-k-hint");
    hint.set_valign(gtk::Align::Center);
    layout.append(&hint);

    row.set_child(Some(&layout));
    row
}

fn evaluate_expr(expr: &str) -> String {
    // Simple safe evaluator: only digits, operators, parens, spaces, dots
    let safe: String = expr
        .chars()
        .filter(|c| c.is_ascii_digit() || "+-*/()%. \t.".contains(*c))
        .collect();
    if safe.is_empty() {
        return "—".to_string();
    }
    // Use the existing calculator from the search module if available
    // Fallback: return expression as-is (the search module handles evaluation)
    safe
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


pub(crate) fn show_clipboard_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    clipboard_view: &crate::ui::ClipboardHistoryView,
    clipboard_items: &Rc<RefCell<Vec<ClipboardSummary>>>,
    launcher: &Rc<RefCell<Zeshicast>>,
) {
    refresh_clipboard_view(launcher, clipboard_view, clipboard_items);
    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::Clipboard);
    if let Some(row) = clipboard_view.list.row_at_index(0) {
        clipboard_view.list.select_row(Some(&row));
    }
    clipboard_view.list.grab_focus();
}

pub(crate) fn refresh_clipboard_view(
    launcher: &Rc<RefCell<Zeshicast>>,
    clipboard_view: &crate::ui::ClipboardHistoryView,
    clipboard_items: &Rc<RefCell<Vec<ClipboardSummary>>>,
) {
    let filter = selected_clipboard_filter(clipboard_view);
    let items = launcher
        .borrow()
        .list_clipboard_history()
        .into_iter()
        .filter(|item| clipboard_filter_matches(filter, item))
        .collect::<Vec<_>>();
    crate::ui::set_clipboard_history_items(&clipboard_view.list, &items);
    *clipboard_items.borrow_mut() = items;
    let selected_item = clipboard_view
        .list
        .selected_row()
        .and_then(|row| clipboard_items.borrow().get(row.index() as usize).cloned());
    crate::ui::set_clipboard_detail(clipboard_view, selected_item.as_ref());
}

fn selected_clipboard_filter(view: &crate::ui::ClipboardHistoryView) -> ClipboardFilter {
    match view.filter.selected() {
        1 => ClipboardFilter::Kind(ClipboardKind::Text),
        2 => ClipboardFilter::Kind(ClipboardKind::Url),
        3 => ClipboardFilter::Kind(ClipboardKind::Command),
        4 => ClipboardFilter::Kind(ClipboardKind::Code),
        5 => ClipboardFilter::Kind(ClipboardKind::Image),
        _ => ClipboardFilter::All,
    }
}

fn clipboard_filter_matches(filter: ClipboardFilter, item: &ClipboardSummary) -> bool {
    match filter {
        ClipboardFilter::All => true,
        ClipboardFilter::Kind(kind) => item.kind == kind,
    }
}

pub(crate) fn terminate_selected_system_process_or_confirm<F>(
    window: &ApplicationWindow,
    system_monitor_view: &crate::ui::SystemMonitorView,
    on_done: F,
) where
    F: Fn() + 'static,
{
    let Some(row) = system_monitor_view.list.selected_row() else {
        return;
    };
    let Some(process) = crate::top_processes_by_memory(8)
        .get(row.index() as usize)
        .cloned()
    else {
        return;
    };

    let detail = format!("Kill process {} ({})", process.name, process.pid);
    crate::ui::show_confirmation_panel(
        window,
        ActionRisk::ProcessKill.label(),
        &detail,
        "Confirm",
        move || {
            run_command_request("kill", [process.pid.to_string()]);
            on_done();
        },
    );
}

fn copy_selected_network_value(list: &ListBox, value: NetworkCopyValue) {
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

fn run_selected_network_command(list: &ListBox, value: NetworkCommandValue) {
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
            run_command_request("nmcli", ["device", "disconnect", interface.name.as_str()]);
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
            // "--" so an SSID like "-w" is not parsed as an nmcli option.
            run_command_request("nmcli", [
                "dev",
                "wifi",
                "connect",
                "--",
                network.ssid.as_str(),
            ]);
        }
    }
}

pub(crate) fn copy_clipboard_row(
    list: &ListBox,
    index: usize,
    clipboard_items: &Rc<RefCell<Vec<ClipboardSummary>>>,
) {
    let Some(row) = list.row_at_index(index as i32) else {
        return;
    };
    let index = row.index() as usize;
    if let Some(item) = clipboard_items.borrow().get(index) {
        if let Some(path) = crate::clipboard_image_path(&item.value) {
            crate::copy_clipboard_image(path);
        } else {
            crate::copy_text(&item.value);
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

fn apply_status_strip_preferences(
    status_strip: &crate::ui::StatusStrip,
    launcher: &Rc<RefCell<Zeshicast>>,
) {
    status_strip
        .widget()
        .set_visible(preference_enabled(launcher, "show_status_strip", true));
    let items = preference_list(
        launcher,
        "status_items",
        &[
            "clock", "date", "network", "battery", "audio", "media", "layout",
        ],
    );
    status_strip.set_items(&items);
}

fn action_bar(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    entry: &Entry,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    navigation: &crate::ui::NavigationStack,
    action_panel_view: &crate::ui::ActionPanelView,
    current_action: &Rc<RefCell<Option<Action>>>,
    action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    filtered_action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    displayed_action_panel_rows: &Rc<RefCell<Vec<DisplayedActionPanelRow>>>,
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
) -> (GtkBox, Label) {
    let bar = GtkBox::new(Orientation::Horizontal, 6);
    bar.add_css_class("action-bar");
    bar.set_valign(gtk::Align::Center);

    // Left icon buttons
    let folder = icon_bar_button("⊟", "Show in Files  Ctrl+Shift+F");
    let pin = icon_bar_button("◈", "Pin / Unpin  Ctrl+P");
    let copy = icon_bar_button("⎘", "Copy  Ctrl+Enter");
    let run = icon_bar_button("↵", "Run  Enter");

    // Center: result counter (hidden when no results)
    let counter = Label::new(None);
    counter.add_css_class("result-counter");
    counter.set_hexpand(true);
    counter.set_halign(gtk::Align::Center);
    counter.set_visible(false);

    // Right: Actions button
    let actions = footer_button("Actions  ⌃K");

    {
        let window = window.clone();
        let launcher = Rc::clone(launcher);
        let hold = Rc::clone(hold);
        let entry = entry.clone();
        let list = list.clone();
        let results = Rc::clone(results);
        let navigation = navigation.clone();
        let bar = bar.clone();
        let ai_chat_view = ai_chat_view.clone();
        let audio_view = audio_view.clone();
        let dashboard_view = dashboard_view.clone();
        let emoji_view = emoji_view.clone();
        let font_view = font_view.clone();
        let system_monitor_view = system_monitor_view.clone();
        let media_view = media_view.clone();
        let network_list = network_list.clone();
        let notifications_view = notifications_view.clone();
        let script_output_view = script_output_view.clone();
        let window_grid_view = window_grid_view.clone();
        run.connect_clicked(move |_| {
            run_selected_with_views(
                &window,
                &launcher,
                &hold,
                &entry,
                &list,
                &results,
                &navigation,
                &bar,
                &ai_chat_view,
                &audio_view,
                &dashboard_view,
                &emoji_view,
                &font_view,
                &system_monitor_view,
                &media_view,
                &network_list,
                &notifications_view,
                &script_output_view,
                &window_grid_view,
            )
        });
    }

    {
        let navigation = navigation.clone();
        let entry = entry.clone();
        let bar = bar.clone();
        let action_panel_view = action_panel_view.clone();
        let current_action = Rc::clone(current_action);
        let action_panel_items = Rc::clone(action_panel_items);
        let filtered_action_panel_items = Rc::clone(filtered_action_panel_items);
        let displayed_action_panel_rows = Rc::clone(displayed_action_panel_rows);
        let launcher = Rc::clone(launcher);
        let list = list.clone();
        let results = Rc::clone(results);
        actions.connect_clicked(move |_| {
            show_action_panel_view(
                &navigation,
                &entry,
                &bar,
                &action_panel_view,
                &current_action,
                &action_panel_items,
                &filtered_action_panel_items,
                &displayed_action_panel_rows,
                &launcher,
                &list,
                &results,
            );
        });
    }

    {
        let launcher = Rc::clone(launcher);
        let list = list.clone();
        let results = Rc::clone(results);
        copy.connect_clicked(move |_| {
            run_secondary_for_selected(&launcher, &list, &results, SecondaryActionKind::CopyValue)
        });
    }

    {
        let launcher = Rc::clone(launcher);
        let list = list.clone();
        let results = Rc::clone(results);
        folder.connect_clicked(move |_| {
            run_secondary_for_selected(&launcher, &list, &results, SecondaryActionKind::OpenParent)
        });
    }

    {
        let launcher = Rc::clone(launcher);
        let list = list.clone();
        let results = Rc::clone(results);
        pin.connect_clicked(move |_| {
            if let Some(action) = selected_action(&list, &results) {
                let kind = if launcher.borrow().is_pinned(&action) {
                    SecondaryActionKind::Unpin
                } else {
                    SecondaryActionKind::Pin
                };
                if let Err(error) = launcher.borrow_mut().run_secondary_action(&action, kind) {
                    eprintln!("failed to update pin: {error}");
                }
            }
        });
    }

    bar.append(&folder);
    bar.append(&pin);
    bar.append(&copy);
    bar.append(&run);
    bar.append(&counter);
    bar.append(&actions);
    (bar, counter)
}

fn footer_button(label: &str) -> Button {
    let button = Button::with_label(label);
    button.add_css_class("action-bar-more");
    button
}

fn icon_bar_button(icon: &str, tooltip: &str) -> Button {
    let button = Button::with_label(icon);
    button.add_css_class("action-bar-btn");
    button.set_tooltip_text(Some(tooltip));
    button
}

fn show_form_for_action(
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
                            crate::percent_encode(&val)
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
            launcher.borrow_mut().run_form_action(&action, values);
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
                crate::LauncherCommand::CreateSnippet(content) => {
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
                crate::LauncherCommand::AiChatWithPrompt(prompt) => {
                    run_launcher_command(
                        crate::LauncherCommand::AiChatWithPrompt(prompt),
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

fn run_json_command_action_or_confirm(
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
        let _ = sender.send(crate::run_json_command_actions(&command));
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

fn run_action_or_confirm(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    action: Action,
) {
    if !action.risk.requires_confirmation() {
        launcher.borrow_mut().run_action(&action);
        finish_interaction(window, hold);
        return;
    }

    let title = action.risk.label().to_string();
    let detail = action_confirmation_detail(&action);
    let launcher = Rc::clone(launcher);
    let hold = Rc::clone(hold);
    let finish_window = window.clone();
    crate::ui::show_confirmation_panel(window, &title, &detail, "Confirm", move || {
        launcher.borrow_mut().run_action_confirmed(&action);
        finish_interaction(&finish_window, &hold);
    });
}

fn action_confirmation_detail(action: &Action) -> String {
    let value = action.value();
    if value == action.title {
        format!("{}: {}", action.category, action.title)
    } else {
        format!("{}: {}\n{}", action.category, action.title, value)
    }
}

fn run_command_request<I, S>(program: &str, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    crate::run_execution_request(crate::ExecutionRequest::Command(
        crate::ProcessCommand::new(
            program,
            args.into_iter()
                .map(|arg| arg.as_ref().to_string())
                .collect(),
        ),
    ));
}

fn run_shell_request(command: &str) {
    crate::run_execution_request(crate::ExecutionRequest::Shell {
        command: crate::ShellCommand::new(command),
    });
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

fn run_secondary_action(
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

fn secondary_action_risk(action: &Action, kind: SecondaryActionKind) -> ActionRisk {
    match kind {
        SecondaryActionKind::Run | SecondaryActionKind::RunInTerminal => action.risk,
        SecondaryActionKind::DeleteClipboardItem => ActionRisk::Destructive,
        SecondaryActionKind::ClearClipboardHistory => ActionRisk::ClipboardClear,
        _ => ActionRisk::Normal,
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

fn run_secondary_for_selected(
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

fn action_for_row(
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    row: &gtk::ListBoxRow,
) -> Option<Action> {
    let index = action_index_for_row(list, row)?;
    results.borrow().get(index).cloned()
}

fn action_index_for_row(list: &ListBox, row: &gtk::ListBoxRow) -> Option<usize> {
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

/// Outcome of the capture path for a Script action. Distinguishes "the script
/// already ran but produced no stdout" (must never be spawned again — that was
/// the P0-3 double-execution bug) from "nothing was ever executed" (the caller
/// may fall back to a plain confirmed spawn).
enum ScriptCaptureOutcome {
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
fn run_script_capture(action: &Action) -> ScriptCaptureOutcome {
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

fn execute_script_action(
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
    let mode = action.script_mode().unwrap_or(crate::ScriptMode::FullOutput);

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
            glib::timeout_add_local(
                std::time::Duration::from_millis(30),
                move || match receiver.try_recv() {
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
                },
            );
        }
    }
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
        action_panel_display_items, action_panel_display_rows, decode_clipboard_text,
        empty_state_fallback_actions, read_png_from_stream, run_script_capture,
        secondary_action_risk, watch_clipboard_text_with,
    };
    use crate::{
        Action, ActionKind, ActionPanelSection, ActionRisk, ExecutionDecision, ExecutionPolicy,
        SecondaryActionKind, ShellCommand, Zeshicast, ui::ActionPanelDisplayItem,
    };

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
            0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 31, 21, 99, 52, // IHDR
            0, 0, 0, 10, 73, 68, 65, 84, 120, 156, 99, 0, 1, 0, 0, 5, 0, 1, 13, 10, 45, 180, // IDAT
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
            Some(crate::LauncherCommand::AiChatWithPrompt("rust borrow checker".to_string()))
        );
        assert_eq!(actions[1].title, "Search Web for \"rust borrow checker\"");
        assert!(matches!(
            actions[1].kind,
            ActionKind::OpenUrl(ref url) if url.contains("rust") && url.contains("google.com")
        ));
        assert_eq!(actions[2].title, "Create Snippet with \"rust borrow checker\"");
        assert_eq!(
            actions[2].launcher_command(),
            Some(crate::LauncherCommand::CreateSnippet("rust borrow checker".to_string()))
        );

        let long_query = "this is a very long query that definitely exceeds 36 characters in total length";
        let long_actions = empty_state_fallback_actions(long_query);
        assert_eq!(long_actions.len(), 3);
        assert!(long_actions[0].title.contains('…'));
        assert_eq!(
            long_actions[0].launcher_command(),
            Some(crate::LauncherCommand::AiChatWithPrompt(long_query.to_string()))
        );
    }
}


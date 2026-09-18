//! Widget construction: the window, the entry, the results list and the
//! launcher's chrome (P5.2).

use super::*;

pub(crate) fn build_ui(
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
        super::super::notify_server::install_notification_server();
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
    // Timers and pollers follow the window (P3.3): a hidden palette does no
    // per-second work and forks nothing.
    crate::ui::track_window_visibility(&window);

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
    navigation.add_page(crate::ui::LauncherView::WindowGrid, &window_grid_view.root);

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
    status_strip.set_battery_snapshot(&crate::cached_battery_snapshot());
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
        let search_flow = crate::ui::search_flow::SearchFlow::new();
        entry.connect_changed(move |entry| {
            let query = entry.text().to_string();
            update_mode_badge(&mode_badge, &query);

            // Debounced (M-1): a burst of keystrokes runs one search, and a
            // result that a newer query made stale is not rendered.
            let launcher = Rc::clone(&launcher);
            let results = Rc::clone(&results);
            let list = list.clone();
            let result_counter = result_counter.clone();
            let displayed_query = query.clone();
            let snapshot_source = Rc::clone(&launcher);
            search_flow.request_off_thread(
                query,
                // The snapshot is taken on the main thread (cheap clones)...
                move || snapshot_source.borrow().search_data(),
                // ...the search runs on a worker thread (providers fork processes)...
                |data: &crate::SearchData, query| data.search(query),
                // ...and the result is rendered back on the main thread.
                move |actions| {
                    render_results(
                        &launcher.borrow(),
                        &results,
                        &list,
                        &displayed_query,
                        Some(&result_counter),
                        actions,
                    );
                },
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
            if let Some(action) = action_for_row(&results, row) {
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
                &LauncherUi {
                    window: &controller_window,
                    launcher: &launcher,
                    hold: &hold,
                    entry: &entry,
                    list: &list,
                    results: &results,
                    action_bar: &action_bar,
                    navigation: &navigation,
                    action_panel_view: &action_panel_view,
                    ai_chat_view: &ai_chat_view,
                    audio_view: &audio_view,
                    dashboard_view: &dashboard_view,
                    emoji_view: &emoji_view,
                    font_view: &font_view,
                    system_monitor_view: &system_monitor_view,
                    media_view: &media_view,
                    network_list: &network_list,
                    notifications_view: &notifications_view,
                    window_grid_view: &window_grid_view,
                    current_action: &current_action,
                    action_panel_items: &action_panel_items,
                    filtered_action_panel_items: &filtered_action_panel_items,
                    displayed_action_panel_rows: &displayed_action_panel_rows,
                    clipboard_view: &clipboard_view,
                    clipboard_items: &clipboard_items,
                    extension_list: &extension_list,
                    snippet_list: &snippet_list,
                    snippet_items: &snippet_items,
                    script_output_view: &script_output_view,
                },
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
            if crate::ui::hidden() {
                return glib::ControlFlow::Continue;
            }
            if preference_enabled(&launcher, "show_status_strip", true) {
                status_strip.set_network_snapshot(&crate::cached_network_snapshot());
                status_strip.set_battery_snapshot(&crate::cached_battery_snapshot());
                status_strip.set_audio_snapshot(&crate::cached_audio_snapshot());
                status_strip.set_media_snapshot(&crate::cached_media_snapshot());
                status_strip.set_keyboard_layout(crate::cached_keyboard_layout().as_deref());
            }
            if navigation.current() == crate::ui::LauncherView::Media {
                crate::ui::set_media_snapshot(&media_view, &crate::cached_media_snapshot());
            } else if navigation.current() == crate::ui::LauncherView::Audio {
                let snapshot = crate::cached_audio_snapshot();
                if *last_audio.borrow() != snapshot {
                    *last_audio.borrow_mut() = snapshot.clone();
                    crate::ui::set_audio_snapshot(&audio_view, &snapshot);
                }
            } else if navigation.current() == crate::ui::LauncherView::Dashboard {
                crate::ui::set_dashboard_media_snapshot(
                    &dashboard_view,
                    &crate::cached_media_snapshot(),
                );
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
            if crate::ui::hidden() {
                return glib::ControlFlow::Continue;
            }
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
                    &crate::cached_battery_snapshot(),
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
            if crate::ui::hidden() {
                return glib::ControlFlow::Continue;
            }
            if navigation.current() == crate::ui::LauncherView::Dashboard {
                crate::ui::set_dashboard_snapshot(
                    &dashboard_view,
                    &crate::cached_system_snapshot(),
                );
                crate::ui::set_dashboard_thermal(
                    &dashboard_view,
                    crate::cached_thermal_snapshot()
                        .hottest_zone()
                        .map(|z| z.temperature_c),
                );
            } else if navigation.current() == crate::ui::LauncherView::SystemMonitor {
                crate::ui::set_system_monitor_snapshot(
                    &system_monitor_view,
                    &crate::cached_system_snapshot(),
                    &crate::cached_top_processes(),
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
                "grid" | "window-grid" => {
                    show_window_grid_view(&navigation, &entry, &action_bar, &window_grid_view)
                }
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

pub(crate) fn action_bar(
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

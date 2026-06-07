use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::launcher_helpers::{
    ai_snippet_name, ask_ai_from_view, preference_duration_ms, preference_enabled, preference_list,
};
use crate::ui::launcher_views::{
    run_launcher_command, show_ai_chat_view, show_audio_view, show_dashboard_view, show_emoji_view,
    show_font_browser_view, show_media_view,
    show_network_view, show_notifications_view, show_script_output_view, show_system_monitor_view,
};
use crate::{
    Action, ActionKind, ActionPanelSection, ClipboardKind, ClipboardSummary, SecondaryActionKind,
    SnippetSummary, Zeshicast, ui::ActionPanelDisplayItem,
};
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, Entry, EventControllerKey, Label,
    ListBox, Orientation,
};

pub type WindowConfigurator = fn(&ApplicationWindow);

#[derive(Clone)]
struct ActionPanelItem {
    display: ActionPanelDisplayItem,
    section: ActionPanelSection,
    kind: ActionPanelItemKind,
}

#[derive(Clone, Copy)]
enum ActionPanelItemKind {
    Secondary(SecondaryActionKind),
    SetAlias,
}

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
    let launcher = Rc::new(RefCell::new(Zeshicast::load()));
    let results = Rc::new(RefCell::new(Vec::<Action>::new()));
    let current_action = Rc::new(RefCell::new(None::<Action>));
    let action_panel_items = Rc::new(RefCell::new(Vec::<ActionPanelItem>::new()));
    let filtered_action_panel_items = Rc::new(RefCell::new(Vec::<ActionPanelItem>::new()));
    let clipboard_items = Rc::new(RefCell::new(Vec::<ClipboardSummary>::new()));
    let snippet_items = Rc::new(RefCell::new(Vec::<SnippetSummary>::new()));
    install_clipboard_monitor(&launcher);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Zeshicast")
        .default_width(860)
        .default_height(600)
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
    let audio_view = crate::ui::audio_view(&crate::audio_snapshot());
    let dashboard_view = crate::ui::dashboard_view(&crate::system_snapshot());
    let system_monitor_view = crate::ui::system_monitor_view(
        &crate::system_snapshot(),
        &crate::top_processes_by_memory(8),
    );
    let media_view = crate::ui::media_view(&crate::media_snapshot());
    let network_view = crate::ui::network_view(&crate::network_snapshot());
    let notifications_view = crate::ui::notifications_view(&crate::notification_snapshot());
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
    navigation.add_page(crate::ui::LauncherView::ScriptOutput, &script_output_view.root);
    navigation.add_page(crate::ui::LauncherView::Snippets, &snippet_view.root);
    navigation.add_page(
        crate::ui::LauncherView::SystemMonitor,
        &system_monitor_view.root,
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
        &ai_chat_view,
        &audio_view,
        &dashboard_view,
        &emoji_view,
        &font_view,
        &system_monitor_view,
        &media_view,
        &network_view.list,
        &notifications_view,
    );

    let status_strip = crate::ui::StatusStrip::new();
    apply_status_strip_preferences(&status_strip, &launcher);
    status_strip.set_network_snapshot(&crate::network_snapshot());
    status_strip.set_battery_snapshot(&crate::battery_snapshot());
    status_strip.set_audio_snapshot(&crate::audio_snapshot());
    status_strip.set_media_snapshot(&crate::media_snapshot());

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
            } else if q.starts_with("ssh ") {
                mode_badge.set_text("SSH");
                mode_badge.set_visible(true);
            } else if q.starts_with("file ") || q.starts_with("find ") {
                mode_badge.set_text("File Search");
                mode_badge.set_visible(true);
            } else {
                mode_badge.set_visible(false);
            }
            update_results(&launcher.borrow(), &results, &list, q, Some(&result_counter));
        });
    }

    // Footer counter follows selection: "8 of 24"
    {
        let results = Rc::clone(&results);
        let result_counter = result_counter.clone();
        list.connect_row_selected(move |_, row| {
            let total = results.borrow().len();
            const OVERFLOW_THRESHOLD: usize = 6;
            if total > OVERFLOW_THRESHOLD {
                if let Some(row) = row {
                    result_counter.set_text(&format!("{} of {}", row.index() + 1, total));
                    result_counter.set_visible(true);
                }
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
        list.connect_row_activated(move |_, row| {
            if let Some(action) = action_for_row(&list_ref, &results, row) {
                if let Some(command) = action.launcher_command() {
                    run_launcher_command(
                        command,
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
                    );
                } else if action.form_data().is_some() {
                    show_form_for_action(
                        &window, &launcher, &hold, &entry, &list_ref, &results, action,
                    );
                } else if action.category == "Script" {
                    if let Some(stdout) = run_script_capture(&action) {
                        show_script_output_view(
                            &navigation,
                            &entry,
                            &action_bar,
                            &script_output_view,
                            &action.title,
                            &stdout,
                        );
                    } else {
                        launcher.borrow_mut().run_action(&action);
                        finish_interaction(&window, &hold);
                    }
                } else {
                    launcher.borrow_mut().run_action(&action);
                    finish_interaction(&window, &hold);
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
        let current_action = Rc::clone(&current_action);
        let action_panel_items = Rc::clone(&action_panel_items);
        let filtered_action_panel_items = Rc::clone(&filtered_action_panel_items);
        let clipboard_view = clipboard_view.clone();
        let extension_list = extension_view.list.clone();
        let clipboard_items = Rc::clone(&clipboard_items);
        let snippet_list = snippet_view.list.clone();
        let snippet_items = Rc::clone(&snippet_items);
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, _, state| {
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
                &current_action,
                &action_panel_items,
                &filtered_action_panel_items,
                &clipboard_view,
                &clipboard_items,
                &extension_list,
                &snippet_list,
                &snippet_items,
                key,
                state,
            )
        });
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
        let media_view = media_view.clone();
        media_view.previous.clone().connect_clicked(move |_| {
            crate::spawn_command("playerctl", &["previous"]);
        });
    }

    {
        let media_view = media_view.clone();
        media_view.play_pause.clone().connect_clicked(move |_| {
            crate::spawn_command("playerctl", &["play-pause"]);
        });
    }

    {
        let media_view = media_view.clone();
        media_view.next.clone().connect_clicked(move |_| {
            crate::spawn_command("playerctl", &["next"]);
        });
    }

    {
        let navigation = navigation.clone();
        let media_view = media_view.clone();
        let audio_view = audio_view.clone();
        let dashboard_view = dashboard_view.clone();
        let status_strip = status_strip.clone();
        let launcher = Rc::clone(&launcher);
        glib::timeout_add_seconds_local(5, move || {
            if preference_enabled(&launcher, "show_status_strip", true) {
                status_strip.set_network_snapshot(&crate::network_snapshot());
                status_strip.set_battery_snapshot(&crate::battery_snapshot());
                status_strip.set_audio_snapshot(&crate::audio_snapshot());
                status_strip.set_media_snapshot(&crate::media_snapshot());
            }
            if navigation.current() == crate::ui::LauncherView::Media {
                crate::ui::set_media_snapshot(&media_view, &crate::media_snapshot());
            } else if navigation.current() == crate::ui::LauncherView::Audio {
                crate::ui::set_audio_snapshot(&audio_view, &crate::audio_snapshot());
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
        glib::timeout_add_seconds_local(5, move || {
            if navigation.current() == crate::ui::LauncherView::Network {
                crate::ui::set_network_snapshot(&network_list, &crate::network_snapshot());
            } else if navigation.current() == crate::ui::LauncherView::Dashboard {
                crate::ui::set_dashboard_network_snapshot(
                    &dashboard_view,
                    &crate::network_snapshot(),
                );
                crate::ui::set_dashboard_battery_snapshot(
                    &dashboard_view,
                    &crate::battery_snapshot(),
                );
                crate::ui::set_dashboard_audio_snapshot(&dashboard_view, &crate::audio_snapshot());
                crate::ui::set_dashboard_notification_snapshot(
                    &dashboard_view,
                    &crate::notification_snapshot(),
                );
            } else if navigation.current() == crate::ui::LauncherView::Notifications {
                crate::ui::set_notification_snapshot(
                    &notifications_view,
                    &crate::notification_snapshot(),
                );
            }
            glib::ControlFlow::Continue
        });
    }

    {
        let navigation = navigation.clone();
        let dashboard_view = dashboard_view.clone();
        let system_monitor_view = system_monitor_view.clone();
        let dashboard_poll_interval =
            preference_duration_ms(&launcher, "dashboard_poll_interval_ms", 2000);
        glib::timeout_add_local(dashboard_poll_interval, move || {
            if navigation.current() == crate::ui::LauncherView::Dashboard {
                crate::ui::set_dashboard_snapshot(&dashboard_view, &crate::system_snapshot());
                crate::ui::set_dashboard_thermal(
                    &dashboard_view,
                    crate::thermal_snapshot().hottest_zone().map(|z| z.temperature_c),
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
        let system_monitor_view = system_monitor_view.clone();
        system_monitor_view.kill.clone().connect_clicked(move |_| {
            terminate_selected_system_process(&system_monitor_view);
        });
    }

    {
        dashboard_view
            .toggle_wifi
            .clone()
            .connect_clicked(move |_| {
                crate::spawn_command("nmcli", &["radio", "wifi", "toggle"]);
            });
    }

    {
        dashboard_view
            .toggle_bluetooth
            .clone()
            .connect_clicked(move |_| {
                crate::spawn_shell(&crate::ShellCommand::new(
                    "if bluetoothctl show | grep -q 'Powered: yes'; then bluetoothctl power off; else bluetoothctl power on; fi",
                ));
            });
    }

    {
        dashboard_view.toggle_dnd.clone().connect_clicked(move |_| {
            crate::spawn_shell(&crate::ShellCommand::new(
                "swaync-client --toggle-dnd || dunstctl set-paused toggle",
            ));
        });
    }

    {
        dashboard_view
            .toggle_mute
            .clone()
            .connect_clicked(move |_| {
                crate::spawn_command("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]);
            });
    }

    {
        dashboard_view.lock.clone().connect_clicked(move |_| {
            crate::spawn_command("loginctl", &["lock-session"]);
        });
    }

    {
        dashboard_view.suspend.clone().connect_clicked(move |_| {
            crate::spawn_command("systemctl", &["suspend"]);
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
            crate::spawn_command("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]);
            crate::ui::set_audio_snapshot(&audio_view, &crate::audio_snapshot());
        });
    }

    {
        let audio_view = audio_view.clone();
        audio_view.mute_input.clone().connect_clicked(move |_| {
            crate::spawn_command("wpctl", &["set-mute", "@DEFAULT_AUDIO_SOURCE@", "toggle"]);
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
                crate::spawn_shell(&crate::ShellCommand::new(
                    "swaync-client --toggle-dnd || dunstctl set-paused toggle",
                ));
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
                crate::spawn_shell(&crate::ShellCommand::new(
                    "swaync-client --close-all || dunstctl close-all",
                ));
                crate::ui::set_notification_snapshot(
                    &notifications_view,
                    &crate::notification_snapshot(),
                );
            });
    }

    {
        notifications_view
            .open_panel
            .clone()
            .connect_clicked(move |_| {
                crate::spawn_command("swaync-client", &["--toggle-panel"]);
            });
    }

    {
        let action_panel_list = action_panel_view.list.clone();
        let action_panel_items = Rc::clone(&action_panel_items);
        let filtered_action_panel_items = Rc::clone(&filtered_action_panel_items);
        action_panel_view.search.connect_changed(move |search| {
            filter_action_panel_items(
                search.text().as_str(),
                &action_panel_items,
                &filtered_action_panel_items,
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
        let filtered_action_panel_items = Rc::clone(&filtered_action_panel_items);
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
                &filtered_action_panel_items,
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

    {
        let launcher = Rc::clone(&launcher);
        let entry = entry.clone();
        let action_bar = action_bar.clone();
        let navigation = navigation.clone();
        let fields = preferences_view.fields.clone();
        let status_strip = status_strip.clone();
        preferences_view.save.connect_clicked(move |_| {
            let mut borrow = launcher.borrow_mut();
            for (key, field) in &fields {
                let value = field.text().to_string();
                if let Err(error) = borrow.set_preference(key.clone(), value) {
                    eprintln!("failed to save preference {key}: {error}");
                }
            }
            apply_status_strip_preferences(&status_strip, &launcher);
            show_root_view(&navigation, &entry, &action_bar);
        });
    }

    {
        let entry = entry.clone();
        let action_bar = action_bar.clone();
        let navigation = navigation.clone();
        preferences_view.cancel.connect_clicked(move |_| {
            show_root_view(&navigation, &entry, &action_bar);
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
    GuiState {
        launcher,
        results,
        window,
        entry,
        list,
        action_bar,
        navigation,
    }
}

pub fn present_launcher(state: &GuiState) {
    state.entry.set_text("");
    show_root_view(&state.navigation, &state.entry, &state.action_bar);
    update_results(
        &state.launcher.borrow(),
        &state.results,
        &state.list,
        state.entry.text().as_str(),
        None,
    );
    state.entry.grab_focus();
    state.window.present();
}

fn install_clipboard_monitor(launcher: &Rc<RefCell<Zeshicast>>) {
    let Some(display) = gdk::Display::default() else {
        return;
    };

    let clipboard = display.clipboard();
    let last_text = Rc::new(RefCell::new(None::<String>));
    let launcher = Rc::clone(launcher);

    capture_clipboard_text(&clipboard, &launcher, &last_text);

    clipboard.connect_changed(move |clipboard| {
        capture_clipboard_text(clipboard, &launcher, &last_text);
    });
}

fn capture_clipboard_text(
    clipboard: &gdk::Clipboard,
    launcher: &Rc<RefCell<Zeshicast>>,
    last_text: &Rc<RefCell<Option<String>>>,
) {
    let launcher = Rc::clone(launcher);
    let last_text = Rc::clone(last_text);
    clipboard.read_text_async(gio::Cancellable::NONE, move |result| {
        let Ok(Some(text)) = result else {
            return;
        };

        let text = text.to_string();
        if last_text.borrow().as_deref() == Some(text.as_str()) {
            return;
        }

        *last_text.borrow_mut() = Some(text.clone());
        if let Err(error) = launcher.borrow_mut().add_clipboard_text(&text) {
            eprintln!("failed to save clipboard history: {error}");
        }
    });
}

fn update_results(
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
    } else {
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
    };

    // No results empty state
    if displayed_actions.is_empty() && !query.trim().is_empty() && !query.starts_with('=') {
        let row = gtk::ListBoxRow::new();
        row.set_selectable(false);
        row.set_activatable(false);
        let lbl = Label::new(Some(&format!("No results for \"{query}\"")));
        lbl.add_css_class("no-results-label");
        lbl.set_halign(gtk::Align::Center);
        lbl.set_hexpand(true);
        lbl.set_margin_top(30);
        lbl.set_margin_bottom(30);
        row.set_child(Some(&lbl));
        list.append(&row);
    }

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

    let expr_lbl = Label::new(Some(if expr.is_empty() { "Enter expression…" } else { expr }));
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

fn handle_key(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    entry: &Entry,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    action_bar: &GtkBox,
    navigation: &crate::ui::NavigationStack,
    action_panel_view: &crate::ui::ActionPanelView,
    ai_chat_view: &crate::ui::AiChatView,
    audio_view: &crate::ui::AudioView,
    dashboard_view: &crate::ui::DashboardView,
    emoji_view: &crate::ui::EmojiPickerView,
    font_view: &crate::ui::FontBrowserView,
    system_monitor_view: &crate::ui::SystemMonitorView,
    media_view: &crate::ui::MediaView,
    network_list: &ListBox,
    notifications_view: &crate::ui::NotificationsView,
    current_action: &Rc<RefCell<Option<Action>>>,
    action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    filtered_action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    clipboard_view: &crate::ui::ClipboardHistoryView,
    clipboard_items: &Rc<RefCell<Vec<ClipboardSummary>>>,
    extension_list: &ListBox,
    snippet_list: &ListBox,
    snippet_items: &Rc<RefCell<Vec<SnippetSummary>>>,
    key: gdk::Key,
    state: gdk::ModifierType,
) -> glib::Propagation {
    if navigation.current() != crate::ui::LauncherView::Root {
        return handle_view_key(
            window,
            launcher,
            list,
            results,
            navigation,
            entry,
            action_bar,
            &action_panel_view.list,
            ai_chat_view,
            audio_view,
            dashboard_view,
            system_monitor_view,
            media_view,
            network_list,
            notifications_view,
            current_action,
            filtered_action_panel_items,
            clipboard_view,
            clipboard_items,
            extension_list,
            snippet_list,
            snippet_items,
            key,
            state,
        );
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
        gdk::Key::f if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_font_browser_view(navigation, entry, action_bar, font_view);
            glib::Propagation::Stop
        }
        gdk::Key::comma if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_preferences_view(navigation, entry, action_bar);
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
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    action_panel_list: &ListBox,
    ai_chat_view: &crate::ui::AiChatView,
    audio_view: &crate::ui::AudioView,
    dashboard_view: &crate::ui::DashboardView,
    system_monitor_view: &crate::ui::SystemMonitorView,
    media_view: &crate::ui::MediaView,
    network_list: &ListBox,
    notifications_view: &crate::ui::NotificationsView,
    current_action: &Rc<RefCell<Option<Action>>>,
    filtered_action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    clipboard_view: &crate::ui::ClipboardHistoryView,
    clipboard_items: &Rc<RefCell<Vec<ClipboardSummary>>>,
    extension_list: &ListBox,
    snippet_list: &ListBox,
    snippet_items: &Rc<RefCell<Vec<SnippetSummary>>>,
    key: gdk::Key,
    state: gdk::ModifierType,
) -> glib::Propagation {
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
                        filtered_action_panel_items,
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
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::SystemMonitor => {
                show_root_view(navigation, entry, action_bar);
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
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Media => {
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Network => {
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Notifications => {
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Snippets => {
                if let Some(row) = snippet_list.selected_row() {
                    copy_snippet_row(row.index() as usize, snippet_items);
                }
                show_root_view(navigation, entry, action_bar);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        },
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
                crate::ui::set_dashboard_snapshot(dashboard_view, &crate::system_snapshot());
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::SystemMonitor => {
                crate::ui::move_selection(&system_monitor_view.list, 1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Audio => {
                crate::ui::move_selection(&audio_view.streams_list, 1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Media => {
                crate::ui::set_media_snapshot(media_view, &crate::media_snapshot());
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Network => {
                crate::ui::move_selection(network_list, 1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Notifications => {
                crate::ui::set_notification_snapshot(
                    notifications_view,
                    &crate::notification_snapshot(),
                );
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Snippets => {
                crate::ui::move_selection(snippet_list, 1);
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
                crate::ui::set_dashboard_snapshot(dashboard_view, &crate::system_snapshot());
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::SystemMonitor => {
                crate::ui::move_selection(&system_monitor_view.list, -1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Audio => {
                crate::ui::move_selection(&audio_view.streams_list, -1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Media => {
                crate::ui::set_media_snapshot(media_view, &crate::media_snapshot());
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Network => {
                crate::ui::move_selection(network_list, -1);
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Notifications => {
                crate::ui::set_notification_snapshot(
                    notifications_view,
                    &crate::notification_snapshot(),
                );
                glib::Propagation::Stop
            }
            crate::ui::LauncherView::Snippets => {
                crate::ui::move_selection(snippet_list, -1);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        },
        gdk::Key::Delete if navigation.current() == crate::ui::LauncherView::Clipboard => {
            if state.contains(gdk::ModifierType::CONTROL_MASK) {
                if let Err(error) = launcher.borrow_mut().clear_clipboard_history() {
                    eprintln!("failed to clear clipboard history: {error}");
                }
            } else if let Some(row) = clipboard_view.list.selected_row() {
                if let Some(item) = clipboard_items.borrow().get(row.index() as usize) {
                    if let Err(error) = launcher.borrow_mut().delete_clipboard_value(&item.value) {
                        eprintln!("failed to delete clipboard item: {error}");
                    }
                }
            }
            refresh_clipboard_view(launcher, clipboard_view, clipboard_items);
            glib::Propagation::Stop
        }
        gdk::Key::Delete if navigation.current() == crate::ui::LauncherView::Snippets => {
            if let Some(row) = snippet_list.selected_row() {
                if let Some(item) = snippet_items.borrow().get(row.index() as usize) {
                    if let Err(error) = launcher
                        .borrow_mut()
                        .delete_snippet(&item.name, &item.value)
                    {
                        eprintln!("failed to delete snippet: {error}");
                    }
                }
            }
            refresh_snippet_view(launcher, snippet_list, snippet_items);
            glib::Propagation::Stop
        }
        gdk::Key::Delete if navigation.current() == crate::ui::LauncherView::SystemMonitor => {
            terminate_selected_system_process(system_monitor_view);
            crate::ui::set_system_monitor_snapshot(
                system_monitor_view,
                &crate::system_snapshot(),
                &crate::top_processes_by_memory(8),
            );
            glib::Propagation::Stop
        }
        _ => glib::Propagation::Proceed,
    }
}

fn show_action_panel_view(
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

fn filter_action_panel_items(
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

fn action_panel_displays(items: &[ActionPanelItem]) -> Vec<ActionPanelDisplayItem> {
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

fn run_action_panel_row(
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
            update_results(&launcher.borrow(), results, list, entry.text().as_str(), None);
        }
        ActionPanelItemKind::SetAlias => {
            crate::ui::show_alias_panel(window, launcher, &action);
        }
    }
    show_root_view(navigation, entry, action_bar);
}

fn show_clipboard_view(
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

fn refresh_clipboard_view(
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
        _ => ClipboardFilter::All,
    }
}

fn clipboard_filter_matches(filter: ClipboardFilter, item: &ClipboardSummary) -> bool {
    match filter {
        ClipboardFilter::All => true,
        ClipboardFilter::Kind(kind) => item.kind == kind,
    }
}

fn terminate_selected_system_process(system_monitor_view: &crate::ui::SystemMonitorView) {
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

fn copy_clipboard_row(
    list: &ListBox,
    index: usize,
    clipboard_items: &Rc<RefCell<Vec<ClipboardSummary>>>,
) {
    let Some(row) = list.row_at_index(index as i32) else {
        return;
    };
    let index = row.index() as usize;
    if let Some(item) = clipboard_items.borrow().get(index) {
        crate::copy_text(&item.value);
    }
}

fn show_snippet_view(
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

fn refresh_snippet_view(
    launcher: &Rc<RefCell<Zeshicast>>,
    snippet_list: &ListBox,
    snippet_items: &Rc<RefCell<Vec<SnippetSummary>>>,
) {
    let items = launcher.borrow().list_snippets();
    crate::ui::set_snippet_items(snippet_list, &items);
    *snippet_items.borrow_mut() = items;
}

fn copy_snippet_row(index: usize, snippet_items: &Rc<RefCell<Vec<SnippetSummary>>>) {
    if let Some(item) = snippet_items.borrow().get(index) {
        crate::copy_text(&item.value);
    }
}

fn show_preferences_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
) {
    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::Preferences);
}

fn show_extension_view(
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

fn show_root_view(navigation: &crate::ui::NavigationStack, entry: &Entry, action_bar: &GtkBox) {
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
        &["clock", "date", "network", "battery", "audio", "media"],
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
    ai_chat_view: &crate::ui::AiChatView,
    audio_view: &crate::ui::AudioView,
    dashboard_view: &crate::ui::DashboardView,
    emoji_view: &crate::ui::EmojiPickerView,
    font_view: &crate::ui::FontBrowserView,
    system_monitor_view: &crate::ui::SystemMonitorView,
    media_view: &crate::ui::MediaView,
    network_list: &ListBox,
    notifications_view: &crate::ui::NotificationsView,
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
        update_results(&launcher.borrow(), &results, &list, entry.text().as_str(), None);
        finish_interaction(&finish_window, &hold);
    });
}

fn run_selected_with_views(
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
                emoji_view,
                font_view,
                system_monitor_view,
                media_view,
                network_list,
                notifications_view,
            );
        } else if action.form_data().is_some() {
            show_form_for_action(window, launcher, hold, entry, list, results, action);
        } else {
            launcher.borrow_mut().run_action(&action);
            finish_interaction(window, hold);
        }
    }
}

fn finish_interaction(
    window: &ApplicationWindow,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
) {
    if hold.borrow().is_some() {
        window.hide();
    } else {
        window.close();
    }
}

fn copy_selected(list: &ListBox, results: &Rc<RefCell<Vec<Action>>>) {
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

fn selected_action(list: &ListBox, results: &Rc<RefCell<Vec<Action>>>) -> Option<Action> {
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

/// Run a Script action and return stdout if the script produces output (fullOutput / compact).
/// Returns None if the script should just be spawned without capturing output.
fn run_script_capture(action: &Action) -> Option<String> {
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

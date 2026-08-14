mod actions;
mod clipboard;
mod footer;
mod root_list;
mod search_entry;

pub(crate) use actions::*;
pub(crate) use clipboard::*;
pub(crate) use footer::*;
pub(crate) use root_list::*;
pub(crate) use search_entry::*;

use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::launcher_helpers::{
    ai_snippet_name, ask_ai_from_view, preference_duration_ms, preference_enabled,
};
use crate::ui::launcher_views::{
    run_launcher_command, show_ai_chat_view, show_audio_view, show_media_view,
    show_network_view, show_notifications_view, show_script_output_view, show_system_monitor_view,
};
use crate::{
    Action, ClipboardSummary, SnippetSummary, Zeshicast,
};
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Entry, EventControllerKey, ListBox, Orientation,
};

pub type WindowConfigurator = fn(&ApplicationWindow);

#[derive(Clone)]
pub struct GuiState {
    pub(crate) launcher: Rc<RefCell<Zeshicast>>,
    pub(crate) results: Rc<RefCell<Vec<Action>>>,
    pub(crate) window: ApplicationWindow,
    pub(crate) entry: Entry,
    pub(crate) list: ListBox,
    pub(crate) action_bar: GtkBox,
    pub(crate) navigation: crate::ui::NavigationStack,
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
    root.set_margin_top(0);
    root.set_margin_bottom(0);
    root.set_margin_start(0);
    root.set_margin_end(0);

    let entry = Entry::builder()
        .placeholder_text("Search apps, files, clipboard, snippets, quicklinks, or type calc 2 + 2")
        .hexpand(true)
        .build();
    entry.add_css_class("search-entry");

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
    let preferences_view = crate::ui::preferences_view(launcher.borrow().get_preferences());
    let script_output_view = crate::ui::script_output_view();
    let window_grid_view = crate::ui::window_grid_view();

    navigation.add_page(crate::ui::LauncherView::Root, &search_page);
    navigation.add_page(crate::ui::LauncherView::Actions, &action_panel_view.root);
    navigation.add_page(crate::ui::LauncherView::AiChat, &ai_chat_view.root);
    navigation.add_page(crate::ui::LauncherView::Audio, &audio_view.root);
    navigation.add_page(crate::ui::LauncherView::Clipboard, &clipboard_view.root);
    navigation.add_page(crate::ui::LauncherView::Dashboard, &dashboard_view.root);
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
    navigation.add_page(
        crate::ui::LauncherView::WindowGrid,
        &window_grid_view.root,
    );

    let action_bar = action_bar(
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
        &system_monitor_view,
        &media_view,
        &network_view.list,
        &notifications_view,
        &window_grid_view,
    );

    let status_strip = crate::ui::StatusStrip::new();
    apply_status_strip_preferences(&status_strip, &launcher);
    status_strip.set_network_snapshot(&crate::network_snapshot());
    status_strip.set_battery_snapshot(&crate::battery_snapshot());
    status_strip.set_audio_snapshot(&crate::audio_snapshot());
    status_strip.set_media_snapshot(&crate::media_snapshot());

    let search_shell = GtkBox::new(Orientation::Vertical, 0);
    search_shell.add_css_class("search-shell");
    search_shell.append(&entry);

    root.append(&search_shell);
    root.append(navigation.widget());
    root.append(&action_bar);
    root.append(status_strip.widget());
    window.set_child(Some(&root));

    {
        let launcher = Rc::clone(&launcher);
        let results = Rc::clone(&results);
        let list = list.clone();
        entry.connect_changed(move |entry| {
            update_results(&launcher.borrow(), &results, &list, entry.text().as_str());
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
        let system_monitor_view = system_monitor_view.clone();
        let media_view = media_view.clone();
        let network_list = network_view.list.clone();
        let notifications_view = notifications_view.clone();
        let script_output_view = script_output_view.clone();
        let window_grid_view = window_grid_view.clone();
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
                        &system_monitor_view,
                        &media_view,
                        &network_list,
                        &notifications_view,
                        &window_grid_view,
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
        let system_monitor_view = system_monitor_view.clone();
        let media_view = media_view.clone();
        let network_list = network_view.list.clone();
        let notifications_view = notifications_view.clone();
        let window_grid_view = window_grid_view.clone();
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
                &system_monitor_view,
                &media_view,
                &network_list,
                &notifications_view,
                &window_grid_view,
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

    update_results(&launcher.borrow(), &results, &list, "");
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
    );
    state.entry.grab_focus();
    state.window.present();
}

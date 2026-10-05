//! Switching between the launcher's sub-views (clipboard, snippets, network,
//! processes, preferences, extensions) and filling them (P5.2).

use super::*;

/// Fetch the Ollama model list off the main thread and fill the AI model bar.
pub(crate) fn populate_ai_models(launcher: &Rc<RefCell<Zeshicast>>, view: &crate::ui::AiChatView) {
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
        let _ = tx.send(crate::services::local_ai::list_models(&endpoint));
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
    // M-7: the row's own process, not `cached_top_processes()[row.index()]` --
    // the poller may have reordered the list since the row was painted.
    let Some(process) = crate::ui::row_process(
        &system_monitor_view.displayed_processes.borrow(),
        row.index() as usize,
    ) else {
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

/// The interface a network row stands for, or `None` for other rows (M-7).
fn row_interface_name(row: &gtk::ListBoxRow) -> Option<String> {
    row.widget_name().strip_prefix("iface:").map(str::to_string)
}

/// The SSID a Wi-Fi row stands for, or `None` for other rows.
fn row_wifi_ssid(row: &gtk::ListBoxRow) -> Option<String> {
    row.widget_name().strip_prefix("wifi:").map(str::to_string)
}

pub(crate) fn copy_selected_network_value(list: &ListBox, value: NetworkCopyValue) {
    let Some(row) = list.selected_row() else {
        return;
    };
    // The list holds section headers and Wi-Fi rows, so a row index is not an
    // index into `interfaces` (M-7).
    let Some(name) = row_interface_name(&row) else {
        return;
    };
    let snapshot = crate::services::poll_cache::cached_network_snapshot();
    let Some(interface) = snapshot
        .interfaces
        .iter()
        .find(|interface| interface.name == name)
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
        crate::action::copy_text(&value);
        crate::ui::show_toast_osd(None, "✓ Copied to clipboard");
    }
}

pub(crate) fn activate_selected_network_row(list: &ListBox) {
    if let Some(row) = list.selected_row() {
        activate_network_row(list, &row);
    }
}

pub(crate) fn activate_network_row(list: &ListBox, row: &gtk::ListBoxRow) {
    if let Some(ssid) = row_wifi_ssid(row) {
        if row.has_css_class("network-active") {
            run_command_request("nmcli", ["connection", "down", "id", ssid.as_str()]);
        } else {
            run_command_request("nmcli", ["dev", "wifi", "connect", "--", ssid.as_str()]);
        }
    } else if row_interface_name(row).is_some() {
        copy_selected_network_value(list, NetworkCopyValue::Ip);
    } else if let Some(vpn) = row.widget_name().strip_prefix("vpn:") {
        run_command_request("nmcli", ["connection", "down", "id", vpn]);
    }
}

pub(crate) fn run_selected_network_command(list: &ListBox, value: NetworkCommandValue) {
    let Some(row) = list.selected_row() else {
        return;
    };
    match value {
        NetworkCommandValue::DisconnectInterface => {
            if let Some(name) = row_interface_name(&row) {
                run_command_request("nmcli", ["device", "disconnect", name.as_str()]);
            } else if let Some(ssid) = row_wifi_ssid(&row) {
                run_command_request("nmcli", ["connection", "down", "id", ssid.as_str()]);
            }
        }
        NetworkCommandValue::ConnectWifi => {
            let Some(ssid) = row_wifi_ssid(&row) else {
                return;
            };
            // "--" so an SSID like "-w" is not parsed as an nmcli option.
            run_command_request("nmcli", ["dev", "wifi", "connect", "--", ssid.as_str()]);
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
        // M-15: only a validated cache path may be read as an image; anything
        // else (including forged `\x01zeshicast-image:` text) is plain text.
        if let Some(path) = crate::services::clipboard_store::validated_clipboard_image(&item.value)
        {
            crate::services::clipboard_store::copy_clipboard_image(&path.to_string_lossy());
        } else {
            crate::action::copy_text(&item.value);
        }
        crate::ui::show_toast_osd(None, "✓ Copied to clipboard");
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
        crate::action::copy_text(&item.value);
        crate::ui::show_toast_osd(None, "✓ Copied to clipboard");
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

pub(crate) fn show_root_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
) {
    navigation.reset();
    entry.set_visible(true);
    action_bar.set_visible(true);
    entry.grab_focus();
}

pub(crate) fn apply_status_strip_preferences(
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

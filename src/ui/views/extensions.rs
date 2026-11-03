use std::rc::Rc;

use crate::app::{CommandSummary, ExtensionDetail, ExtensionSummary};
use gtk::prelude::*;
use gtk::{Box as GtkBox, Label, ListBox, Orientation};

#[derive(Clone)]
pub struct ExtensionBrowserView {
    pub root: GtkBox,
    pub list: ListBox,
    /// Extension id behind each list row (`None`: a header or a built-in
    /// command), so Enter can open that extension's screen (Q5.5).
    pub row_extensions: Rc<Vec<Option<String>>>,
}

pub fn extension_browser_view(
    commands: &[CommandSummary],
    details: &[ExtensionDetail],
) -> ExtensionBrowserView {
    let root = super::panel_root(8, 12);
    root.set_vexpand(true);

    // Binaries have no command of their own; list them so a binary-only
    // extension is visible too.
    let mut commands = commands.to_vec();
    commands.extend(details.iter().flat_map(binary_summaries));
    crate::app::sort_command_summaries(&mut commands);
    let commands = commands.as_slice();

    let active_count = commands.iter().filter(|c| c.enabled).count();
    let header_text = format!(
        "Extensions  ·  {} of {} active",
        active_count,
        commands.len()
    );
    let header = super::panel_title(&header_text);
    root.append(&header);

    let list = super::results_list();
    let mut row_extensions = Vec::new();
    let mut current_group = String::new();
    for command in commands {
        let group = command.extension_group();
        if group != current_group {
            current_group = group.clone();
            list.append(&extension_section_header(
                &group,
                &command.extension_detail(),
            ));
            row_extensions.push(None);
        }
        let row = extension_row(command);
        list.append(&row);
        row_extensions.push(command.extension.as_ref().map(|e| e.id.clone()));
    }

    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
    }

    let scroller = super::scrollable_list(&list);
    root.append(&scroller);

    // Q5.4: what extensions asked to do. Re-read whenever the view is shown,
    // so a run that just happened is already there.
    let audit_title = super::panel_title("Permission log");
    root.append(&audit_title);
    let audit_list = super::results_list();
    audit_list.set_selection_mode(gtk::SelectionMode::None);
    let audit_scroller = super::scrollable_list(&audit_list);
    audit_scroller.set_min_content_height(120);
    audit_scroller.set_vexpand(false);
    root.append(&audit_scroller);
    root.connect_map(move |_| fill_audit_list(&audit_list));

    ExtensionBrowserView {
        root,
        list,
        row_extensions: Rc::new(row_extensions),
    }
}

fn binary_summaries(detail: &ExtensionDetail) -> Vec<CommandSummary> {
    detail
        .items
        .iter()
        .filter(|item| item.kind == "Binary")
        .map(|item| CommandSummary {
            kind: "Binary".to_string(),
            name: item.name.clone(),
            category: "Binary".to_string(),
            description: item.blocked.clone().unwrap_or_default(),
            keyword: None,
            icon_name: "application-x-executable-symbolic".to_string(),
            tags: Vec::new(),
            permissions: item.granted.clone(),
            capabilities: item.granted.clone(),
            extension: Some(ExtensionSummary {
                id: detail.id.clone(),
                name: detail.name.clone(),
                version: detail.version.clone(),
                capabilities: detail.manifest.clone(),
            }),
            enabled: item.blocked.is_none(),
        })
        .collect()
}

/// The extension screen (Q5.5): the manifest ceiling, what each item asked
/// for and was granted, why it is blocked, and its latest permission log.
pub fn show_extension_detail_panel(parent: &gtk::ApplicationWindow, detail: &ExtensionDetail) {
    let title = format!("{} {}", detail.name, detail.version);
    let Some(panel) = super::action_panel(parent, &title, 620, 520) else {
        return;
    };
    let root = super::panel_root(8, 14);
    root.append(&super::panel_title(&title));

    let manifest = if detail.manifest.is_empty() {
        "nothing".to_string()
    } else {
        detail.manifest.join(", ")
    };
    let summary = Label::new(Some(&format!(
        "{} · manifest allows: {manifest}",
        detail.id
    )));
    summary.add_css_class("result-subtitle");
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    root.append(&summary);

    let list = super::results_list();
    list.set_selection_mode(gtk::SelectionMode::None);
    for item in &detail.items {
        let mut text = item.grant_line();
        if let Some(reason) = &item.blocked {
            text.push('\n');
            text.push_str(reason);
        }
        list.append(&extension_section_header(
            &format!("{} · {}", item.kind, item.name),
            &text,
        ));
    }
    list.append(&extension_section_header("Permission log", ""));
    let rows = crate::audit::recent(Some(&detail.id), 20);
    if rows.is_empty() {
        list.append(&extension_section_header("No requests yet", ""));
    }
    for row in &rows {
        let (title, subtitle) =
            crate::audit::describe(row, &crate::config::format_time_ago(row.at));
        list.append(&extension_section_header(&title, &subtitle));
    }
    root.append(&super::scrollable_list(&list));
    panel.set_child(Some(&root));

    let key_controller = gtk::EventControllerKey::new();
    {
        let panel = panel.clone();
        key_controller.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                panel.close();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    panel.add_controller(key_controller);
    panel.present();
}

/// Rows shown in the permission log; older ones stay in the database.
const AUDIT_ROWS_SHOWN: usize = 50;

fn fill_audit_list(list: &ListBox) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    let rows = crate::audit::recent(None, AUDIT_ROWS_SHOWN);
    if rows.is_empty() {
        list.append(&extension_section_header(
            "No requests yet",
            "Extension commands, scripts and binaries are logged when they ask to run",
        ));
        return;
    }
    for row in &rows {
        let (title, subtitle) =
            crate::audit::describe(row, &crate::config::format_time_ago(row.at));
        list.append(&extension_section_header(&title, &subtitle));
    }
}

fn extension_row(command: &CommandSummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("result-row");
    if !command.enabled {
        row.add_css_class("extension-disabled");
    }

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_start(14);
    layout.set_margin_end(14);
    layout.set_valign(gtk::Align::Center);

    // 32×32 icon box
    let icon_box = GtkBox::new(Orientation::Vertical, 0);
    icon_box.set_width_request(32);
    icon_box.set_height_request(32);
    icon_box.add_css_class("control-card-icon");
    if command.enabled {
        icon_box.add_css_class("active");
    }
    icon_box.set_valign(gtk::Align::Center);

    let icon = gtk::Image::from_icon_name(&command.icon_name);
    icon.set_pixel_size(18);
    icon.set_halign(gtk::Align::Center);
    icon.set_valign(gtk::Align::Center);
    icon.set_hexpand(true);
    icon.set_vexpand(true);
    icon_box.append(&icon);
    layout.append(&icon_box);

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);

    let title = Label::new(Some(&command.name));
    title.add_css_class("result-title");
    title.set_xalign(0.0);
    title.set_hexpand(true);

    let mut meta = vec![command.kind.clone()];
    if command.category != command.kind {
        meta.push(command.category.clone());
    }
    if let Some(keyword) = &command.keyword {
        meta.push(format!("keyword: {keyword}"));
    }
    if !command.tags.is_empty() {
        meta.push(format!("tags: {}", command.tags.join(", ")));
    }
    let permission_text = if command.permissions.is_empty() {
        "permissions: none".to_string()
    } else {
        format!("permissions: {}", command.permissions.join(", "))
    };
    meta.push(permission_text);

    let meta_text = meta.join(" - ");
    let subtitle_text = if command.description.is_empty() {
        meta_text
    } else {
        format!("{} - {meta_text}", command.description)
    };
    let subtitle = Label::new(Some(&subtitle_text));
    subtitle.add_css_class("result-subtitle");
    subtitle.set_xalign(0.0);
    subtitle.set_hexpand(true);

    text.append(&title);
    text.append(&subtitle);
    layout.append(&text);

    // Toggle switch — toggling changes row opacity visually
    let toggle = gtk::Switch::new();
    toggle.set_active(command.enabled);
    toggle.set_valign(gtk::Align::Center);
    {
        let row_ref = row.clone();
        let icon_box_ref = icon_box.clone();
        toggle.connect_active_notify(move |sw| {
            if sw.is_active() {
                row_ref.remove_css_class("extension-disabled");
                icon_box_ref.add_css_class("active");
            } else {
                row_ref.add_css_class("extension-disabled");
                icon_box_ref.remove_css_class("active");
            }
        });
    }
    layout.append(&toggle);

    row.set_child(Some(&layout));
    row
}

fn extension_section_header(title: &str, detail: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("action-section-row");
    row.set_selectable(false);
    row.set_activatable(false);

    let layout = GtkBox::new(Orientation::Vertical, 1);
    layout.set_margin_start(10);
    layout.set_margin_end(10);
    layout.set_margin_top(6);
    layout.set_margin_bottom(4);

    let title_label = Label::new(Some(title));
    title_label.add_css_class("action-section-label");
    title_label.set_xalign(0.0);

    let detail_label = Label::new(Some(detail));
    detail_label.add_css_class("result-subtitle");
    detail_label.set_xalign(0.0);

    layout.append(&title_label);
    layout.append(&detail_label);
    row.set_child(Some(&layout));
    row
}

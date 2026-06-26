use std::cell::RefCell;
use std::rc::Rc;

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    ApplicationWindow, Box as GtkBox, Button, Entry, EventControllerKey, Label, Orientation,
};

use crate::{Action, Zeshicast};

pub fn show_confirmation_panel<F>(
    parent: &ApplicationWindow,
    title: &str,
    subtitle: &str,
    confirm_label: &str,
    on_confirm: F,
) where
    F: Fn() + 'static,
{
    let Some(panel) = super::action_panel(parent, title, 460, 170) else {
        return;
    };
    let root = super::panel_root(12, 14);
    let header = super::panel_title(title);
    root.append(&header);

    let detail = Label::new(Some(subtitle));
    detail.add_css_class("result-subtitle");
    detail.set_wrap(true);
    detail.set_xalign(0.0);
    detail.set_margin_bottom(8);
    root.append(&detail);

    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);

    let cancel = Button::with_label("Cancel");
    let confirm = Button::with_label(confirm_label);
    confirm.add_css_class("destructive-action");

    buttons.append(&cancel);
    buttons.append(&confirm);
    root.append(&buttons);
    panel.set_child(Some(&root));

    let on_confirm: Rc<dyn Fn()> = Rc::new(on_confirm);
    {
        let panel = panel.clone();
        cancel.connect_clicked(move |_| panel.close());
    }
    {
        let panel = panel.clone();
        let on_confirm = Rc::clone(&on_confirm);
        confirm.connect_clicked(move |_| {
            on_confirm();
            panel.close();
        });
    }
    {
        let panel_keys = panel.clone();
        let on_confirm = Rc::clone(&on_confirm);
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, _, _| match key {
            gdk::Key::Escape => {
                panel_keys.close();
                glib::Propagation::Stop
            }
            gdk::Key::Return | gdk::Key::KP_Enter => {
                on_confirm();
                panel_keys.close();
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
        panel.add_controller(key_controller);
    }

    confirm.grab_focus();
    panel.present();
}

pub fn show_alias_panel(
    parent: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    action: &Action,
) {
    let Some(panel) = super::action_panel(parent, "Set Alias", 420, 130) else {
        return;
    };
    let root = super::panel_root(10, 12);
    let label = super::panel_title(&format!("Alias for {}", action.title));

    let entry = Entry::builder()
        .placeholder_text("Alias")
        .hexpand(true)
        .build();
    entry.add_css_class("search-entry");

    {
        let panel = panel.clone();
        let launcher = Rc::clone(launcher);
        let action = action.clone();
        entry.connect_activate(move |entry| {
            match launcher
                .borrow_mut()
                .set_alias_for_action(entry.text().as_str(), &action)
            {
                Ok(_) => panel.close(),
                Err(error) => eprintln!("failed to save alias: {error}"),
            }
        });
    }

    {
        let panel_for_keys = panel.clone();
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, _, _| match key {
            gdk::Key::Escape => {
                panel_for_keys.close();
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
        panel.add_controller(key_controller);
    }

    root.append(&label);
    root.append(&entry);
    panel.set_child(Some(&root));
    entry.grab_focus();
    panel.present();
}

pub fn show_preferences_editor(parent: &ApplicationWindow, launcher: &Rc<RefCell<Zeshicast>>) {
    let Some(panel) = super::action_panel(parent, "Preferences", 560, 340) else {
        return;
    };
    let root = super::panel_root(12, 16);
    let header = super::panel_title("Preferences");
    root.append(&header);

    let current = launcher.borrow().get_preferences().clone();
    let mut field_entries: Vec<(String, Entry)> = Vec::new();

    for (key, description) in super::preferences::KNOWN_PREFERENCES {
        let row = GtkBox::new(Orientation::Horizontal, 8);

        let label = Label::new(Some(description));
        label.set_width_chars(36);
        label.set_xalign(0.0);
        label.set_halign(gtk::Align::Start);
        row.append(&label);

        let entry = Entry::new();
        entry.set_hexpand(true);
        if let Some(value) = current.get(*key) {
            entry.set_text(value);
        }
        entry.set_placeholder_text(Some(key));
        row.append(&entry);

        field_entries.push((key.to_string(), entry));
        root.append(&row);
    }

    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(4);

    let cancel = Button::with_label("Cancel");
    let save = Button::with_label("Save");
    save.add_css_class("suggested-action");

    buttons.append(&cancel);
    buttons.append(&save);
    root.append(&buttons);

    {
        let panel = panel.clone();
        cancel.connect_clicked(move |_| panel.close());
    }

    {
        let panel = panel.clone();
        let launcher = launcher.clone();
        let field_entries = field_entries.clone();
        save.connect_clicked(move |_| {
            let mut borrow = launcher.borrow_mut();
            for (key, entry) in &field_entries {
                let value = entry.text().to_string();
                if let Err(err) = borrow.set_preference(key.clone(), value) {
                    eprintln!("failed to save preference {key}: {err}");
                }
            }
            panel.close();
        });
    }

    {
        let panel_keys = panel.clone();
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, _, _| match key {
            gdk::Key::Escape => {
                panel_keys.close();
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
        panel.add_controller(key_controller);
    }

    panel.set_child(Some(&root));
    panel.present();
}

pub fn show_extension_browser(parent: &ApplicationWindow, launcher: &Rc<RefCell<Zeshicast>>) {
    let commands = launcher.borrow().list_commands();

    let Some(panel) = super::action_panel(parent, "Extension Browser", 600, 400) else {
        return;
    };
    let root = super::panel_root(8, 12);
    let header = super::panel_title("Extensions");
    root.append(&header);

    let list = super::results_list();

    for cmd in &commands {
        list.append(&super::secondary_action_row(&cmd.icon_name, &cmd.name));
    }

    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
    }

    {
        let panel = panel.clone();
        list.connect_row_activated(move |_, _| {
            panel.close();
        });
    }

    {
        let panel_keys = panel.clone();
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, _, _| match key {
            gdk::Key::Escape => {
                panel_keys.close();
                glib::Propagation::Stop
            }
            gdk::Key::Return | gdk::Key::KP_Enter => {
                panel_keys.close();
                glib::Propagation::Stop
            }
            gdk::Key::Down => glib::Propagation::Proceed,
            gdk::Key::Up => glib::Propagation::Proceed,
            _ => glib::Propagation::Proceed,
        });
        panel.add_controller(key_controller);
    }

    root.append(&list);
    panel.set_child(Some(&root));
    panel.present();
}

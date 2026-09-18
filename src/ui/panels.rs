use std::cell::RefCell;
use std::rc::Rc;

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    ApplicationWindow, Box as GtkBox, Button, Entry, EventControllerKey, Label, Orientation,
};

use crate::action::Action;
use crate::app::Zeshicast;

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

pub fn show_snippet_editor_panel<F>(
    parent: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    snippet_id: Option<i64>,
    initial_title: &str,
    initial_prefix: &str,
    initial_content: &str,
    on_saved: F,
) where
    F: Fn() + 'static,
{
    let panel_title = if snippet_id.is_some() {
        "Edit Snippet"
    } else {
        "New Snippet"
    };
    let Some(panel) = super::action_panel(parent, panel_title, 480, 260) else {
        return;
    };
    let root = super::panel_root(10, 12);
    let title_label = super::panel_title(panel_title);
    root.append(&title_label);

    let title_entry = Entry::builder()
        .placeholder_text("Snippet Name / Title")
        .text(initial_title)
        .hexpand(true)
        .build();
    title_entry.add_css_class("search-entry");

    let prefix_entry = Entry::builder()
        .placeholder_text("Keyword / Prefix (e.g. :shrug:)")
        .text(initial_prefix)
        .hexpand(true)
        .build();
    prefix_entry.add_css_class("search-entry");

    let content_entry = Entry::builder()
        .placeholder_text("Snippet Content (supports {{clipboard}}, {{date}})")
        .text(initial_content)
        .hexpand(true)
        .build();
    content_entry.add_css_class("search-entry");

    root.append(&title_entry);
    root.append(&prefix_entry);
    root.append(&content_entry);

    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(8);

    let cancel = Button::with_label("Cancel");
    let save = Button::with_label("Save");
    save.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&save);
    root.append(&buttons);

    let on_saved: Rc<dyn Fn()> = Rc::new(on_saved);
    {
        let panel = panel.clone();
        cancel.connect_clicked(move |_| panel.close());
    }
    {
        let panel = panel.clone();
        let launcher = Rc::clone(launcher);
        let on_saved = Rc::clone(&on_saved);
        let title_entry = title_entry.clone();
        let prefix_entry = prefix_entry.clone();
        let content_entry = content_entry.clone();
        save.connect_clicked(move |_| {
            let title = title_entry.text();
            let prefix = prefix_entry.text();
            let content = content_entry.text();
            if !title.trim().is_empty() && !content.trim().is_empty() {
                if let Err(error) = launcher.borrow_mut().save_snippet(
                    snippet_id,
                    title.as_str(),
                    prefix.as_str(),
                    content.as_str(),
                    &[],
                ) {
                    eprintln!("failed to save snippet: {error}");
                } else {
                    on_saved();
                    panel.close();
                }
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

    panel.set_child(Some(&root));
    title_entry.grab_focus();
    panel.present();
}

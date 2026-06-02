use std::collections::HashMap;
use std::rc::Rc;

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    ApplicationWindow, Box as GtkBox, Button, CheckButton, DropDown, Entry, EventControllerKey,
    Orientation, StringList,
};

use crate::{Action, ActionFormField, CommandArgumentKind};

pub fn show_form_panel<F>(parent: &ApplicationWindow, action: Action, on_submit: F)
where
    F: Fn(Action, HashMap<String, String>) + 'static,
{
    let Some(form) = action.form_data() else {
        return;
    };
    let Some(panel) = super::action_panel(
        parent,
        &format!("{} — Fill arguments", form.name),
        480,
        60 + form.fields.len() as i32 * 60,
    ) else {
        return;
    };
    let root = super::panel_root(10, 14);
    let header = super::panel_title(&form.name);
    root.append(&header);

    let mut field_widgets: Vec<(String, gtk::Widget)> = Vec::new();
    let mut first_widget: Option<gtk::Widget> = None;

    for field in &form.fields {
        let row = GtkBox::new(Orientation::Horizontal, 8);

        let label = gtk::Label::new(Some(&field.name));
        label.set_width_chars(14);
        label.set_xalign(1.0);
        row.append(&label);

        let widget = form_widget(field);

        if first_widget.is_none() {
            first_widget = Some(widget.clone());
        }

        row.append(&widget);
        root.append(&row);
        field_widgets.push((field.name.clone(), widget));
    }

    let btn_row = GtkBox::new(Orientation::Horizontal, 8);
    let submit_btn = Button::builder().label("Run").build();
    btn_row.append(&submit_btn);
    root.append(&btn_row);

    panel.set_child(Some(&root));

    let on_submit: Rc<dyn Fn(Action, HashMap<String, String>)> = Rc::new(on_submit);
    let form_fields = form.fields.clone();

    {
        let panel = panel.clone();
        let action = action.clone();
        let field_widgets = field_widgets.clone();
        let form_fields = form_fields.clone();
        let on_submit = Rc::clone(&on_submit);
        submit_btn.connect_clicked(move |_| {
            let values = collect_form_values(&field_widgets, &form_fields);
            on_submit(action.clone(), values);
            panel.close();
        });
    }

    {
        let panel_keys = panel.clone();
        let action = action.clone();
        let field_widgets = field_widgets.clone();
        let on_submit = Rc::clone(&on_submit);
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, _, _| match key {
            gdk::Key::Escape => {
                panel_keys.close();
                glib::Propagation::Stop
            }
            gdk::Key::Return | gdk::Key::KP_Enter => {
                let values = collect_form_values(&field_widgets, &form_fields);
                on_submit(action.clone(), values);
                panel_keys.close();
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
        panel.add_controller(key_controller);
    }

    if let Some(w) = first_widget {
        w.grab_focus();
    }
    panel.present();
}

fn form_widget(field: &ActionFormField) -> gtk::Widget {
    match field.kind {
        CommandArgumentKind::Bool => CheckButton::builder()
            .label(&field.name)
            .active(
                field.current_value == "true"
                    || field.current_value == "1"
                    || field.current_value == "yes",
            )
            .build()
            .upcast(),
        CommandArgumentKind::Enum if !field.options.is_empty() => {
            let string_list = StringList::new(
                field
                    .options
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice(),
            );
            let selected = field
                .options
                .iter()
                .position(|o| o == &field.current_value)
                .unwrap_or(0) as u32;
            let dropdown = DropDown::new(Some(string_list), gtk::Expression::NONE);
            dropdown.set_selected(selected);
            dropdown.set_hexpand(true);
            dropdown.upcast()
        }
        _ => {
            let entry = Entry::builder()
                .placeholder_text(&field.name)
                .text(&field.current_value)
                .hexpand(true)
                .build();
            entry.add_css_class("search-entry");
            entry.upcast()
        }
    }
}

fn collect_form_values(
    field_widgets: &[(String, gtk::Widget)],
    form_fields: &[ActionFormField],
) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for (name, widget) in field_widgets {
        let value = if let Some(checkbox) = widget.downcast_ref::<CheckButton>() {
            if checkbox.is_active() {
                "true"
            } else {
                "false"
            }
            .to_string()
        } else if let Some(dropdown) = widget.downcast_ref::<DropDown>() {
            form_fields
                .iter()
                .find(|field| &field.name == name)
                .and_then(|field| field.options.get(dropdown.selected() as usize))
                .cloned()
                .unwrap_or_default()
        } else if let Some(entry) = widget.downcast_ref::<Entry>() {
            entry.text().to_string()
        } else {
            String::new()
        };
        values.insert(name.clone(), value);
    }
    values
}

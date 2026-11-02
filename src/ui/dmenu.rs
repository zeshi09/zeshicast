//! The `--dmenu` window (Q1.4). Toolkit-free logic is in `crate::dmenu`.
//!
//! Runs as its own short-lived, non-unique GTK application: it never talks to
//! the daemon, so it works whether one is running or not and cannot disturb
//! the palette, and its stdin/stdout are this process's own.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Entry, Label, ListBox, ListBoxRow, ScrolledWindow, gdk, gio,
    glib,
};

use crate::dmenu::{
    DmenuOptions, EXIT_CANCELLED, EXIT_CHOSEN, enter_output, filter_items, has_input,
};

const DMENU_APP_ID: &str = "dev.zeshi.Zeshicast.Dmenu";
/// Rows actually built; filtering still covers every item.
const MAX_ROWS: usize = 200;

/// Show the picker and return the process exit code. The choice, if any, has
/// been written to stdout by then.
pub fn run_dmenu(options: DmenuOptions, items: Vec<String>) -> i32 {
    if !has_input(&items, &options) {
        return EXIT_CANCELLED;
    }

    let app = Application::builder()
        .application_id(DMENU_APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let exit = Rc::new(Cell::new(EXIT_CANCELLED));
    let picker = Rc::new(Picker {
        items,
        options,
        shown: RefCell::new(Vec::new()),
    });
    {
        let exit = Rc::clone(&exit);
        app.connect_activate(move |app| build_window(app, &picker, &exit));
    }
    // Our flags are not GApplication's: do not let it parse them.
    app.run_with_args::<&str>(&[]);
    exit.get()
}

struct Picker {
    items: Vec<String>,
    options: DmenuOptions,
    /// Item index behind each visible row, in row order.
    shown: RefCell<Vec<usize>>,
}

fn build_window(app: &Application, picker: &Rc<Picker>, exit: &Rc<Cell<i32>>) {
    crate::ui::install_css();

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Zeshicast")
        .default_width(640)
        .default_height(if picker.options.password { 80 } else { 440 })
        .resizable(false)
        .decorated(false)
        .build();
    window.add_css_class("launcher-window");
    super::layer_shell::configure_launcher(&window);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("launcher-frame");

    let entry = Entry::builder()
        .placeholder_text(picker.options.prompt.as_deref().unwrap_or("Filter…"))
        .hexpand(true)
        .build();
    entry.add_css_class("search-input");
    let search_bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    search_bar.add_css_class("search-bar");
    search_bar.append(&entry);
    root.append(&search_bar);

    let list = ListBox::new();
    list.add_css_class("results-list");
    list.set_vexpand(true);
    let scroller = crate::ui::scrollable_list(&list);

    if picker.options.password {
        entry.set_visibility(false);
        entry.set_input_purpose(gtk::InputPurpose::Password);
    } else {
        root.append(&scroller);
        refresh(&list, picker, "");
        let list = list.clone();
        let picker = Rc::clone(picker);
        entry.connect_changed(move |entry| refresh(&list, &picker, entry.text().as_str()));
    }
    window.set_child(Some(&root));

    let choose = {
        let window = window.clone();
        let entry = entry.clone();
        let list = list.clone();
        let picker = Rc::clone(picker);
        let exit = Rc::clone(exit);
        Rc::new(move || {
            let selected = list
                .selected_row()
                .and_then(|row| usize::try_from(row.index()).ok())
                .and_then(|row| picker.shown.borrow().get(row).copied());
            let query = entry.text();
            if let Some(output) =
                enter_output(&picker.items, selected, query.as_str(), &picker.options)
            {
                println!("{output}");
                exit.set(EXIT_CHOSEN);
                window.close();
            }
        })
    };
    {
        let choose = Rc::clone(&choose);
        entry.connect_activate(move |_| choose());
    }
    {
        let choose = Rc::clone(&choose);
        list.connect_row_activated(move |list, row| {
            list.select_row(Some(row));
            choose();
        });
    }

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let window = window.clone();
        keys.connect_key_pressed(move |_, key, _, _| match key {
            gdk::Key::Escape => {
                window.close();
                glib::Propagation::Stop
            }
            gdk::Key::Down | gdk::Key::KP_Down => {
                move_selection(&list, &scroller, 1);
                glib::Propagation::Stop
            }
            gdk::Key::Up | gdk::Key::KP_Up => {
                move_selection(&list, &scroller, -1);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
    }
    window.add_controller(keys);

    window.present();
    entry.grab_focus();
}

/// Rebuild the rows for `query` and select the best match.
fn refresh(list: &ListBox, picker: &Picker, query: &str) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    let mut shown = filter_items(&picker.items, query);
    shown.truncate(MAX_ROWS);
    for &index in &shown {
        list.append(&item_row(&picker.items[index]));
    }
    *picker.shown.borrow_mut() = shown;
    list.select_row(list.row_at_index(0).as_ref());
}

fn item_row(text: &str) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("result-row");
    let label = Label::new(Some(text));
    label.add_css_class("result-title");
    label.set_xalign(0.0);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_margin_top(10);
    label.set_margin_bottom(10);
    label.set_margin_start(12);
    label.set_margin_end(12);
    row.set_child(Some(&label));
    row
}

/// Move the selection by `step` rows (clamped) and keep it in view, while the
/// keyboard focus stays in the entry.
fn move_selection(list: &ListBox, scroller: &ScrolledWindow, step: i32) {
    let current = list.selected_row().map_or(-1, |row| row.index());
    let Some(row) = list
        .row_at_index((current + step).max(0))
        .or_else(|| list.selected_row())
    else {
        return;
    };
    list.select_row(Some(&row));

    if let Some(bounds) = row.compute_bounds(list) {
        let adjustment = scroller.vadjustment();
        let top = f64::from(bounds.y());
        let bottom = top + f64::from(bounds.height());
        if top < adjustment.value() {
            adjustment.set_value(top);
        } else if bottom > adjustment.value() + adjustment.page_size() {
            adjustment.set_value(bottom - adjustment.page_size());
        }
    }
}

use std::cell::RefCell;
use std::rc::Rc;

use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, CheckButton, CssProvider, DropDown,
    Entry, EventControllerKey, Image, Label, ListBox, ListBoxRow, Orientation, StringList,
    STYLE_PROVIDER_PRIORITY_APPLICATION,
};
use zeshicast::{Action, CommandArgumentKind, SecondaryActionKind, Zeshicast};

const APP_ID: &str = "dev.zeshi.Zeshicast";

// GTK layer shell enum values from the C headers
#[cfg(feature = "layer-shell")]
const GTK_LAYER_SHELL_LAYER_OVERLAY: u32 = 3;
#[cfg(feature = "layer-shell")]
const GTK_LAYER_SHELL_KEYBOARD_MODE_EXCLUSIVE: u32 = 1;

#[cfg(feature = "layer-shell")]
unsafe extern "C" {
    fn gtk_layer_is_supported() -> i32;
    fn gtk_layer_init_for_window(window: *mut std::ffi::c_void);
    fn gtk_layer_set_layer(window: *mut std::ffi::c_void, layer: u32);
    fn gtk_layer_set_keyboard_mode(window: *mut std::ffi::c_void, mode: u32);
}

#[cfg(feature = "layer-shell")]
fn configure_layer_shell(window: &ApplicationWindow) {
    use gtk::prelude::*;
    let ptr = window.as_ptr() as *mut std::ffi::c_void;
    unsafe {
        if gtk_layer_is_supported() == 0 {
            return;
        }
        gtk_layer_init_for_window(ptr);
        gtk_layer_set_layer(ptr, GTK_LAYER_SHELL_LAYER_OVERLAY);
        gtk_layer_set_keyboard_mode(ptr, GTK_LAYER_SHELL_KEYBOARD_MODE_EXCLUSIVE);
    }
}

#[cfg(not(feature = "layer-shell"))]
fn configure_layer_shell(_window: &ApplicationWindow) {}

fn main() -> glib::ExitCode {
    let state = Rc::new(RefCell::new(None::<GuiState>));
    let hold = Rc::new(RefCell::new(None::<gio::ApplicationHoldGuard>));

    let app = Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    app.connect_startup(|_| install_css());
    {
        let state = Rc::clone(&state);
        let hold = Rc::clone(&hold);
        app.connect_command_line(move |app, command_line| {
            let args: Vec<String> = command_line
                .arguments()
                .into_iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect();

            if args.iter().any(|arg| arg == "--help" || arg == "-h") {
                println!("{}", help_text());
                return glib::ExitCode::SUCCESS;
            }

            if args.iter().any(|arg| arg == "--quit") {
                app.quit();
                return glib::ExitCode::SUCCESS;
            }

            let daemon = args.iter().any(|arg| arg == "--daemon");
            ensure_ui(app, &state, &hold, daemon);

            if !daemon {
                if let Some(state) = state.borrow().as_ref() {
                    present_launcher(state);
                }
            }

            glib::ExitCode::SUCCESS
        });
    }

    app.run()
}

#[derive(Clone)]
struct GuiState {
    launcher: Rc<RefCell<Zeshicast>>,
    results: Rc<RefCell<Vec<Action>>>,
    window: ApplicationWindow,
    entry: Entry,
    list: ListBox,
}

fn ensure_ui(
    app: &Application,
    state: &Rc<RefCell<Option<GuiState>>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    daemon: bool,
) {
    if daemon && hold.borrow().is_none() {
        *hold.borrow_mut() = Some(app.hold());
    }

    if state.borrow().is_none() {
        let gui = build_ui(app, hold);
        if daemon {
            gui.window.hide();
        } else {
            present_launcher(&gui);
        }
        *state.borrow_mut() = Some(gui);
    }
}

fn build_ui(app: &Application, hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>) -> GuiState {
    let launcher = Rc::new(RefCell::new(Zeshicast::load()));
    let results = Rc::new(RefCell::new(Vec::<Action>::new()));
    install_clipboard_monitor(&launcher);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Zeshicast")
        .default_width(760)
        .default_height(520)
        .resizable(false)
        .decorated(false)
        .build();
    window.add_css_class("launcher-window");
    configure_layer_shell(&window);

    let root = GtkBox::new(Orientation::Vertical, 10);
    root.set_margin_top(14);
    root.set_margin_bottom(14);
    root.set_margin_start(14);
    root.set_margin_end(14);

    let entry = Entry::builder()
        .placeholder_text("Search apps, files, clipboard, snippets, quicklinks, or type calc 2 + 2")
        .hexpand(true)
        .build();
    entry.add_css_class("search-entry");

    let list = ListBox::new();
    list.add_css_class("results-list");
    list.set_vexpand(true);
    list.set_activate_on_single_click(false);

    let action_bar = action_bar(&window, &launcher, &entry, &list, &results, hold);

    root.append(&entry);
    root.append(&list);
    root.append(&action_bar);
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
        list.connect_row_activated(move |_, row| {
            if let Some(action) = results.borrow().get(row.index() as usize).cloned() {
                if action.form_data().is_some() {
                    show_form_panel(
                        &window,
                        &launcher,
                        &hold,
                        &entry,
                        &list_ref,
                        &results,
                        action,
                    );
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
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, _, state| {
            handle_key(
                &controller_window,
                &launcher,
                &hold,
                &entry,
                &list,
                &results,
                key,
                state,
            )
        });
        window.add_controller(key_controller);
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
    }
}

fn present_launcher(state: &GuiState) {
    state.entry.set_text("");
    update_results(
        &state.launcher.borrow(),
        &state.results,
        &state.list,
        state.entry.text().as_str(),
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
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let actions = launcher.search(query);
    for action in &actions {
        list.append(&result_row(action));
    }

    *results.borrow_mut() = actions;
    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
    }
}

fn result_row(action: &Action) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 12);
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(10);
    layout.set_margin_end(10);

    let icon = Image::from_icon_name(&action.icon_name);
    icon.add_css_class("result-icon");
    icon.set_pixel_size(24);

    let text = GtkBox::new(Orientation::Vertical, 2);
    text.set_hexpand(true);

    let title_row = GtkBox::new(Orientation::Horizontal, 8);
    let title = Label::new(Some(&action.title));
    title.add_css_class("result-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    title.set_hexpand(true);

    let category = Label::new(Some(&action.category));
    category.add_css_class("category-pill");
    category.set_xalign(0.5);

    title_row.append(&title);
    title_row.append(&category);

    let subtitle = Label::new(Some(&action.subtitle));
    subtitle.add_css_class("result-subtitle");
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    subtitle.set_xalign(0.0);
    subtitle.set_hexpand(true);

    text.append(&title_row);
    text.append(&subtitle);

    layout.append(&icon);
    layout.append(&text);
    row.set_child(Some(&layout));
    row
}

fn handle_key(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    entry: &Entry,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    key: gdk::Key,
    state: gdk::ModifierType,
) -> glib::Propagation {
    match key {
        gdk::Key::Escape => {
            finish_interaction(window, hold);
            glib::Propagation::Stop
        }
        gdk::Key::Return | gdk::Key::KP_Enter => {
            if state.contains(gdk::ModifierType::CONTROL_MASK) {
                copy_selected(list, results);
            } else {
                run_selected(window, launcher, hold, entry, list, results);
            }
            glib::Propagation::Stop
        }
        gdk::Key::k if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_action_panel(window, launcher, entry, list, results);
            glib::Propagation::Stop
        }
        gdk::Key::b if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_extension_browser(window, launcher);
            glib::Propagation::Stop
        }
        gdk::Key::comma if state.contains(gdk::ModifierType::CONTROL_MASK) => {
            show_preferences_editor(window, launcher);
            glib::Propagation::Stop
        }
        gdk::Key::Down => {
            move_selection(list, 1);
            glib::Propagation::Stop
        }
        gdk::Key::Up => {
            move_selection(list, -1);
            glib::Propagation::Stop
        }
        _ => glib::Propagation::Proceed,
    }
}

fn action_bar(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    entry: &Entry,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
) -> GtkBox {
    let bar = GtkBox::new(Orientation::Horizontal, 8);
    bar.add_css_class("action-bar");

    let run = icon_button("media-playback-start-symbolic", "Run");
    let copy = icon_button("edit-copy-symbolic", "Copy value");
    let folder = icon_button("folder-open-symbolic", "Open containing folder");
    let pin = icon_button("view-pin-symbolic", "Pin or unpin");

    {
        let window = window.clone();
        let launcher = Rc::clone(launcher);
        let hold = Rc::clone(hold);
        let entry = entry.clone();
        let list = list.clone();
        let results = Rc::clone(results);
        run.connect_clicked(
            move |_| run_selected(&window, &launcher, &hold, &entry, &list, &results),
        );
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

    bar.append(&run);
    bar.append(&copy);
    bar.append(&folder);
    bar.append(&pin);
    bar
}

fn icon_button(icon_name: &str, tooltip: &str) -> Button {
    let button = Button::builder().icon_name(icon_name).build();
    button.add_css_class("action-button");
    button.set_tooltip_text(Some(tooltip));
    button
}

fn run_selected(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    entry: &Entry,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
) {
    if let Some(action) = selected_action(list, results) {
        if action.form_data().is_some() {
            show_form_panel(window, launcher, hold, entry, list, results, action);
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
    results.borrow().get(row.index() as usize).cloned()
}

fn show_action_panel(
    parent: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    entry: &Entry,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
) {
    let Some(action) = selected_action(list, results) else {
        return;
    };
    let Some(app) = parent.application() else {
        return;
    };

    let secondary_actions = Rc::new(launcher.borrow().available_secondary_actions(&action));
    let panel = ApplicationWindow::builder()
        .application(&app)
        .title("Actions")
        .transient_for(parent)
        .default_width(420)
        .default_height(280)
        .resizable(false)
        .decorated(false)
        .build();
    panel.add_css_class("action-panel");

    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let header = Label::new(Some(&action.title));
    header.add_css_class("action-panel-title");
    header.set_ellipsize(gtk::pango::EllipsizeMode::End);
    header.set_xalign(0.0);

    let action_list = ListBox::new();
    action_list.add_css_class("results-list");
    action_list.set_vexpand(true);
    action_list.set_activate_on_single_click(false);

    for secondary in secondary_actions.iter() {
        action_list.append(&secondary_action_row(
            &secondary.icon_name,
            &secondary.title,
        ));
    }
    action_list.append(&secondary_action_row("insert-link-symbolic", "Set Alias"));
    if let Some(row) = action_list.row_at_index(0) {
        action_list.select_row(Some(&row));
    }

    {
        let panel = panel.clone();
        let launcher = Rc::clone(launcher);
        let entry = entry.clone();
        let list = list.clone();
        let results = Rc::clone(results);
        let action = action.clone();
        let secondary_actions = Rc::clone(&secondary_actions);
        action_list.connect_row_activated(move |_, row| {
            let index = row.index() as usize;
            if let Some(secondary) = secondary_actions.get(index) {
                if let Err(error) = launcher
                    .borrow_mut()
                    .run_secondary_action(&action, secondary.kind)
                {
                    eprintln!("failed to run action: {error}");
                }
                update_results(&launcher.borrow(), &results, &list, entry.text().as_str());
                panel.close();
            } else if index == secondary_actions.len() {
                show_alias_panel(&panel, &launcher, &action);
            }
        });
    }

    {
        let panel_for_keys = panel.clone();
        let action_list = action_list.clone();
        let launcher = Rc::clone(launcher);
        let entry = entry.clone();
        let list = list.clone();
        let results = Rc::clone(results);
        let action = action.clone();
        let secondary_actions = Rc::clone(&secondary_actions);
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, _, _| match key {
            gdk::Key::Escape => {
                panel_for_keys.close();
                glib::Propagation::Stop
            }
            gdk::Key::Return | gdk::Key::KP_Enter => {
                if let Some(row) = action_list.selected_row() {
                    let index = row.index() as usize;
                    if let Some(secondary) = secondary_actions.get(index) {
                        if let Err(error) = launcher
                            .borrow_mut()
                            .run_secondary_action(&action, secondary.kind)
                        {
                            eprintln!("failed to run action: {error}");
                        }
                        update_results(&launcher.borrow(), &results, &list, entry.text().as_str());
                        panel_for_keys.close();
                    } else if index == secondary_actions.len() {
                        show_alias_panel(&panel_for_keys, &launcher, &action);
                    }
                }
                glib::Propagation::Stop
            }
            gdk::Key::Down => {
                move_selection(&action_list, 1);
                glib::Propagation::Stop
            }
            gdk::Key::Up => {
                move_selection(&action_list, -1);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
        panel.add_controller(key_controller);
    }

    root.append(&header);
    root.append(&action_list);
    panel.set_child(Some(&root));
    panel.present();
}

fn show_alias_panel(
    parent: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    action: &Action,
) {
    let Some(app) = parent.application() else {
        return;
    };

    let panel = ApplicationWindow::builder()
        .application(&app)
        .title("Set Alias")
        .transient_for(parent)
        .default_width(420)
        .default_height(130)
        .resizable(false)
        .decorated(false)
        .build();
    panel.add_css_class("action-panel");

    let root = GtkBox::new(Orientation::Vertical, 10);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let label = Label::new(Some(&format!("Alias for {}", action.title)));
    label.add_css_class("action-panel-title");
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_xalign(0.0);

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

fn secondary_action_row(icon_name: &str, title: &str) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("result-row");

    let layout = GtkBox::new(Orientation::Horizontal, 10);
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(10);
    layout.set_margin_end(10);

    let icon = Image::from_icon_name(icon_name);
    icon.set_pixel_size(20);
    icon.add_css_class("result-icon");

    let label = Label::new(Some(title));
    label.add_css_class("result-title");
    label.set_xalign(0.0);
    label.set_hexpand(true);

    layout.append(&icon);
    layout.append(&label);
    row.set_child(Some(&layout));
    row
}

fn show_form_panel(
    parent: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    search_entry: &Entry,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    action: Action,
) {
    let Some(form) = action.form_data() else {
        return;
    };
    let Some(app) = parent.application() else {
        return;
    };

    let panel = ApplicationWindow::builder()
        .application(&app)
        .title(&format!("{} — Fill arguments", form.name))
        .transient_for(parent)
        .default_width(480)
        .default_height(60 + form.fields.len() as i32 * 60)
        .resizable(false)
        .decorated(false)
        .build();
    panel.add_css_class("action-panel");

    let root = GtkBox::new(Orientation::Vertical, 10);
    root.set_margin_top(14);
    root.set_margin_bottom(14);
    root.set_margin_start(14);
    root.set_margin_end(14);

    let header = Label::new(Some(&form.name));
    header.add_css_class("action-panel-title");
    header.set_xalign(0.0);
    root.append(&header);

    let mut field_widgets: Vec<(String, gtk::Widget)> = Vec::new();
    let mut first_widget: Option<gtk::Widget> = None;

    for field in &form.fields {
        let row = GtkBox::new(Orientation::Horizontal, 8);

        let label = Label::new(Some(&field.name));
        label.set_width_chars(14);
        label.set_xalign(1.0);
        row.append(&label);

        let widget: gtk::Widget = match field.kind {
            CommandArgumentKind::Bool => {
                let btn = CheckButton::builder()
                    .label(&field.name)
                    .active(
                        field.current_value == "true"
                            || field.current_value == "1"
                            || field.current_value == "yes",
                    )
                    .build();
                btn.upcast()
            }
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
                let dd = DropDown::new(Some(string_list), gtk::Expression::NONE);
                dd.set_selected(selected);
                dd.set_hexpand(true);
                dd.upcast()
            }
            _ => {
                let e = Entry::builder()
                    .placeholder_text(&field.name)
                    .text(&field.current_value)
                    .hexpand(true)
                    .build();
                e.add_css_class("search-entry");
                e.upcast()
            }
        };

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

    let collect_values = {
        let field_widgets = field_widgets.clone();
        let form_fields = form.fields.clone();
        move || -> std::collections::HashMap<String, String> {
            let mut values = std::collections::HashMap::new();
            for (name, widget) in &field_widgets {
                let field_kind = form_fields
                    .iter()
                    .find(|f| &f.name == name)
                    .map(|f| f.kind)
                    .unwrap_or(CommandArgumentKind::Text);
                let value = if let Some(cb) = widget.downcast_ref::<CheckButton>() {
                    if cb.is_active() { "true" } else { "false" }.to_string()
                } else if let Some(dd) = widget.downcast_ref::<DropDown>() {
                    let form_field = form_fields.iter().find(|f| &f.name == name);
                    form_field
                        .and_then(|f| f.options.get(dd.selected() as usize))
                        .cloned()
                        .unwrap_or_default()
                } else if let Some(e) = widget.downcast_ref::<Entry>() {
                    e.text().to_string()
                } else {
                    String::new()
                };
                let _ = field_kind;
                values.insert(name.clone(), value);
            }
            values
        }
    };

    {
        let panel = panel.clone();
        let parent = parent.clone();
        let launcher = Rc::clone(launcher);
        let hold = Rc::clone(hold);
        let search_entry = search_entry.clone();
        let list = list.clone();
        let results = Rc::clone(results);
        let action = action.clone();
        let collect = collect_values.clone();
        submit_btn.connect_clicked(move |_| {
            let values = collect();
            launcher.borrow_mut().run_form_action(&action, values);
            update_results(
                &launcher.borrow(),
                &results,
                &list,
                search_entry.text().as_str(),
            );
            finish_interaction(&parent, &hold);
            panel.close();
        });
    }

    {
        let panel_keys = panel.clone();
        let parent = parent.clone();
        let launcher = Rc::clone(launcher);
        let hold = Rc::clone(hold);
        let search_entry = search_entry.clone();
        let list = list.clone();
        let results = Rc::clone(results);
        let action = action.clone();
        let collect = collect_values;
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, _, _| match key {
            gdk::Key::Escape => {
                panel_keys.close();
                glib::Propagation::Stop
            }
            gdk::Key::Return | gdk::Key::KP_Enter => {
                let values = collect();
                launcher.borrow_mut().run_form_action(&action, values);
                update_results(
                    &launcher.borrow(),
                    &results,
                    &list,
                    search_entry.text().as_str(),
                );
                finish_interaction(&parent, &hold);
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

fn show_preferences_editor(parent: &ApplicationWindow, launcher: &Rc<RefCell<Zeshicast>>) {
    let Some(app) = parent.application() else {
        return;
    };

    const KNOWN_KEYS: &[(&str, &str)] = &[
        ("ai_endpoint", "AI endpoint (OpenAI-compatible URL)"),
        ("ai_model", "AI model"),
        ("ai_api_key", "AI API key"),
        ("translate_endpoint", "Translate endpoint (LibreTranslate URL)"),
        ("translate_api_key", "Translate API key"),
        ("translate_target", "Translate target language (e.g. en, ru, de)"),
    ];

    let panel = ApplicationWindow::builder()
        .application(&app)
        .title("Preferences")
        .transient_for(parent)
        .default_width(560)
        .default_height(340)
        .resizable(false)
        .decorated(false)
        .build();
    panel.add_css_class("action-panel");

    let root = GtkBox::new(Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);

    let header = Label::new(Some("Preferences"));
    header.add_css_class("action-panel-title");
    header.set_xalign(0.0);
    root.append(&header);

    let current = launcher.borrow().get_preferences().clone();
    let mut field_entries: Vec<(String, Entry)> = Vec::new();

    for (key, description) in KNOWN_KEYS {
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

fn show_extension_browser(parent: &ApplicationWindow, launcher: &Rc<RefCell<Zeshicast>>) {
    let Some(app) = parent.application() else {
        return;
    };
    let commands = launcher.borrow().list_commands();

    let panel = ApplicationWindow::builder()
        .application(&app)
        .title("Extension Browser")
        .transient_for(parent)
        .default_width(600)
        .default_height(400)
        .resizable(false)
        .decorated(false)
        .build();
    panel.add_css_class("action-panel");

    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let header = Label::new(Some("Extensions"));
    header.add_css_class("action-panel-title");
    header.set_xalign(0.0);
    root.append(&header);

    let list = ListBox::new();
    list.add_css_class("results-list");
    list.set_vexpand(true);
    list.set_activate_on_single_click(false);

    for cmd in &commands {
        let subtitle = if !cmd.description.is_empty() {
            cmd.description.clone()
        } else {
            cmd.keyword
                .as_deref()
                .unwrap_or_default()
                .to_string()
        };
        list.append(&secondary_action_row(&cmd.icon_name, &cmd.name));
        let _ = subtitle;
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
            gdk::Key::Down => {
                glib::Propagation::Proceed
            }
            gdk::Key::Up => {
                glib::Propagation::Proceed
            }
            _ => glib::Propagation::Proceed,
        });
        panel.add_controller(key_controller);
    }

    root.append(&list);
    panel.set_child(Some(&root));
    panel.present();
}

fn move_selection(list: &ListBox, delta: i32) {
    let current = list.selected_row().map(|row| row.index()).unwrap_or(0);
    let next = (current + delta).max(0);
    if let Some(row) = list.row_at_index(next) {
        list.select_row(Some(&row));
    }
}

fn install_css() {
    let provider = CssProvider::new();
    provider.load_from_data(
        "
        .launcher-window {
          background: alpha(@window_bg_color, 0.98);
          border: 1px solid alpha(@accent_color, 0.35);
          border-radius: 8px;
        }

        .action-panel {
          background: alpha(@window_bg_color, 0.99);
          border: 1px solid alpha(@accent_color, 0.35);
          border-radius: 8px;
        }

        .action-panel-title {
          font-size: 16px;
          font-weight: 600;
        }

        .search-entry {
          min-height: 44px;
          font-size: 18px;
          border-radius: 8px;
          padding: 0 12px;
        }

        .results-list {
          background: transparent;
        }

        .result-row {
          border-radius: 7px;
        }

        .result-row:selected {
          background: alpha(@accent_color, 0.22);
        }

        .category-pill {
          color: alpha(@window_fg_color, 0.7);
          font-size: 12px;
          padding: 1px 6px;
          border-radius: 6px;
          background: alpha(@window_fg_color, 0.08);
        }

        .result-title {
          font-size: 15px;
        }

        .result-subtitle {
          color: alpha(@window_fg_color, 0.58);
          font-size: 12px;
        }

        .result-icon {
          color: alpha(@window_fg_color, 0.8);
        }

        .action-bar {
          padding-top: 4px;
        }

        .action-button {
          min-width: 38px;
          min-height: 34px;
          border-radius: 7px;
        }
        ",
    );

    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn help_text() -> &'static str {
    "\
Usage:
  zeshicast-gtk            Show the launcher window
  zeshicast-gtk --daemon   Start hidden, keep the index warm, record clipboard history
  zeshicast-gtk --quit     Stop the running daemon

In the window:
  Enter                   Run selected result (opens form panel for commands with missing args)
  Ctrl+Enter              Copy selected result value
  Ctrl+K                  Open action panel (pin, alias, secondary actions)
  Ctrl+B                  Open extension browser (list all custom commands)
  Ctrl+,                  Open preferences editor (AI endpoint, model, translate settings)
  Esc                     Hide in daemon mode, otherwise quit
  Up/Down                 Move selection

Prefix searches:
  ai <text>               Ask AI — response copied to clipboard
  trans <text> in <lang>  Translate via LibreTranslate — result copied to clipboard
  shell <cmd>             Run an arbitrary shell command
  system / sys            System actions (lock, suspend, reboot, power off)
  audio / vol / volume    Audio/brightness actions
  net / wifi / network    Network actions
  niri                    Niri compositor actions
  clip / clipboard        Search clipboard history
  file / find             Search indexed files
  proc / process          Search and kill processes
"
}

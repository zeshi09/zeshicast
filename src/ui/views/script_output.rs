use gtk::prelude::*;
use gtk::{Box as GtkBox, Label};

#[derive(Clone)]
pub struct ScriptOutputView {
    pub root: GtkBox,
    pub title: gtk::Label,
    pub output: gtk::Label,
}

pub fn script_output_view() -> ScriptOutputView {
    let root = crate::ui::panel_root(10, 12);
    root.set_vexpand(true);

    let title = crate::ui::panel_title("Script Output");
    root.append(&title);

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();
    scroll.add_css_class("results-scroll");
    let output = Label::new(None);
    output.add_css_class("result-subtitle");
    output.set_wrap(true);
    output.set_xalign(0.0);
    output.set_yalign(0.0);
    output.set_selectable(true);
    output.set_margin_top(6);
    output.set_margin_start(4);
    scroll.set_child(Some(&output));
    root.append(&scroll);

    ScriptOutputView { root, title, output }
}

pub fn set_script_output(view: &ScriptOutputView, script_title: &str, stdout: &str) {
    view.title.set_text(&format!("Script: {script_title}"));
    view.output.set_text(stdout.trim());
}

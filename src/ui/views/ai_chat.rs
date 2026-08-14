use gtk::prelude::*;
use gtk::{Box as GtkBox, Box, Button, Entry, Label, Orientation};
use super::dashboard::{dashboard_button, dashboard_card_actions, dashboard_plain_card};

#[derive(Clone)]
pub struct AiChatView {
    pub root: GtkBox,
    pub input: Entry,
    pub output: Label,
    pub ask: Button,
    pub stop: Button,
    pub status: Label,
    pub copy: Button,
    pub use_clipboard: Button,
    pub save: Button,
}

pub fn ai_chat_view() -> AiChatView {
    let root = crate::ui::panel_root(8, 12);
    root.set_vexpand(true);

    let header_row = Box::new(Orientation::Horizontal, 8);
    let title = crate::ui::panel_title("AI Chat");
    title.set_hexpand(true);
    let model_chip = Label::new(Some("local model"));
    model_chip.add_css_class("ai-model-chip");
    header_row.append(&title);
    header_row.append(&model_chip);
    root.append(&header_row);

    let answer_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .propagate_natural_height(false)
        .vexpand(true)
        .build();
    answer_scroll.add_css_class("results-scroll");
    let output = Label::new(Some(
        "Ask a quick question to a local Ollama-compatible model.",
    ));
    output.add_css_class("result-subtitle");
    output.set_wrap(true);
    output.set_xalign(0.0);
    output.set_yalign(0.0);
    output.set_margin_start(4);
    output.set_margin_end(4);
    output.set_margin_top(6);
    output.set_margin_bottom(6);
    output.set_selectable(true);
    answer_scroll.set_child(Some(&output));
    root.append(&answer_scroll);

    let composer = dashboard_plain_card("Composer", "document-edit-symbolic");

    let context_row = Box::new(Orientation::Horizontal, 6);
    let context_chip = Label::new(Some(""));
    context_chip.add_css_class("ai-context-chip");
    context_chip.set_visible(false);
    context_row.append(&context_chip);
    composer.append(&context_row);

    let input = Entry::builder()
        .placeholder_text("Ask local AI…")
        .hexpand(true)
        .build();
    input.add_css_class("search-entry");
    composer.append(&input);

    let status = Label::new(None);
    status.add_css_class("result-subtitle");
    status.set_xalign(0.0);
    status.set_visible(false);
    composer.append(&status);

    let buttons = dashboard_card_actions();
    buttons.set_halign(gtk::Align::End);
    let copy = dashboard_button("Copy");
    let use_clipboard = dashboard_button("Use Clipboard");
    let save = dashboard_button("Save Snippet");
    let ask = dashboard_button("Ask");
    ask.add_css_class("suggested-action");
    let stop = dashboard_button("Stop");
    stop.add_css_class("destructive-action");
    stop.set_visible(false);
    buttons.append(&copy);
    buttons.append(&use_clipboard);
    buttons.append(&save);
    buttons.append(&ask);
    buttons.append(&stop);
    composer.append(&buttons);
    root.append(&composer);

    AiChatView {
        root,
        input,
        output,
        ask,
        stop,
        status,
        copy,
        use_clipboard,
        save,
    }
}

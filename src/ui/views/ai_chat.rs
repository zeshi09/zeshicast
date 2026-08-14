use gtk::prelude::*;
use gtk::{Button, Box as GtkBox, Entry, Label, Orientation};

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
    /// Container the dynamic model buttons are filled into.
    pub model_list: GtkBox,
    /// Re-fetch the model list from Ollama.
    pub refresh_models: Button,
}


pub fn ai_chat_view() -> AiChatView {
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);

    // ── Model selector bar ───────────────────────────────────────────────────
    let model_bar = GtkBox::new(Orientation::Horizontal, 6);
    model_bar.add_css_class("ai-model-bar");

    let model_label = Label::new(Some("Model"));
    model_label.add_css_class("action-panel-label");
    model_label.set_valign(gtk::Align::Center);
    model_bar.append(&model_label);

    // Filled at runtime from the Ollama server (see populate_ai_models).
    let model_list = GtkBox::new(Orientation::Horizontal, 6);
    model_list.set_hexpand(true);
    model_bar.append(&model_list);

    let refresh_models = Button::with_label("⟳");
    refresh_models.add_css_class("ai-model-btn");
    refresh_models.set_valign(gtk::Align::Center);
    refresh_models.set_tooltip_text(Some("Refresh models"));
    model_bar.append(&refresh_models);

    root.append(&model_bar);

    // ── Messages scroll area ─────────────────────────────────────────────────
    let answer_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();
    answer_scroll.add_css_class("results-scroll");

    let output = Label::new(Some("Hi! Running on Ollama. Ask me anything."));
    output.add_css_class("ai-message-assistant");
    output.set_wrap(true);
    output.set_xalign(0.0);
    output.set_yalign(0.0);
    output.set_margin_start(14);
    output.set_margin_end(14);
    output.set_margin_top(10);
    output.set_margin_bottom(6);
    output.set_selectable(true);
    answer_scroll.set_child(Some(&output));
    root.append(&answer_scroll);

    let status = Label::new(None);
    status.add_css_class("result-subtitle");
    status.set_xalign(0.0);
    status.set_margin_start(14);
    status.set_visible(false);
    root.append(&status);

    // ── Input row ────────────────────────────────────────────────────────────
    let input_row = GtkBox::new(Orientation::Horizontal, 8);
    input_row.add_css_class("ai-input-row");
    input_row.set_valign(gtk::Align::Center);

    let input = Entry::builder()
        .placeholder_text("Ask anything…")
        .hexpand(true)
        .build();
    input.add_css_class("search-entry");
    input_row.append(&input);

    let ask = Button::with_label("↑");
    ask.add_css_class("ai-send-btn");
    ask.set_valign(gtk::Align::Center);
    input_row.append(&ask);

    let stop = Button::with_label("■");
    stop.add_css_class("dashboard-button");
    stop.add_css_class("widget-btn");
    stop.set_visible(false);
    input_row.append(&stop);

    let copy = Button::with_label("Copy");
    copy.add_css_class("action-bar-more");

    let use_clipboard = Button::with_label("Use Clipboard");
    use_clipboard.add_css_class("action-bar-more");

    let save = Button::with_label("Save");
    save.add_css_class("action-bar-more");

    // Secondary actions row (copy / clipboard / save)
    let sec_row = GtkBox::new(Orientation::Horizontal, 4);
    sec_row.add_css_class("action-bar");
    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    sec_row.append(&spacer);
    sec_row.append(&use_clipboard);
    sec_row.append(&copy);
    sec_row.append(&save);

    root.append(&input_row);
    root.append(&sec_row);

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
        model_list,
        refresh_models,
    }
}


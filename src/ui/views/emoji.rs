use std::cell::RefCell;
use std::rc::Rc;
use gtk::prelude::*;
use gtk::{Button, Box as GtkBox, Entry, Label, Orientation};

#[derive(Clone)]
pub struct EmojiPickerView {
    pub root: GtkBox,
    pub search: Entry,
    pub flow: gtk::FlowBox,
    pub confirm: Label,
}


pub fn emoji_picker_view() -> EmojiPickerView {
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_vexpand(true);

    // Search row
    let search_row = GtkBox::new(Orientation::Horizontal, 0);
    search_row.add_css_class("search-bar");
    let search = Entry::builder()
        .placeholder_text("Search emoji…")
        .hexpand(true)
        .build();
    search.add_css_class("search-entry");
    search_row.append(&search);
    root.append(&search_row);

    // Category tab bar (horizontally scrollable)
    let cat_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .build();
    let cat_bar = GtkBox::new(Orientation::Horizontal, 4);
    cat_bar.set_margin_top(6);
    cat_bar.set_margin_bottom(6);
    cat_bar.set_margin_start(10);
    cat_bar.set_margin_end(10);
    cat_scroll.set_child(Some(&cat_bar));
    root.append(&cat_scroll);

    // Emoji grid (FlowBox)
    let flow_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();
    let flow = gtk::FlowBox::new();
    flow.set_homogeneous(true);
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_max_children_per_line(12);
    flow.set_min_children_per_line(6);
    flow.set_column_spacing(2);
    flow.set_row_spacing(2);
    flow.set_margin_start(8);
    flow.set_margin_end(8);
    flow.set_margin_top(4);
    flow_scroll.set_child(Some(&flow));
    root.append(&flow_scroll);

    // Confirmation strip
    let confirm = Label::new(None);
    confirm.add_css_class("emoji-confirm");
    confirm.set_halign(gtk::Align::Center);
    confirm.set_hexpand(true);
    confirm.set_visible(false);
    confirm.set_margin_top(4);
    confirm.set_margin_bottom(6);
    root.append(&confirm);

    // Build category buttons and initial grid
    const CATEGORIES: &[(&str, &str)] = &[
        ("all", "All"),
        ("smileys", "😀 Smileys"),
        ("gestures", "👍 Gestures"),
        ("body", "👁 Body"),
        ("symbols", "❤️ Symbols"),
        ("celebration", "🎉 Celebration"),
        ("travel", "✈️ Travel"),
        ("food", "🍎 Food"),
        ("animals", "🐾 Animals"),
        ("nature", "🌿 Nature"),
        ("music", "🎵 Music"),
        ("sports", "⚽ Sports"),
        ("technology", "💻 Technology"),
        ("tools", "🔧 Tools"),
        ("office", "📁 Office"),
        ("communication", "💬 Communication"),
        ("weather", "☀️ Weather"),
    ];

    let active_cat = Rc::new(RefCell::new("all".to_string()));

    for &(cat_id, cat_label) in CATEGORIES {
        let btn = Button::with_label(cat_label);
        btn.add_css_class("ai-model-btn");
        if cat_id == "all" {
            btn.add_css_class("active");
        }
        let flow_c = flow.clone();
        let confirm_c = confirm.clone();
        let active_cat_c = Rc::clone(&active_cat);
        let cat_bar_c = cat_bar.clone();
        let cat_id_s = cat_id.to_string();
        btn.connect_clicked(move |clicked_btn| {
            // Update active category
            *active_cat_c.borrow_mut() = cat_id_s.clone();
            // Update button styles
            let mut child = cat_bar_c.first_child();
            while let Some(w) = child {
                if let Some(b) = w.downcast_ref::<Button>() {
                    b.remove_css_class("active");
                }
                child = w.next_sibling();
            }
            clicked_btn.add_css_class("active");
            // Repopulate grid
            populate_emoji_flow(&flow_c, &cat_id_s, "", &confirm_c);
        });
        cat_bar.append(&btn);
    }

    // Initial population
    populate_emoji_flow(&flow, "all", "", &confirm);

    // Search updates grid
    {
        let flow_c = flow.clone();
        let confirm_c = confirm.clone();
        let active_cat_c = Rc::clone(&active_cat);
        search.connect_changed(move |entry| {
            let query = entry.text().to_string();
            populate_emoji_flow(&flow_c, &active_cat_c.borrow(), &query, &confirm_c);
        });
    }

    EmojiPickerView {
        root,
        search,
        flow,
        confirm,
    }
}

fn populate_emoji_flow(flow: &gtk::FlowBox, category: &str, query: &str, confirm: &Label) {
    while let Some(child) = flow.first_child() {
        flow.remove(&child);
    }

    let emoji_data = crate::search::emoji::emoji_data();
    let query_lower = query.to_lowercase();

    for &(emoji, name, cat) in emoji_data {
        let cat_match = category == "all" || cat == category;
        let query_match = query_lower.is_empty()
            || emoji.contains(&*query_lower)
            || name.contains(&*query_lower)
            || cat.contains(&*query_lower);
        if !cat_match || !query_match {
            continue;
        }

        let btn = Button::with_label(emoji);
        btn.add_css_class("emoji-btn");
        btn.set_width_request(36);
        btn.set_height_request(36);
        btn.set_tooltip_text(Some(name));

        let confirm_c = confirm.clone();
        let emoji_s = emoji.to_string();
        let name_s = name.to_string();
        btn.connect_clicked(move |_| {
            crate::copy_text(&emoji_s);
            confirm_c.set_text(&format!("Copied  {emoji_s}  {name_s}"));
            confirm_c.set_visible(true);
        });

        flow.insert(&btn, -1);
    }
}

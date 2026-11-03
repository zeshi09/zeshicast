pub(crate) const PREFERENCE_DEFAULTS: &[(&str, &str)] = &[
    ("ui_font_family", "Outfit, Inter, Noto Sans, sans-serif"),
    ("ui_font_size", "15"),
    ("ui_density", "compact"),
    ("ui_theme", "system"),
    ("ui_renderer", "cairo"),
    ("show_status_strip", "true"),
    ("ui_hints", "true"),
    (
        "status_items",
        "clock,date,network,battery,audio,media,layout",
    ),
    ("dashboard_enabled", "true"),
    ("dashboard_poll_interval_ms", "1000"),
    ("network_enabled", "true"),
    ("media_enabled", "true"),
    ("notifications_enabled", "true"),
    ("notifications_history_enabled", "true"),
    ("clipboard_history_enabled", "true"),
    ("clipboard_private_mode", "false"),
    ("clipboard_retention", "100"),
    ("clipboard_capture_images", "true"),
    ("clipboard_enter_action", "paste"),
    (
        "clipboard_ignore_apps",
        crate::services::clipboard_formats::DEFAULT_IGNORED_APPS,
    ),
    ("export_include_secrets", "false"),
    ("ai_enabled", "true"),
    ("ai_provider", "ollama"),
    ("ollama_endpoint", "http://localhost:11434"),
    ("ollama_model", "llama3.2:3b"),
    ("ai_endpoint", "https://api.openai.com/v1"),
    ("ai_model", "gpt-4o-mini"),
    ("ai_api_key", ""),
    ("translate_endpoint", "http://localhost:5000"),
    ("translate_api_key", ""),
    ("translate_target", "en"),
    ("script_dirs", "~/.config/zeshicast/scripts"),
    ("files_in_root", "true"),
    ("files_exclude", ""),
];

pub(crate) struct PrefSection {
    pub(crate) name: &'static str,
    pub(crate) keys: &'static [(&'static str, &'static str)],
}

pub(crate) const PREFERENCE_SECTIONS: &[PrefSection] = &[
    PrefSection {
        name: "General",
        keys: &[
            ("show_status_strip", "Show status strip"),
            ("ui_hints", "Tips under the search"),
            ("status_items", "Status items"),
            ("dashboard_enabled", "Dashboard enabled"),
            ("dashboard_poll_interval_ms", "Refresh interval (ms)"),
            ("clipboard_enter_action", "Enter on clipboard & snippets"),
        ],
    },
    PrefSection {
        name: "Appearance",
        keys: &[
            ("ui_font_family", "Font family"),
            ("ui_font_size", "Font size"),
            ("ui_density", "Row density"),
            ("ui_theme", "Theme"),
            ("ui_renderer", "Renderer (after restart)"),
        ],
    },
    PrefSection {
        name: "Keyboard",
        keys: &[],
    },
    PrefSection {
        name: "Extensions",
        keys: &[
            ("network_enabled", "Network features"),
            ("media_enabled", "Media features"),
            ("notifications_enabled", "Notification features"),
            ("ai_enabled", "AI features"),
            ("ai_provider", "AI provider"),
            ("ollama_endpoint", "Ollama endpoint"),
            ("ollama_model", "Ollama model"),
            ("ai_endpoint", "AI/OpenAI endpoint"),
            ("ai_model", "AI model"),
            ("ai_api_key", "AI API key"),
            ("script_dirs", "Script directories"),
            ("translate_endpoint", "Translate endpoint"),
            ("translate_api_key", "Translate API key"),
            ("translate_target", "Translate target language"),
        ],
    },
    PrefSection {
        name: "Privacy",
        keys: &[
            ("clipboard_history_enabled", "Clipboard history"),
            ("clipboard_private_mode", "Pause clipboard capture"),
            ("clipboard_retention", "Clipboard retention"),
            ("clipboard_capture_images", "Clipboard images"),
            ("clipboard_ignore_apps", "Never record copies from"),
            ("notifications_history_enabled", "Notification history"),
            ("files_in_root", "Files in root search"),
            ("files_exclude", "Exclude from file search"),
            ("export_include_secrets", "Export secrets"),
        ],
    },
    PrefSection {
        name: "About",
        keys: &[],
    },
];

use gtk::prelude::*;
use gtk::{Box as GtkBox, Entry, Label, Orientation, Paned, Stack};
use std::collections::HashMap;

#[derive(Clone)]
pub struct PreferencesView {
    pub root: GtkBox,
    pub fields: Vec<(String, Entry)>,
}

/// A linked segmented toggle (radio) for a fixed set of `(value, label)`
/// choices, writing the chosen value into the bound preference `entry`.
fn segmented_choice(options: &[(&str, &str)], current: &str, entry: &Entry) -> GtkBox {
    let btn_box = GtkBox::new(Orientation::Horizontal, 0);
    btn_box.add_css_class("linked");
    btn_box.set_valign(gtk::Align::Center);

    let has_match = options.iter().any(|(value, _)| *value == current);
    let mut anchor: Option<gtk::ToggleButton> = None;

    for (i, (value, label)) in options.iter().enumerate() {
        let btn = gtk::ToggleButton::with_label(label);
        match &anchor {
            Some(group) => btn.set_group(Some(group)),
            None => anchor = Some(btn.clone()),
        }
        let entry_c = entry.clone();
        let value_owned = value.to_string();
        btn.connect_toggled(move |b| {
            if b.is_active() {
                entry_c.set_text(&value_owned);
            }
        });
        // Activate the matching option (or the first when the stored value is
        // unknown). Done after wiring so the entry reflects the selection.
        btn.set_active(*value == current || (!has_match && i == 0));
        btn_box.append(&btn);
    }

    btn_box
}

/// Render one preference key as a field row bound to an auto-saving entry.
/// Boolean defaults get a switch; known enum/numeric keys get specialized
/// widgets; everything else is a text entry. Shared by the generic sections
/// and special sections (Privacy) that also carry dynamic keys.
fn preference_field_row(
    key: &str,
    description: &str,
    current: &HashMap<String, String>,
    fields_box: &GtkBox,
    fields: &mut Vec<(String, Entry)>,
) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 10);
    row.add_css_class("pref-field-row");

    let label = Label::new(Some(description));
    label.add_css_class("pref-field-label");
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_valign(gtk::Align::Center);
    row.append(&label);

    let default_val = PREFERENCE_DEFAULTS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
        .unwrap_or("");
    let effective_val = current.get(key).map(String::as_str).unwrap_or(default_val);

    // A preference is boolean when its default is true/false.
    let is_bool = matches!(default_val, "true" | "false");
    let is_numeric = key.contains("_ms") || key.contains("_size");

    let entry = Entry::new();
    entry.add_css_class("pref-entry");
    entry.set_width_chars(if is_bool { 0 } else { 14 });
    entry.set_valign(gtk::Align::Center);
    entry.set_text(effective_val);
    entry.set_placeholder_text(Some(default_val));

    if is_bool {
        let current_val = effective_val;
        let sw = gtk::Switch::new();
        sw.set_active(current_val != "false");
        sw.set_valign(gtk::Align::Center);
        let entry_c = entry.clone();
        sw.connect_active_notify(move |sw| {
            entry_c.set_text(if sw.is_active() { "true" } else { "false" });
        });
        row.append(&sw);
    } else if key == "ui_font_size" {
        let scale = gtk::Scale::with_range(Orientation::Horizontal, 12.0, 22.0, 1.0);
        scale.set_hexpand(true);
        scale.set_draw_value(true);
        scale.set_value_pos(gtk::PositionType::Right);
        scale.set_value(effective_val.parse::<f64>().unwrap_or(15.0));
        scale.set_valign(gtk::Align::Center);
        let entry_c = entry.clone();
        scale.connect_value_changed(move |s| {
            entry_c.set_text(&format!("{}", s.value() as u32));
        });
        row.append(&scale);
    } else if key == "ui_density" {
        row.append(&segmented_choice(
            &[("compact", "Compact"), ("comfortable", "Comfortable")],
            effective_val,
            &entry,
        ));
    } else if key == "ui_theme" {
        row.append(&segmented_choice(
            &[("system", "System"), ("dark", "Dark"), ("light", "Light")],
            effective_val,
            &entry,
        ));
    } else if key == "ui_renderer" {
        row.append(&segmented_choice(
            &[("cairo", "Cairo"), ("gl", "GL")],
            effective_val,
            &entry,
        ));
    } else if key == "clipboard_enter_action" {
        row.append(&segmented_choice(
            &[("paste", "Paste"), ("copy", "Copy")],
            effective_val,
            &entry,
        ));
    } else if key == "ai_provider" {
        row.append(&segmented_choice(
            &[("ollama", "Ollama"), ("openai", "OpenAI")],
            effective_val,
            &entry,
        ));
    } else if key == "dashboard_poll_interval_ms" {
        let spin = gtk::SpinButton::with_range(500.0, 5000.0, 100.0);
        spin.add_css_class("pref-entry");
        spin.set_value(effective_val.parse::<f64>().unwrap_or(1000.0));
        spin.set_valign(gtk::Align::Center);
        let entry_c = entry.clone();
        spin.connect_value_changed(move |s| {
            entry_c.set_text(&format!("{}", s.value() as u32));
        });
        row.append(&spin);
    } else if is_numeric {
        entry.set_input_purpose(gtk::InputPurpose::Digits);
        row.append(&entry);
    } else {
        // Text values (lists, endpoints…) can be long: let the
        // field take the row's free width so they aren't clipped.
        label.set_hexpand(false);
        entry.set_hexpand(true);
        entry.set_width_chars(0);
        // Mask secrets.
        if key.ends_with("_api_key") {
            entry.set_visibility(false);
            entry.set_input_purpose(gtk::InputPurpose::Password);
        }
        row.append(&entry);
    }

    fields.push((key.to_string(), entry));
    fields_box.append(&row);
    row
}

/// Whether a settings row matches the search (Q3.3): every word of `query`
/// appears, case-insensitively, in the section name or the row's text. An
/// empty query matches everything.
pub(crate) fn settings_row_matches(query: &str, section: &str, text: &str) -> bool {
    let haystack = format!("{section} {text}").to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|word| haystack.contains(word))
}

/// Search text of a preference field: its label, its key, and the key with
/// underscores as spaces (`clipboard_enter_action` → "clipboard enter action").
fn field_search_text(key: &str, description: &str) -> String {
    format!("{description} {key} {}", key.replace('_', " "))
}

/// One section's sidebar row and its searchable rows.
struct SearchableSection {
    name: &'static str,
    sidebar_row: gtk::ListBoxRow,
    rows: Vec<(gtk::Widget, String)>,
}

/// Stack page shown when no setting matches the search.
const NO_MATCH_PAGE: &str = "no-match";

/// Show only matching rows and sections, open the first matching section,
/// or explain that nothing matched. An empty query restores everything.
fn filter_settings(
    query: &str,
    sections: &[SearchableSection],
    sidebar: &gtk::ListBox,
    stack: &Stack,
    no_match: &Label,
) {
    let mut first_match: Option<&SearchableSection> = None;
    for section in sections {
        let mut any = false;
        for (row, text) in &section.rows {
            let matches = settings_row_matches(query, section.name, text);
            row.set_visible(matches);
            any |= matches;
        }
        // Sections without rows (none today) stay listed for an empty query.
        let shown = any || query.trim().is_empty();
        section.sidebar_row.set_visible(shown);
        if shown && first_match.is_none() {
            first_match = Some(section);
        }
    }

    match first_match {
        Some(section) => {
            if query.trim().is_empty() {
                // Back to the section the user had selected.
                let selected = sidebar
                    .selected_row()
                    .and_then(|row| usize::try_from(row.index()).ok())
                    .and_then(|index| sections.get(index))
                    .unwrap_or(section);
                stack.set_visible_child_name(selected.name);
            } else {
                sidebar.select_row(Some(&section.sidebar_row));
                stack.set_visible_child_name(section.name);
            }
        }
        None => {
            no_match.set_text(&format!(
                "No setting matches “{}”. Try a shorter word, or a key such as clipboard_retention.",
                query.trim()
            ));
            stack.set_visible_child_name(NO_MATCH_PAGE);
        }
    }
}

pub fn preferences_view(current: &HashMap<String, String>) -> PreferencesView {
    let outer = super::panel_root(0, 0);
    outer.set_vexpand(true);
    outer.set_hexpand(true);

    // No in-view title/search: the nav header already shows "‹ Preferences",
    // and the design goes straight to the sidebar + content panes.
    let paned = Paned::new(Orientation::Horizontal);
    paned.set_vexpand(true);
    paned.set_hexpand(true);
    paned.set_position(138);
    paned.set_shrink_start_child(false);
    paned.set_shrink_end_child(false);

    let sidebar = super::results_list();
    sidebar.add_css_class("pref-sidebar");
    sidebar.set_vexpand(true);
    sidebar.set_activate_on_single_click(true);

    let content_stack = Stack::new();
    content_stack.set_vexpand(true);
    content_stack.set_hexpand(true);

    let mut fields = Vec::new();
    let mut searchable: Vec<SearchableSection> = Vec::new();

    for section in PREFERENCE_SECTIONS {
        let mut rows: Vec<(gtk::Widget, String)> = Vec::new();
        let sidebar_row = gtk::ListBoxRow::new();
        sidebar_row.add_css_class("pref-sidebar-row");
        let sidebar_label = Label::new(Some(section.name));
        sidebar_label.add_css_class("pref-sidebar-label");
        sidebar_label.set_xalign(0.0);
        sidebar_label.set_margin_start(12);
        sidebar_label.set_margin_end(12);
        sidebar_row.set_child(Some(&sidebar_label));
        sidebar.append(&sidebar_row);

        let fields_box = GtkBox::new(Orientation::Vertical, 6);
        fields_box.add_css_class("pref-content");
        fields_box.set_vexpand(true);

        // Special static sections
        match section.name {
            "About" => {
                let about_text = format!(
                    "zeshicast  v{}\nRaycast-inspired launcher for Wayland / Niri\n\nBuilt with Rust + GTK4\nOS: {}\nArch: {}",
                    env!("CARGO_PKG_VERSION"),
                    std::env::var("PRETTY_NAME")
                        .or_else(|_| std::fs::read_to_string("/etc/os-release")
                            .ok()
                            .and_then(|s| s
                                .lines()
                                .find(|l| l.starts_with("PRETTY_NAME"))
                                .and_then(|l| l.split('=').nth(1))
                                .map(|v| v.trim_matches('"').to_string()))
                            .ok_or(std::env::VarError::NotPresent))
                        .unwrap_or_else(|_| "Linux".to_string()),
                    std::env::consts::ARCH,
                );
                let lbl = Label::new(Some(&about_text));
                lbl.add_css_class("result-subtitle");
                lbl.set_xalign(0.0);
                lbl.set_wrap(true);
                lbl.set_selectable(true);
                lbl.set_margin_start(14);
                lbl.set_margin_top(14);
                fields_box.append(&lbl);
                rows.push((lbl.upcast(), format!("about version {about_text}")));
            }
            "Keyboard" => {
                let shortcuts = [
                    ("Super+Space", "Open launcher"),
                    ("Escape", "Close / go back / clear"),
                    ("↑ / ↓", "Navigate results"),
                    ("Enter", "Launch selected"),
                    ("Tab", "Jump to AI Chat"),
                    ("Ctrl+K", "Open Action Panel"),
                    ("Ctrl+D", "Dashboard"),
                    ("Ctrl+T", "System Monitor"),
                    ("Ctrl+I", "AI Chat"),
                    ("Ctrl+M", "Media"),
                    ("Ctrl+O", "Audio"),
                    ("Ctrl+N", "Network"),
                    ("Ctrl+H", "Clipboard"),
                    ("Ctrl+B", "Extensions"),
                    ("Ctrl+,", "Preferences"),
                    ("=", "Calculator mode"),
                ];
                for (key, desc) in shortcuts {
                    let row = GtkBox::new(Orientation::Horizontal, 10);
                    row.add_css_class("pref-field-row");
                    let lbl = Label::new(Some(desc));
                    lbl.add_css_class("pref-field-label");
                    lbl.set_xalign(0.0);
                    lbl.set_hexpand(true);
                    row.append(&lbl);
                    let kbd = Label::new(Some(key));
                    kbd.add_css_class("ctrl-k-hint");
                    kbd.set_xalign(1.0);
                    row.append(&kbd);
                    fields_box.append(&row);
                    rows.push((row.upcast(), format!("{desc} {key} shortcut")));
                }
            }
            "Privacy" => {
                let privacy_rows = [
                    (
                        "Clipboard history",
                        "Stores last 100 clipboard entries locally (configurable)",
                    ),
                    (
                        "Usage frequency",
                        "Tracks launch frequency for frecency scoring",
                    ),
                    ("No telemetry", "Zero data sent to remote servers"),
                    ("Config location", "~/.config/zeshicast/"),
                ];
                for (name, detail) in privacy_rows {
                    let row = GtkBox::new(Orientation::Horizontal, 10);
                    row.add_css_class("pref-field-row");
                    let text = GtkBox::new(Orientation::Vertical, 2);
                    text.set_hexpand(true);
                    let name_lbl = Label::new(Some(name));
                    name_lbl.add_css_class("pref-field-label");
                    name_lbl.set_xalign(0.0);
                    let detail_lbl = Label::new(Some(detail));
                    detail_lbl.add_css_class("result-subtitle");
                    detail_lbl.set_xalign(0.0);
                    text.append(&name_lbl);
                    text.append(&detail_lbl);
                    row.append(&text);
                    fields_box.append(&row);
                    rows.push((row.upcast(), format!("{name} {detail}")));
                }
                // Dynamic keys of this section (e.g. export_include_secrets)
                // render through the same auto-saving field mechanism as the
                // generic sections so the toggle actually exists in the UI.
                for (key, description) in section.keys {
                    let row =
                        preference_field_row(key, description, current, &fields_box, &mut fields);
                    rows.push((row.upcast(), field_search_text(key, description)));
                }
            }
            _ => {
                for (key, description) in section.keys {
                    let row =
                        preference_field_row(key, description, current, &fields_box, &mut fields);
                    rows.push((row.upcast(), field_search_text(key, description)));
                }
            }
        }

        let content_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .child(&fields_box)
            .build();
        content_scroller.set_vexpand(true);
        content_stack.add_named(&content_scroller, Some(section.name));
        searchable.push(SearchableSection {
            name: section.name,
            sidebar_row,
            rows,
        });
    }

    let no_match = Label::new(None);
    no_match.add_css_class("result-subtitle");
    no_match.set_wrap(true);
    no_match.set_margin_top(24);
    content_stack.add_named(&no_match, Some(NO_MATCH_PAGE));

    let sidebar_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&sidebar)
        .build();
    sidebar_scroller.add_css_class("pref-sidebar");
    sidebar_scroller.set_vexpand(true);

    let sidebar_clone = sidebar.clone();
    let stack_clone = content_stack.clone();
    sidebar.connect_row_activated(move |_, row| {
        let index = row.index() as usize;
        if let Some(section) = PREFERENCE_SECTIONS.get(index) {
            stack_clone.set_visible_child_name(section.name);
        }
        let _ = &sidebar_clone;
    });

    if let Some(row) = sidebar.row_at_index(0) {
        sidebar.select_row(Some(&row));
    }
    if let Some(first) = PREFERENCE_SECTIONS.first() {
        content_stack.set_visible_child_name(first.name);
    }

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search settings"));
    search.add_css_class("pref-search");
    let start_pane = GtkBox::new(Orientation::Vertical, 6);
    start_pane.append(&search);
    start_pane.append(&sidebar_scroller);

    {
        let sidebar = sidebar.clone();
        let content_stack = content_stack.clone();
        search.connect_search_changed(move |search| {
            filter_settings(
                search.text().as_str(),
                &searchable,
                &sidebar,
                &content_stack,
                &no_match,
            );
        });
    }

    paned.set_start_child(Some(&start_pane));
    paned.set_end_child(Some(&content_stack));
    outer.append(&paned);

    // No Save/Cancel buttons (not in the mockup): changes auto-save, wired in
    // launcher.rs against each field's `changed` signal.
    PreferencesView {
        root: outer,
        fields,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_match_on_label_key_section_and_every_word() {
        let text = field_search_text("clipboard_retention", "Clipboard retention");
        assert!(settings_row_matches("retention", "Privacy", &text));
        assert!(settings_row_matches(
            "CLIPBOARD_RETENTION",
            "Privacy",
            &text
        ));
        // Every word must appear, in any order.
        assert!(settings_row_matches(
            "retention clipboard",
            "Privacy",
            &text
        ));
        assert!(!settings_row_matches("retention ollama", "Privacy", &text));
        // The section name matches all of its rows.
        assert!(settings_row_matches("privacy", "Privacy", "Export secrets"));
        // An empty query matches everything.
        assert!(settings_row_matches("", "General", "anything"));
        assert!(settings_row_matches("   ", "General", "anything"));
    }

    #[test]
    fn every_preference_key_is_findable_by_its_key() {
        for section in PREFERENCE_SECTIONS {
            for (key, description) in section.keys {
                let text = field_search_text(key, description);
                assert!(
                    settings_row_matches(key, section.name, &text),
                    "{key} in {}",
                    section.name
                );
                assert!(
                    settings_row_matches(&key.replace('_', " "), section.name, &text),
                    "{key} by words"
                );
            }
        }
    }

    #[test]
    fn every_listed_preference_has_a_default() {
        for section in PREFERENCE_SECTIONS {
            for (key, _) in section.keys {
                assert!(
                    PREFERENCE_DEFAULTS
                        .iter()
                        .any(|(default, _)| default == key),
                    "{key} has no default"
                );
            }
        }
    }
}

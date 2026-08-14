use std::cell::RefCell;
use std::rc::Rc;
use crate::{ClipboardKind, ClipboardSummary, Zeshicast};
use gtk::gdk;
use gtk::gio;
use gtk::prelude::*;
use gtk::{Box as GtkBox, Entry, ListBox};

#[derive(Clone, Copy)]
pub(crate) enum ClipboardFilter {
    All,
    Kind(ClipboardKind),
}


pub(crate) fn install_clipboard_monitor(launcher: &Rc<RefCell<Zeshicast>>) {
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

pub(crate) fn capture_clipboard_text(
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

pub(crate) fn show_clipboard_view(
    navigation: &crate::ui::NavigationStack,
    entry: &Entry,
    action_bar: &GtkBox,
    clipboard_view: &crate::ui::ClipboardHistoryView,
    clipboard_items: &Rc<RefCell<Vec<ClipboardSummary>>>,
    launcher: &Rc<RefCell<Zeshicast>>,
) {
    refresh_clipboard_view(launcher, clipboard_view, clipboard_items);
    entry.set_visible(false);
    action_bar.set_visible(false);
    navigation.push(crate::ui::LauncherView::Clipboard);
    if let Some(row) = clipboard_view.list.row_at_index(0) {
        clipboard_view.list.select_row(Some(&row));
    }
    clipboard_view.list.grab_focus();
}

pub(crate) fn refresh_clipboard_view(
    launcher: &Rc<RefCell<Zeshicast>>,
    clipboard_view: &crate::ui::ClipboardHistoryView,
    clipboard_items: &Rc<RefCell<Vec<ClipboardSummary>>>,
) {
    let filter = selected_clipboard_filter(clipboard_view);
    let items = launcher
        .borrow()
        .list_clipboard_history()
        .into_iter()
        .filter(|item| clipboard_filter_matches(filter, item))
        .collect::<Vec<_>>();
    crate::ui::set_clipboard_history_items(&clipboard_view.list, &items);
    *clipboard_items.borrow_mut() = items;
    let selected_item = clipboard_view
        .list
        .selected_row()
        .and_then(|row| clipboard_items.borrow().get(row.index() as usize).cloned());
    crate::ui::set_clipboard_detail(clipboard_view, selected_item.as_ref());
}

pub(crate) fn selected_clipboard_filter(view: &crate::ui::ClipboardHistoryView) -> ClipboardFilter {
    match view.filter.selected() {
        1 => ClipboardFilter::Kind(ClipboardKind::Text),
        2 => ClipboardFilter::Kind(ClipboardKind::Url),
        3 => ClipboardFilter::Kind(ClipboardKind::Command),
        4 => ClipboardFilter::Kind(ClipboardKind::Code),
        _ => ClipboardFilter::All,
    }
}

pub(crate) fn clipboard_filter_matches(filter: ClipboardFilter, item: &ClipboardSummary) -> bool {
    match filter {
        ClipboardFilter::All => true,
        ClipboardFilter::Kind(kind) => item.kind == kind,
    }
}

pub(crate) fn copy_clipboard_row(
    list: &ListBox,
    index: usize,
    clipboard_items: &Rc<RefCell<Vec<ClipboardSummary>>>,
) {
    let Some(row) = list.row_at_index(index as i32) else {
        return;
    };
    let index = row.index() as usize;
    if let Some(item) = clipboard_items.borrow().get(index) {
        crate::copy_text(&item.value);
    }
}
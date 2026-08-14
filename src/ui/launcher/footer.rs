use std::cell::RefCell;
use std::rc::Rc;
use crate::ui::launcher_helpers::{preference_enabled, preference_list};
use crate::{Action, SecondaryActionKind, Zeshicast};
use gtk::gio;
use gtk::prelude::*;
use gtk::{ApplicationWindow, Box as GtkBox, Entry, ListBox, Orientation};
use super::actions::*;


pub(crate) fn apply_status_strip_preferences(
    status_strip: &crate::ui::StatusStrip,
    launcher: &Rc<RefCell<Zeshicast>>,
) {
    status_strip
        .widget()
        .set_visible(preference_enabled(launcher, "show_status_strip", true));
    let items = preference_list(
        launcher,
        "status_items",
        &["clock", "date", "network", "battery", "audio", "media"],
    );
    status_strip.set_items(&items);
}

pub(crate) fn action_bar(
    window: &ApplicationWindow,
    launcher: &Rc<RefCell<Zeshicast>>,
    entry: &Entry,
    list: &ListBox,
    results: &Rc<RefCell<Vec<Action>>>,
    hold: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    navigation: &crate::ui::NavigationStack,
    action_panel_view: &crate::ui::ActionPanelView,
    current_action: &Rc<RefCell<Option<Action>>>,
    action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    filtered_action_panel_items: &Rc<RefCell<Vec<ActionPanelItem>>>,
    ai_chat_view: &crate::ui::AiChatView,
    audio_view: &crate::ui::AudioView,
    dashboard_view: &crate::ui::DashboardView,
    system_monitor_view: &crate::ui::SystemMonitorView,
    media_view: &crate::ui::MediaView,
    network_list: &ListBox,
    notifications_view: &crate::ui::NotificationsView,
    window_grid_view: &crate::ui::WindowGridView,
) -> GtkBox {
    let bar = GtkBox::new(Orientation::Horizontal, 8);
    bar.add_css_class("action-bar");

    let run = footer_button("Run  Enter");
    let actions = footer_button("Actions  Ctrl+K");
    let copy = footer_button("Copy  Ctrl+Enter");
    let folder = footer_button("Folder");
    let pin = footer_button("Pin");

    {
        let window = window.clone();
        let launcher = Rc::clone(launcher);
        let hold = Rc::clone(hold);
        let entry = entry.clone();
        let list = list.clone();
        let results = Rc::clone(results);
        let navigation = navigation.clone();
        let bar = bar.clone();
        let ai_chat_view = ai_chat_view.clone();
        let audio_view = audio_view.clone();
        let dashboard_view = dashboard_view.clone();
        let system_monitor_view = system_monitor_view.clone();
        let media_view = media_view.clone();
        let network_list = network_list.clone();
        let notifications_view = notifications_view.clone();
        let window_grid_view = window_grid_view.clone();
        run.connect_clicked(move |_| {
            run_selected_with_views(
                &window,
                &launcher,
                &hold,
                &entry,
                &list,
                &results,
                &navigation,
                &bar,
                &ai_chat_view,
                &audio_view,
                &dashboard_view,
                &system_monitor_view,
                &media_view,
                &network_list,
                &notifications_view,
                &window_grid_view,
            )
        });
    }

    {
        let navigation = navigation.clone();
        let entry = entry.clone();
        let bar = bar.clone();
        let action_panel_view = action_panel_view.clone();
        let current_action = Rc::clone(current_action);
        let action_panel_items = Rc::clone(action_panel_items);
        let filtered_action_panel_items = Rc::clone(filtered_action_panel_items);
        let launcher = Rc::clone(launcher);
        let list = list.clone();
        let results = Rc::clone(results);
        actions.connect_clicked(move |_| {
            show_action_panel_view(
                &navigation,
                &entry,
                &bar,
                &action_panel_view,
                &current_action,
                &action_panel_items,
                &filtered_action_panel_items,
                &launcher,
                &list,
                &results,
            );
        });
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
    bar.append(&actions);
    bar.append(&copy);
    bar.append(&folder);
    bar.append(&pin);
    bar
}

pub(crate) fn footer_button(label: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.add_css_class("footer-action");
    button
}
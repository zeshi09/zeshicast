use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{Stack, Widget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherView {
    Root,
    Actions,
    AiChat,
    Audio,
    Clipboard,
    Dashboard,
    Extensions,
    Media,
    Network,
    Notifications,
    Preferences,
    Snippets,
    SystemMonitor,
}

impl LauncherView {
    fn name(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Actions => "actions",
            Self::AiChat => "ai-chat",
            Self::Audio => "audio",
            Self::Clipboard => "clipboard",
            Self::Dashboard => "dashboard",
            Self::Extensions => "extensions",
            Self::Media => "media",
            Self::Network => "network",
            Self::Notifications => "notifications",
            Self::Preferences => "preferences",
            Self::Snippets => "snippets",
            Self::SystemMonitor => "system-monitor",
        }
    }
}

#[derive(Clone)]
pub struct NavigationStack {
    stack: Stack,
    history: Rc<RefCell<Vec<LauncherView>>>,
}

impl NavigationStack {
    pub fn new() -> Self {
        let stack = Stack::new();
        stack.set_vexpand(true);
        Self {
            stack,
            history: Rc::new(RefCell::new(vec![LauncherView::Root])),
        }
    }

    pub fn widget(&self) -> &Stack {
        &self.stack
    }

    pub fn add_page(&self, view: LauncherView, child: &impl IsA<Widget>) {
        self.stack.add_named(child, Some(view.name()));
    }

    pub fn current(&self) -> LauncherView {
        self.history
            .borrow()
            .last()
            .copied()
            .unwrap_or(LauncherView::Root)
    }

    pub fn push(&self, view: LauncherView) {
        if self.current() != view {
            self.history.borrow_mut().push(view);
        }
        self.stack.set_visible_child_name(view.name());
    }

    pub fn pop(&self) -> Option<LauncherView> {
        {
            let mut history = self.history.borrow_mut();
            if history.len() <= 1 {
                return None;
            }
            history.pop();
        }

        let current = self.current();
        self.stack.set_visible_child_name(current.name());
        Some(current)
    }

    pub fn reset(&self) {
        *self.history.borrow_mut() = vec![LauncherView::Root];
        self.stack.set_visible_child_name(LauncherView::Root.name());
    }
}

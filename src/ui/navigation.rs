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
    Emoji,
    Extensions,
    Media,
    Network,
    Notifications,
    Preferences,
    ScriptOutput,
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
            Self::Emoji => "emoji",
            Self::Extensions => "extensions",
            Self::Media => "media",
            Self::Network => "network",
            Self::Notifications => "notifications",
            Self::Preferences => "preferences",
            Self::ScriptOutput => "script-output",
            Self::Snippets => "snippets",
            Self::SystemMonitor => "system-monitor",
        }
    }

    pub fn back_label(self) -> &'static str {
        match self {
            Self::Root | Self::Actions => "",
            Self::AiChat => "AI Chat",
            Self::Audio => "Audio",
            Self::Clipboard => "Clipboard",
            Self::Dashboard => "Dashboard",
            Self::Emoji => "Emoji",
            Self::Extensions => "Extensions",
            Self::Media => "Media",
            Self::Network => "Network",
            Self::Notifications => "Notifications",
            Self::Preferences => "Preferences",
            Self::ScriptOutput => "Output",
            Self::Snippets => "Snippets",
            Self::SystemMonitor => "System Monitor",
        }
    }
}

type ViewCallback = Box<dyn Fn(LauncherView)>;

#[derive(Clone)]
pub struct NavigationStack {
    stack: Stack,
    history: Rc<RefCell<Vec<LauncherView>>>,
    callbacks: Rc<RefCell<Vec<ViewCallback>>>,
}

impl NavigationStack {
    pub fn new() -> Self {
        let stack = Stack::new();
        stack.set_vexpand(true);
        Self {
            stack,
            history: Rc::new(RefCell::new(vec![LauncherView::Root])),
            callbacks: Rc::new(RefCell::new(Vec::new())),
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
        self.notify(view);
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
        self.notify(current);
        Some(current)
    }

    pub fn reset(&self) {
        *self.history.borrow_mut() = vec![LauncherView::Root];
        self.stack.set_visible_child_name(LauncherView::Root.name());
        self.notify(LauncherView::Root);
    }

    /// Register a callback fired on every push/pop/reset with the new current view.
    pub fn connect_view_changed(&self, cb: impl Fn(LauncherView) + 'static) {
        self.callbacks.borrow_mut().push(Box::new(cb));
    }

    fn notify(&self, view: LauncherView) {
        let cbs = self.callbacks.borrow();
        for cb in cbs.iter() {
            cb(view);
        }
    }
}

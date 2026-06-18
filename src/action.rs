use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{copy_to_clipboard, execute_http_request, spawn_command, spawn_shell};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandArgumentKind {
    Text,
    Number,
    Path,
    Bool,
    Enum,
}

#[derive(Debug, Clone)]
pub struct ActionFormField {
    pub name: String,
    pub kind: CommandArgumentKind,
    pub required: bool,
    pub default: String,
    pub options: Vec<String>,
    pub current_value: String,
}

#[derive(Debug, Clone)]
pub struct ActionForm {
    pub name: String,
    pub fields: Vec<ActionFormField>,
    pub(crate) command: String,
    pub(crate) env: HashMap<String, String>,
    pub(crate) preferences: HashMap<String, String>,
    pub(crate) current_args: HashMap<String, String>,
    pub(crate) partial_query: String,
}

#[derive(Debug, Clone)]
pub enum HttpRequest {
    Translate {
        endpoint: String,
        text: String,
        target: String,
        api_key: String,
    },
    LocalAiGenerate {
        endpoint: String,
        model: String,
        query: String,
    },
    AiChat {
        endpoint: String,
        model: String,
        query: String,
        api_key: String,
    },
}

#[derive(Debug, Clone)]
pub struct Action {
    pub category: String,
    pub title: String,
    pub subtitle: String,
    pub icon_name: String,
    pub(crate) kind: ActionKind,
    pub score: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondaryActionKind {
    Run,
    CopyValue,
    TypeText,
    OpenParent,
    Pin,
    Unpin,
    DeleteClipboardItem,
    ClearClipboardHistory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionPanelSection {
    Primary,
    Manage,
    Clipboard,
    Danger,
}

impl ActionPanelSection {
    pub fn title(self) -> &'static str {
        match self {
            Self::Primary => "Primary",
            Self::Manage => "Manage",
            Self::Clipboard => "Clipboard",
            Self::Danger => "Danger",
        }
    }

    pub fn is_danger(self) -> bool {
        matches!(self, Self::Danger)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherCommand {
    AiChat,
    Audio,
    Dashboard,
    Emoji,
    Fonts,
    Media,
    Network,
    Notifications,
    SystemMonitor,
}

#[derive(Debug, Clone)]
pub struct SecondaryAction {
    pub kind: SecondaryActionKind,
    pub title: String,
    pub icon_name: String,
    pub section: ActionPanelSection,
}

impl SecondaryAction {
    pub(crate) fn new(
        kind: SecondaryActionKind,
        title: impl Into<String>,
        icon_name: impl Into<String>,
        section: ActionPanelSection,
    ) -> Self {
        Self {
            kind,
            title: title.into(),
            icon_name: icon_name.into(),
            section,
        }
    }
}

impl Action {
    pub(crate) fn new(
        category: impl Into<String>,
        title: impl Into<String>,
        kind: ActionKind,
        score: i32,
    ) -> Self {
        Self {
            category: category.into(),
            title: title.into(),
            subtitle: String::new(),
            icon_name: "system-run-symbolic".to_string(),
            kind,
            score,
        }
    }

    pub(crate) fn with_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = subtitle.into();
        self
    }

    pub(crate) fn with_icon(mut self, icon_name: impl Into<String>) -> Self {
        self.icon_name = icon_name.into();
        self
    }

    pub fn run(&self) {
        match &self.kind {
            ActionKind::Launch(command) => spawn_shell(&ShellCommand::new(command)),
            ActionKind::OpenPath(path) => {
                spawn_command("xdg-open", &[path.to_string_lossy().as_ref()])
            }
            ActionKind::OpenUrl(url) => spawn_command("xdg-open", &[url]),
            ActionKind::Copy(text) => copy_to_clipboard(text),
            ActionKind::Shell(command) => spawn_shell(command),
            ActionKind::HttpCopy(req) => {
                // Network round-trips (translate / OpenAI / Ollama) can take
                // seconds. Run them off the caller's thread so the GUI never
                // freezes; `copy_to_clipboard` shells out to wl-copy/xclip and is
                // safe to call from a worker thread.
                let req = req.clone();
                std::thread::spawn(move || {
                    if let Some(result) = execute_http_request(&req) {
                        copy_to_clipboard(&result);
                    } else {
                        eprintln!("http request failed");
                    }
                });
            }
            ActionKind::Launcher(_) => {}
            ActionKind::Form(_) => {}
            ActionKind::Media(control) => crate::media_control(*control),
            ActionKind::Notification(action) => match action {
                crate::NotificationAction::ToggleDnd => {
                    crate::toggle_dnd();
                }
                crate::NotificationAction::ClearAll => crate::clear_notifications(),
            },
            ActionKind::None => {}
        }
    }

    pub fn launcher_command(&self) -> Option<LauncherCommand> {
        match self.kind {
            ActionKind::Launcher(command) => Some(command),
            _ => None,
        }
    }

    pub fn form_data(&self) -> Option<&ActionForm> {
        match &self.kind {
            ActionKind::Form(f) => Some(f),
            _ => None,
        }
    }

    pub fn copy_value(&self) {
        copy_to_clipboard(&self.value());
    }

    pub fn value(&self) -> String {
        match &self.kind {
            ActionKind::Launch(command) | ActionKind::OpenUrl(command) => command.clone(),
            ActionKind::Shell(command) => command.command.clone(),
            ActionKind::OpenPath(path) => path.display().to_string(),
            ActionKind::Copy(text) => text.clone(),
            ActionKind::HttpCopy(req) => match req {
                HttpRequest::Translate { text, .. } => text.clone(),
                HttpRequest::LocalAiGenerate { query, .. } => query.clone(),
                HttpRequest::AiChat { query, .. } => query.clone(),
            },
            ActionKind::Launcher(_) => self.title.clone(),
            ActionKind::Form(form) => form.command.clone(),
            ActionKind::Media(_) => self.title.clone(),
            ActionKind::Notification(_) => self.title.clone(),
            ActionKind::None => self.title.clone(),
        }
    }

    pub fn parent_dir(&self) -> Option<PathBuf> {
        match &self.kind {
            ActionKind::OpenPath(path) => path.parent().map(Path::to_path_buf),
            _ => None,
        }
    }

    pub fn open_parent_dir(&self) {
        if let Some(parent) = self.parent_dir() {
            spawn_command("xdg-open", &[parent.to_string_lossy().as_ref()]);
        }
    }

    pub fn identity(&self) -> String {
        format!("{}:{}", self.category, self.title)
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ActionKind {
    Launch(String),
    OpenPath(PathBuf),
    OpenUrl(String),
    Copy(String),
    Shell(ShellCommand),
    HttpCopy(HttpRequest),
    Launcher(LauncherCommand),
    Form(ActionForm),
    /// Playback control routed to the active MPRIS player over D-Bus.
    Media(crate::MediaControl),
    /// Notification action routed to our own notification store.
    Notification(crate::NotificationAction),
    None,
}

#[derive(Debug, Clone)]
pub(crate) struct ShellCommand {
    pub(crate) command: String,
    pub(crate) env: HashMap<String, String>,
}

impl ShellCommand {
    pub(crate) fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            env: HashMap::new(),
        }
    }

    pub(crate) fn with_env(command: impl Into<String>, env: HashMap<String, String>) -> Self {
        Self {
            command: command.into(),
            env,
        }
    }
}

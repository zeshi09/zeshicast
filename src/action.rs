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
    pub(crate) command: ActionFormCommand,
    pub(crate) env: HashMap<String, String>,
    pub(crate) preferences: HashMap<String, String>,
    pub(crate) current_args: HashMap<String, String>,
    pub(crate) partial_query: String,
}

#[derive(Debug, Clone)]
pub(crate) enum ActionFormCommand {
    Shell(String),
    Argv { program: String, args: Vec<String> },
}

impl ActionFormCommand {
    pub(crate) fn display(&self) -> String {
        match self {
            Self::Shell(command) => command.clone(),
            Self::Argv { program, args } => {
                ProcessCommand::new(program.clone(), args.clone()).display()
            }
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct JsonCommandAction {
    pub(crate) category: String,
    pub(crate) command: ShellCommand,
    pub(crate) capabilities: Vec<Capability>,
    pub(crate) score: i32,
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
pub(crate) enum ExecutionRequest {
    Shell { command: ShellCommand },
    Command(ProcessCommand),
    OpenPath(PathBuf),
    OpenUrl(String),
    Copy(String),
    Http(HttpRequest),
    Media(crate::MediaControl),
    Notification(crate::NotificationAction),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionDecision {
    RunNow,
    NeedsConfirmation(ActionRisk),
    Denied(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionPolicy {
    confirmed: bool,
}

impl ExecutionPolicy {
    pub fn interactive() -> Self {
        Self { confirmed: false }
    }

    pub fn confirmed() -> Self {
        Self { confirmed: true }
    }

    pub fn decide(self, action: &Action) -> ExecutionDecision {
        if action.execution_request().is_none() {
            return ExecutionDecision::Denied("action has no executable request".to_string());
        }
        if action.risk.requires_confirmation() && !self.confirmed {
            ExecutionDecision::NeedsConfirmation(action.risk)
        } else {
            ExecutionDecision::RunNow
        }
    }
}

#[derive(Debug, Clone)]
pub struct Action {
    pub category: String,
    pub title: String,
    pub subtitle: String,
    pub icon_name: String,
    pub risk: ActionRisk,
    pub(crate) kind: ActionKind,
    pub score: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionRisk {
    Normal,
    Shell,
    Destructive,
    SystemPower,
    ProcessKill,
    ClipboardClear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    Shell,
    Network,
    Filesystem,
    ClipboardRead,
    ClipboardWrite,
    OpenUrl,
    OpenPath,
}

impl Capability {
    pub fn label(self) -> &'static str {
        match self {
            Self::Shell => "shell",
            Self::Network => "network",
            Self::Filesystem => "filesystem",
            Self::ClipboardRead => "clipboard_read",
            Self::ClipboardWrite => "clipboard_write",
            Self::OpenUrl => "open_url",
            Self::OpenPath => "open_path",
        }
    }
}

impl ActionRisk {
    pub fn requires_confirmation(self) -> bool {
        !matches!(self, Self::Normal)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Run Action",
            Self::Shell => "Run Shell Command",
            Self::Destructive => "Confirm Destructive Action",
            Self::SystemPower => "Confirm System Power Action",
            Self::ProcessKill => "Confirm Process Kill",
            Self::ClipboardClear => "Clear Clipboard History",
        }
    }
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
            risk: ActionRisk::Normal,
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

    pub(crate) fn with_risk(mut self, risk: ActionRisk) -> Self {
        self.risk = risk;
        self
    }

    pub fn run(&self) {
        let _ = self.run_with_policy(ExecutionPolicy::interactive());
    }

    pub fn execution_decision(&self) -> ExecutionDecision {
        ExecutionPolicy::interactive().decide(self)
    }

    pub(crate) fn execution_request(&self) -> Option<ExecutionRequest> {
        match &self.kind {
            ActionKind::Launch(command) => Some(ExecutionRequest::Shell {
                command: ShellCommand::new(command),
            }),
            ActionKind::OpenPath(path) => Some(ExecutionRequest::OpenPath(path.clone())),
            ActionKind::OpenUrl(url) => Some(ExecutionRequest::OpenUrl(url.clone())),
            ActionKind::Copy(text) => Some(ExecutionRequest::Copy(text.clone())),
            ActionKind::Shell(command) => Some(ExecutionRequest::Shell {
                command: command.clone(),
            }),
            ActionKind::Command(command) => Some(ExecutionRequest::Command(command.clone())),
            ActionKind::HttpCopy(req) => Some(ExecutionRequest::Http(req.clone())),
            ActionKind::Media(control) => Some(ExecutionRequest::Media(*control)),
            ActionKind::Notification(action) => Some(ExecutionRequest::Notification(*action)),
            ActionKind::Launcher(_)
            | ActionKind::Form(_)
            | ActionKind::JsonCommand(_)
            | ActionKind::None => None,
        }
    }

    pub(crate) fn run_with_policy(&self, policy: ExecutionPolicy) -> ExecutionDecision {
        let decision = policy.decide(self);
        if matches!(decision, ExecutionDecision::RunNow)
            && let Some(request) = self.execution_request()
        {
            run_execution_request(request);
        }
        decision
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

    #[allow(dead_code)]
    pub(crate) fn json_command_data(&self) -> Option<&JsonCommandAction> {
        match &self.kind {
            ActionKind::JsonCommand(command) => Some(command),
            _ => None,
        }
    }

    pub fn copy_value(&self) {
        run_execution_request(ExecutionRequest::Copy(self.value()));
    }

    pub fn value(&self) -> String {
        match &self.kind {
            ActionKind::Launch(command) | ActionKind::OpenUrl(command) => command.clone(),
            ActionKind::Shell(command) => command.command.clone(),
            ActionKind::Command(command) => command.display(),
            ActionKind::OpenPath(path) => path.display().to_string(),
            ActionKind::Copy(text) => text.clone(),
            ActionKind::HttpCopy(req) => match req {
                HttpRequest::Translate { text, .. } => text.clone(),
                HttpRequest::LocalAiGenerate { query, .. } => query.clone(),
                HttpRequest::AiChat { query, .. } => query.clone(),
            },
            ActionKind::Launcher(_) => self.title.clone(),
            ActionKind::Form(form) => form.command.display(),
            ActionKind::JsonCommand(command) => command.command.command.clone(),
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
            run_execution_request(ExecutionRequest::Command(ProcessCommand::new(
                "xdg-open",
                vec![parent.display().to_string()],
            )));
        }
    }

    pub fn identity(&self) -> String {
        format!("{}:{}", self.category, self.title)
    }
}

pub(crate) fn run_execution_request(request: ExecutionRequest) {
    match request {
        ExecutionRequest::Shell { command } => spawn_shell(&command),
        ExecutionRequest::Command(command) => spawn_command(&command),
        ExecutionRequest::OpenPath(path) => {
            spawn_command(&ProcessCommand::new(
                "xdg-open",
                vec![path.to_string_lossy().to_string()],
            ));
        }
        ExecutionRequest::OpenUrl(url) => {
            spawn_command(&ProcessCommand::new("xdg-open", vec![url]))
        }
        ExecutionRequest::Copy(text) => copy_to_clipboard(&text),
        ExecutionRequest::Http(req) => {
            // Network round-trips (translate / OpenAI / Ollama) can take
            // seconds. Run them off the caller's thread so the GUI never
            // freezes; `copy_to_clipboard` shells out to wl-copy/xclip and is
            // safe to call from a worker thread.
            std::thread::spawn(move || {
                if let Some(result) = execute_http_request(&req) {
                    copy_to_clipboard(&result);
                } else {
                    eprintln!("http request failed");
                }
            });
        }
        ExecutionRequest::Media(control) => crate::media_control(control),
        ExecutionRequest::Notification(action) => match action {
            crate::NotificationAction::ToggleDnd => {
                crate::toggle_dnd();
            }
            crate::NotificationAction::ClearAll => crate::clear_notifications(),
        },
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ActionKind {
    Launch(String),
    OpenPath(PathBuf),
    OpenUrl(String),
    Copy(String),
    Shell(ShellCommand),
    Command(ProcessCommand),
    HttpCopy(HttpRequest),
    Launcher(LauncherCommand),
    Form(ActionForm),
    JsonCommand(JsonCommandAction),
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

#[derive(Debug, Clone)]
pub(crate) struct ProcessCommand {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) env: HashMap<String, String>,
}

impl ProcessCommand {
    pub(crate) fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            env: HashMap::new(),
        }
    }

    pub(crate) fn with_env(
        program: impl Into<String>,
        args: Vec<String>,
        env: HashMap<String, String>,
    ) -> Self {
        Self {
            program: program.into(),
            args,
            env,
        }
    }

    pub(crate) fn display(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
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

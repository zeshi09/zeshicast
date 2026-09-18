use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::execute_http_request;

#[cfg(test)]
thread_local! {
    /// Per-thread count of requests that actually reached [`execute`]'s tail.
    /// Thread-local so parallel tests cannot observe each other's executions.
    pub(crate) static EXEC_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn take_exec_count() -> usize {
    EXEC_COUNT.with(std::cell::Cell::take)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandArgumentKind {
    Text,
    Number,
    Path,
    Bool,
    Enum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScriptMode {
    #[default]
    FullOutput,
    Compact,
    Silent,
    Inline,
}

pub fn percent_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                use std::fmt::Write;
                let _ = write!(encoded, "%{:02X}", byte);
            }
        }
    }
    encoded
}

#[derive(Debug, Clone)]
pub struct ActionFormField {
    pub name: String,
    pub kind: CommandArgumentKind,
    pub required: bool,
    pub default: String,
    pub options: Vec<String>,
    pub current_value: String,
    pub percent_encoded: bool,
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
    /// Capability ceiling inherited from the manifest that produced the form.
    pub(crate) capabilities: CapabilitySet,
    /// Risk of submitting this form; a form is never more trusted than the
    /// action it came from.
    pub(crate) risk: ActionRisk,
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
    Shell {
        command: ShellCommand,
    },
    Command(ProcessCommand),
    OpenPath(PathBuf),
    OpenUrl(String),
    Copy(String),
    Http(HttpRequest),
    Media(crate::MediaControl),
    Notification(crate::NotificationAction),
    /// Run one item of an extension through that extension's own JSON-RPC
    /// `execute` method (P1.5c). The id names an item *in that extension*, so it
    /// is not a command line and never reaches a shell.
    ExtensionExec {
        binary: PathBuf,
        id: String,
        /// What the producing extension was granted. The reply's effects are
        /// checked against this, so a reply cannot ask for more than the
        /// manifest declared.
        granted: CapabilitySet,
    },
}

impl ExecutionRequest {
    /// Capabilities this request needs. An empty set means "in-process effect
    /// only" (media keys, notification toggles).
    pub(crate) fn required_capabilities(&self) -> CapabilitySet {
        match self {
            Self::Shell { .. } | Self::Command(_) => CapabilitySet::new(vec![Capability::Shell]),
            // An extension item may do anything the extension itself could; it
            // stays behind the manifest's `shell` capability (no relaxation).
            Self::ExtensionExec { .. } => CapabilitySet::new(vec![Capability::Shell]),
            Self::OpenPath(_) => CapabilitySet::new(vec![Capability::OpenPath]),
            Self::OpenUrl(_) => CapabilitySet::new(vec![Capability::OpenUrl]),
            Self::Copy(_) => CapabilitySet::new(vec![Capability::ClipboardWrite]),
            Self::Http(_) => CapabilitySet::new(vec![Capability::Network]),
            Self::Media(_) | Self::Notification(_) => CapabilitySet::empty(),
        }
    }
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

    fn is_confirmed(self) -> bool {
        self.confirmed
    }

    /// Preflight decision for `action`: capability ceiling, then confirmation.
    /// Does not execute anything — execution always goes through [`execute`].
    pub fn decide(self, action: &Action) -> ExecutionDecision {
        ExecutionTicket::for_action(self, action).preflight(action.execution_request().as_ref())
    }
}

/// Everything an execution path must present before a request may run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecutionTicket {
    pub(crate) policy: ExecutionPolicy,
    /// Capabilities granted to the action that produced the request.
    pub(crate) capabilities: CapabilitySet,
    /// Risk of the originating action; drives the confirmation gate.
    pub(crate) risk: ActionRisk,
}

impl ExecutionTicket {
    fn for_action(policy: ExecutionPolicy, action: &Action) -> Self {
        Self {
            policy,
            capabilities: action.capabilities.clone(),
            risk: action.risk,
        }
    }

    /// Ticket for built-in launcher code that acts with the daemon's own
    /// authority and whose confirmation the caller already obtained.
    pub(crate) fn confirmed() -> Self {
        Self {
            policy: ExecutionPolicy::confirmed(),
            capabilities: CapabilitySet::trusted(),
            risk: ActionRisk::Normal,
        }
    }

    /// The single gate: capability ceiling first, then confirmation. Fails
    /// closed — an unknown/absent request is denied, not run.
    pub(crate) fn preflight(&self, request: Option<&ExecutionRequest>) -> ExecutionDecision {
        let Some(request) = request else {
            return ExecutionDecision::Denied("action has no executable request".to_string());
        };
        let required = request.required_capabilities();
        if !self.capabilities.covers(&required) {
            return ExecutionDecision::Denied(missing_capabilities(&required, &self.capabilities));
        }
        if self.risk.requires_confirmation() && !self.policy.is_confirmed() {
            return ExecutionDecision::NeedsConfirmation(self.risk);
        }
        ExecutionDecision::RunNow
    }
}

fn missing_capabilities(required: &CapabilitySet, granted: &CapabilitySet) -> String {
    let missing = required
        .iter()
        .filter(|capability| !granted.allows(*capability))
        .map(Capability::label)
        .collect::<Vec<_>>()
        .join(", ");
    format!("missing capability: {missing}")
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
    pub(crate) script_mode: Option<ScriptMode>,
    /// Capability ceiling for this action. Built-in actions are `trusted`;
    /// actions derived from commands/scripts/manifests narrow it to what the
    /// manifest declared.
    pub(crate) capabilities: CapabilitySet,
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

/// Capability ceiling attached to one execution path.
///
/// Capabilities are enforced in exactly one place — [`execute`] — so a new
/// call site cannot skip the check by forgetting to ask for one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CapabilitySet {
    capabilities: Vec<Capability>,
}

impl CapabilitySet {
    pub(crate) fn new(capabilities: Vec<Capability>) -> Self {
        let mut set = Self::empty();
        for capability in capabilities {
            set.grant(capability);
        }
        set
    }

    /// Add one capability to the set (idempotent).
    pub(crate) fn grant(&mut self, capability: Capability) {
        if !self.capabilities.contains(&capability) {
            self.capabilities.push(capability);
        }
    }

    /// No capability at all: only in-process effects (media keys,
    /// notification toggles) may run with this set.
    pub(crate) fn empty() -> Self {
        Self {
            capabilities: Vec::new(),
        }
    }

    /// Everything the daemon itself may do. Reserved for code paths that are
    /// not derived from user-authored manifests (built-in launcher rows,
    /// internal helpers); manifests must never grant this.
    pub(crate) fn trusted() -> Self {
        Self::new(vec![
            Capability::Shell,
            Capability::Network,
            Capability::Filesystem,
            Capability::ClipboardRead,
            Capability::ClipboardWrite,
            Capability::OpenUrl,
            Capability::OpenPath,
        ])
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = Capability> + '_ {
        self.capabilities.iter().copied()
    }

    /// Like [`Self::iter`]-membership, but `network` also grants `open_url` and
    /// `filesystem` also grants `open_path`, matching the documented manifest
    /// semantics.
    pub(crate) fn allows(&self, required: Capability) -> bool {
        self.capabilities.iter().any(|capability| {
            *capability == required
                || matches!(
                    (*capability, required),
                    (Capability::Network, Capability::OpenUrl)
                        | (Capability::Filesystem, Capability::OpenPath)
                )
        })
    }

    pub(crate) fn covers(&self, required: &CapabilitySet) -> bool {
        required.iter().all(|capability| self.allows(capability))
    }
}

impl From<Vec<Capability>> for CapabilitySet {
    fn from(capabilities: Vec<Capability>) -> Self {
        Self::new(capabilities)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A no-op request outside the GUI: media control does nothing without the
    /// `gui` feature, so the counters below stay deterministic.
    fn media_action(risk: ActionRisk) -> Action {
        Action::new(
            "Media",
            "Play/Pause",
            ActionKind::Media(crate::MediaControl::PlayPause),
            0,
        )
        .with_risk(risk)
    }

    #[test]
    fn gateway_denies_shell_without_capability() {
        let _ = take_exec_count();
        let action = Action::new(
            "Ext",
            "run",
            ActionKind::Shell(ShellCommand::new("true")),
            0,
        )
        .with_capabilities(CapabilitySet::empty());

        let decision = action.run_with_policy(ExecutionPolicy::confirmed());
        assert!(
            matches!(decision, ExecutionDecision::Denied(_)),
            "expected denial, got {decision:?}"
        );
        assert_eq!(take_exec_count(), 0, "a denied request must not execute");
    }

    #[test]
    fn gateway_requires_confirmation_for_risk() {
        let _ = take_exec_count();
        let action = media_action(ActionRisk::Shell);

        assert_eq!(
            action.run_with_policy(ExecutionPolicy::interactive()),
            ExecutionDecision::NeedsConfirmation(ActionRisk::Shell)
        );
        assert_eq!(take_exec_count(), 0);

        assert_eq!(
            action.run_with_policy(ExecutionPolicy::confirmed()),
            ExecutionDecision::RunNow
        );
        assert_eq!(take_exec_count(), 1);
    }

    #[test]
    fn gateway_is_the_only_execution_point() {
        let _ = take_exec_count();
        let action = media_action(ActionRisk::Normal);
        assert_eq!(
            action.run_with_policy(ExecutionPolicy::interactive()),
            ExecutionDecision::RunNow
        );
        assert_eq!(take_exec_count(), 1);

        assert_eq!(
            execute(
                ExecutionRequest::Media(crate::MediaControl::PlayPause),
                &ExecutionTicket::confirmed()
            ),
            ExecutionDecision::RunNow
        );
        assert_eq!(take_exec_count(), 1);

        let denied = execute(
            ExecutionRequest::Shell {
                command: ShellCommand::new("true"),
            },
            &ExecutionTicket {
                policy: ExecutionPolicy::confirmed(),
                capabilities: CapabilitySet::empty(),
                risk: ActionRisk::Normal,
            },
        );
        assert!(matches!(denied, ExecutionDecision::Denied(_)));
        assert_eq!(take_exec_count(), 0);
    }

    #[test]
    fn secondary_actions_share_single_risk_table() {
        let shell = Action::new(
            "Script",
            "s",
            ActionKind::Shell(ShellCommand::new("true")),
            0,
        )
        .with_risk(ActionRisk::Shell);
        let clipboard = Action::new("Clipboard", "c", ActionKind::Copy("x".to_string()), 0);

        assert_eq!(
            secondary_action_risk(&shell, SecondaryActionKind::Run),
            ActionRisk::Shell
        );
        assert_eq!(
            secondary_action_risk(&shell, SecondaryActionKind::RunInTerminal),
            ActionRisk::Shell
        );
        assert_eq!(
            secondary_action_risk(&clipboard, SecondaryActionKind::Run),
            ActionRisk::Normal
        );
        assert_eq!(
            secondary_action_risk(&clipboard, SecondaryActionKind::CopyValue),
            ActionRisk::Normal
        );
        assert_eq!(
            secondary_action_risk(&clipboard, SecondaryActionKind::DeleteClipboardItem),
            ActionRisk::Destructive
        );
        assert_eq!(
            secondary_action_risk(&clipboard, SecondaryActionKind::ClearClipboardHistory),
            ActionRisk::ClipboardClear
        );
    }

    #[test]
    fn an_extension_item_never_becomes_a_shell_command() {
        // The regression this guards: the item id used to be handed to `sh -c`
        // as if it were a command line.
        let action = Action::new(
            "Extension: demo",
            "Deploy",
            ActionKind::ExtensionItem {
                binary: PathBuf::from("/usr/bin/demo-extension"),
                id: "deploy".to_string(),
            },
            100,
        )
        .with_capabilities(CapabilitySet::new(vec![Capability::Shell]));

        match action.execution_request() {
            Some(ExecutionRequest::ExtensionExec { binary, id, .. }) => {
                assert_eq!(binary, PathBuf::from("/usr/bin/demo-extension"));
                assert_eq!(id, "deploy");
            }
            other => panic!("expected an extension exec request, got {other:?}"),
        }
    }

    #[test]
    fn an_extension_reply_effect_needs_the_capability_it_asks_for() {
        let reply = crate::services::extension_protocol::ExtensionExecuteResult {
            success: true,
            output: None,
            open_url: Some("https://example.com".to_string()),
            copy_text: Some("secret".to_string()),
            notify: None,
        };

        assert!(
            extension_reply_effects(&reply, &CapabilitySet::empty()).is_empty(),
            "an empty ceiling must neither open a URL nor copy text"
        );

        let granted = CapabilitySet::new(vec![Capability::OpenUrl, Capability::ClipboardWrite]);
        assert_eq!(extension_reply_effects(&reply, &granted).len(), 2);
    }

    #[test]
    fn a_reply_cannot_reach_the_filesystem_through_a_file_url() {
        let reply = crate::services::extension_protocol::ExtensionExecuteResult {
            success: true,
            output: None,
            open_url: Some("file:///etc/shadow".to_string()),
            copy_text: None,
            notify: None,
        };

        // `open_url` alone must not open local paths, exactly as for search
        // results.
        let granted = CapabilitySet::new(vec![Capability::OpenUrl]);
        assert!(extension_reply_effects(&reply, &granted).is_empty());
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
    RunInTerminal,
    CopyValue,
    TypeText,
    OpenParent,
    Pin,
    Unpin,
    DeleteClipboardItem,
    ClearClipboardHistory,
}

/// Risk of a secondary action. Single source of truth: both the model
/// (`Zeshicast`) and the launcher UI ask this function, so the two tables can
/// never drift apart (P1.1).
pub(crate) fn secondary_action_risk(action: &Action, kind: SecondaryActionKind) -> ActionRisk {
    match kind {
        SecondaryActionKind::Run => action.risk,
        // Running an action in a terminal hands its text to a shell, so it is
        // always at least Shell risk — even for an action that is otherwise
        // blocked (whose `value()` falls back to manifest-controlled text).
        SecondaryActionKind::RunInTerminal => ActionRisk::Shell,
        SecondaryActionKind::DeleteClipboardItem => ActionRisk::Destructive,
        SecondaryActionKind::ClearClipboardHistory => ActionRisk::ClipboardClear,
        _ => ActionRisk::Normal,
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LauncherCommand {
    AiChat,
    AiChatWithPrompt(String),
    Audio,
    Dashboard,
    Emoji,
    Fonts,
    Media,
    Network,
    Notifications,
    SystemMonitor,
    WindowGrid,
    CreateSnippet(String),
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

#[allow(dead_code)]
pub(crate) const SCORE_PIN_BOOST: i32 = 700;
#[allow(dead_code)]
pub(crate) const SCORE_WINDOW_BOOST: i32 = 280;
#[allow(dead_code)]
pub(crate) const SCORE_COMPOSITOR_BOOST: i32 = 260;
#[allow(dead_code)]
pub(crate) const SCORE_PROCESS_BOOST: i32 = 240;
#[allow(dead_code)]
pub(crate) const SCORE_APP_BOOST: i32 = 100;
#[allow(dead_code)]
pub(crate) const SCORE_CLIPBOARD_BOOST: i32 = 35;

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
            script_mode: None,
            capabilities: CapabilitySet::trusted(),
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

    pub(crate) fn with_capabilities(mut self, capabilities: impl Into<CapabilitySet>) -> Self {
        self.capabilities = capabilities.into();
        self
    }

    pub fn with_script_mode(mut self, mode: ScriptMode) -> Self {
        self.script_mode = Some(mode);
        self
    }

    pub fn script_mode(&self) -> Option<ScriptMode> {
        self.script_mode
    }

    pub fn run(&self) {
        let _ = self.run_with_policy(ExecutionPolicy::interactive());
    }

    pub fn execution_decision(&self) -> ExecutionDecision {
        ExecutionPolicy::interactive().decide(self)
    }

    pub(crate) fn execution_request(&self) -> Option<ExecutionRequest> {
        match &self.kind {
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
            ActionKind::ExtensionItem { binary, id } => Some(ExecutionRequest::ExtensionExec {
                binary: binary.clone(),
                id: id.clone(),
                granted: self.capabilities.clone(),
            }),
            ActionKind::Launcher(_)
            | ActionKind::Form(_)
            | ActionKind::JsonCommand(_)
            | ActionKind::None => None,
        }
    }

    pub(crate) fn run_with_policy(&self, policy: ExecutionPolicy) -> ExecutionDecision {
        let ticket = ExecutionTicket::for_action(policy, self);
        let Some(request) = self.execution_request() else {
            return ExecutionDecision::Denied("action has no executable request".to_string());
        };
        execute(request, &ticket)
    }

    pub fn launcher_command(&self) -> Option<LauncherCommand> {
        match &self.kind {
            ActionKind::Launcher(command) => Some(command.clone()),
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
        execute(
            ExecutionRequest::Copy(self.value()),
            &ExecutionTicket::confirmed(),
        );
    }

    pub fn value(&self) -> String {
        match &self.kind {
            ActionKind::OpenUrl(command) => command.clone(),
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
            // The id is the value a row shows and copies; the binary is not.
            ActionKind::ExtensionItem { id, .. } => id.clone(),
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
            execute(
                ExecutionRequest::Command(ProcessCommand::new(
                    "xdg-open",
                    vec![parent.display().to_string()],
                )),
                &ExecutionTicket::confirmed(),
            );
        }
    }

    pub fn identity(&self) -> String {
        format!("{}:{}", self.category, self.title)
    }
}

/// **The** execution point of the daemon. Every path that can run a command,
/// open a URL or path, copy to the clipboard, or touch the network must go
/// through here: [`ExecutionTicket::preflight`] enforces the capability ceiling
/// and the confirmation gate before anything is spawned (fail-closed).
///
/// `run_verified_request` below is private and has exactly one caller — this
/// function; `grep -rn "run_execution_request" src/` is expected to stay empty.
pub(crate) fn execute(request: ExecutionRequest, ticket: &ExecutionTicket) -> ExecutionDecision {
    match ticket.preflight(Some(&request)) {
        ExecutionDecision::RunNow => {
            run_verified_request(request);
            ExecutionDecision::RunNow
        }
        decision => decision,
    }
}

/// Private tail of [`execute`]: only reached once a request passed preflight.
fn run_verified_request(request: ExecutionRequest) {
    #[cfg(test)]
    EXEC_COUNT.with(|count| count.set(count.get() + 1));
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
                    #[cfg(feature = "gui")]
                    gtk::glib::idle_add_once(move || {
                        crate::push_notification(
                            "Zeshicast",
                            "HTTP Request Failed",
                            "Check AI or translation endpoint configuration",
                            0,
                        );
                        if !crate::is_dnd_enabled() {
                            crate::ui::show_notification_osd(
                                None,
                                "Zeshicast",
                                "HTTP Request Failed",
                                "Check AI or translation endpoint configuration",
                                "",
                                4500,
                            );
                        }
                    });
                    eprintln!("http request failed");
                }
            });
        }
        ExecutionRequest::ExtensionExec {
            binary,
            id,
            granted,
        } => {
            // A JSON-RPC round trip with its own timeout: keep it off the
            // caller's thread, like the HTTP round trips above.
            std::thread::spawn(move || run_extension_item(&binary, &id, &granted));
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

/// How long one extension `execute` call may run before its child is killed.
const EXTENSION_EXEC_TIMEOUT_MS: u64 = 5_000;

/// Run one extension item over the extension's own JSON-RPC protocol (P1.5c).
///
/// This is what an extension result does now: the *extension* decides what the
/// id means. Previously the id was handed to `sh -c` as if it were a command
/// line, so an item whose id happened to look like a command ran that command
/// instead of the item the user picked.
fn run_extension_item(binary: &Path, id: &str, granted: &CapabilitySet) {
    let reply = match crate::services::extension_protocol::execute(
        binary,
        id,
        None,
        EXTENSION_EXEC_TIMEOUT_MS,
    ) {
        Ok(reply) => reply,
        Err(error) => {
            report_extension_problem(&format!("'{id}' failed: {error}"));
            return;
        }
    };

    if !reply.success {
        let detail = reply
            .output
            .clone()
            .unwrap_or_else(|| "the extension refused the item".to_string());
        report_extension_problem(&format!("'{id}': {detail}"));
        return;
    }

    for effect in extension_reply_effects(&reply, granted) {
        apply_extension_effect(effect);
    }

    if let Some(message) = reply.output.clone().or_else(|| reply.notify.clone()) {
        crate::push_notification("Zeshicast", "Extension", &message, 0);
    }
}

/// The effects an extension's reply may have, filtered through the same gate the
/// items themselves pass (fail closed): a reply asking for more than the
/// manifest granted is dropped, not applied.
fn extension_reply_effects(
    reply: &crate::services::extension_protocol::ExtensionExecuteResult,
    granted: &CapabilitySet,
) -> Vec<ActionKind> {
    use crate::search::commands::{ActionIntent, gate_action_intent};

    let granted: Vec<Capability> = granted.iter().collect();
    let requested = [
        reply.open_url.clone().map(ActionIntent::OpenUrl),
        reply.copy_text.clone().map(ActionIntent::Copy),
    ];

    requested
        .into_iter()
        .flatten()
        .filter_map(|intent| {
            // A denial here means "do not do this", never "do it anyway".
            let gated = gate_action_intent(intent, &granted);
            gated.denial.is_none().then_some(gated.kind)
        })
        .collect()
}

/// Apply one effect an extension's reply asked for.
fn apply_extension_effect(effect: ActionKind) {
    match effect {
        ActionKind::OpenUrl(url) => {
            spawn_command(&ProcessCommand::new("xdg-open", vec![url]));
        }
        ActionKind::OpenPath(path) => {
            spawn_command(&ProcessCommand::new(
                "xdg-open",
                vec![path.to_string_lossy().to_string()],
            ));
        }
        ActionKind::Copy(text) => copy_to_clipboard(&text),
        // `gate_action_intent` cannot let anything else through for the intents
        // above; doing nothing is the safe response if that ever changes.
        _ => {}
    }
}

/// Tell the user an extension item failed. The old path could not: it ran the id
/// as a shell command and whatever happened to stderr was invisible.
fn report_extension_problem(detail: &str) {
    eprintln!("extension item {detail}");
    crate::push_notification("Zeshicast", "Extension Item Failed", detail, 0);
}

#[derive(Debug, Clone)]
pub(crate) enum ActionKind {
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
    /// One item of an external extension, run over its own protocol.
    ExtensionItem {
        binary: PathBuf,
        id: String,
    },
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

fn spawn_shell(command: &ShellCommand) {
    let mut process = Command::new("sh");
    process.arg("-c").arg(&command.command).envs(&command.env);
    match crate::process::spawn_detached(&mut process) {
        // Deliberately no command text: the daemon's stdout may be read by
        // other processes and shell commands can embed secrets.
        Ok(child) => log::info!("started: sh -c <command> (pid {})", child.pid()),
        // No command text here either: placeholder-expanded commands can
        // carry secrets and the journal is not private.
        Err(error) => log::warn!("failed to start shell command: {error}"),
    }
}

fn spawn_command(command: &ProcessCommand) {
    let mut process = Command::new(&command.program);
    process.args(&command.args).envs(&command.env);
    match crate::process::spawn_detached(&mut process) {
        Ok(child) => log::info!("started: {} (pid {})", command.program, child.pid()),
        Err(error) => log::warn!("failed to start {}: {error}", command.program),
    }
}

fn copy_to_clipboard(text: &str) {
    if let Some(path) = crate::clipboard_image_path(text) {
        if crate::copy_clipboard_image(path) {
            log::info!("copied image to clipboard");
        } else {
            log::warn!("copy failed; install wl-clipboard or xclip to copy images");
        }
        return;
    }

    let copied =
        copy_with("wl-copy", &[], text) || copy_with("xclip", &["-selection", "clipboard"], text);

    if copied {
        log::info!("copied to clipboard");
    } else {
        // Never echo the copied value: it may be a secret and the journal is
        // not guaranteed to stay private either.
        log::warn!("copy failed; install wl-clipboard or xclip to copy automatically");
    }
}

pub fn copy_text(text: &str) {
    copy_to_clipboard(text);
}

fn copy_with(program: &str, args: &[&str], text: &str) -> bool {
    let Ok(child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
    else {
        return false;
    };
    // M-9: every early return below used to leave the writer running (and
    // unreaped). The guard kills and waits for it on all paths.
    let mut child = crate::process::ChildGuard::new(child);

    let Some(mut stdin) = child.child_mut().stdin.take() else {
        return false;
    };

    if stdin.write_all(text.as_bytes()).is_err() {
        return false;
    }
    drop(stdin);

    child
        .child_mut()
        .wait()
        .map(|status| status.success())
        .unwrap_or(false)
}

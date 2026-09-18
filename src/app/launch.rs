//! Running an action and applying its consequences -- the policy from the
//! user's preferences, confirmation, and recording what the action did. The
//! execution point itself is `action.rs` (`execute`, `ExecutionTicket`); this is
//! what the model does around it (P5.3).

use super::*;

impl Zeshicast {
    pub fn run_action(&mut self, action: &Action) -> ExecutionDecision {
        self.run_action_with_policy(action, ExecutionPolicy::interactive())
    }

    pub fn run_action_confirmed(&mut self, action: &Action) -> ExecutionDecision {
        self.run_action_with_policy(action, ExecutionPolicy::confirmed())
    }

    fn run_action_with_policy(
        &mut self,
        action: &Action,
        policy: ExecutionPolicy,
    ) -> ExecutionDecision {
        let decision = action.run_with_policy(policy);
        if !matches!(decision, ExecutionDecision::RunNow) {
            return decision;
        }
        if action.category == "Calculator"
            && let Some((expr, result)) = action.title.split_once(" = ")
        {
            self.record_calc(expr.trim(), result.trim());
        }
        if let Err(error) = self.record_recent(action) {
            eprintln!("failed to record recent action: {error}");
        }
        decision
    }

    pub fn available_secondary_actions(&self, action: &Action) -> Vec<SecondaryAction> {
        use crate::action::ActionPanelSection as S;
        let can_type = is_wtype_available();
        let is_text_action = matches!(action.category.as_str(), "Snippet" | "Clipboard");
        let mut actions = vec![
            SecondaryAction::new(
                SecondaryActionKind::Run,
                "Run",
                "media-playback-start-symbolic",
                S::Primary,
            ),
            SecondaryAction::new(
                SecondaryActionKind::CopyValue,
                "Copy Value",
                "edit-copy-symbolic",
                S::Primary,
            ),
        ];

        if can_type && is_text_action {
            actions.push(SecondaryAction::new(
                SecondaryActionKind::TypeText,
                "Expand (type text)",
                "input-keyboard-symbolic",
                S::Primary,
            ));
        }

        let is_app_or_script = matches!(
            action.category.as_str(),
            "Application" | "Script" | "Command" | "System"
        );
        if is_app_or_script {
            actions.push(SecondaryAction::new(
                SecondaryActionKind::RunInTerminal,
                "Run in Terminal",
                "utilities-terminal-symbolic",
                S::Primary,
            ));
        }

        if action.parent_dir().is_some() {
            actions.push(SecondaryAction::new(
                SecondaryActionKind::OpenParent,
                "Open Containing Folder",
                "folder-open-symbolic",
                S::Primary,
            ));
        }

        if action.category == "Clipboard" {
            actions.push(SecondaryAction::new(
                SecondaryActionKind::DeleteClipboardItem,
                "Delete Clipboard Item",
                "edit-delete-symbolic",
                S::Clipboard,
            ));
            actions.push(SecondaryAction::new(
                SecondaryActionKind::ClearClipboardHistory,
                "Clear Clipboard History",
                "edit-clear-symbolic",
                S::Danger,
            ));
        }

        if self.is_pinned(action) {
            actions.push(SecondaryAction::new(
                SecondaryActionKind::Unpin,
                "Unpin",
                "view-pin-symbolic",
                S::Manage,
            ));
        } else {
            actions.push(SecondaryAction::new(
                SecondaryActionKind::Pin,
                "Pin",
                "view-pin-symbolic",
                S::Manage,
            ));
        }

        actions
    }

    /// Runs a secondary action under an interactive policy. Risky secondary
    /// operations (`DeleteClipboardItem`, `ClearClipboardHistory`) are not
    /// executed and return `ExecutionDecision::NeedsConfirmation` instead.
    pub fn run_secondary_action(
        &mut self,
        action: &Action,
        secondary: SecondaryActionKind,
    ) -> io::Result<ExecutionDecision> {
        self.run_secondary_action_with_confirmation(action, secondary, false)
    }

    /// Runs a secondary action after the caller has obtained user
    /// confirmation for its risk.
    pub fn run_secondary_action_confirmed(
        &mut self,
        action: &Action,
        secondary: SecondaryActionKind,
    ) -> io::Result<ExecutionDecision> {
        self.run_secondary_action_with_confirmation(action, secondary, true)
    }

    fn run_secondary_action_with_confirmation(
        &mut self,
        action: &Action,
        secondary: SecondaryActionKind,
        confirmed: bool,
    ) -> io::Result<ExecutionDecision> {
        let risk = crate::action::secondary_action_risk(action, secondary);
        if risk.requires_confirmation() && !confirmed {
            return Ok(ExecutionDecision::NeedsConfirmation(risk));
        }
        match secondary {
            SecondaryActionKind::Run => {
                let policy = if confirmed {
                    ExecutionPolicy::confirmed()
                } else {
                    ExecutionPolicy::interactive()
                };
                Ok(self.run_action_with_policy(action, policy))
            }
            SecondaryActionKind::CopyValue => {
                action.copy_value();
                Ok(ExecutionDecision::RunNow)
            }
            SecondaryActionKind::TypeText => {
                type_text_via_wtype(&action.value());
                Ok(ExecutionDecision::RunNow)
            }
            SecondaryActionKind::OpenParent => {
                action.open_parent_dir();
                Ok(ExecutionDecision::RunNow)
            }
            SecondaryActionKind::Pin => {
                self.pin_action(action)?;
                Ok(ExecutionDecision::RunNow)
            }
            SecondaryActionKind::Unpin => {
                self.unpin_action(action)?;
                Ok(ExecutionDecision::RunNow)
            }
            SecondaryActionKind::DeleteClipboardItem => {
                self.delete_clipboard_item(action)?;
                Ok(ExecutionDecision::RunNow)
            }
            SecondaryActionKind::ClearClipboardHistory => {
                self.clear_clipboard_history()?;
                Ok(ExecutionDecision::RunNow)
            }
            SecondaryActionKind::RunInTerminal => {
                // The terminal runs the action's text through a login shell, so
                // it is gated exactly like a command: capabilities from the
                // action, Shell risk, and the decision returned upward instead
                // of spawning directly (P1.1).
                let pref = self
                    .get_preferences()
                    .get("default_terminal")
                    .map(String::as_str);
                let command = crate::services::terminal::terminal_process_command(
                    &action.value(),
                    pref,
                    true,
                );
                let ticket = ExecutionTicket {
                    policy: if confirmed {
                        ExecutionPolicy::confirmed()
                    } else {
                        ExecutionPolicy::interactive()
                    },
                    capabilities: action.capabilities.clone(),
                    risk: ActionRisk::Shell,
                };
                Ok(execute(ExecutionRequest::Command(command), &ticket))
            }
        }
    }

    /// Submits a form under the interactive policy: the manifest capability
    /// ceiling carried by the form is checked first, and a risky form returns
    /// [`ExecutionDecision::NeedsConfirmation`] instead of executing.
    pub fn run_form_action(
        &mut self,
        action: &Action,
        values: HashMap<String, String>,
    ) -> ExecutionDecision {
        self.run_form_action_with_policy(action, values, ExecutionPolicy::interactive())
    }

    /// Submits a form after the caller obtained user confirmation.
    pub fn run_form_action_confirmed(
        &mut self,
        action: &Action,
        values: HashMap<String, String>,
    ) -> ExecutionDecision {
        self.run_form_action_with_policy(action, values, ExecutionPolicy::confirmed())
    }

    fn run_form_action_with_policy(
        &mut self,
        action: &Action,
        values: HashMap<String, String>,
        policy: ExecutionPolicy,
    ) -> ExecutionDecision {
        let ActionKind::Form(form) = &action.kind else {
            return ExecutionDecision::Denied("action is not a form".to_string());
        };
        let ticket = ExecutionTicket {
            policy,
            capabilities: form.capabilities.clone(),
            risk: form.risk,
        };
        let mut args = form.current_args.clone();
        args.extend(values);
        let context = PlaceholderContext {
            query: form.partial_query.clone(),
            clipboard: self.clipboard_history.first().cloned().unwrap_or_default(),
            args,
            preferences: Cow::Owned(form.preferences.clone()),
            now: SystemTime::now(),
        };
        let env: HashMap<String, String> = form
            .env
            .iter()
            .map(|(k, v)| (k.clone(), expand_placeholders(v, &context)))
            .collect();
        let request = match &form.command {
            ActionFormCommand::Shell(command) => ExecutionRequest::Shell {
                command: ShellCommand::with_env(expand_placeholders_shell(command, &context), env),
            },
            ActionFormCommand::Argv { program, args } => {
                ExecutionRequest::Command(ProcessCommand::with_env(
                    expand_placeholders(program, &context),
                    args.iter()
                        .map(|arg| expand_placeholders(arg, &context))
                        .collect(),
                    env,
                ))
            }
        };
        let decision = execute(request, &ticket);
        if matches!(decision, ExecutionDecision::RunNow)
            && let Err(e) = self.record_recent(action)
        {
            eprintln!("failed to record recent: {e}");
        }
        decision
    }

    pub(crate) fn record_recent(&mut self, action: &Action) -> io::Result<()> {
        let identity = action.identity().to_lowercase();
        storage::usage_record(&self.config_dir, &identity)
            .map_err(|e| io::Error::other(e.to_string()))?;
        self.recent.retain(|e| e != &identity);
        self.recent.insert(0, identity.clone());
        self.recent.truncate(50);
        *self.frequencies.entry(identity).or_insert(0) += 1;
        Ok(())
    }
}

# Security

This document describes the security model for zeshicast's local launcher,
daemon, and custom command system.

## Assets

Zeshicast handles local user data:

- clipboard text and cached clipboard images;
- notification history received through `org.freedesktop.Notifications`;
- command preferences, including API keys and tokens in `preferences.toml`;
- recent action usage, pins, aliases, quicklinks, snippets, and command files;
- file paths found by the local file index;
- AI and translation prompts sent to configured endpoints.

## Trust Boundaries

Zeshicast is a local-first app. It does not sandbox custom commands. Anything in
`~/.config/zeshicast/commands/*.toml` or
`~/.config/zeshicast/extensions/*/` is user-trusted extension code.

Important boundaries:

- GUI and CLI code run as the current user.
- Custom argv commands run as the current user via direct `program` + `args`
  (no shell), but they still require the `shell`/`exec` permission and a
  confirmation when they come from an extension manifest.
- Custom shell commands run as the current user through `sh -c`.
- JSON command producers run as shell commands and can return actions.
- Executable extension `binaries` (JSON-RPC) run as the current user; they are
  loaded only when the manifest grants `capabilities = ["shell"]` and every
  result they return is gated like a JSON action.
- External tools such as `niri`, `hyprctl`, `swaymsg`, `wpctl`, `nmcli`,
  `wl-copy`, `wl-paste`, `xclip`, `wtype`, `grim`, `slurp`, and `tar` are
  trusted as installed on the host.
- AI/translation requests go to the configured endpoint. Remote providers can
  receive prompts and API credentials needed for the request.

## Extension Permissions

The `permissions` field is enforced for custom commands.

- Shell-mode commands require `shell`.
- JSON-mode producer commands require `shell`.
- Argv-mode commands run without a shell, but they still execute local code:
  an argv command from an extension requires the manifest to grant `shell`
  (or `exec`), and every argv action asks for confirmation. A hand-written
  argv command in the user's own config directory (no manifest) is gated by
  that same confirmation prompt.
- An extension manifest is a **ceiling**: a command's effective permissions are
  `declared ∩ manifest.capabilities`. A command cannot widen what its manifest
  granted, and a command inside a manifest with `capabilities = []` gets
  nothing at all.
- Commands with required (missing) arguments are only offered as an argument
  form if the command has the capability to run at all; otherwise the action is
  blocked. A submitted form carries the same capability ceiling and risk as the
  action it came from.
- Returned JSON actions require matching capabilities:
  - `shell` for shell actions;
  - `network` or `open_url` for remote URL opening;
  - `filesystem` or `open_path` for path opening;
  - `clipboard_write` for copy actions.
- Extension search results (JSON-RPC `search`) pass through the same gate:
  `open_url`/`copy_text` results need `open_url`/`clipboard_write`, and results
  that execute the extension (no `open_url`/`copy_text`) need `shell` and always
  ask for confirmation. Such a result carries the extension's item **id**, which
  goes back to that extension over its own JSON-RPC `execute` method
  (`ExecutionRequest::ExtensionExec`) -- the id is never a shell command line.
  Effects in the `execute` reply (`open_url`, `copy_text`) pass through the same
  gate again, so a reply cannot ask for more than the manifest granted; `file://`
  URLs still need `open_path`.
- The `commands` field of a manifest accepts only `*.toml` files. Executable
  JSON-RPC extensions must be declared in `binaries`, which requires
  `capabilities = ["shell"]`; entries without it are not loaded.

Unknown or undeclared capabilities cause actions to be blocked rather than run.
Risky actions are also routed through execution policy and confirmation where
appropriate.

Extension scripts (Raycast/Vicinae-style script commands) require the `shell`
capability from the extension manifest; without it they are shown as blocked,
and with it they still go through the shell confirmation prompt. A script
header cannot opt out of that prompt with `@raycast.needsConfirmation false`
when the script belongs to an extension. Note that listing an extension's
directories in `script_dirs` loads its scripts as plain user scripts
(`origin: None`): they skip the manifest capability check and are treated as
trusted user scripts, gated only by the shell confirmation prompt.

## Placeholder Handling

Placeholder values such as `{{query}}`, `{{clipboard}}`, `{{arg:*}}`, and
`{{pref:*}}` are escaped for the quoting context they are inserted into before
expansion into a shell command. All three contexts are covered:

- unquoted (`--env {{arg:env}}`) → wrapped in `'…'` with `'\''` for embedded
  apostrophes;
- inside an author-written single-quoted run (`'prefix {{arg:env}}'`) → the
  value is inserted literally, only an apostrophe is escaped, and the run is
  not closed early;
- inside an author-written double-quoted run (`"{{arg:env}}"`) → `\`, `"`,
  `$` and `` ` `` are backslash-escaped.

This protects against user input like `$(...)` or `; reboot` being interpreted
as extra shell syntax. In argv mode, placeholders are expanded as literal
argument strings and are never passed through a shell.

Not covered by the escaping (documented gap, tracked in the plan): a placeholder
inside a **here-document body** (`cat <<EOF`), where the shell keeps expanding
`$()` while treating quotes literally. A placeholder sitting inside a `#` comment
is emitted literally instead of being substituted, since no escaping can make a
multi-line value safe there.

The command template itself is still executable shell code. Review the whole
template before installing a command, especially when it uses `{{clipboard}}` or
preferences containing secrets.

## Destructive Actions

System power actions, process kill actions, clipboard clear, shell actions, and
other risky actions are marked with `ActionRisk` and go through confirmation in
the GTK UI. The action executor also refuses to run risky actions without a
confirmed policy path.

Dashboard and view-level command buttons route through typed execution requests;
the UI should not call raw process-spawn helpers directly. Every launcher and
CLI *action* now goes through a single gateway (`execute` in `src/action.rs`),
which checks the request's required capabilities against the action's
`ExecutionTicket` and then applies the confirmation policy. A request that fails
either gate is returned as `Denied`/`NeedsConfirmation` and never reaches the
spawn helpers.

Two deliberate exceptions remain, both reachable only *after* the gateway
confirmed the action and both spawning argv rather than shell text:

- script output capture (`search/scripts.rs::run_script_stdout_with_args`) runs
the already-confirmed script to collect stdout for the result view;
- the JSON command producer (`search/commands.rs::run_json_command`) runs the
command's own `sh -c` template (already gated by the JSON action's confirmation
and its `shell` capability).

Extension items are executed over the extension's JSON-RPC `execute` method
(P1.5c). A failure there is reported to the user instead of being invisible on a
shell's stderr, and the item's `output`/`notify` are shown as a notification.

## Known Gaps (as of 2026-08)

No known gaps as of 2026-08-23. The previously documented deviation (CLI REPL
clear/delete of clipboard history executing without confirmation) was closed:
risky secondary actions in `run_secondary_action` (`src/app.rs`) now require a
confirmed execution path on all callers.

## Import And Export

Import validates archive members before extraction:

- only a single `zeshicast/` root is accepted;
- absolute paths and `..` components are rejected;
- symlinks in the archive are rejected;
- extraction happens in a staging directory before replacement.

Export excludes API keys and secret-like preference keys by default. Use
`zeshicast --export <file> --include-secrets` only for trusted backups and
trusted storage.

## Reporting Security Issues

Treat command files and exported configs as sensitive. If you find a path where
untrusted input bypasses capability checks, confirmation, archive validation, or
placeholder quoting, document the exact command/config needed to reproduce it.

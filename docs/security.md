# Security

This document describes the security model for zeshicast's local launcher,
daemon, and custom command system.

The claims below carry the test that holds them up, in an HTML comment
(`&lt;!-- test: name --&gt;`) so it is invisible when rendered. `cargo test` runs
`documented_invariants_have_the_tests_they_name` (`src/lib.rs`), which fails if a
named test does not exist: a renamed test cannot leave this document quietly
asserting something nothing verifies.

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
  <!-- test: argv_command_requires_confirmation -->
- Custom shell commands run as the current user through `sh -c`.
- JSON command producers run as shell commands and can return actions.
- Executable extension `binaries` (JSON-RPC) run as the current user; they are
  loaded only when the manifest grants `capabilities = ["shell"]` and every
  result they return is gated like a JSON action.
- A clipboard value that claims to be an image is only read as one when the path
  is an existing `.png` file directly inside the clipboard cache directory;
  anything else stays text, so a pasted `\x01zeshicast-image:/etc/passwd`
  cannot make the daemon read an arbitrary file (M-15).
  <!-- test: spoofed_image_entry_is_rejected_and_stored_as_text -->
  <!-- test: image_path_outside_cache_is_rejected -->
- External tools such as `niri`, `hyprctl`, `swaymsg`, `wpctl`, `nmcli`,
  `wl-copy`, `wl-paste`, `xclip`, `wtype`, `grim`, `slurp`, and `tar` are
  trusted as installed on the host.
- AI/translation requests go to the configured endpoint. Remote providers can
  receive prompts and API credentials needed for the request.

## Extension Permissions

The `permissions` field is enforced for custom commands.

- Shell-mode commands require `shell`.
  <!-- test: shell_command_without_shell_permission_is_blocked -->
- JSON-mode producer commands require `shell`.
  <!-- test: json_shell_actions_without_shell_permission_are_blocked -->
- Argv-mode commands run without a shell, but they still execute local code:
  an argv command from an extension requires the manifest to grant `shell`
  (or `exec`), and every argv action asks for confirmation. A hand-written
  argv command in the user's own config directory (no manifest) is gated by
  that same confirmation prompt.
- An extension manifest is a **ceiling**: a command's effective permissions are
  `declared ∩ manifest.capabilities`. A command cannot widen what its manifest
  granted, and a command inside a manifest with `capabilities = []` gets
  nothing at all.
  <!-- test: extension_manifest_caps_command_capabilities -->
- Commands with required (missing) arguments are only offered as an argument
  form if the command has the capability to run at all; otherwise the action is
  blocked. A submitted form carries the same capability ceiling and risk as the
  action it came from.
  <!-- test: command_arguments_disable_action_when_required_value_missing -->
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
  <!-- test: an_extension_item_never_becomes_a_shell_command -->
  <!-- test: gate_action_intent_blocks_an_extension_item_without_shell -->
  Effects in the `execute` reply (`open_url`, `copy_text`) pass through the same
  gate again, so a reply cannot ask for more than the manifest granted; `file://`
  URLs still need `open_path`.
  <!-- test: an_extension_reply_effect_needs_the_capability_it_asks_for -->
  <!-- test: a_reply_cannot_reach_the_filesystem_through_a_file_url -->
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
when the script belongs to an extension.
<!-- test: extension_script_without_shell_capability_is_blocked -->
<!-- test: extension_script_with_shell_capability_runs_with_confirmation -->
<!-- test: extension_script_ignores_needs_confirmation_false -->
<!-- test: extension_script_form_inherits_manifest_capabilities -->
Note that listing an extension's
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
as extra shell syntax.
<!-- test: command_placeholders_neutralize_shell_injection -->
<!-- test: command_placeholders_neutralize_apostrophe_injection -->
<!-- test: command_placeholders_neutralize_shell_injection_inside_double_quotes -->
In argv mode, placeholders are expanded as literal
argument strings and are never passed through a shell.
<!-- test: argv_placeholders_expand_without_shell_quoting -->

A placeholder inside a **here-document body** (`cat <<EOF`) is a fourth case,
because a body is not a quoting context: quotes are literal there while `$`,
backticks and backslashes keep expanding unless the delimiter was quoted. The
body is detected, and the value is escaped for it -- `\`, `$` and a backtick get
a backslash, and nothing else is special. With a quoted delimiter (`<<'EOF'`)
nothing expands, so the value is inserted unchanged. One case cannot be made
safe: a value containing a line equal to the delimiter would end the body and
turn the rest into commands, so the placeholder is emitted literally instead of
being substituted.
<!-- test: a_placeholder_in_a_heredoc_body_is_escaped_for_that_body -->
<!-- test: a_quoted_delimiter_keeps_the_body_literal -->
<!-- test: a_value_that_would_end_the_body_is_not_substituted -->
<!-- test: two_heredocs_on_one_line_are_treated_as_expanding -->
<!-- test: a_placeholder_on_the_command_line_is_not_a_body -->

A placeholder sitting inside a `#` comment is likewise emitted literally, since
no escaping can make a multi-line value safe there.

The command template itself is still executable shell code. Review the whole
template before installing a command, especially when it uses `{{clipboard}}` or
preferences containing secrets.

## The systemd Unit

The daemon is a user service, and systemd applies its hardening to the daemon's
*children* as well: mount namespaces, seccomp filters, `NoNewPrivileges` and
capability bounds are all inherited across `fork` + `exec`. Since the daemon's
job is to launch the user's applications, opening paths and URLs, any directive
that constrains a child is a behaviour change for everything started from the
palette -- the very thing a launcher must not have.

The unit therefore keeps only directives that a launched application cannot
notice, plus one deliberate exception. What was removed, and what it broke for a
launched app (P4.1):

| Removed | What it broke |
| --- | --- |
| `ProtectSystem=full`, `ProtectHome=read-only`, `ReadWritePaths` | an app could not write `~/.mozilla`, `~/.config`, or its own state |
| `PrivateTmp=true` | a launched app saw a different `/tmp` than a shell (sockets and IPC files there became invisible) |
| `MemoryDenyWriteExecute=true` | JIT runtimes died: firefox, electron, Java, Wine |
| `SystemCallArchitectures=native` | 32-bit binaries lost their syscalls: Steam, 32-bit Wine, older games |
| `SystemCallFilter=@system-service` | `ptrace` was blocked, breaking debuggers and anything using it |
| `RestrictSUIDSGID=true` | setuid/setgid helpers stopped working when launched from the palette |
| `CapabilityBoundingSet=` | file-capability binaries lost their capabilities (`ping`, `dumpcap`) |
| `RestrictAddressFamilies=` | `AF_BLUETOOTH`, `AF_PACKET`, `AF_VSOCK` were unavailable to a launched app |
| `LockPersonality=true` | Wine and runtimes that call `personality()` failed |
| `RestrictRealtime=true` | a launched app could not request realtime scheduling |

Kept: `NoNewPrivileges`, `ProtectClock`, `ProtectHostname`,
`ProtectKernelTunables`, `ProtectKernelModules`, `ProtectControlGroups`. The
`Protect*` ones only make kernel interfaces read-only, which no application
started from the palette depends on.
<!-- test: the_daemon_unit_does_not_restrict_what_it_launches -->

`NoNewPrivileges` is kept on purpose and is the single known difference from
launching the same application in a shell: a setuid or file-capability binary
runs without the privilege escalation, so `sudo`/`pkexec` do not work when
started from the palette.

The consequence is measured, not estimated: `systemd-analyze security` scores
this unit **8.1 EXPOSED** where the sandboxed version scored **3.4 OK**. That is
the price of being able to launch applications at all, and it is why the daemon's
protection against untrusted input lives in code -- the single execution point,
the capability gate, ids that never become shell command lines -- rather than in
systemd. The alternative, if the systemd-level hardening is ever wanted back, is
not a stronger sandbox on this unit: it is routing every launch through a
transient unit so the application is not an inherited child.

## Destructive Actions

System power actions, process kill actions, clipboard clear, shell actions, and
other risky actions are marked with `ActionRisk` and go through confirmation in
the GTK UI. The action executor also refuses to run risky actions without a
confirmed policy path.
<!-- test: executor_requests_confirmation_for_power_action -->
<!-- test: executor_denies_non_executable_action -->
<!-- test: executor_allows_copy_without_confirmation -->

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
risky secondary actions in `run_secondary_action` (`src/app/launch.rs`) now
require a confirmed execution path on all callers.
<!-- test: executor_requests_confirmation_for_power_action -->

## Import And Export

Import validates archive members before extraction:

- only a single `zeshicast/` root is accepted;
- absolute paths and `..` components are rejected;
- symlinks in the archive are rejected;
- extraction happens in a staging directory before replacement.
<!-- test: import_rejects_traversal_and_unsafe_members -->
<!-- test: import_rejects_bomb -->

Export excludes API keys and secret-like preference keys by default. Use
`zeshicast --export <file> --include-secrets` only for trusted backups and
trusted storage.
<!-- test: export_preferences_sanitizer_removes_secret_keys -->
<!-- test: safe_export_excludes_clipboard_db -->
<!-- test: export_does_not_follow_symlinks -->

## Reporting Security Issues

Treat command files and exported configs as sensitive. If you find a path where
untrusted input bypasses capability checks, confirmation, archive validation, or
placeholder quoting, document the exact command/config needed to reproduce it.

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
- Custom argv commands run as the current user via direct `program` + `args`.
- Custom shell commands run as the current user through `sh -c`.
- JSON command producers run as shell commands and can return actions.
- External tools such as `niri`, `hyprctl`, `swaymsg`, `wpctl`, `nmcli`,
  `wl-copy`, `wl-paste`, `xclip`, `wtype`, `grim`, `slurp`, and `tar` are
  trusted as installed on the host.
- AI/translation requests go to the configured endpoint. Remote providers can
  receive prompts and API credentials needed for the request.

## Extension Permissions

The `permissions` field is enforced for custom commands.

- Shell-mode commands require `shell`.
- JSON-mode producer commands require `shell`.
- Argv-mode commands do not require `shell` because they do not invoke `sh -c`.
- Returned JSON actions require matching capabilities:
  - `shell` for shell actions;
  - `network` or `open_url` for remote URL opening;
  - `filesystem` or `open_path` for path opening;
  - `clipboard_write` for copy actions.

Unknown or undeclared capabilities cause actions to be blocked rather than run.
Risky actions are also routed through execution policy and confirmation where
appropriate.

## Placeholder Handling

Placeholder values such as `{{query}}`, `{{clipboard}}`, `{{arg:*}}`, and
`{{pref:*}}` are shell-quoted before expansion into shell commands. This protects
against user input like `$(...)` or `; reboot` being interpreted as extra shell
syntax. In argv mode, placeholders are expanded as literal argument strings and
are never passed through a shell.

The command template itself is still executable shell code. Review the whole
template before installing a command, especially when it uses `{{clipboard}}` or
preferences containing secrets.

## Destructive Actions

System power actions, process kill actions, clipboard clear, shell actions, and
other risky actions are marked with `ActionRisk` and go through confirmation in
the GTK UI. The action executor also refuses to run risky actions without a
confirmed policy path.

Dashboard and view-level command buttons route through typed execution requests;
the UI should not call raw process-spawn helpers directly.

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

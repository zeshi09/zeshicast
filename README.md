# zeshicast

Raycast-like launcher for Linux, written in Rust. The CLI works without GUI
dependencies; the GTK4 launcher is behind the `gui` feature.

## Run

```bash
cargo run --bin zeshicast -- firefox
# With layer-shell overlay (Wayland, recommended):
nix develop -f shell.nix --command cargo run --features gui,layer-shell --bin zeshicast-gtk
# Without layer-shell (X11 or fallback):
nix develop -f shell.nix --command cargo run --features gui --bin zeshicast-gtk
```

Results carry Raycast-style metadata: title, subtitle, category, score and icon.
The GTK launcher renders this as a compact command list with action buttons.
Secondary actions are provided by the core: run, copy value, open containing
folder, pin, unpin, and clipboard cleanup actions.

Built-in Linux actions:

```text
system lock       lock screen
system suspend    suspend machine
system settings   open desktop settings
system restart    reboot machine
system power      power off machine
proc firefox      search processes and build kill actions
ai explain monads ask AI (LiteLLM/OpenAI-compatible) — response copied to clipboard
trans hello in ru translate via LibreTranslate — result copied to clipboard
translate hi in de same as trans, explicit prefix
```

Restart and power-off actions are only returned for explicit `system ...`
queries. AI and translate results are only shown for their respective prefixes.

App detection reads `$XDG_DATA_HOME` and `$XDG_DATA_DIRS` so NixOS paths like
`/run/current-system/sw/share/applications` and per-user profile directories
are discovered automatically.

GTK shortcuts:

```text
Enter       run selected result (opens form panel for commands with missing arguments)
Ctrl+Enter  copy selected value
Ctrl+K      open action panel (pin, alias, secondary actions)
Ctrl+B      open extension browser (list all custom commands)
Esc         hide or close
```

## Daemon mode

Start a hidden resident GTK process. It keeps the launcher index warm, so later
invocations show the window quickly. It also records text clipboard changes into
the searchable clipboard history.

```bash
nix develop -f shell.nix --command cargo run --features gui,layer-shell --bin zeshicast-gtk -- --daemon
nix develop -f shell.nix --command cargo run --features gui,layer-shell --bin zeshicast-gtk
nix develop -f shell.nix --command cargo run --features gui,layer-shell --bin zeshicast-gtk -- --quit
```

## User install

```bash
scripts/install-user.sh --enable-daemon --start-daemon
```

This installs binaries to `~/.local/bin`, desktop entries to
`~/.local/share/applications`, and a systemd user service to
`~/.config/systemd/user/zeshicast-gtk.service`.

Useful commands:

```bash
systemctl --user status zeshicast-gtk.service
systemctl --user restart zeshicast-gtk.service
systemctl --user enable zeshicast-gtk.service
systemctl --user disable zeshicast-gtk.service
```

## Wayland hotkey

Bind this command in your compositor:

```bash
~/.local/bin/zeshicast-gtk
```

Examples:

```ini
# sway / i3-style config
bindsym $mod+space exec ~/.local/bin/zeshicast-gtk
```

```ini
# Hyprland
bind = SUPER, SPACE, exec, ~/.local/bin/zeshicast-gtk
```

For GNOME/KDE, add the same command through the desktop environment keyboard
shortcuts settings.

## Config

```text
~/.config/zeshicast/quicklinks.txt   lines: Name | tag1,tag2 = https://example.com?q={{query}}
~/.config/zeshicast/snippets.txt     lines: Name | tag1,tag2 = text to copy
~/.config/zeshicast/commands/*.toml  custom shell commands
~/.config/zeshicast/preferences.toml global extension preferences
~/.config/zeshicast/clipboard.txt    updated automatically by GTK daemon
~/.config/zeshicast/aliases.txt      lines: ff = Firefox
~/.config/zeshicast/pins.txt         lines: App:Firefox or Firefox
~/.config/zeshicast/recent.txt       updated automatically
```

Pins can be edited from the CLI action menu or the GTK action bar.

Supported placeholders in quicklinks, snippets and commands:

```text
{{query}}       current search query
{{arg:name}}    typed command argument
{{pref:name}}   extension preference value
{{clipboard}}   latest clipboard history entry
{{date}}        local date as YYYY-MM-DD
{{time}}        local time as HH:MM:SS
{{datetime}}    local date and time
{{date:%d.%m.%Y}} custom chrono/strftime format
{{time:%H:%M}}  custom chrono/strftime format
{{calc:2 + 2}}  calculator result
```

Examples:

```text
# ~/.config/zeshicast/quicklinks.txt
Google | web,search = https://www.google.com/search?q={{query}}
GitHub | dev,search = https://github.com/search?q={{query}}

# ~/.config/zeshicast/snippets.txt
Today | date = {{date}}
Meeting | work,date = {{date:%d.%m.%Y}} {{time:%H:%M}}
Debug | dev = Query: {{query}}, Clipboard: {{clipboard}}
VAT | finance = Total: {{calc:100 * 1.2}}
```

Tags are optional. Search matches both names and tags.

Custom commands are one TOML file per command:

```toml
# ~/.config/zeshicast/commands/deploy.toml
name = "Deploy"
mode = "shell"
keyword = "deploy"
argument_hint = "<env> <service>"
command = "cd '{{pref:workspace}}' && deploy --env {{arg:env}} --service '{{arg:service}}' --force {{arg:force}}"
description = "Deploy a service"
tags = ["work", "devops"]
icon = "utilities-terminal-symbolic"
arguments = [
  { name = "env", type = "enum", required = true, options = ["dev", "prod"] },
  { name = "service", type = "text", required = true },
  { name = "force", type = "bool", default = "false" }
]

[preferences]
workspace = "~/Code"

[env]
DEPLOY_ENV = "{{arg:env}}"
DEPLOY_TOKEN = "{{pref:deploy_token}}"
  permissions = ["shell"]   # optional: "shell", "network", "filesystem"
```

Only `name` and `command` are required. Optional fields are `category`,
`keyword`, `argument_hint`, `arguments`, `preferences`, `env`, `description`,
`tags` and `icon`. A keyword turns the command into a direct command mode:
searching `deploy prod api worker` runs the command with `{{query}}` set to
`prod api worker`, `{{arg:env}}` set to `prod`, and `{{arg:service}}` set to
`api worker`. Supported argument types are `text`, `number`, `path`, `bool` and
`enum`. Commands with missing required arguments are shown as disabled warning
actions until the input is complete. Commands run through `sh -c`, so keep files
under your own config directory and treat them like executable scripts.

Command `[env]` values are expanded with the same placeholders as `command` and
are injected only into that command process.

JSON commands can return dynamic result lists:

```toml
# ~/.config/zeshicast/commands/search-docs.toml
name = "Search Docs"
mode = "json"
keyword = "docs"
argument_hint = "<query>"
command = "zeshicast-doc-search '{{query}}'"
arguments = [
  { name = "query", type = "text", required = true }
]
```

The command stdout must be either an array or an object with `results`:

```json
[
  {
    "title": "Rust",
    "subtitle": "Open rust-lang.org",
    "icon": "emblem-web-symbolic",
    "action": { "type": "open_url", "value": "https://www.rust-lang.org" }
  },
  {
    "title": "Copy crate name",
    "action": { "type": "copy", "value": "gtk4" }
  }
]
```

Supported JSON action types are `open_url`, `open_path`, `copy`, `shell` and
`none`. JSON commands execute only on direct keyword invocation, for example
`docs gtk listbox`.

Global preferences live in `~/.config/zeshicast/preferences.toml`:

```toml
workspace = "/home/me/Code"
default_env = "dev"
dry_run = true
```

The global file overrides defaults from each command's `[preferences]` table.
Preferences currently support scalar TOML values: string, integer, float and
boolean.

AI and translate preferences:

```toml
# ~/.config/zeshicast/preferences.toml
ai_endpoint    = "http://localhost:4000/v1"   # LiteLLM or any OpenAI-compatible endpoint
ai_model       = "gpt-4"
ai_api_key     = ""
translate_endpoint = "https://libretranslate.com"
translate_api_key  = ""
translate_target   = "en"
```

## Nix package

Build and run with Nix flakes:

```bash
nix build
./result/bin/zeshicast-gtk
```

## Raycast Platform Plan

Implemented foundation:

```text
Core actions       apps, files, calculator, shell, quicklinks, snippets
User memory        recent actions, pins, aliases
Action UX          primary action, secondary actions, action panel
Automation         clipboard history, placeholders, custom TOML commands
Linux shell        xdg-open, .desktop parsing, GTK4/Wayland launcher
Audio actions      volume up/down, mute, mic mute, brightness (wpctl/brightnessctl)
Network actions    wifi toggle, network settings (nmcli)
Niri actions       screenshot, workspace/window control (niri msg)
Layer shell        gtk4-layer-shell overlay window (Wayland, --features layer-shell)
XDG app detection  XDG_DATA_DIRS/XDG_DATA_HOME so NixOS paths are found
HTTP actions       AI chat (OpenAI-compatible) and translation (LibreTranslate)
Command forms      GTK form panel for commands with missing required arguments
Extension browser  Ctrl+B panel listing all custom TOML commands
Nix package        flake.nix packages.default for nix build
```

Next platform layers:

```text
1. Extension commands
   - command manifests, keywords, argument hints: implemented
   - typed arguments: text, number, path, enum, boolean: implemented in core
   - per-command preferences: implemented in core
   - environment variables: implemented in core
   - JSON result lists: implemented in core
   - GTK argument form panel: implemented

2. Built-in Linux extensions
   - system actions: lock screen, suspend, settings, restart, power off: implemented in core
   - process actions: search process and kill: implemented in core
   - audio actions: volume up/down, mute, mic mute, brightness: implemented in core
   - network actions: wifi toggle, network settings: implemented in core
   - Niri IPC actions: screenshot, workspace/window control: implemented in core
   - window/workspace actions for other compositors (Hyprland, sway)

3. GUI platform UX
   - command forms for typed arguments: implemented
   - extension browser: implemented (Ctrl+B)
   - preferences editor
   - richer action panel with command metadata

4. Extension runtime
   - executable/script commands with JSON input/output
   - result lists emitted by extensions
   - permission model for shell/network/filesystem capabilities

5. Distribution
   - example extension pack
   - import/export user config
   - Nix package: implemented (nix build)
```

The short-term goal is to make custom commands powerful enough that built-in
features and user extensions share the same action model. After that, GTK can
render forms and preferences without changing the core execution model.

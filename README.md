# zeshicast

Raycast-like launcher for Linux, written in Rust. The CLI works without GUI
dependencies; the GTK4 launcher is behind the `gui` feature.

See [docs/vicinae-parity-roadmap.md](docs/vicinae-parity-roadmap.md) for the
plan to evolve Zeshicast toward a Vicinae-like Rust/GTK application.

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
Secondary actions (copy value, open folder, pin, unpin, set alias) are available
in the action panel.

## Built-in queries

```text
firefox                   Search installed .desktop apps (reads XDG_DATA_DIRS)
file invoice              Search files under $HOME, open via xdg-open
calc (12 + 8) / 5         Evaluate an expression
shell systemctl status    Run a shell command
system lock               Lock screen
system suspend            Suspend machine
system settings           Open desktop settings
system restart            Reboot
system power              Power off
proc firefox              Search processes and build kill actions
audio vol                 Volume up/down, mute, mic mute
audio brightness          Brightness up/down
media next                MPRIS playback controls through playerctl
notify dnd                Notification/DND actions for swaync or dunst
net wifi                  Toggle Wi-Fi, open network settings
niri screenshot           Interactive screenshot (region)
niri workspace            Focus next/previous workspace, move window
hypr fullscreen           Hyprland: fullscreen, float, close, workspaces
sway reload               Sway: reload, fullscreen, float, close, workspaces
win firefox               Focus a running window (niri/Hyprland/sway)
ai explain monads         Ask local AI through Ollama; response copied to clipboard
trans hello in ru         Translate via LibreTranslate — result copied to clipboard
translate hi in de        Same as trans, explicit prefix
clip password             Search clipboard history
```

`system restart` and `system power` are only returned for explicit `system ...`
queries. `ai`, `trans`/`translate`, `niri`, `hypr`, and `sway` results are only
shown for their respective prefixes.

App detection reads `$XDG_DATA_HOME` and `$XDG_DATA_DIRS` so NixOS paths like
`/run/current-system/sw/share/applications` are discovered automatically.
Flatpak apps in `~/.local/share/flatpak/exports/share/applications` and
`/var/lib/flatpak/exports/share/applications` are also included.
App icons are sourced from the `Icon=` field in each `.desktop` file.
The binary stem (e.g. `zen-beta`) is added to the search haystack so typing
the executable name finds the app.

## GTK shortcuts

```text
Enter        Run selected result (opens form panel for commands with missing args)
Ctrl+Enter   Copy selected value
Ctrl+K       Action panel — pin, unpin, set alias, secondary actions
Ctrl+B       Extension browser — list all custom commands
Ctrl+,       Preferences editor — AI endpoint, model, translate settings
Ctrl+D       Dashboard — clock, system, network, audio, media, notifications
Ctrl+T       System Monitor — load, memory, disk, temperatures, top processes
Ctrl+N       Network view — interfaces, Wi-Fi, VPN, DNS
Ctrl+M       Media view — playerctl/MPRIS playback
Ctrl+U       Notifications view — DND, close all, dunst history when available
Ctrl+I       AI Chat — local prompt/answer view
Ctrl+H       Clipboard history — copy, delete, clear recorded items
Esc          Hide (daemon mode) or quit
Up/Down      Move selection
```

## CLI usage

```bash
zeshicast                       Start interactive REPL
zeshicast firefox               Print matching actions and exit
zeshicast --export [file]       Export config to tar.gz (default: zeshicast-config.tar.gz)
zeshicast --import file.tar.gz  Import config from tar.gz
```

## Daemon mode

A hidden resident GTK process keeps the launcher index warm for instant display
and records text clipboard changes into the searchable clipboard history.

```bash
nix develop -f shell.nix --command cargo run --features gui,layer-shell --bin zeshicast-gtk -- --daemon
nix develop -f shell.nix --command cargo run --features gui,layer-shell --bin zeshicast-gtk
nix develop -f shell.nix --command cargo run --features gui,layer-shell --bin zeshicast-gtk -- --quit
```

## User install

```bash
scripts/install-user.sh --enable-daemon --start-daemon
```

Installs binaries to `~/.local/bin`, desktop entries to
`~/.local/share/applications`, and a systemd user service to
`~/.config/systemd/user/zeshicast-gtk.service`.

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

```ini
# Niri
spawn-at-startup "~/.local/bin/zeshicast-gtk" "--daemon"
# bind in config.kdl: key "Super+Space" { spawn "~/.local/bin/zeshicast-gtk"; }

# Hyprland
bind = SUPER, SPACE, exec, ~/.local/bin/zeshicast-gtk

# sway / i3
bindsym $mod+space exec ~/.local/bin/zeshicast-gtk
```

For GNOME/KDE, add the command through the desktop environment keyboard shortcuts
settings.

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

Pins and aliases can be set from the CLI action menu or the GTK action panel.

### Placeholders

```text
{{query}}           current search query
{{arg:name}}        typed command argument
{{pref:name}}       extension preference value
{{clipboard}}       latest clipboard history entry
{{date}}            local date as YYYY-MM-DD
{{time}}            local time as HH:MM:SS
{{datetime}}        local date and time
{{date:%d.%m.%Y}}   custom chrono/strftime format
{{time:%H:%M}}      custom time format
{{calc:2 + 2}}      calculator result
```

### Quicklinks and snippets

```text
# ~/.config/zeshicast/quicklinks.txt
Google    | web,search = https://www.google.com/search?q={{query}}
GitHub    | dev,search = https://github.com/search?q={{query}}

# ~/.config/zeshicast/snippets.txt
Today     | date      = {{date}}
Meeting   | work,date = {{date:%d.%m.%Y}} {{time:%H:%M}}
Debug     | dev       = Query: {{query}}, Clipboard: {{clipboard}}
VAT       | finance   = Total: {{calc:100 * 1.2}}
```

Tags are optional. Search matches both names and tags.

### Custom commands (shell mode)

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
permissions = ["shell"]
arguments = [
  { name = "env",     type = "enum", required = true, options = ["dev", "prod"] },
  { name = "service", type = "text", required = true },
  { name = "force",   type = "bool", default = "false" }
]

[preferences]
workspace = "~/Code"

[env]
DEPLOY_ENV   = "{{arg:env}}"
DEPLOY_TOKEN = "{{pref:deploy_token}}"
```

Only `name` and `command` are required. Optional fields: `category`, `keyword`,
`argument_hint`, `arguments`, `preferences`, `env`, `description`, `tags`,
`icon`, `permissions`.

A keyword enables direct command mode: `deploy prod api worker` sets `{{query}}`
to `prod api worker`, `{{arg:env}}` to `prod`, and `{{arg:service}}` to
`api worker`. Supported argument types: `text`, `number`, `path`, `bool`, `enum`.
Commands with missing required arguments are shown as disabled warning actions
until the input is complete. Commands run through `sh -c`.

`[env]` values are expanded with the same placeholders as `command` and injected
only into that command process.

`permissions` is informational: `"shell"`, `"network"`, `"filesystem"`.

### Custom commands (JSON mode)

```toml
# ~/.config/zeshicast/commands/search-docs.toml
name = "Search Docs"
mode = "json"
keyword = "docs"
argument_hint = "<query>"
command = "my-doc-search '{{query}}'"
arguments = [
  { name = "query", type = "text", required = true }
]
```

The command stdout must be a JSON array (or `{"results": [...]}`) of objects:

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

Supported action types: `open_url`, `open_path`, `copy`, `shell`, `none`.
JSON commands execute only on direct keyword invocation, e.g. `docs gtk listbox`.

### Example extension pack

Ready-to-copy examples are in `packaging/examples/commands/`:

```text
github.toml     gh <query>       — open GitHub search in browser
weather.toml    weather <city>   — open wttr.in forecast
dict.toml       dict <word>      — open Merriam-Webster
docker-ps.toml  docker <filter>  — list running containers (JSON mode, Enter stops)
git-log.toml    git-log <path>   — show recent commits in pref:workspace
```

Copy any file to `~/.config/zeshicast/commands/` to enable it.

### Global preferences

```toml
# ~/.config/zeshicast/preferences.toml
workspace  = "/home/me/Code"
default_env = "dev"
```

The global file overrides per-command `[preferences]` defaults. Supported TOML
scalar types: string, integer, float, boolean.

Edit via `Ctrl+,` in the GTK launcher or directly in the file.

### AI and translate preferences

```toml
# ~/.config/zeshicast/preferences.toml
ai_provider        = "ollama"                      # set to "openai" for /v1/chat/completions
ui_font_family    = "Outfit, Inter, Noto Sans, sans-serif"
ui_font_size      = "15"
show_status_strip  = "true"
status_items       = "clock,date,network,battery,audio,media"
dashboard_enabled  = "true"
network_enabled    = "true"
media_enabled      = "true"
notifications_enabled = "true"
ai_enabled         = "true"
dashboard_poll_interval_ms = "2000"
ollama_endpoint    = "http://localhost:11434"
ollama_model       = "gemma4:e4b"
ai_endpoint        = "http://localhost:11434/v1"   # used when ai_provider = "openai"
ai_model           = "gemma4:e4b"
ai_api_key         = ""
translate_endpoint = "https://libretranslate.com"
translate_api_key  = ""
translate_target   = "en"
```

## Nix package

```bash
nix build
./result/bin/zeshicast-gtk
```

## Implemented features

```text
Core search        apps (.desktop, XDG_DATA_DIRS), files, calculator, shell
Quicklinks         keyword-triggered browser URLs with placeholders
Snippets           copy-to-clipboard text templates with placeholders
Custom commands    shell and JSON modes, typed args, forms, env, preferences
User memory        recent actions, pins, aliases
Action UX          primary action, secondary actions, action panel (Ctrl+K)
Clipboard history  GTK daemon records changes; searchable via clip prefix
Placeholders       {{query}} {{arg:}} {{pref:}} {{clipboard}} {{date}} {{calc:}}
System actions     lock, suspend, settings, restart, power off
Process actions    search running processes, build kill actions
Audio actions      volume up/down, mute, mic mute, brightness
Network actions    Wi-Fi toggle, network settings
Niri actions       screenshot, workspaces, window control (niri msg)
Hyprland actions   screenshot, fullscreen, float, close, workspaces (hyprctl)
Sway actions       screenshot, fullscreen, float, close, workspaces (swaymsg)
Window switching   win <query> — focus open windows (niri/Hyprland/sway, live query)
AI chat            Ollama-compatible local endpoint, OpenAI-compatible quick mode optional
Dashboard          optional control view with clock, system, network, audio, media, notifications
System monitor     /proc stats, thermal sensors, top process list, terminate selected process
Network view       interfaces, IP/MAC copy, DNS, nmcli Wi-Fi/VPN snapshot and actions
Media view         playerctl/MPRIS status and previous/play-pause/next controls
Notifications      swaync/dunst state, DND/close-all actions, dunst history parsing
Translation        LibreTranslate with language suffix (trans hello in ru)
GTK4 launcher      Layer-shell overlay (Wayland), daemon mode, clipboard monitor
Command forms      GTK form panel for commands with missing required arguments
Extension browser  Ctrl+B — list and inspect all custom commands
Preferences editor Ctrl+, — edit AI/translate settings without touching files
Permission field   permissions = ["shell","network","filesystem"] in command TOML
Import/export      zeshicast --export / --import for config backup and migration
Example extensions packaging/examples/commands/ — github, weather, dict, docker, git
Nix package        flake.nix packages.default for nix build
```

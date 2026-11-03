# Changelog

All notable user-facing changes. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versions follow [SemVer](https://semver.org/) (0.x: anything may change).

Every commit that changes behaviour adds a line under **Unreleased**
(`docs/product-plan-2026-11-02.md`, Definition of Done #4). A release moves
the section under a version heading and tags it `vX.Y.Z`.

## [Unreleased]

### Changed

- Visual pass (kill-ai-slop, impeccable, ui-ux-pro-max): secondary text uses
  two contrast tiers (≥4.5:1) instead of 23 ad-hoc dim values; result tiles
  follow the handoff mockup (tinted, not saturated); the footer and status
  chips use theme icons instead of Unicode glyphs; the back button is a
  visible pill; brightness rows say "Display", not "Audio"; System Monitor
  shows memory and disk in GiB with the detail small beside the figure and
  labels the load average; selected text takes the accent colour.
- Empty states: the clipboard explains that nothing was copied yet and
  disables "Copy", notifications centre "No notifications" and disable
  "Clear All", audio greys out sliders without a device.

### Fixed

- Four CSS properties GTK rejects (`overflow`, `max-width`, `cursor` ×2) are
  gone; the theme loads without parser warnings.
- Wi-Fi rows no longer touch, process memory bars sit on their row's centre,
  and the monitor's graphs grow from the right edge.

### Added

- Volume OSD: `zeshicast-gtk --volume +5|-5|40|mute` and `--mic-mute`
  change the level with `wpctl` and show it at the bottom of the screen with
  a gliding bar; a burst of key presses updates one OSD. Bind the volume
  keys to it (README, "Volume keys"). Shown under Do Not Disturb too.
- Brightness OSD for laptops: `zeshicast-gtk --brightness +10|-10|60`
  changes the backlight through logind (no root, never below 1 %) and shows
  the level; changes from firmware keys or other tools show it too. External
  monitors (DDC/CI) are not supported.
- Volume changes made elsewhere (pavucontrol, a headset, a new default
  device) reach the palette at once through PipeWire events
  (`pw-dump --monitor`) instead of the one-second poll, and show the level
  OSD while the palette is closed. The microphone shows only when its mute
  flips, since calls move its volume on their own.

### Fixed

- The palette shows current data as soon as it opens: the clock redraws on
  show and ticks on the minute, the background poller wakes on show instead
  of finishing a one-second sleep, its first tick refreshes everything and
  publishes each value as soon as it is read, and the status strip and views
  redraw within ~100 ms of new data instead of on the next second.
- With the palette open, network, battery and media changes show at once:
  NetworkManager, UPower and MPRIS signals trigger a read instead of waiting
  for the next poll (access-point scan chatter is ignored).

## [0.3.0] — 2026-11-03

### Upgrading

- The database moves from schema 2 to 7 on first start. Clipboard history
  and snippets are then encrypted; the key goes to your keyring (or
  `~/.config/zeshicast/db.key`). An older zeshicast refuses the migrated
  database, so downgrading means starting over with
  `zeshicast --reset-encrypted-data`.
- JSON-RPC extensions (`binaries`) need `protocol = 1` in `extension.toml`.
- Plaintext files from before SQLite (`clipboard.txt`, `recent.txt`,
  `frequencies.txt`) are only imported into an empty database and never
  deleted; once your history is in SQLite, delete them by hand.

### Changed

- Root search shows a file only when the query is part of its name (scattered
  matches stay under `file …`), and an app always ranks above a file with
  the same name: an app's own name now counts on its own, not only joined
  with its command and comment. `files_in_root = false` keeps files out of
  root search entirely.
- The file index is stored in `zeshicast.db` and kept live with inotify:
  search over files works as soon as the daemon starts (no `$HOME` walk),
  files saved while it runs are found within a second, and files saved
  while it was stopped are found on the next start. `files_exclude` (Privacy
  settings) keeps names and paths out. The database moves to schema 4; an
  older zeshicast refuses to open it.
- **Breaking for JSON-RPC extensions:** an `extension.toml` with `binaries`
  must declare `protocol = 1`. Without it (or with another version) the
  binaries are not run, and searching for the extension's name shows why.
  Requests now carry `params.protocol`.
- The first palette show after the daemon starts takes ~115 ms instead of
  ~190 ms: fonts and styles are prepared while the daemon is idle.

### Added

- Quick AI: select text in any window, open the palette and run `fix`,
  `translate` or `summarize`; the answer from the configured AI endpoint
  (Ollama or any OpenAI-compatible one) replaces the selection. The
  selection is read only when the action runs; without a configured model
  the row says what to set. `quick_ai_language` (or `translate_target`)
  picks the translation language.
- systemd user units (`unit …`): failed units first, restart or start in
  one step (restart and stop ask first), and the journal of a unit in the
  output view (`unit <name>` → "Logs").
- System Monitor: pressure stall (PSI) for CPU, memory and I/O, read/write
  rate and busy time per disk, and GPU load and VRAM for amdgpu (clock for
  i915); NVIDIA through `nvidia-smi` only with `gpu_nvidia_smi = true`.
  Parts the system does not report are left out.
- Power: `profile …` switches the power profile through
  power-profiles-daemon (and says so when it is not running); the battery
  comes from UPower when it runs, with the time left or to full on the
  dashboard, from sysfs otherwise.
- Bluetooth (`bt …`, or a device's name in the main search): turn the
  adapter on or off, connect and disconnect paired devices, with their
  battery level. Read from BlueZ over D-Bus; without BlueZ, `bt` says why.
  `bluetooth_enabled = false` turns it off.
- Notification history survives a daemon restart: the newest 100 are saved
  in `zeshicast.db`, encrypted; private mode saves them without a body. The
  database moves to schema 7.
- Clipboard history keeps the original format: rich text (HTML) and copied
  files paste back as they were copied, together with their plain text;
  "Paste as Plain Text" pastes only the text. Copies marked as passwords
  (`x-kde-passwordManagerHint`) and copies from apps in
  `clipboard_ignore_apps` (password managers by default) are never
  recorded. One `wl-paste` watcher instead of two. The database moves to
  schema 6.
- Clipboard history and snippets are encrypted in `zeshicast.db`
  (ChaCha20-Poly1305). The key lives in the Secret Service keyring, or in
  `~/.config/zeshicast/db.key` (0600) without one. Existing rows are
  encrypted on first start. A lost or wrong key shows nothing and says so
  instead of starting over; `zeshicast --reset-encrypted-data` deletes the
  encrypted data and the key. `--export --include-history` carries the key.
  The database moves to schema 5.
- `zeshicast --extension-install <id>`: installs an extension from the git
  registry (`docs/extensions-registry.md`) only on that command. It checks
  the tarball's sha256, unpacks it only inside `extensions/`, refuses a
  manifest that asks for more than the registry entry lists or fails the
  linter, and asks before installing. `--root` installs into a sandbox.
- `mkZeshicastExtension` (flake `lib`): package an extension for NixOS or
  Home Manager; the build runs the linter with the declared capabilities.
- `--extension-lint --allow cap1,cap2`: an error for every manifest
  capability outside the list.
- Extension screen: Enter on an extension's row in the Extension Browser
  shows what its manifest allows, what each command, script and binary asked
  for and was granted, why a blocked one is blocked, and its latest
  permission log entries. Binaries now appear in the browser too.
- Extension permission log: every capability request from an extension is
  recorded as granted, confirmed, cancelled or denied (`extension_audit` in
  `zeshicast.db`, newest 1000 rows, kept in private mode too). The
  Extension Browser (Ctrl+B) shows the latest under "Permission log".
- Binary cache: CI pushes the x86_64-linux package to
  `zeshicast.cachix.org`; `nix build`/`nix run` on the flake offer it via
  `nixConfig`, and the README shows the two `nix.settings` lines for a system
  that uses the flake as an input.
- `docs/extensions.md`: the guide for extension authors (manifest,
  capabilities, JSON-RPC protocol and limits), a JSON Schema for
  `extension.toml` (`resources/extension.schema.json`) and a template
  extension in `packaging/examples/extension-template/` (sh + jq or Python).
- `zeshicast --extension-lint <dir> [--json]`: explains everything the
  loader would silently skip or block in an extension (mistyped keys,
  unknown capabilities, unsafe or missing paths, missing `shell` or
  `protocol`, scripts without metadata, non-executable binaries); exit 1 on
  errors.

### Fixed

- Snippets are no longer written to `snippets.txt` in plaintext after every
  change; the database is the only copy.
- An extension whose `binaries` are refused for lack of `shell` now shows a
  blocked row when you search for its name, like one refused for its
  protocol, instead of vanishing silently.
- `zeshicast-gtk --quit` (or a crash) no longer leaves the clipboard
  watchers (`wl-paste --watch`) and the niri event stream running: they now
  die with the daemon even when it was started outside systemd.
- A script command saved a moment before it runs no longer fails with
  "Text file busy" when the command captures output.

## [0.2.0] — 2026-11-02

### Added

- `zeshicast-gtk --toggle`: one hotkey shows and hides the palette; ready-made
  niri, Hyprland and Sway bindings in the README.
- Enter on a clipboard or snippet row pastes it into the window the palette
  was opened over (via `wtype`, now bundled in the Nix package). Ctrl+Enter
  still copies; Preferences → General switches Enter back to copy.
- "Paste to Active Window" in the action panel (Ctrl+K) of clipboard and
  snippet rows.
- `zeshicast-gtk --dmenu [-p PROMPT] [--password]`: pick a line from stdin,
  print it on stdout, exit 1 on Esc.
- Command ids (`command/weather`, `script/backup-home`):
  `zeshicast-gtk --cmd list` and `--cmd run <id>`, which runs a command as if
  picked with Enter (confirmations and argument forms included).
- `zeshicast://view/<name>`, `zeshicast://search?q=…` and
  `zeshicast://command/<id>` links; the desktop entry registers the scheme
  and the Nix package now installs the desktop entry. A link never runs a
  command by itself, it only selects it.
- "Copy Hotkey Binding" in the action panel of commands: a binding for the
  running compositor that calls `--cmd run <id>`.
- Wrong keyboard layout recovery: `ашкуащч` finds Firefox.
- `ZESHICAST_TRACE=1` timing marks (startup, show → first frame,
  keystroke → results p50/p95, index builds, RSS) and `scripts/measure.sh`
  to turn them into a report.
- Tips under the empty search (Ctrl+K, prefixes, `--toggle`, paste, views),
  one per opening, 30 in total; Preferences → General → Tips turns them off.
- Search in Preferences: matches labels, keys (`clipboard_retention`) and
  keyboard shortcuts, opens the first matching section, and says so when
  nothing matches.
- Preferences → Appearance → Renderer (Cairo, the default, or GL);
  `GSK_RENDERER` still overrides it.

- When another daemon (mako, swaync, dunst…) owns
  `org.freedesktop.Notifications`, zeshicast now names it in a toast and
  the journal and explains the two ways out, instead of silently losing
  notification history and Do Not Disturb.

### Changed

- Fuzzy search ranks exact > prefix > word start > acronym (`vsc` → Visual
  Studio Code) > substring > compact subsequence, counts characters instead
  of bytes (no double penalty for Cyrillic), and no longer matches letters
  scattered across a long name.
- `file …` / `find …` return only files.
- Results appear about 9 ms after a keystroke (p50, was 108 ms): the search
  starts at once after a pause and only debounces (50 ms) inside a burst.

### Fixed

- Files in `~/Downloads` (and other priority folders) were missing from the
  index when another tree under `$HOME` filled it first, or when the
  launcher was started from `/`.
- `zeshicast-gtk --help` printed into the daemon's journal instead of the
  terminal when a daemon was running.
- Paste and "type text" did nothing when the palette ran without a daemon:
  the delay before `wtype` now lives in the child process.
- Script and JSON command timeouts now hold: a command whose shell does not
  exec its last command (Ubuntu's dash), or that leaves a background job on
  its output, is killed with its whole process group at the deadline instead
  of blocking until the leftover process exits.

### Developer

- `rustfmt.toml` pins edition 2024, so editors' bare `rustfmt` agrees with
  `cargo fmt`; `scripts/pre-commit` runs fmt and clippy.
- One module for all gtk4-layer-shell FFI (`src/ui/layer_shell.rs`).

## 0.1.0 — baseline (2026-05-18 … 2026-10-07)

Unreleased development history before this changelog (170 commits): the
GTK4 palette and daemon, built-in notification server, clipboard history,
MPRIS media, audio, network, system monitor, dashboard, window grid,
commands/scripts/extensions with the capability ceiling, and the security
reviews of 2026-08-23, 2026-09-11 and 2026-10-04. See `git log` and
`docs/` for details.

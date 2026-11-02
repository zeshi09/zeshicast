# Changelog

All notable user-facing changes. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versions follow [SemVer](https://semver.org/) (0.x: anything may change).

Every commit that changes behaviour adds a line under **Unreleased**
(`docs/product-plan-2026-11-02.md`, Definition of Done #4). A release moves
the section under a version heading and tags it `vX.Y.Z`.

## [Unreleased]

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

### Changed

- Fuzzy search ranks exact > prefix > word start > acronym (`vsc` → Visual
  Studio Code) > substring > compact subsequence, counts characters instead
  of bytes (no double penalty for Cyrillic), and no longer matches letters
  scattered across a long name.
- `file …` / `find …` return only files.

### Fixed

- Files in `~/Downloads` (and other priority folders) were missing from the
  index when another tree under `$HOME` filled it first, or when the
  launcher was started from `/`.
- `zeshicast-gtk --help` printed into the daemon's journal instead of the
  terminal when a daemon was running.
- Paste and "type text" did nothing when the palette ran without a daemon:
  the delay before `wtype` now lives in the child process.

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

# Vicinae-like Roadmap for Zeshicast

Goal: make Zeshicast feel like Vicinae, but keep the implementation native Rust
and GTK4 instead of porting Vicinae's C++/Qt/QML code directly.

Companion product plan: `docs/linux-command-center-plan.md` covers the broader
Linux command-center direction: status strip, dashboard, network/media/system
views, notifications, and local AI.

## Current State

Zeshicast already has a useful launcher core:

- GTK4 launcher window with optional layer-shell overlay.
- CLI and GTK binaries.
- XDG `.desktop` application search.
- Home-directory file search.
- Calculator.
- Clipboard history, captured by the GTK daemon.
- Quicklinks and snippets from config files.
- Custom TOML commands in shell and JSON result modes.
- Command argument forms.
- Pins, aliases, recent items, and frequency scoring.
- System, audio, network, process, window, Niri, Hyprland, and sway actions.
- AI and translation actions through HTTP-compatible endpoints.
- User install files for desktop entries and systemd user service.

The prototype-era monolith is gone: `src/lib.rs` is now a thin public facade
(~200 lines of module declarations, re-exports, and tests). Behavior lives in
`src/app.rs` (application state and clipboard store), `src/search/*` (14+
providers behind the `SearchProvider` trait), `src/services/*` (snapshot-style
services), and the UI layer under `src/ui/`. The largest files are now
`src/ui/launcher.rs`, `src/ui/views.rs`, and `src/app.rs`; slimming those is
the remaining architectural work.

## What Vicinae Adds

Vicinae is not just a launcher window. Its repository is organized around a
resident application with services, root providers, extension runtimes, rich
view hosts, and persistent data stores.

Important Vicinae concepts to reproduce in Rust/GTK:

- Root search providers: apps, scripts, extensions, browser tabs, shortcuts.
- Services: app database, clipboard, files, snippets, window manager, audio,
  calculator, extension registry, local storage, toast, settings.
- Multiple view types: list, detail, grid, forms, action panel, settings,
  clipboard history, snippet manager, extension browser.
- Script command compatibility.
- Extension runtime and registry.
- Theming and keyboard-driven UI.

Important Vicinae concepts to avoid copying literally:

- Qt/QML view host architecture.
- CMake/vendor layout.
- TypeScript extension runtime as the first milestone.
- Store integration before local extension/script compatibility is solid.

## Target Architecture

Split the project into clear Rust modules before adding more features:

```text
src/
  lib.rs                    public facade, small
  app.rs                    high-level application state
  action.rs                 Action, ActionKind, secondary actions
  config.rs                 paths, preferences, import/export
  search/
    mod.rs                  SearchProvider trait and aggregation
    apps.rs                 XDG desktop apps
    files.rs                file index and matching
    clipboard.rs            clipboard search
    commands.rs             TOML/script commands
    system.rs               system/audio/network/process actions
    windows.rs              compositor window actions
    calculator.rs           calculator provider
  services/
    clipboard.rs            capture/storage/delete/clear
    app_index.rs            app loading and refresh
    file_index.rs           indexing, future watcher
    preferences.rs          persisted preferences
    window_manager.rs       niri/hypr/sway abstraction
  ui/
    launcher.rs             GTK launcher shell
    result_row.rs           reusable result row
    action_panel.rs
    form_panel.rs
    preferences.rs
    extension_browser.rs
```

The immediate architectural target is not "many files because many files"; it is
to make every new Vicinae-style surface land in the right place without turning
`lib.rs` and the GTK binary into unreviewable files.

## Milestones

### 1. Stabilize the Launcher Core

- Move data types and providers out of `src/lib.rs`. Done for actions,
  config, placeholders, app state, and search providers.
- Introduce a `SearchProvider` trait with a common `search(query, context)` API.
  Done for the current built-in providers.
- Keep current behavior and tests passing during the split.
- Add provider-level tests so future ranking changes do not silently regress.
- Keep the GTK UI working through the same public facade.

Done when the project still passes `cargo test` and
`nix develop -f shell.nix --command cargo check --features gui`, but core files
are small enough to work on independently.

Status: done for the split itself — `lib.rs` is a facade and all providers sit
behind the `SearchProvider` trait with provider-level tests. The "core files
small enough" criterion is not fully met yet: `app.rs`, `ui/launcher.rs`, and
`ui/views.rs` are still large.

### 2. Make the GTK UI Vicinae-like

- Replace the single flat launcher file with reusable GTK components.
  Started: CSS, result rows, panel shell helpers, result lists, action buttons,
  alias panel, form panel, preferences editor, and extension browser now live
  under `src/ui`.
- Add a real navigation stack: root search, detail view, form view, settings,
  clipboard history, snippet manager, extension browser.
  Started: root search, clipboard history, extension browser, and preferences
  now run inside an in-window GTK stack. Snippet manager is also a stack view
  with copy/delete support.
- Make the action panel a first-class component with searchable actions.
  Started: `Ctrl+K` opens an in-window searchable action panel instead of a
  separate popup window.
- Add footer status/hints and consistent keyboard navigation.
- Keep layer-shell and daemon behavior.

GTK should remain native widgets where practical. Custom drawing should be added
only when the native widgets cannot produce the needed command-palette feel.

### 3. Persistent Services

- Replace plain text storage where it limits behavior with SQLite:
  clipboard, snippets, recent/frequency, command metadata, local storage.
  Done for clipboard history and recent/frequency usage (`services/storage.rs`,
  WAL journal mode); legacy `clipboard.txt`/`recent.txt`/`frequencies.txt` are
  migrated on startup. Snippets remain plain-text `snippets.txt`; command
  metadata and local storage are unchanged.
- Make indexes refreshable without rebuilding all state on every launch.
- Add file/app refresh actions equivalent to Vicinae's internal refresh commands.
- Add migrations early, before data formats spread across the codebase.
  Done: schema versioning via `PRAGMA user_version` with transactional
  migrations in `services/storage.rs`.

### 4. Script Command Compatibility

- Support Raycast/Vicinae-style script command metadata.
  Done (partial): `@raycast.*`/`@vicinae.*` metadata comments
  (schemaVersion/title/description/packageName/icon/mode) are parsed in
  `search/scripts.rs`; `mode` is parsed but not used yet; script arguments and
  preferences are not supported yet.
- Scan configured script directories. Done (`script_dirs` preference plus
  extension manifest `scripts = [...]`).
- Parse metadata comments. Done.
- Expose script commands in root search. Done.
- Support stdout parsing, arguments, preferences, and action panel entries.
  Partially done: script stdout is captured and shown; arguments, preferences,
  and per-script action panel entries are missing.

This is the highest-value extension mechanism before a full TypeScript/React
runtime.

### 5. Rich Built-in Modules

- Clipboard history view with delete, clear, pin, paste/copy. Done.
- Snippet manager with create/edit/delete/search. Done (create/delete/search;
  no in-place edit).
- Status strip and dashboard/control-center views are tracked in
  `docs/linux-command-center-plan.md`. Done — well beyond the original plan:
  the strip renders clock, date, network, battery, audio, media, and keyboard
  layout items via `status_items`.
- Emoji picker. Done (`search/emoji.rs`, dedicated view, CLI flag `--emoji`).
- Font browser. Done.
- Calculator history. Done (persisted to `calc_history.json`).
- Browser tab switcher if a companion browser extension or native messaging host
  is added. Not started.
- Window switcher with compositor-specific backends. Done
  (niri/Hyprland/sway backends with cached live queries).

### 6. Extension Runtime

Only start this after script commands and built-in module views are stable.

Pragmatic path:

- Define a Rust-side extension protocol first. Partially done:
  `extension.toml` manifests with capabilities and command/script lists,
  grouped in the Extension Browser. No JS/TS runtime — intentionally deferred.
- Add a local extension manifest and command registry. Partially done (see
  above).
- Add process isolation for extensions. Not started.
- Add a JS/TypeScript runtime later if Raycast ecosystem compatibility is still
  worth the complexity.

## Near-term Implementation Order

1. Refactor core types from `lib.rs` into `action.rs`, `config.rs`, and
   `search/*` while preserving behavior. Done.
2. Refactor GTK panels from `zeshicast-gtk.rs` into `ui/*`. Done.
3. Introduce a navigation stack and make clipboard/snippet browsers full views.
   Done.
4. Add SQLite storage for clipboard and usage history. Done.
5. Add script command scanning compatible with Raycast/Vicinae metadata. Done
   for metadata parsing and root-search exposure; arguments/preferences pending.

## Verification Baseline

Current baseline:

- `cargo test --features gui` passes with 119 tests (as of 2026-08).
- `cargo check --features gui` requires a Rust toolchain new enough for the
  `gtk4` crate (currently 0.11.3). In this repo, run it through Nix:

```bash
nix develop -f shell.nix --command cargo check --features gui
```

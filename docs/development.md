# Development Guide

This guide covers local build/test workflows and the conventions used when
adding search providers or executable actions.

## Build Commands

Headless CLI:

```bash
cargo build
cargo run -- firefox
cargo test
```

GTK launcher without layer shell:

```bash
nix develop --command cargo run --features gui --bin zeshicast-gtk
nix develop --command cargo test --features gui
```

GTK launcher with layer shell:

```bash
nix develop --command cargo check --features gui,layer-shell
nix develop --command cargo run --features gui,layer-shell --bin zeshicast-gtk
```

Nix package/module checks:

```bash
nix flake check
nix build
```

Supply chain check:

```bash
nix shell nixpkgs#cargo-deny -c cargo deny check
```

## Timing Marks

`ZESHICAST_TRACE=1` logs where the time goes (Q2.1), with a `trace:` prefix:

```text
trace: apps load took 33.7 ms              model loads and the file index
trace: first frame at 212.4 ms since start first frame after process start
trace: show took 18.3 ms                   --toggle / launch → first frame
trace: rss while hidden 81.2 MiB           every time the palette hides
trace: keystroke to results n=42 p50=… p95=… max=…   summary at exit
trace: rss at exit 83.0 MiB
```

Off by default and free when off. For the daemon:
`systemctl --user stop zeshicast && ZESHICAST_TRACE=1 zeshicast-gtk --daemon`,
then `zeshicast-gtk --quit` prints the summary. `scripts/measure.sh` automates it.

`scripts/measure.sh --gui` types into the live session with `wtype` (a few
hundred short-lived Wayland clients). It refuses to start when 5 or more
clients have stopped reading their sockets — hung screen lockers, for
example — because their in-flight fds plus the keymaps wtype sends can hit
the per-user limit, after which every new Wayland client fails with
`ETOOMANYREFS` until the stuck ones are killed. It also asks before typing
(`--yes` skips the question, `--force` skips the check).

## Measured Performance

`scripts/measure.sh --gui`, release build, 2026-11-02, AMD Ryzen 7 5700X,
kernel 6.18.52, niri, 10 runs, typing at 120 ms per key:

| metric | before (`dd050c5`) | after (Q2.4) |
| --- | --- | --- |
| show (toggle → first frame), median | 34.4 ms | 34.3 ms |
| show, first after daemon start (cold) | 203–252 ms | 243 ms |
| keystroke → results, p50 | 108.3 ms | 8.8 ms |
| keystroke → results, p95 | 109.3 ms | 24.4 ms |
| RSS while hidden, cairo | 114 MiB | 121 MiB |
| RSS while hidden, ngl | 249 MiB | — |
| file index build (CLI) | 16 ms | 16 ms |

"Before" keystroke latency was almost entirely the fixed 80 ms debounce plus
the 25 ms result poll; Q2.4 made the debounce leading-edge (50 ms only inside
a burst) and the poll 5 ms.

**Cold first show** (2026-11-03, three daemon starts, `--toggle` only, no
typing): 190 ms → 111–121 ms; later shows stay at ~15 ms. The main thread
was busy for the whole gap between map and the first frame, opening ~130
font files: fontconfig set-up and fallback-font selection on the first text
layout, plus the first CSS pass. The daemon now fills, realizes and measures
the hidden palette once while idle ("warm up", ~110 ms, logged with
`ZESHICAST_TRACE=1`). What is left is font face set-up and glyph
rasterisation on the first paint (freetype autohinter, fontconfig sort,
harfbuzz in a flat profile); drawing every label once into an offscreen
texture with the window's renderer did not move it, so that was not kept.
`show: results` and `show: present` split the show mark further.

## Feature Flags

- Default build: headless CLI, no GTK and no D-Bus services.
- `desktop`: gio/glib services and execution — MPRIS media control, the
  NetworkManager D-Bus providers. This is what a headless build needs to do real
  work, and it is the build the CLI is meant to be built with.
- `gui`: the GTK widgets, daemon mode, notification server, clipboard image
  capture. Implies `desktop`.
- `layer-shell`: enables the Wayland layer-shell integration and implies `gui`.

`gui` adds widgets, not capability. Anything a headless build can use belongs to
`desktop`: gating a service on `gui` meant the CLI silently could not do it
(media actions did nothing, for one), and it made `gui` the feature the daemon
needed for its D-Bus work. Two tests hold the line —
`the_gui_feature_is_widgets_on_top_of_the_desktop_build` pins
`gui = ["desktop", …]` and fails if the widget toolkit is reached for outside
`src/ui`.

The palette's own machinery is the documented exception: running a JSON command
or capturing a script's stdout produces *rows*, so it stays behind `gui` even
though it needs no widget.

```sh
# Headless build with the D-Bus services, no GTK:
nix develop --command cargo test --features desktop
nix develop --command cargo clippy --all-targets --features desktop -- -D warnings
```

## Nix Dev Shell

Use `nix develop` for GTK work. It supplies Rust tools, GTK4, layer-shell,
Wayland libraries, and common runtime tools used by launcher integrations.

```bash
nix develop
nix develop --command cargo fmt --check
nix develop --command cargo clippy --all-targets --features gui -- -D warnings
```

If a command fails because `gtk4-layer-shell-0.pc` or GTK pkg-config files are
missing, rerun it through `nix develop`.

## Test Strategy

Run these before committing code changes:

```bash
cargo fmt --check
cargo test
nix develop --command cargo test --features gui
nix develop --command cargo clippy --all-targets --features gui -- -D warnings
nix develop --command cargo check --features gui,layer-shell
```

A fast subset of this (`cargo fmt --check` and clippy on the default build)
runs as a git hook. Install it once per checkout:

```bash
ln -s ../../scripts/pre-commit .git/hooks/pre-commit
```

`rustfmt.toml` pins `edition = "2024"`, so a bare `rustfmt` started by an
editor or a tool formats exactly like `cargo fmt`. Without it rustfmt falls
back to the 2015 style and reorders imports, which turns `cargo fmt --check`
red.

Use focused unit tests for parsers, placeholder expansion, capability checks,
storage migrations, and action execution policy. Use GUI-feature tests when a
change touches GTK-facing structs, launcher actions, or daemon behavior.

## Where The Launcher Lives

The palette was one 3300-line file; it is now a module per responsibility (P5.2),
and everything in it is re-exported at `ui::launcher::`, which is the path the
rest of `ui` uses:

- `ui/launcher/mod.rs` — `GuiState`, `ensure_ui`, presenting the window, and the
  launcher's own tests;
- `ui/launcher/build.rs` — widget construction (`build_ui` and the action bar);
- `ui/launcher/results.rs` — actions into rows, and a row back into its action;
- `ui/launcher/views.rs` — switching to and filling the sub-views;
- `ui/launcher/run.rs` — running a selected action, confirmations, the form panel;
- `ui/launcher/scripts.rs` — script actions and their stdout capture;
- `ui/keybindings.rs` — key handling, which takes a `LauncherUi` context (the
  window, the shared state and every view) instead of 32 positional arguments.

`src/app.rs` followed the same treatment (P5.3): `app/mod.rs` holds the types,
loading and search, and the model's own behaviours live in `app/clipboard.rs`,
`app/preferences.rs`, `app/snippets.rs` and `app/launch.rs`. `launch.rs` is where
an action's consequences are applied (policy from preferences, confirmation,
history recording); the execution point itself stays in `action.rs`.

A row does not carry an index into `results`: it is tagged with the action it
was built from (`ACTION_ROW_TAG`), so a stale or unregistered row runs nothing
instead of a neighbour's action (B-4).

## The Crate Root

The crate root is not a prelude (P5.4). `src/lib.rs` declares the modules, keeps
`reap_finished_children`, and re-exports exactly what the two binaries import:
`Action`, `ActionRisk`, `ExecutionDecision`, `SecondaryActionKind`, `Zeshicast`,
the export/import helpers and `logging`/`cli`/`ui`. Everything inside the crate
names the module a thing lives in (`crate::search::apps::load_apps`), and the
tests inside a module use those paths too instead of a `#[cfg(test)]`
re-export.

Nine service modules carry `#[cfg_attr(not(feature = "gui"), allow(dead_code))]`:
the palette is their only consumer, so without the `gui` feature they are
unreachable rather than dead. That attribute is a marker for the reader, not a
license: gating those readers on the feature is what should replace it.

## Adding A Provider

1. Put provider-specific parsing and search logic under `src/search/` or
   `src/services/`.
2. Expose a small provider type implementing `SearchProvider` in
   `src/search/mod.rs`.
3. Add it to `Zeshicast::search` in `src/app.rs`.
4. Keep search functions fast and side-effect-free. If they need process output
   or network data, use a cached service snapshot or defer execution to an
   action.
5. Add parser/search tests near the provider.

Provider results should include a stable category, clear title/subtitle, icon,
and score. Prefer explicit prefixes for actions that can change system state.

## Adding A Safe Command Action

Prefer typed actions over shell strings:

- Use `ActionKind::OpenUrl`, `OpenPath`, `Copy`, `Media`, `Notification`, or
  `Command(ProcessCommand)` when possible.
- Use `ActionKind::Shell` only for commands that truly need shell syntax.
- Route execution through `Action::run_with_policy` or the gateway
  (`execute(request, &ExecutionTicket { .. })` in `src/action.rs`); do not spawn
  processes directly from UI callbacks unless the callback is only building an
  `ExecutionRequest`. The gateway checks the request's required capabilities and
  the confirmation policy before anything is spawned.
- Mark risky actions with `ActionRisk` so confirmation policy can intercept
  them.

For custom TOML commands, prefer `mode = "argv"`:

```toml
name = "Git Log"
mode = "argv"
program = "git"
args = ["log", "--oneline", "--", "{{arg:path}}"]
```

Use `mode = "shell"` only for pipelines, redirects, command chaining, or shell
builtins. Placeholder values in shell mode are shell-quoted automatically; argv
mode passes placeholder values as literal process arguments.

## Avoid Blocking The GTK Main Thread

Search, parsing, and UI updates must stay responsive:

- Do not run subprocesses, filesystem indexing, network requests, or long SQLite
  operations directly in GTK callbacks.
- Use cached snapshots for polling services such as audio, network, compositor,
  windows, and system stats.
- For slow action execution, build an `ExecutionRequest` and let the execution
  layer spawn the process or worker thread.
- Bound subprocess output and timeouts when a search provider must inspect live
  command output.
- Keep startup work minimal; build heavy indexes after the window/daemon is
  initialized.

If a provider needs fresh state, prefer a background refresh path that updates a
cache, then render the latest cached value on the UI thread.

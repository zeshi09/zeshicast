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

## Feature Flags

- Default build: headless CLI, no GTK dependency.
- `gui`: enables GTK UI, daemon mode, notification server, clipboard image
  capture, and GUI-only integrations.
- `layer-shell`: enables the Wayland layer-shell integration and implies `gui`.

Keep non-GUI logic outside `#[cfg(feature = "gui")]` where practical so the
headless CLI remains a useful fast test target.

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

Use focused unit tests for parsers, placeholder expansion, capability checks,
storage migrations, and action execution policy. Use GUI-feature tests when a
change touches GTK-facing structs, launcher actions, or daemon behavior.

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
- Route execution through `Action::run_with_policy` or `run_execution_request`;
  do not spawn processes directly from UI callbacks unless the callback is only
  building an `ExecutionRequest`.
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

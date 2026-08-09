# Action Plan from Full Review

Дата: 2026-06-27

Основание: `FULL_REVIEW.md`, зафиксированный по результатам полного технического, архитектурного, security, performance, UX, Rust и NixOS review.

Цель: превратить выводы review в исполнимый план работ с приоритетами, конкретными файлами, критериями приемки и тестами. Этот документ не заменяет старый `ACTION_PLAN.md`; он фиксирует новый план после свежего review.

## 0. Current Verification Baseline

Последняя проверка:

| Command | Result | Notes |
| --- | --- | --- |
| `cargo test` | PASS | 70 tests |
| `cargo check` | PASS | default/headless path |
| `cargo check --features gui` | PASS | GUI path builds |
| `cargo check --features gui,layer-shell` | FAIL / environment | missing host `gtk4-layer-shell-0.pc` |
| `cargo fmt --check` | FAIL | formatting diffs across many files |
| `cargo clippy --all-targets --features gui -- -D warnings` | FAIL | 53 diagnostics |
| `cargo clippy --all-targets --all-features -- -D warnings` | FAIL / environment | blocked by missing `gtk4-layer-shell-0.pc` |
| `nix flake check` | NOT COMPLETED | Nix fetch/unpack stalled; interrupted |
| `nix develop --command cargo check --features gui,layer-shell` | NOT COMPLETED | Nix fetch/store copy in environment; interrupted |

Working tree note: before this plan there were already deleted `.claude/worktrees/*` entries. Do not revert them unless explicitly requested.

## 1. Priority Overview

| Priority | Theme | Goal |
| --- | --- | --- |
| P0 | Safety/correctness blockers | Prevent wrong/destructive action execution |
| P1 | Responsiveness/build hygiene | Make launcher reliably fast and CI-clean |
| P2 | Privacy/storage/product readiness | Make daily use trustworthy and maintainable |
| P3 | Extension architecture | Prepare local-first extension model without unsafe arbitrary command sprawl |
| Deferred | Future runtime/store | Avoid JS/TS runtime/store until core safety is stable |

Recommended order:

1. Fix action panel action mapping.
2. Add destructive action confirmation.
3. Add real capability enforcement for commands/JSON/script actions.
4. Fix formatting and actionable clippy issues.
5. Remove startup/main-thread subprocess stalls.
6. Add privacy/storage cleanup.
7. Improve packaging/docs/CI.

## 2. P0: Safety and Correctness Blockers

### P0-1. Fix Action Panel Row Index Bug

Severity: High correctness.

Why: the UI can execute a different secondary action than the visible selected row.

Evidence:

- `src/ui/views.rs`: `set_action_panel_list` inserts non-selectable section header rows.
- `src/ui/launcher.rs`: `run_action_panel_row(... row.index())` indexes `filtered_action_panel_items` directly.
- Displayed row indexes include headers; `filtered_action_panel_items` does not.

Affected files:

- `src/ui/views.rs`
- `src/ui/launcher.rs`

Implementation steps:

1. Introduce an explicit displayed item model for action panel rows:
   - `DisplayedActionPanelRow::Header(ActionPanelSection)`
   - `DisplayedActionPanelRow::Action(ActionPanelItem)`
2. Store the displayed vector in `filtered_action_panel_items`, or keep a separate `displayed_action_panel_rows`.
3. Change activation to resolve selected row through displayed rows, not raw `row.index()` into action-only items.
4. Ensure section headers remain non-selectable and non-activatable.
5. Update filtering so headers are included only when their section has visible actions.

Quick fix option:

- Add a row-to-action index map when building the list.
- Store the action index as row data if gtk-rs supports it cleanly, or pass a vector that includes headers.

Robust fix:

- Make `ActionPanelView` own a typed display model and expose `selected_action_item()`.

Acceptance criteria:

- Selecting and pressing Enter on `Run` executes `Run`.
- Selecting and pressing Enter on `Copy Value` executes `Copy Value`.
- Selecting and pressing Enter on `Clear Clipboard History` executes only that action after P0-2 confirmation.
- Section headers cannot be selected or activated.

Suggested tests:

- `action_panel_row_index_ignores_section_headers`
- `action_panel_filter_preserves_row_mapping`
- Manual GTK smoke test:
  - Open root search.
  - Select clipboard item.
  - Open `Ctrl+K`.
  - Verify each visible action triggers the matching behavior.

### P0-2. Add Confirmation for Destructive Actions

Severity: High safety.

Why: current UI allows destructive/system actions via a normal Enter/Delete path.

Evidence:

- `src/search/system.rs`: `Restart` and `Power Off` produce shell actions.
- `src/ui/launcher.rs`: `terminate_selected_system_process` runs `kill`.
- `src/ui/launcher.rs`: `Ctrl+Delete` in clipboard view clears clipboard history.
- `packaging/examples/commands/docker-ps.toml`: JSON action can stop containers.

Affected files:

- `src/action.rs`
- `src/app.rs`
- `src/search/system.rs`
- `src/search/processes.rs`
- `src/search/commands.rs`
- `src/ui/launcher.rs`
- `src/ui/forms.rs` or a new confirmation panel module

Implementation steps:

1. Add `ActionRisk`:

   ```rust
   pub enum ActionRisk {
       Normal,
       Shell,
       Destructive,
       SystemPower,
       ProcessKill,
       ClipboardClear,
   }
   ```

2. Add `risk: ActionRisk` to `Action` or derive it from `ActionKind` plus metadata.
3. Mark built-ins:
   - `systemctl reboot` -> `SystemPower`
   - `systemctl poweroff` -> `SystemPower`
   - process kill actions -> `ProcessKill`
   - clear clipboard -> `ClipboardClear`
   - JSON `shell` actions -> `Shell` or `Destructive` if command declares it.
4. Add a confirmation UI:
   - title: action title;
   - subtitle: exact command/target;
   - buttons: Cancel / Confirm;
   - keyboard: Esc cancels, Enter confirms only when confirm button/prompt is active.
5. Route all action execution through one method:
   - `Zeshicast::execute_action_with_policy`
   - or UI-level `run_action_or_confirm`.
6. Do not call `action.run()` directly from UI for risky actions.

Acceptance criteria:

- `system power` + Enter does not power off immediately.
- `system restart` + Enter does not reboot immediately.
- Process kill shows PID and process name before sending `kill`.
- Clipboard clear requires confirmation and clearly says it clears local history.
- Confirmation can be keyboard-only.

Suggested tests:

- `system_power_action_is_marked_system_power`
- `process_kill_action_is_marked_process_kill`
- `clipboard_clear_secondary_action_is_marked_clipboard_clear`
- `risky_action_returns_confirmation_request_instead_of_running`

Manual tests:

- Run in sandbox/temp config only.
- Do not actually confirm power/reboot.
- Use fake process action in unit/integration test instead of killing real processes.

### P0-3. Enforce Real Capabilities for Custom Commands and JSON Actions

Severity: High security.

Why: `permissions = ["shell", "network", "filesystem"]` is informational. JSON command output can produce shell actions regardless of manifest permissions.

Evidence:

- `README.md` says `permissions` is informational.
- `src/search/commands.rs` parses `permissions` into `CommandEntry`.
- `parse_json_action_kind` accepts `shell`, `open_path`, `open_url`, `copy`, `none`.
- No enforcement before constructing `ActionKind::Shell`.

Affected files:

- `src/search/commands.rs`
- `src/search/scripts.rs`
- `src/action.rs`
- `src/app.rs`
- `src/ui/views.rs` extension browser
- `README.md`

Implementation steps:

1. Add:

   ```rust
   pub enum Capability {
       Shell,
       Network,
       Filesystem,
       ClipboardRead,
       ClipboardWrite,
       OpenUrl,
       OpenPath,
   }
   ```

2. Parse command permissions into `CapabilitySet`, not raw strings.
3. Add `ActionOrigin`:
   - `BuiltIn`
   - `Command { name, capabilities }`
   - `Script { path, capabilities }`
   - `JsonCommand { name, capabilities }`
4. Change JSON action parsing:
   - `shell` requires `Capability::Shell`;
   - `open_path` requires `Filesystem` or `OpenPath`;
   - `open_url` requires `Network` or `OpenUrl` for remote schemes;
   - unknown action types become `ActionKind::None` with warning subtitle.
5. Add a user trust layer:
   - first run of a command with dangerous capabilities should ask approval;
   - store approvals in `~/.config/zeshicast/permissions.toml` or SQLite.
6. Update extension browser to show capabilities and risk.

Acceptance criteria:

- A JSON command without `shell` cannot return executable shell actions.
- A command with `permissions = ["shell"]` can return shell only after user approval.
- Extension browser shows `shell`, `network`, `filesystem` clearly.
- README no longer says permissions are merely informational once enforcement lands.

Suggested tests:

- `json_shell_action_requires_shell_capability`
- `json_open_path_requires_filesystem_capability`
- `json_open_url_requires_network_or_open_url_capability`
- `unknown_permission_is_rejected_or_warned`
- `command_permissions_parse_known_values`

### P0-4. Route All Action Execution Through One Executor

Severity: High architecture/security.

Why: direct calls to `action.run()`, `spawn_shell`, `spawn_command`, and `run_secondary_action` make it hard to enforce confirmation/capabilities consistently.

Evidence:

- `src/action.rs`: `Action::run`.
- `src/app.rs`: `run_action`, `run_form_action`, `run_secondary_action`.
- `src/ui/launcher.rs`: direct `spawn_command`/`spawn_shell` calls for dashboard/network/audio/process.

Affected files:

- `src/action.rs`
- `src/app.rs`
- `src/ui/launcher.rs`
- `src/search/*`

Implementation steps:

1. Add `ExecutionRequest`:

   ```rust
   pub enum ExecutionRequest {
       Shell { command: ShellCommand },
       Command { program: String, args: Vec<String> },
       OpenPath(PathBuf),
       OpenUrl(String),
       Copy(String),
       Http(HttpRequest),
       BuiltIn(BuiltInAction),
   }
   ```

2. Add `ExecutionDecision`:
   - `RunNow`
   - `NeedsConfirmation`
   - `Denied`
3. Move risk/capability checks into `ExecutionPolicy`.
4. UI asks executor for a decision before running.
5. Remove direct risky `spawn_*` calls from UI handlers.

Acceptance criteria:

- Grep for `spawn_shell(` and `spawn_command(` in UI returns only executor-adjacent calls or well-reviewed safe controls.
- All shell/custom/destructive actions pass through policy.

Suggested tests:

- `executor_denies_shell_without_capability`
- `executor_requests_confirmation_for_power_action`
- `executor_allows_copy_without_confirmation`

## 3. P1: Responsiveness and Build Hygiene

### P1-1. Make `cargo fmt --check` Green

Severity: Medium, release/CI blocker.

Evidence:

- `cargo fmt --check` fails with diffs in `src/app.rs`, `src/lib.rs`, `src/search/*`, `src/services/*`, `src/ui/*`.

Implementation steps:

1. Run `cargo fmt`.
2. Review diff to ensure no semantic changes.
3. Commit separately from behavior changes.

Acceptance criteria:

- `cargo fmt --check` passes.

Suggested tests:

- No runtime tests required beyond normal suite.

### P1-2. Make Clippy Green or Define Scoped Allows

Severity: Medium, CI readiness.

Evidence:

- `cargo clippy --all-targets --features gui -- -D warnings` fails with 53 diagnostics.
- Important categories:
  - `too_many_arguments` in UI wiring;
  - `io_other_error`;
  - `collapsible_if`;
  - `manual_split_once`;
  - `type_complexity`;
  - `new_without_default`.

Affected files:

- `src/app.rs`
- `src/config.rs`
- `src/search/*`
- `src/services/*`
- `src/ui/*`

Implementation steps:

1. Fix mechanical lints:
   - `io::Error::other`;
   - `split_once`;
   - `sort_by_key`;
   - `RangeInclusive::contains`;
   - `new_without_default`.
2. For large UI functions:
   - prefer refactor into controller/context structs;
   - only use `#[allow(clippy::too_many_arguments)]` as a temporary local allow with a comment and follow-up task.
3. Add type aliases for complex statics, e.g. network speed state.
4. Run clippy again.

Acceptance criteria:

- `cargo clippy --all-targets --features gui -- -D warnings` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes in Nix dev shell.

### P1-3. Remove Main-Thread Startup Subprocess Work

Severity: Medium performance.

Why: launcher must show instantly.

Evidence:

- `src/ui/launcher.rs` creates initial:
  - `audio_view(&crate::audio_snapshot())`
  - `dashboard_view(&crate::system_snapshot())`
  - `system_monitor_view(&crate::system_snapshot(), &crate::top_processes_by_memory(8))`
  - `media_view(&crate::media_snapshot())`
  - `network_view(&crate::network_snapshot())`
- `src/services/poll_cache.rs::cache()` seeds synchronously with `network_snapshot()`, `audio_snapshot()`, `keyboard_layout()`.

Affected files:

- `src/ui/launcher.rs`
- `src/services/poll_cache.rs`
- `src/ui/views.rs`

Implementation steps:

1. Build views with default/empty snapshots:
   - `AudioSnapshot::default()`
   - `NetworkSnapshot::default()`
   - `SystemSnapshot::default()`
   - `MediaSnapshot::default()`
2. Present window before starting snapshot hydration.
3. Start background poller with empty cache:
   - do not call subprocess snapshots inside `cache().get_or_init`.
4. Push first real snapshots asynchronously through GLib timeout/channel.
5. Add visible loading/empty states where needed.

Acceptance criteria:

- `build_ui` does not call subprocess-heavy snapshot functions before `window.present()`.
- `start_poll_cache()` does not fork on main thread.
- Manual observation: launcher appears immediately even when `nmcli`/`wpctl` are slow or unavailable.

Suggested tests:

- Unit-level test is hard for GTK startup.
- Add architecture check via grep in CI later:
  - no `network_snapshot()` call in initial view construction except async worker.

### P1-4. Move JSON Commands Out of Search Hot Path

Severity: Medium performance/security.

Why: current JSON commands execute synchronously during search-as-you-type. A 1s timeout prevents infinite freeze, but the UI can still block for up to 1s per query.

Evidence:

- `src/search/commands.rs::search_json_command`
- `src/search/commands.rs::run_json_command`
- `JSON_COMMAND_TIMEOUT = 1s`

Affected files:

- `src/search/commands.rs`
- `src/app.rs`
- `src/ui/launcher.rs`

Implementation steps:

1. Change provider behavior:
   - root search returns command shell/form entry;
   - JSON command execution happens on direct activation or async result job.
2. If keeping search-as-you-type JSON:
   - debounce 150-250ms;
   - worker thread;
   - cancellation token;
   - stale result replacement.
3. Add timeout and output size cap:
   - stdout max e.g. 512 KiB;
   - stderr max e.g. 8 KiB.

Acceptance criteria:

- Typing in launcher does not block while JSON command runs.
- New query cancels previous JSON job.
- Slow JSON command shows timeout/error result without freezing UI.

Suggested tests:

- `json_command_timeout_returns_error_result`
- `json_command_stdout_size_is_bounded`
- Integration test with fake script sleeping longer than timeout.

### P1-5. Make Window Search Non-Blocking

Severity: Medium performance/reliability.

Why: `win` query tries compositor CLIs synchronously.

Evidence:

- `src/search/windows.rs::search_windows`
- Calls `niri msg windows`, `hyprctl clients -j`, `swaymsg -t get_tree`.

Affected files:

- `src/search/windows.rs`
- optionally new `src/services/windows.rs`

Implementation steps:

1. Add a window snapshot service:
   - poll only on `win` prefix or view open;
   - cache last successful compositor result;
   - timeout each CLI call.
2. Prefer detected compositor:
   - check env vars or successful first backend;
   - do not try all three every query if one is known.
3. Add hard timeout wrapper for subprocess output.

Acceptance criteria:

- `win firefox` cannot hang the launcher if compositor CLI hangs.
- Missing compositor tools return empty result quickly.

Suggested tests:

- Parser tests already useful; add process timeout wrapper tests with fake command.

## 4. P2: Privacy, Storage, Product Readiness

### P2-1. Clipboard Image Cache Retention and Clear

Severity: Medium privacy/storage.

Why: SQLite clipboard rows are pruned, but cached image PNG files under `~/.cache/zeshicast/clipboard/` are not visibly pruned.

Evidence:

- `src/ui/launcher.rs::capture_clipboard_image` writes PNGs.
- `src/services/storage.rs::clipboard_insert` prunes DB rows.
- `clear_clipboard_history` clears SQLite rows but does not delete cache directory.

Affected files:

- `src/app.rs`
- `src/services/storage.rs`
- `src/ui/launcher.rs`

Implementation steps:

1. Add helper:
   - `clipboard_cache_dir() -> PathBuf`
   - `prune_clipboard_image_cache(config_dir/cache_dir)`
2. On DB prune:
   - collect remaining image sentinel paths;
   - delete unreferenced PNGs in cache dir.
3. On `clear_clipboard_history`:
   - delete all cache PNGs.
4. On delete single clipboard item:
   - if image sentinel, delete image file if no remaining DB reference.

Acceptance criteria:

- Clearing clipboard history deletes cached images.
- Pruning DB to retention limit also prunes unreferenced image files.
- Existing referenced images are preserved.

Suggested tests:

- `clipboard_clear_removes_image_cache_files`
- `clipboard_delete_removes_unreferenced_image`
- `clipboard_prune_removes_orphan_images`

### P2-2. Privacy Controls: Disable, Pause, Retention, Export Policy

Severity: Medium privacy/product.

Why: clipboard, notification history, AI prompts, API keys, recents and file paths are sensitive.

Affected files:

- `src/app.rs`
- `src/config.rs`
- `src/ui/preferences.rs`
- `src/ui/views.rs`
- `README.md`

Implementation steps:

1. Add preferences:
   - `clipboard_history_enabled = true`
   - `clipboard_private_mode = false`
   - `clipboard_retention = 100`
   - `clipboard_capture_images = true`
   - `notifications_history_enabled = true`
   - `export_include_secrets = false`
   - `file_index_exclude = ".git,node_modules,target,.cache,..."`
2. Update daemon:
   - do not install clipboard watcher if disabled;
   - if private mode enabled, ignore new clipboard entries.
3. Update UI:
   - Preferences > Privacy section with toggles;
   - Clipboard view shows clear/pause state.
4. Update export:
   - default excludes secrets and cache;
   - explicit flag can include secrets.

Acceptance criteria:

- User can pause clipboard capture.
- User can disable image capture.
- Export default excludes API keys and cache.
- README clearly explains local sensitive data.

Suggested tests:

- `clipboard_disabled_does_not_record_text`
- `private_mode_does_not_record_clipboard`
- `export_excludes_api_keys_by_default`

### P2-3. Atomic Writes and File Permissions

Severity: Medium reliability/security.

Why: preferences and secrets are plaintext; writes can be interrupted.

Evidence:

- `src/config.rs::write_preferences`
- `src/config.rs::write_lines`
- `append_alias`
- `save_calc_history`

Affected files:

- `src/config.rs`
- `src/app.rs`

Implementation steps:

1. Add `write_file_atomic(path, content, mode)`:
   - create parent dir;
   - write temp file in same dir;
   - set permissions before rename;
   - fsync file where feasible;
   - rename.
2. Use `0600` for:
   - `preferences.toml`
   - `zeshicast.db` if possible after open/create;
   - `aliases.txt`
   - `pins.txt`
   - `calc_history.json`
   - permission approvals.
3. Use `0644` only for non-sensitive generated desktop/service files.

Acceptance criteria:

- New preferences file has `0600`.
- Interrupted write never leaves partially written target.
- Tests use temp dir and metadata permissions on Unix.

Suggested tests:

- `write_preferences_creates_0600_file`
- `write_lines_atomic_replaces_existing_content`
- `atomic_write_does_not_follow_symlink_if_hardened`

### P2-4. SQLite Schema Version and Migrations

Severity: Medium maintainability/reliability.

Why: DB will grow beyond clipboard/usage. Without migrations, upgrades risk silent failures.

Evidence:

- `src/services/storage.rs::init` uses `CREATE TABLE IF NOT EXISTS`, no schema version table.

Affected files:

- `src/services/storage.rs`

Implementation steps:

1. Add `meta` table or use `PRAGMA user_version`.
2. Define migration functions:
   - `migrate_0_to_1`
   - future migrations.
3. Wrap migrations in transaction.
4. Add indexes:
   - `clipboard(added_at)`
   - `usage(last_used)`
5. Set permissions on DB file.

Acceptance criteria:

- Fresh DB initializes at current version.
- Old DB migrates in tests.
- Corrupted DB error is surfaced enough for UI/logs.

Suggested tests:

- `fresh_db_has_current_schema_version`
- `old_db_migrates_to_current_schema`
- `clipboard_added_at_index_exists`

### P2-5. Documentation Sync and Warning Cleanup

Severity: Medium product readiness.

Why: docs contain stale statements and need stronger security/privacy warnings.

Affected files:

- `README.md`
- `ACTION_PLAN.md`
- `FULL_REVIEW.md`
- `docs/*`

Implementation steps:

1. Update `README.md`:
   - clarify `zeshicast.db`;
   - clarify image cache retention;
   - clarify notification server ownership/replacement behavior;
   - document runtime dependencies.
2. Update `ACTION_PLAN.md`:
   - mark old fixed items as fixed with code references;
   - link to `ACTION_PLAN_FULL_REVIEW.md`.
3. Add `docs/security.md`:
   - assets;
   - attackers;
   - extension command risks;
   - permissions model.
4. Add `docs/privacy.md`:
   - what is stored;
   - where;
   - retention;
   - clear/export behavior.

Acceptance criteria:

- README no longer says `permissions` are informational after P0-3 lands.
- README does not claim stale text files as active storage where SQLite is used.
- Security/privacy warnings are visible before custom command examples.

## 5. P2/P3: NixOS and Packaging

### PKG-1. Runtime Dependencies and Wrapper Policy

Severity: Medium packaging.

Why: flake wrapper includes only `wl-clipboard`, but app shells to many tools.

Evidence:

- `flake.nix` `preFixup` wraps only `wl-clipboard`.
- Code uses `wpctl`, `nmcli`, `ip`, `brightnessctl`, `bluetoothctl`, `wtype`, `grim`, `slurp`, `df`, `kill`, compositor CLIs, `xclip`.

Affected files:

- `flake.nix`
- `shell.nix`
- `README.md`

Implementation steps:

1. Classify runtime tools:
   - hard dependency: `wl-clipboard`;
   - common optional controls: `wireplumber`/`wpctl`, `networkmanager`, `iproute2`, `brightnessctl`, `bluez`, `procps`, `coreutils`;
   - compositor-specific: `niri`, `hyprland`, `sway`;
   - screenshot: `grim`, `slurp`;
   - typing: `wtype`;
   - fallback clipboard: `xclip`.
2. Add module option:
   - `extraRuntimePackages`.
3. Decide wrapper defaults:
   - include harmless common tools, or document that app uses session PATH.
4. README: list feature-to-tool mapping.

Acceptance criteria:

- NixOS user can enable common runtime packages declaratively.
- Missing optional tools degrade gracefully and are documented.

### PKG-2. Align Systemd User Service Targets

Severity: Low/Medium reliability.

Evidence:

- `flake.nix` uses `graphical-session.target`.
- `packaging/zeshicast-gtk.service` uses `default.target`.

Affected files:

- `packaging/zeshicast-gtk.service`
- `scripts/install-user.sh`
- `README.md`

Implementation steps:

1. Change non-Nix service to graphical-session where appropriate:
   - `PartOf=graphical-session.target`
   - `After=graphical-session.target`
   - `WantedBy=graphical-session.target`
2. Document fallback for distros without graphical-session target.
3. Ensure install script enables the correct unit.

Acceptance criteria:

- Non-Nix daemon starts after graphical session.
- README commands match actual service name and target behavior.

### PKG-3. Nix CI Verification

Severity: Medium release readiness.

Implementation steps:

1. Add GitHub Actions job with Nix installed.
2. Run:
   - `nix flake check`
   - `nix build`
   - `nix develop --command cargo check --features gui,layer-shell`
3. Cache Nix store if practical.

Acceptance criteria:

- CI catches `gtk4-layer-shell` / pkg-config regressions.
- Local host missing `.pc` file no longer blocks official verification.

## 6. P3: Extension Architecture Roadmap

### EXT-1. Local Extension Manifest

Severity: Medium future foundation.

Why: script/TOML commands are enough for now, but need stable metadata before registry/runtime.

Affected files:

- `src/search/commands.rs`
- `src/search/scripts.rs`
- new `src/extensions/*`

Implementation steps:

1. Define manifest:

   ```toml
   id = "example.git-tools"
   name = "Git Tools"
   version = "0.1.0"
   capabilities = ["shell", "filesystem"]
   commands = ["git-log.toml"]
   ```

2. Load manifests from:
   - `~/.config/zeshicast/extensions/*/extension.toml`
3. Map commands/scripts to extension origin.
4. Show extension details and permissions in extension browser.

Acceptance criteria:

- Local extension can contain multiple command TOMLs/scripts.
- Extension browser groups commands by extension.
- Capability approvals attach to extension id and version.

### EXT-2. Safe Command Execution Abstraction

This overlaps P0-4 but becomes more important for extension scaling.

Implementation steps:

1. Add `argv` command mode in addition to `shell`:

   ```toml
   mode = "argv"
   program = "git"
   args = ["log", "--oneline", "-20", "{{arg:path}}"]
   ```

2. Keep `shell` mode available but high-risk.
3. Update docs:
   - prefer `argv`;
   - use `shell` only for pipelines/compound commands.

Acceptance criteria:

- `argv` placeholders do not require shell quoting.
- Command injection tests cover `{{query}}`, `{{clipboard}}`, `{{arg:*}}`, `{{pref:*}}`.

### EXT-3. Defer JS/TS Runtime and Store

Status: explicitly deferred.

Reason:

- Current safety/capability model is not mature enough.
- JS runtime/store would multiply attack surface.
- Script/TOML commands cover most local-first value now.

Revisit only after:

- P0 capabilities done.
- P1 async execution done.
- Extension manifests and approvals stable.
- Security/privacy docs published.

## 7. CI Plan

### CI-1. Minimal Rust CI

Affected file:

- `.github/workflows/rust.yml`

Implementation steps:

1. Replace current build-only workflow with matrix:
   - `cargo fmt --check`
   - `cargo test`
   - `cargo check`
   - `cargo check --features gui`
   - `cargo clippy --all-targets --features gui -- -D warnings`
2. For `gui,layer-shell`, run under Nix or install native packages.
3. Keep default/headless path tested separately.

Acceptance criteria:

- PR fails on formatting.
- PR fails on clippy warnings.
- GUI code path is checked.

### CI-2. Supply Chain Checks

Implementation steps:

1. Add `cargo-deny` config:
   - license policy;
   - advisory policy;
   - duplicate dependencies review.
2. Add optional `cargo machete` for unused deps.
3. Add `cargo audit` if `cargo-deny` not used.

Acceptance criteria:

- CI reports advisories.
- Dependency changes are visible and reviewed.

## 8. Documentation Deliverables

### DOC-1. `docs/security.md`

Must include:

- Assets:
  - clipboard text/images;
  - notifications;
  - API keys;
  - custom commands;
  - SQLite DB;
  - file index;
  - compositor controls.
- Attackers:
  - malicious command TOML;
  - malicious JSON output;
  - malicious clipboard content;
  - malicious notification sender;
  - malicious imported archive.
- Surfaces:
  - shell execution;
  - placeholder expansion;
  - import/export;
  - open URL/path;
  - notification server.
- Current limitations and planned capability model.

### DOC-2. `docs/privacy.md`

Must include:

- What data is stored.
- Paths:
  - `~/.config/zeshicast/zeshicast.db`
  - `~/.config/zeshicast/preferences.toml`
  - `~/.cache/zeshicast/clipboard/`
- Retention limits.
- How to clear data.
- What export includes/excludes.
- AI/translation request behavior.
- Private mode once implemented.

### DOC-3. `docs/development.md`

Must include:

- Build commands.
- Feature flags.
- Nix dev shell.
- Test strategy.
- How to add a provider.
- How to add a safe command action.
- How to avoid blocking GTK main thread.

## 9. Suggested Work Breakdown by Timebox

### One Day

1. Fix action panel row index bug.
2. Run `cargo fmt`.
3. Fix mechanical clippy warnings:
   - `io::Error::other`;
   - `split_once`;
   - `RangeInclusive::contains`;
   - `new_without_default`.
4. Add initial destructive confirmation for:
   - reboot;
   - poweroff;
   - process kill;
   - clear clipboard.

Definition of done:

- `cargo fmt --check` passes.
- `cargo test` passes.
- Manual action panel test passes.
- Dangerous actions no longer run on first Enter/Delete.

### One Week

1. Add `ActionRisk`, `Capability`, `ExecutionPolicy`.
2. Enforce JSON command capabilities.
3. Move startup snapshots to lazy/background hydration.
4. Move JSON command execution out of sync search path.
5. Add image cache cleanup.
6. Add focused tests for P0/P1.

Definition of done:

- `cargo test` passes with new safety tests.
- `cargo check --features gui` passes.
- Clippy either passes or has documented temporary UI-only allows.
- Launcher first paint is not dependent on `nmcli`/`wpctl`.

### One Month

1. Add SQLite migrations.
2. Add atomic writes and `0600`.
3. Add privacy settings/private mode/export policy.
4. Add Nix module options and runtime package policy.
5. Add security/privacy/development docs.
6. Add CI with Rust + Nix checks.
7. Add local extension manifest and approval persistence.

Definition of done:

- Project is credible for public beta.
- Docs explain risks and limitations.
- CI covers default, GUI, layer-shell via Nix.
- Extension commands cannot bypass capability policy.

## 10. Explicitly Deferred

Do not do now:

- JS/TS extension runtime.
- Public extension store/registry.
- Cloud sync.
- Remote telemetry.
- Complex AI agent memory/workflow system.
- Full NetworkManager/PipeWire native rewrites before P0/P1 are fixed.
- Broad UI redesign before safety and responsiveness are stable.

Reason: these add complexity and attack surface before the core command center is safe and predictable.

## 11. Tracking Checklist

P0:

- [x] P0-1 action panel row index bug fixed.
- [x] P0-2 destructive confirmations added.
- [x] P0-3 capabilities enforced.
- [x] P0-4 central executor introduced.

P1:

- [x] P1-1 `cargo fmt --check` green.
- [x] P1-2 clippy green or scoped allows documented.
- [x] P1-3 startup subprocess work removed.
- [x] P1-4 JSON commands async/cancellable.
- [x] P1-5 window search non-blocking.

P2:

- [x] P2-1 clipboard image cache retention implemented.
- [x] P2-2 privacy controls implemented.
- [x] P2-3 atomic writes and `0600` implemented.
- [x] P2-4 SQLite migrations implemented.
- [x] P2-5 docs synchronized.

Packaging/CI:

- [x] PKG-1 runtime dependencies policy implemented.
- [x] PKG-2 systemd targets aligned.
- [x] PKG-3 Nix CI verification added.
- [x] CI-1 Rust CI expanded.
- [x] CI-2 supply chain checks added.

Extensions:

- [x] EXT-1 local extension manifest implemented.
- [x] EXT-2 `argv` command mode implemented.
- [x] EXT-3 JS/TS runtime remains deferred.


# Full Review: zeshicast

Дата фиксации: 2026-06-27

## 1. Executive Summary

Verdict: проект живой и уже сильно вырос за пределы pet-project: есть рабочее ядро launcher/search, daemon mode, GTK views, SQLite для clipboard/usage, импорт с защитой от traversal, shell-placeholder escaping, Nix flake/Home Manager modules. Архитектурно направление перспективное, но сейчас главный риск не "нет фич", а то, что runtime безопасности, UI state и extension model отстают от количества возможностей.

Первые фиксы:

- confirmation/capabilities для shell/destructive actions;
- action panel index bug;
- lazy initialization heavy snapshots;
- `cargo fmt` / `clippy`;
- privacy retention для clipboard images/export/secrets.

Проверки:

- `cargo test` passed: 70 tests.
- `cargo check` passed.
- `cargo check --features gui` passed.
- `cargo check --features gui,layer-shell` failed: host lacks `gtk4-layer-shell-0.pc`.
- `cargo fmt --check` failed: many formatting diffs.
- `cargo clippy --all-targets --features gui -- -D warnings` failed: 53 clippy errors.
- `nix flake check` / `nix develop ...` not completed: Nix spent minutes fetching/unpacking nixpkgs; interrupted. Not counted as project bug.

## 2. Project Map

| Area | Files | Responsibility | Notes |
| --- | --- | --- | --- |
| CLI/core | `src/main.rs`, `src/lib.rs`, `src/app.rs`, `src/action.rs` | REPL, search orchestration, actions, state | Good split started, but `app.rs` still central coordinator |
| Config/security | `src/config.rs`, `src/placeholders.rs` | import/export, prefs, aliases, placeholder expansion | Import hardened; writes non-atomic; secrets plaintext |
| Search | `src/search/*` | apps/files/commands/scripts/system/window/web providers | Provider trait exists; no async/cancel boundary |
| Services | `src/services/*` | SQLite, MPRIS, notification store, system/network/audio polling | Useful snapshot API; mixed CLI/DBus; no migrations version |
| UI | `src/ui/launcher.rs`, `src/ui/views.rs`, `src/ui/style.rs`, helpers | GTK shell, navigation, views, clipboard monitor | Feature-rich but large: `launcher.rs` 2721 lines, `views.rs` 3603 |
| Packaging | `flake.nix`, `shell.nix`, `packaging/*`, `scripts/install-user.sh` | Nix package/modules, desktop/service install | Good base; runtime deps/options incomplete |
| Docs | `README.md`, `ACTION_PLAN.md`, `BACKLOG.md`, `docs/*` | User docs, roadmap, design | Strong README; some docs stale vs code |

## 3. Architecture Assessment

Strong decisions:

- Headless/default build separated from `gui`.
- `SearchProvider` abstraction exists.
- File index is deferred in GUI.
- HTTP actions run off caller thread.
- MPRIS uses D-Bus via `gio`.
- Import archive validation is much better than old `ACTION_PLAN.md` issue.

Weak decisions:

- Search aggregation is synchronous.
- Providers can run subprocesses in search path: `windows`, JSON commands.
- `permissions = [...]` is metadata only.
- Trusted built-ins and untrusted commands/scripts share `ActionKind::Shell`.
- UI wiring passes many widgets through huge functions instead of view/controller state structs.

Recommended target architecture:

- Keep `ActionKind`, but add `ActionRisk`, `CapabilitySet`, `ExecutionRequest`, `ExecutionPolicy`.
- Providers should return inert actions.
- One executor decides confirmation, permission, timeout, logging, environment, and whether `shell` is allowed.
- Split UI state into structs: `RootController`, `ActionPanelController`, `ClipboardController`, `DashboardController`.

## 4. Security Review

| ID | Severity | Area | Problem | Evidence | Fix |
| --- | --- | --- | --- | --- | --- |
| SEC-1 | High | Extensions/commands | No real capability enforcement; permissions informational | `README.md` says informational; `src/search/commands.rs` parses but does not enforce | Enforce capabilities before execution |
| SEC-2 | High | Destructive actions | Reboot/power/kill/clear/JSON shell can execute on Enter/Delete without confirmation | `src/search/system.rs`, `src/ui/launcher.rs`, JSON `shell` action | Confirmation for destructive classes |
| SEC-3 | Medium | Secrets/privacy | API keys and preferences plaintext/exported | `preferences.toml`, `export_config` tars whole config | `0600`, secret redaction/exclusion, keyring optional |
| SEC-4 | Medium | JSON commands | JSON output may produce `shell`, `open_path`, `open_url` actions | `parse_json_action_kind` in `src/search/commands.rs` | Require command-declared capabilities and per-action risk |
| SEC-5 | Medium | Open URL/path | `xdg-open` accepts arbitrary URL/path from config/JSON | `src/action.rs` | Scheme allowlist or confirmation for non-http/file |
| SEC-6 | Low | Archive import | Traversal/symlink fixed, but hardlink/device/special tar entries lack explicit tests | `src/config.rs` validates names and symlinks only | Add malicious tar fixture tests for hardlinks/devices |

### SEC-1: Capabilities Are Informational

Preconditions: user installs a malicious or careless command TOML/script.

Exploit scenario: malicious `commands/*.toml` declares harmless `permissions = ["network"]`, returns JSON with:

```json
{ "action": { "type": "shell", "value": "..." } }
```

Impact: arbitrary shell execution through launcher UI.

Recommended fix:

- Add `CapabilitySet` to command/script metadata.
- Deny `ActionKind::Shell` from JSON unless command has `shell`.
- Deny `OpenPath` unless command has `filesystem` or path is explicitly user-selected.
- Show permissions in extension browser and require approval on first run.

Acceptance criteria:

- JSON shell action is refused unless manifest has `shell` and user approved it.
- Tests cover `json_shell_action_requires_shell_capability`.

### SEC-2: Destructive Actions Need Confirmation

Preconditions: user searches explicit command or opens dangerous view.

Exploit scenario:

- `system power` then Enter powers off.
- `Delete` in System Monitor kills selected process.
- `docker-ps.toml` example says Enter stops a container.

Impact: accidental service stop, process kill, system reboot/poweroff.

Recommended fix:

- Add `ActionRisk::{Normal, Destructive, SystemPower, ProcessKill, Shell}`.
- Confirmation panel for `SystemPower`, `ProcessKill`, `ClearClipboardHistory`, JSON shell actions.
- Require second Enter or typed confirmation for reboot/poweroff.

Acceptance criteria:

- Reboot/poweroff cannot run on first Enter.
- Clipboard clear cannot run from action panel without danger confirmation.
- Process kill shows PID/name confirmation.

## 5. Performance Review

| ID | Impact | Area | Problem | Fix | Acceptance |
| --- | --- | --- | --- | --- | --- |
| PERF-1 | UI freeze up to 1s/query | JSON commands | `run_json_command` sync in search path | worker + debounce 150-250ms + cancel | typing never blocks GTK |
| PERF-2 | Slow first window | UI startup | initial `audio_snapshot`, `network_snapshot`, `system_snapshot`, `media_snapshot` created before first present | lazy/default snapshots, hydrate after present | first paint independent of subprocesses |
| PERF-3 | UI stall on poller start | `poll_cache` | `cache()` seeds network/audio synchronously | seed defaults, worker fills cache | `start_poll_cache` no subprocess on main |
| PERF-4 | Disk growth/privacy | Clipboard images | SQLite prunes entries, image cache files are not pruned | delete orphan PNGs after DB prune/clear/delete | cache bounded with tests |
| PERF-5 | UI freeze on `win` | Window search | compositor CLI calls no timeout | cached window service or timeout worker | unavailable compositor cannot hang search |

Notes:

- Old `ACTION_PLAN.md` items about HTTP action blocking, file index on startup, SQLite clipboard pruning, and import traversal are partly or fully fixed in current code.
- Remaining performance risk is mostly sync execution in search path and startup hydration.

## 6. Reliability and Correctness

Concrete bug: action panel index mismatch.

Evidence:

- `set_action_panel_list` renders non-selectable section headers in `src/ui/views.rs`.
- `run_action_panel_row(... row.index())` indexes `filtered_action_panel_items` directly in `src/ui/launcher.rs`.
- Section headers are displayed rows but not entries in `filtered_action_panel_items`.

Impact: first selectable action after a header can execute the wrong secondary action.

Fix:

- Store a stable action index on each selectable row, or build a displayed vector that includes headers and map only selectable rows.

Acceptance:

- Add test `action_panel_row_index_ignores_section_headers`.
- Activating "Run", "Copy Value", "Clear Clipboard History" always maps to the visible selected row.

Other issues:

- No schema/migration table for SQLite.
- Many silent `.ok()` writes.
- Non-atomic prefs/pins/snippets writes.
- `notify_server` uses `BusNameOwnerFlags::REPLACE`, while README says another daemon prevents ownership.
- Docs still mention stale `recent.txt` / `*.sqlite` while code uses `zeshicast.db`.

## 7. Rust Code Quality

Good:

- Feature-gated GUI.
- Enums for actions.
- Parser tests.
- Shell placeholder quoting.
- D-Bus MPRIS without `playerctl`.

Debt:

- `src/ui/launcher.rs` and `src/ui/views.rs` are God modules.
- Many functions have 10-28 args.
- Error types are mostly `io::Error` / strings.
- `ActionKind::Shell(String)` remains stringly and global.
- `cargo clippy --all-targets --features gui -- -D warnings` fails on 53 diagnostics.

Priority refactors:

- Introduce controller structs for UI views.
- Introduce typed executor.
- Add focused error enums with `thiserror` or small custom enums.
- Split view construction from event wiring.

## 8. NixOS and Packaging

`flake.nix` is a solid base:

- `rustPlatform.buildRustPackage`.
- `buildFeatures = [ "gui" "layer-shell" ]`.
- `wrapGAppsHook4`.
- NixOS and Home Manager modules.
- `x86_64-linux` and `aarch64-linux`.

Missing/incomplete:

- Runtime PATH wrapper only includes `wl-clipboard`.
- App also shells to `wpctl`, `nmcli`, `ip`, `brightnessctl`, `bluetoothctl`, `wtype`, `grim`, `slurp`, compositor CLIs, `df`, `kill`, `xclip`.
- Some should be optional user-session deps, but module options/docs should be explicit.
- Non-Nix service uses `WantedBy=default.target`; flake uses `graphical-session.target`.

Recommended module options:

- `services.zeshicast.enable`.
- `services.zeshicast.package`.
- `services.zeshicast.enableNotificationServer`.
- `services.zeshicast.enableClipboardWatcher`.
- `services.zeshicast.extraRuntimePackages`.
- `services.zeshicast.settings`.
- `services.zeshicast.conflictWithNotificationDaemonsWarning`.

## 9. Product Completeness

Already strong:

- App/file search.
- Calculator.
- Commands.
- Snippets.
- Clipboard text/image.
- Notification server.
- Dashboard views.
- MPRIS.
- Nix modules.
- Docs/screenshots.

Missing for daily use:

- Confirmation model.
- Predictable extension safety.
- Fast first paint.
- Stable settings/migrations.
- Privacy controls.
- Cache cleanup.

Missing for beta:

- CI matrix.
- Nix check in CI.
- `cargo fmt` / `clippy` green.
- Config/db migrations.
- Permission UI.
- Issue/security policy.
- Release artifacts.

Missing for public open source release:

- Threat model.
- Security policy.
- Changelog/semver.
- Release process.
- Reproducible build notes.
- Clear extension compatibility story.

## 10. Testing Plan

Unit/integration tests to add:

- `action_panel_row_index_ignores_section_headers`.
- `json_shell_action_requires_shell_capability`.
- `system_power_requires_confirmation`.
- `clipboard_image_cache_prunes_orphans`.
- `preferences_written_0600_and_atomic`.
- `import_rejects_parent_traversal`.
- `import_rejects_symlink`.
- hardlink/device tar fixture tests.
- `window_provider_timeout_when_compositor_hangs`.
- temp `$HOME` integration with sample `.desktop`, command TOML, fake JSON command, fake SQLite.

CI minimum:

- `cargo fmt --check`.
- `cargo test`.
- `cargo check`.
- `cargo check --features gui`.
- `cargo check --features gui,layer-shell` inside Nix.
- `cargo clippy --all-targets --all-features -- -D warnings`.
- `nix flake check`.
- `cargo-deny` or `cargo-audit`.

## 11. New Roadmap

| Priority | Task | Why | Files/Areas | Acceptance |
| --- | --- | --- | --- | --- |
| P0 | Fix action panel index bug | Wrong command execution | `src/ui/launcher.rs`, `src/ui/views.rs` | row activation maps displayed row to action safely |
| P0 | Add destructive confirmations | Prevent accidental power/kill/clear | actions/UI | reboot/power/kill/clear require confirmation |
| P0 | Enforce capabilities | Extension safety | commands/scripts/action executor | JSON shell denied unless approved |
| P1 | Lazy first paint | Responsiveness | `build_ui`, `poll_cache` | no startup subprocess before present |
| P1 | Async JSON/window providers | Search responsiveness | `search/commands.rs`, `search/windows.rs` | no GTK blocking |
| P1 | fmt/clippy green | CI readiness | all Rust | requested checks pass |
| P2 | Privacy controls | Daily trust | clipboard/storage/export/docs | retention, clear all, exclude export |
| P3 | Local extension manifests | Ecosystem | commands/scripts registry | typed manifests + permissions |
| Deferred | JS/TS runtime/store | Too much unsafe complexity now | extension runtime | only after core safety stable |

## 12. Best Practice Recommendations

Rust:

- Introduce `thiserror` or focused error enums.
- Replace shell strings with typed execution requests.
- Split UI state structs.

GTK4/Wayland:

- Avoid subprocess snapshots before first present.
- Keep heavy views view-gated.
- Add confirmation panels.

Security:

- Capability enforcement.
- Destructive confirmation.
- URL scheme allowlist.
- Private mode.
- Permission UI.

Storage:

- SQLite migrations table.
- Indexes if history grows.
- Atomic writes.
- File permissions `0600`.
- Image cache GC.

NixOS:

- Wrap or document runtime tools.
- Expose module options.
- Align non-Nix service target.
- Test flake in CI.

Docs:

- Add threat model.
- Add privacy model.
- Add command safety guide.
- Add notification daemon conflict details.
- Add migration/release guide.
- Mark `permissions` as currently informational until enforcement lands.

## 13. Extension Runtime Deep Dive

Extension runtime сейчас не нужен. Достаточно script/TOML commands, но только после safety boundary.

Pragmatic path:

1. Typed local script commands.
2. Real permissions/capabilities.
3. Safe command execution abstraction.
4. Local extension manifest.
5. Extension registry.
6. Optional JS runtime only after core is stable.

Required model:

- Built-ins trusted.
- User TOML/scripts semi-trusted but capability-gated.
- Future registry extensions untrusted by default.
- Shell/json commands must never silently bypass capability model.

## 14. Privacy Deep Dive

Sensitive data:

- Clipboard text/images.
- Notification history.
- AI prompts.
- Translation requests.
- API keys.
- Config export.
- Search history/recent/frequency.
- File index/home paths.

Needed controls:

- Retention limit for clipboard text and images.
- Clear all must delete image cache too.
- Per-feature enable/disable.
- Private mode pause capture.
- Exclude patterns for file index and clipboard.
- Export should exclude secrets/cache by default.
- `preferences.toml` and DB should be `0600`.

## 15. Final Verdict

Проект уже близок к personal daily driver для автора на Niri/NixOS, но не готов как public beta без P0/P1.

Сильное:

- local-first Linux-native scope;
- pragmatic GTK/Rust implementation;
- good feature velocity;
- many old review issues actually fixed.

Prototype debt:

- huge UI modules;
- no real extension permissions;
- sync search edges;
- weak privacy defaults.

Что сделать за 1 день:

- Fix action panel bug.
- Add confirmations.
- Run `cargo fmt`.
- Clean clippy basics.

Что сделать за 1 неделю:

- Capability executor.
- Async JSON/window search.
- Lazy startup snapshots.
- Image cache GC.

Что сделать за 1 месяц:

- Migrations.
- Declarative Nix settings.
- Privacy/export policy.
- CI/release/security docs.

# Remediation Acceptance

This is the acceptance record for
[`docs/remediation-plan-2026-09-11.md`](remediation-plan-2026-09-11.md): every
finding in that plan, the step that closed it, the commit, and the test that
holds it closed today.

Base revision `f2bd868`, 52 commits (this record is the last of them).

The table's test column carries the same machine-read markers as
`docs/security.md` (`&lt;!-- test: name --&gt;`), checked by
`documented_invariants_have_the_tests_they_name` (`src/lib.rs`): a test renamed
here without being renamed in the tree fails `cargo test`.

## How this is verified

```sh
cargo fmt --check
cargo clippy --all-targets --features gui,layer-shell -- -D warnings
cargo test --features gui,layer-shell          # GUI palette
cargo clippy --all-targets --features desktop -- -D warnings
cargo test --features desktop                  # headless capability build
cargo clippy --all-targets -- -D warnings

cargo test                                     # no features
```

CI runs those six through `nix-shell` (`.github/workflows/rust.yml`), plus
`cargo-deny` for advisories and licences.

Counts at HEAD: **316** GUI tests (311 library + 4 CLI + 1 gtk binary), **254**
without features (250 + 4 CLI), **258** with `desktop` (254 + 4 CLI); all three
clippy configurations clean.

## Findings

| Finding | Step | Commit | Kept true by |
| --- | --- | --- | --- |
| B-1 `shell` через форму | P1.2 | `c185b2d` | `shell_command_without_shell_permission_has_no_form` <!-- test: shell_command_without_shell_permission_has_no_form --> `shell_command_with_shell_permission_offers_a_confirming_form` <!-- test: shell_command_with_shell_permission_offers_a_confirming_form --> `form_submission_requires_confirmation` <!-- test: form_submission_requires_confirmation --> `form_without_capabilities_is_denied_even_when_confirmed` <!-- test: form_without_capabilities_is_denied_even_when_confirmed --> |
| B-2 инъекция в `'…{{x}}…'` | P1.3, P1.3b | `c5dccf2`, `02a7515` | `placeholder_inside_open_single_quote_run_is_literal` <!-- test: placeholder_inside_open_single_quote_run_is_literal --> `apostrophe_value_in_single_quote_run_round_trips` <!-- test: apostrophe_value_in_single_quote_run_round_trips --> `a_placeholder_in_a_heredoc_body_is_escaped_for_that_body` <!-- test: a_placeholder_in_a_heredoc_body_is_escaped_for_that_body --> `a_value_that_would_end_the_body_is_not_substituted` <!-- test: a_value_that_would_end_the_body_is_not_substituted --> |
| B-3 `wait()` без таймаута | P2.1 | `279976b` | `test_call_json_rpc_with_args_timeout` <!-- test: test_call_json_rpc_with_args_timeout --> `lingering_child_is_reaped_after_answering` <!-- test: lingering_child_is_reaped_after_answering --> `stdin_write_failure_still_reaps_the_child` <!-- test: stdin_write_failure_still_reaps_the_child --> `oversized_response_is_rejected` <!-- test: oversized_response_is_rejected --> |
| B-4 режим `=` | P3.5, P3.5b | `eccdcd1`, `13569d1` | `calc_row_is_the_first_action` <!-- test: calc_row_is_the_first_action --> `calc_row_copies_the_result` <!-- test: calc_row_copies_the_result --> `a_row_reports_the_action_that_was_registered_for_it` <!-- test: a_row_reports_the_action_that_was_registered_for_it --> `an_unregistered_row_never_runs_a_neighbour_action` <!-- test: an_unregistered_row_never_runs_a_neighbour_action --> |
| B-5 `commands`-бинарники | P1.5 | `c185b2d` | `manifest_commands_rejects_non_toml` <!-- test: manifest_commands_rejects_non_toml --> `manifest_binaries_require_shell` <!-- test: manifest_binaries_require_shell --> `command_cannot_self_grant_shell_inside_empty_manifest` <!-- test: command_cannot_self_grant_shell_inside_empty_manifest --> |
| B-6 argv/скрипт-формы | P1.4 | `c185b2d` | `argv_command_requires_confirmation` <!-- test: argv_command_requires_confirmation --> `argv_sh_c_is_not_silent` <!-- test: argv_sh_c_is_not_silent --> `argv_placeholders_expand_without_shell_quoting` <!-- test: argv_placeholders_expand_without_shell_quoting --> `argv_command_entries_parse_without_shell_command` <!-- test: argv_command_entries_parse_without_shell_command --> |
| M-1 поиск на UI-потоке | P3.1 | `5457df6`, `c895b3f`, `c457418` | `stale_results_are_discarded` <!-- test: stale_results_are_discarded --> `a_result_that_became_stale_while_searching_is_not_shown` <!-- test: a_result_that_became_stale_while_searching_is_not_shown --> `debounce_coalesces_keystrokes` <!-- test: debounce_coalesces_keystrokes --> `search_runs_off_the_main_thread` <!-- test: search_runs_off_the_main_thread --> `a_panicking_search_does_not_wedge_the_flow` <!-- test: a_panicking_search_does_not_wedge_the_flow --> |
| M-2 media D-Bus | P3.2 | `8fcac3b` | `stuck_player_is_skipped_after_two_timeouts` <!-- test: stuck_player_is_skipped_after_two_timeouts --> `players_that_left_the_bus_are_forgotten` <!-- test: players_that_left_the_bus_are_forgotten --> |
| M-3 `/proc`+niri на таймере | P3.2 | `8fcac3b` | manual: `strace -f -e trace=process` on a hidden palette |
| M-4 таймеры без видимости | P3.3 | `97a44c5` | `poller_can_be_paused_and_resumed` <!-- test: poller_can_be_paused_and_resumed --> manual: `strace` checklist |
| M-5 pipe-deadlock | P3.6 | `c526671` | `large_output_is_not_lost` <!-- test: large_output_is_not_lost --> |
| M-6 стоимость files-поиска | P3.7 | `5028f44` | `file_search_over_ten_thousand_entries_is_fast` <!-- test: file_search_over_ten_thousand_entries_is_fast --> `explicit_queries_do_not_scan_the_index` <!-- test: explicit_queries_do_not_scan_the_index --> |
| M-7 kill по индексу | P3.4 | `7190100` | `kill_uses_row_pid_not_index` <!-- test: kill_uses_row_pid_not_index --> `selection_survives_refresh` <!-- test: selection_survives_refresh --> |
| M-8 таймеры вью | P3.2, P3.9 | `8fcac3b`, `9bc5998`, `b90ea22`, `907e397`, `537dcf0` | `a_drag_applies_only_the_last_value` <!-- test: a_drag_applies_only_the_last_value --> `one_gesture_means_one_wpctl_call_per_sink` <!-- test: one_gesture_means_one_wpctl_call_per_sink --> `a_player_that_never_catches_up_does_not_freeze_the_scrubber` <!-- test: a_player_that_never_catches_up_does_not_freeze_the_scrubber --> `a_snapshot_does_not_yank_the_scrubber_while_a_seek_is_in_flight` <!-- test: a_snapshot_does_not_yank_the_scrubber_while_a_seek_is_in_flight --> |
| M-9 зомби | P2.2 | `93c3e67` | `detached_children_are_reaped_without_zombies` <!-- test: detached_children_are_reaped_without_zombies --> `killed_detached_child_is_reaped_by_the_sweep` <!-- test: killed_detached_child_is_reaped_by_the_sweep --> `guard_reaps_a_child_that_outlives_its_usefulness` <!-- test: guard_reaps_a_child_that_outlives_its_usefulness --> |
| M-10 потеря preferences | P2.3 | `04ed162` | `corrupt_preferences_are_backed_up_not_overwritten` <!-- test: corrupt_preferences_are_backed_up_not_overwritten --> `preference_value_with_newline_round_trips` <!-- test: preference_value_with_newline_round_trips --> `write_preferences_creates_0600_file` <!-- test: write_preferences_creates_0600_file --> |
| M-11 импорт без лимитов | P2.4 | `c26e6a8` | `import_rejects_bomb` <!-- test: import_rejects_bomb --> `import_rejects_traversal_and_unsafe_members` <!-- test: import_rejects_traversal_and_unsafe_members --> `import_rejects_oversized_archive` <!-- test: import_rejects_oversized_archive --> `import_rejects_too_many_members` <!-- test: import_rejects_too_many_members --> |
| M-12 export тянет БД | P2.4 | `c26e6a8` | `safe_export_excludes_clipboard_db` <!-- test: safe_export_excludes_clipboard_db --> `export_does_not_follow_symlinks` <!-- test: export_does_not_follow_symlinks --> `export_preferences_sanitizer_removes_secret_keys` <!-- test: export_preferences_sanitizer_removes_secret_keys --> |
| M-13 prune/сниппеты скрываются | P3.9, P4.4 | `9bc5998`, `1c044f3` | `failing_to_restrict_the_database_is_reported_not_swallowed` <!-- test: failing_to_restrict_the_database_is_reported_not_swallowed --> |
| M-14 D-Bus ввод | P2.5, P2.5b | `186ff41`, `ff49453` | `notify_with_huge_body_is_capped` <!-- test: notify_with_huge_body_is_capped --> `app_icon_outside_icon_dirs_is_ignored` <!-- test: app_icon_outside_icon_dirs_is_ignored --> `an_icon_load_is_refused_when_its_toast_is_gone` <!-- test: an_icon_load_is_refused_when_its_toast_is_gone --> |
| M-15 image-путь | P1.7 | `c185b2d` | `spoofed_image_entry_is_rejected_and_stored_as_text` <!-- test: spoofed_image_entry_is_rejected_and_stored_as_text --> `image_path_outside_cache_is_rejected` <!-- test: image_path_outside_cache_is_rejected --> |
| M-16 путь скрипта в `sh -c` | P1.6 | `c185b2d` | `script_path_with_semicolon_is_not_shell_interpreted` <!-- test: script_path_with_semicolon_is_not_shell_interpreted --> |
| M-17 systemd sandbox | P4.1 | `efd6aa4` | `the_daemon_unit_does_not_restrict_what_it_launches` <!-- test: the_daemon_unit_does_not_restrict_what_it_launches --> manual: launch Firefox from the palette |
| M-18 `.desktop` Exec | P4.2 | `c3ff40d` | `desktop_exec_with_quotes_splits_like_the_spec` <!-- test: desktop_exec_with_quotes_splits_like_the_spec --> `backslash_escapes_are_undone_before_exec` <!-- test: backslash_escapes_are_undone_before_exec --> `field_codes_are_expanded_or_dropped` <!-- test: field_codes_are_expanded_or_dropped --> `an_exec_without_a_program_does_not_become_a_shell_command` <!-- test: an_exec_without_a_program_does_not_become_a_shell_command --> |
| MINOR: риск-таблица, CLI, мёртвый код, docs | P4.3, P5.5, P5.6 | `fb7411f`, `9175075`, `b1de4ea` | `query_with_export_substring_is_a_query` <!-- test: query_with_export_substring_is_a_query --> `a_denial_is_reported_on_stderr` <!-- test: a_denial_is_reported_on_stderr --> `export_failure_exits_nonzero` <!-- test: export_failure_exits_nonzero --> `documented_invariants_have_the_tests_they_name` <!-- test: documented_invariants_have_the_tests_they_name --> |

## Where the plan's expectation changed

The plan measured some things before the code existed, and the fixes measured
again:

- **P3.7 (M-6)**: the plan assumed the file search could go from 8–11 ms to
  under 10 ms. Precomputing the lowercased name and splitting the explicit-query
  path brought 10 000 entries to ~7 ms; the test asserts a loose 30 ms bound and
  prints the measurement instead of encoding a machine-specific number.
- **P3.6 (M-5)**: the defect was not the 200 ms timeout but the pipe nobody
  drained. The timeout was raised to 500 ms anyway, because a compositor that
  answers slowly is not broken.
- **P4.1 (M-17)**: the plan's variant A (`systemd-run --user --scope`) was
  measured and does not work -- the transient scope inherits the daemon's mount
  namespace and seccomp -- so the unit keeps its child-safe subset and drops the
  twelve inherited directives. The cost is documented: `systemd-analyze
  security` moves from 3.4 OK to 8.1 EXPOSED.
- **M-9**: instead of the plan's `scripts/check-zombies.sh`, the sweep has three
  tests (`process.rs`), and `SIGCHLD = SIG_IGN` was rejected because the kernel
  reaping it causes breaks every `wait()` consumer.
- **P5.4**: the crate root ends with ten public items and three public modules,
  not twelve items; `search/commands.rs` and `search/scripts.rs` stayed on
  `gui` (their output is a palette row), which is why `JsonCommandAction` keeps
  a `cfg_attr` for the headless build.
- **P5.6**: the doc checker had two defects of its own, both found while pointing
  it at this record: it read the `&lt;!-- test: name --&gt;` example in the
  documents' prose as a reference to a test called `name`, and it looked for
  four-space-indented `fn`s, so a test inside a nested module was reported
  missing. Fixed in `60ec459`.
- **#46**: the flake was in the test, not the code -- it waited five seconds for
  a marker written by a detached child, which is a race against machine load. The
  claim is structural (the `;` in the script's name never reaches a shell) and the
  run-for-real tail is synchronous now. Fixed in `fce6258`.

## Still owed by a human

These are the plan's hands-on criteria; they need a running session, not a test:

- **M-3/M-4**: `strace -f -e trace=process` around the daemon with the palette
  hidden -- no periodic `execve` for `/proc`, `niri`, `wpctl`, `nmcli` while
  nothing is visible.
- **M-17**: start the user service, open Firefox from the palette, confirm the
  profile is writable and `/tmp` is shared with a shell.

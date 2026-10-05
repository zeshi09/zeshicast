# План исправлений по ревью 2026-10-04 — zeshicast

Источник находок: [`docs/full-review-2026-10-04.md`](full-review-2026-10-04.md) (разделы 1–3, план — §7).
Ревизия на старте: `15a72ec` (+ незакоммиченная правка в `src/search/scripts.rs`).

Как пользоваться: шаги упорядочены по приоритету. Каждый шаг — «находка → правка → тест → критерий приёмки».
Статус: ✅ done — сделано и проверено в этой сессии; 🔜 open — не начато; 🟡 partial.

## Как проверять (полный набор, как CI)

```sh
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test

# gui/desktop-конфигурации доступны через dev-shell (GTK4/glib есть):
nix develop --command cargo clippy --all-targets --features desktop -- -D warnings
nix develop --command cargo test --features desktop
nix develop --command cargo clippy --all-targets --features gui,layer-shell -- -D warnings
nix develop --command cargo test --features gui,layer-shell
```

---

## Фаза 1. Безопасность квотирования и приватности

### P1 — ✅ done. Heredoc-контекст в плейсхолдерах (N-1, Blocker)

* **Правка:** `src/placeholders.rs` — `shell_context` заменён единым `scan_shell` (тот же лексер, что и `open_heredoc`): тела **закрытых** heredoc пропускаются целиком, поэтому кавычка в теле больше не меняет контекст последующих плейсхолдеров. В `expand` — один скан на плейсхолдер (`(context, open_heredoc)`).
* **Тесты:** `apostrophe_in_a_closed_heredoc_body_does_not_unquote_a_later_value`, `heredoc_body_quote_does_not_let_a_value_execute` (запускает `sh -c` и проверяет отсутствие маркера).
* **Критерий приёмки:** шаблон `cat <<EOF\nit's\nEOF\necho {{clipboard}}` со значением `$(touch …)` → значение литерал, маркер не создаётся. ✅

### P2 — ✅ done. Collapse-эвристика `'{{x}}'` (N-2, High)

* **Правка:** `src/placeholders.rs` — эвристика схлопывания удалена. `escape_for_context` корректен во всех трёх контекстах, а эвристика могла сработать только на **закрывающей** кавычке (открывающая оставила бы контекст `Single`), т.е. оставляла значение без квотирования.
* **Тесты:** `author_quotes_beside_a_placeholder_keep_the_value_quoted` (`echo 'x'{{c}}'y'` → `echo 'x''$(x)''y'`; `printf ''{{c}}''` → `printf '''$(x)'''`), `quotes_beside_a_placeholder_do_not_let_a_value_execute` (маркер), `non_shell_expansion_keeps_literal_quotes` (`x'{{q}}'y` → `x'v'y`, не теряется закрывающая кавычка).
* **Критерий приёмки:** оба шаблона не исполняют значение; `echo '{{c}}'` по-прежнему даёт `echo 'v'`; не-shell-режим сохраняет литеральные кавычки. ✅

### P3 — ✅ done (после уточнения). Приватный режим и кэш изображений (N-3)

* **Уточнение:** основная часть первичной находки — ложноположительная: `clipboard_capture_images()` (`app/preferences.rs:147-150`) уже включает `!clipboard_private_mode()`, поэтому история картинок защищена. Реальная утечка — watcher пишет PNG в кэш до проверок.
* **Правка:** `src/ui/clipboard_capture.rs` — `static IMAGE_CAPTURE_ALLOWED: AtomicBool`; главный поток обновляет его из `clipboard_capture_images()` в тике наблюдателя; `watch_clipboard_image_with` проверяет флаг **до** `save_clipboard_image`.
* **Тест:** `disabled_image_capture_writes_nothing` (gui) — при выключенном флаге ни путь не форвардится, ни PNG не пишется.
* **Критерий приёмки:** включение приватного режима после старта не оставляет новых PNG в `~/.cache/zeshicast/clipboard`. ✅ (в пределах кэша; ручная проверка на живой машине желательна)

### P4 — ✅ done. Pipe-deadlock в `run_json_command` (N-4, High)

* **Правка:** новый `crate::process::run_capped(&mut Command, timeout, stdout_cap, stderr_cap)` — читает stdout/stderr на отдельных потоках (не блокируя ребёнка на полном пайпе), ждёт с дедлайном, убивает по таймауту, отдаёт `CappedRun { status, stdout, stderr, timed_out, stdout_truncated, stderr_truncated }`. `run_json_command` переведён на него.
* **Тесты:** `process::tests::run_capped_reads_more_than_the_pipe_buffer`, `run_capped_kills_a_child_that_outlives_the_deadline`, `run_capped_discards_output_past_the_cap`; `search::commands::tests::a_large_json_command_stdout_does_not_deadlock`, `a_json_command_over_the_cap_is_rejected_cleanly` (gui).
* **Критерий приёмки:** вывод 200 КиБ возвращается целиком (не таймаутится), 600 КиБ → внятная ошибка «512 KiB», а не таймаут. ✅

---

## Фаза 2. Ресурсы и жизненный цикл (🔜 open)

### P5 — 🔜 N-5: `join()` читателя виснет на внуках расширения

`src/services/extension_protocol.rs:135-141` — после таймаута `drop(child); reader_handle.join()`; внук, унаследовавший stdout, держит пайп. **Правка:** `process_group(0)` + kill по `-pgid`, либо не `join()`-ить на error-путях. **Тест:** расширение с фоновым `sleep`, удерживающим stdout, не вешает вызов дольше таймаута; число fd не растёт.

### P6 — ✅ done. Уведомления из воркер-потоков (N-6, Medium)

`src/services/notifications.rs` — `thread_local! STATE` заменён на process-global `Mutex<NotificationState>` (`OnceLock` + `lock_state()`, с восстановлением при отравлении); все аксессоры переведены. Теперь `push_notification` из `run_extension_item`/HTTP-worker (`action.rs:887,921`) виден главному потоку. Тесты сериализованы через `isolated()` (общий глобальный store), регрессия — `a_worker_thread_push_is_visible_from_another_thread`. ✅

### P12 — 🔜 N-7: недоверенный `mpris:artUrl`

`src/ui/views/media.rs:305-325` — `file://` синхронно грузится/декодируется на главном цикле без конфайнмента; http — `read_to_end` без лимита/таймаута. **Правка:** конфайнмент `file://` (канонизация + cache/allow-list, как `safe_icon_path_in`), лимит байт и общий дедлайн для http. **Тест:** `file:///etc/passwd` и большой http-ответ не роняют/не вешают UI.

### P7 — ✅ done. Ограничение рекурсии калькулятора (N-13, Medium)

`src/search/calculator.rs` — поле `depth` и `const MAX_DEPTH = 64`; `factor` считает глубину и возвращает `Err("expression nested too deeply")` вместо переполнения стека (рекурсия вынесена в `factor_inner`). **Тест:** `calculator_rejects_deeply_nested_expressions` (`src/lib.rs`) — 50 000 пар скобок дают ошибку, а не `SIGABRT`. ✅

## Фаза 3. Данные и конфигурация (🔜 open)

### P8 — ✅ done. CI проверяет advisories (N-11, Medium)

`.github/workflows/rust.yml:94` — `command: check licenses bans sources` → `check advisories licenses bans sources`. Проверка сразу нашла реальную уязвимость: `rustls v0.23.40` (RUSTSEC-2026-0285, через `ureq`). **Правка:** `cargo update -p rustls` → `rustls 0.23.45` (+ `rustls-webpki 0.103.15`). **Проверка:** `cargo deny check advisories` → ok; `check licenses bans sources` → ok; `cargo check`/`test` (261) зелёные. ✅

### P9 — ✅ done. Раскрытие `~` в `script_dirs` (N-14, Medium)

`src/app/preferences.rs` — `expand_home_dir`: `~` → `$HOME`, `~/…` → `$HOME/…`, иначе без изменений. **Тест:** `script_dirs_expand_a_leading_tilde` — `~/scripts` → `$HOME/scripts`, `~` → `$HOME`, абсолютный путь не трогается. ✅

### P10 — 🔜 N-9/N-10: TOML-экспорт/ключи

`src/config.rs:189-191` — `strip_env_table` не видит `[ env ]`/`[env ]` и inline `env = {…}`; `src/config.rs:493` — ключи пишутся без экранирования. **Правка:** нормализовать заголовок `[...]`, удалять inline `env`, экранировать/валидировать ключи. **Тесты:** safe-export без токенов во всех формах; ключ с `"`/`\n` не ломает файл и не флипает `export_include_secrets`.

### P13 — 🔜 N-8: импорт стирает историю

`src/config.rs:275-297` — не удалять backup автоматически (или мержить `zeshicast.db`) + документировать. **Тест:** импорт safe-экспорта сохраняет историю или явно предупреждает.

### P11 (остаток) — ✅ done для N-12

`run_script_stdout_with_args` переведён на `run_capped` (30 с, 1 МиБ); тест `script_stdout_larger_than_the_pipe_buffer_is_captured`. Остаток — необязательно поднять общий helper для `windows.rs` (там свой дренаж).

## Фаза 4. Инварианты и гигиена (🔜 open)

### P14 — 🔜 N-15: три spawn-пути мимо единого гейта

`src/search/commands.rs:587`, `src/search/scripts.rs:295`, `src/services/text_input.rs:19` — провести через `execute()`/`preflight`; добавить тест-гвард на прямые `Command::new` вне `process.rs`/`action.rs`.

### P15 — 🔜 Minor-пакет

xclip `spawn_detached` (`services/clipboard_store.rs:335-341`); `prune`-`?` → `log::warn!` (`app/clipboard.rs:33`); snippets — не глотать `.ok()` + экранировать формат (`app/snippets.rs:36-119`); `font_desc` через `glib::markup_escape_text` (`ui/fonts.rs:237`); кап декода иконок (`ui/osd.rs:331`); `wait()` с таймаутом в `copy_with` (`action.rs:1178`); лимиты `local_ai`/web; chmod WAL/SHM (`services/storage.rs:14-29`); `replaces_id` → привязка к отправителю; `n_children()`-проверка в `notify_server.rs:109`.

### P16 — 🔜 Ops-пакет

Clippy для дефолтных фич в CI; убрать дубль джобы `gui-check`/`layer-shell-check`; `install-user.sh` → flake-devShell + опция `layer-shell`; README-оговорка для Non-Nix; правка URL во flake-комментарии (`flake.nix:121`).

### P17 — 🔜 UX-долг (из MINOR прошлого ревью)

Фильтр/сортировки процессов и per-row kill (`ui/views/system_monitor.rs`), футер, проверка start-time при kill.

### N-16 — ✅ done. Флейк изоляции теста `browser_tabs`

`src/search/browser_tabs.rs:355` — тест зависел от пустого процесс-глобального кэша; другие тесты успевали его заполнить → недетерминированный провал `cargo test` (воспроизведено на чистом HEAD). **Правка:** `#[cfg(test)] reset_tab_cache()` в начале теста. **Проверка:** `cargo test` дважды — 260 passed.

---

## Сводная таблица: находка → шаг → тест

| Находка | Severity | Шаг | Тест / критерий | Статус |
| --- | --- | --- | --- | --- |
| N-1 heredoc-контекст | Blocker | P1 | `heredoc_body_quote_does_not_let_a_value_execute` | ✅ |
| N-2 collapse `'{{x}}'` | High | P2 | `quotes_beside_a_placeholder_do_not_let_a_value_execute` | ✅ |
| N-3 кэш изображений | Medium | P3 | `disabled_image_capture_writes_nothing` | ✅ |
| N-4 pipe-deadlock JSON | High | P4 | `a_large_json_command_stdout_does_not_deadlock` | ✅ |
| N-16 флейк browser_tabs | Medium | — | `cargo test` ×2 | ✅ |
| N-12 скрипты без таймаута | Medium | P11 | `script_stdout_larger_than_the_pipe_buffer_is_captured` | ✅ |
| N-5 join() на внуках | Medium | P5 | расширение с фоновым процессом | 🔜 |
| N-6 уведомления воркеров | Medium | P6 | уведомление из worker-потока | ✅ |
| N-7 `mpris:artUrl` | Medium | P12 | `file:///etc/passwd`, большой http | 🔜 |
| N-8 импорт стирает историю | Medium | P13 | сохранение истории при импорте | 🔜 |
| N-9 env-strip обход | Medium | P10 | safe-export без токенов | 🔜 |
| N-10 TOML-ключи | Medium | P10 | ключ с `"`/`\n` | 🔜 |
| N-11 CI без advisories | Medium | P8 | `cargo deny check advisories` | ✅ |
| N-13 рекурсия калькулятора | Medium | P7 | глубокое выражение → ошибка | ✅ |
| N-14 `~` в script_dirs | Medium | P9 | `~/…` → `$HOME` | ✅ |
| N-15 spawn мимо гейта | Medium | P14 | grep-гвард | 🔜 |
| Minor-пакет | Low | P15 | по тесту на пункт | 🔜 |
| Ops-пакет | Low | P16 | CI/README | 🔜 |
| UX-долг | Low | P17 | чек-лист | 🔜 |

## Критерии готовности (Definition of Done)

1. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` — зелёные без флагов.
2. `desktop` и `gui,layer-shell` clippy/test — зелёные через `nix develop`.
3. Для каждой закрытой находки есть тест, названный в таблице выше.
4. Ручные проверки, которые тестом не закрываются (strace простоя M-3/M-4; запуск Firefox M-17; `ls -l` на WAL/SHM; `gdbus call` с пустым кортежем), пройдены на живой машине и записаны в `docs/remediation-acceptance.md`.

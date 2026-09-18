# План исправлений по итогам ревью — zeshicast

**Основание:** `docs/full-review-2026-09-11.md` (ревизия `f2bd868`)
**Статус:** выполнен; приёмка по каждому пункту — `docs/remediation-acceptance.md`.
**Принципы:** fail-closed; одна точка исполнения; ни одного изменения поведения без теста; шаг = один коммит = один проверяемый результат.
**Оценка:** фазы 0–2 ≈ 4–7 дней, фаза 3 ≈ 3–5 дней, фаза 4 ≈ 1 день, фаза 5 — фоном.

---

## Как пользоваться планом

* **ID** (`P1.2`, `P3.4`) — ссылка из отчёта о ревью в коммит-сообщения.
* Каждый шаг содержит: **что менять → тест → критерий приёмки**.
* Приёмка любого шага: `cargo fmt --check && cargo clippy --all-targets --features gui,layer-shell -- -D warnings && cargo test --features gui,layer-shell` зелёные (локально после фазы 0, в CI всегда).
* Порядок внутри фазы можно менять, фазы — нет: фаза 1 меняет API исполнения, на который опираются шаги фаз 3–4.
* Параллелить безопасно: `P1.3` (placeholders) ↔ `P1.7` (clipboard-путь) ↔ `P2.*`; `P3.*` можно резать по файлам через отдельные worktree.

---

## Фаза 0. Разблокировать проверку (0.5 дня)

Без этого нельзя ни собрать GUI, ни прогнать CI локально — именно это помешало ревью проверить UI-часть рантаймом.

### P0.1 — Закоммитить `flake.lock`

* **Проблема:** `flake.lock` есть в рабочем дереве, но под `.gitignore:3` → `nix develop` у локального разработчика и в CI пере-резолвит `nixpkgs` (в этой сессии — падение `HTTP 401 Bad credentials`).
* **Действие:** убрать строку `flake.lock` из `.gitignore`, `git add -f flake.lock`, закоммитить. Проверить `nix flake metadata --offline`.
* **Критерий:** `nix develop --command cargo check --features gui,layer-shell` проходит **без сети**.
* **Бонус:** после фиксации rev `nixpkgs` в CI исчезнет класс «сборка сломалась без изменения кода».

### P0.2 — Зафиксировать MSRV и линт-конфиг

* Добавить `rust-version = "1.88"` в `Cargo.toml` (edition 2024 требует ≥1.85; проверить фактический минимум через `cargo msrv`), `[lints]`-секцию или `clippy.toml` — по желанию.
* **Критерий:** CI собирает на Ubuntu LTS-тулчейне без «rustc version too old».

### P0.3 — CI как защита от регрессий

* В `.github/workflows/rust.yml` уже есть `gui-clippy` с `-D warnings` — оставить.
* Добавить шаг `cargo test --features gui,layer-shell` (сейчас `gui-test` есть — убедиться, что не отключён) и `cargo deny check licenses bans sources` отдельным job'ом.
* **Критерий:** на PR видны 3 обязательных зелёных чека: `check+test (gui)`, `clippy -D warnings`, `cargo-deny`.

---

## Фаза 1. Безопасность исполнения (приоритет №1, 2–4 дня)

Закрывает **B-1, B-2, B-5, B-6, M-15, M-16** и делает `docs/security.md` правдой. Это фундамент: шаги фазы 3 меняют UI, но не должны трогать политику исполнения.

### P1.1 — Единая точка исполнения (`ExecutionGateway`)

**Проблема:** `run_execution_request` — `pub(crate)` и вызывается из 4 мест напрямую; рядом живут `run_command_request`/`run_shell_request`/`run_action_*`/`run_form_action`. Любой новый вызов = новый потенциальный обход.

**Действия:**

1. `src/action.rs`: сделать `run_execution_request` приватным; ввести

   ```rust
   pub(crate) struct ExecutionTicket { pub policy: ExecutionPolicy, pub capabilities: CapabilitySet }
   pub(crate) fn execute(request: ExecutionRequest, ticket: &ExecutionTicket) -> ExecutionDecision;
   ```

   `ExecutionRequest` получает `fn required_capabilities(&self) -> CapabilitySet` (Shell→`Shell`, OpenUrl→`OpenUrl|Network`, OpenPath→`OpenPath|Filesystem`, Copy→`ClipboardWrite`, Http→`Network`, Media/Notification→пусто).
2. Внутри `execute`: сначала `ticket.capabilities.allows(request)` (иначе `Denied`), затем текущая логика риска.
3. Удалить дублирующие хелперы `ui/launcher.rs:2277-2296` (`run_command_request`, `run_shell_request`) — заменить типизированными вызовами `execute(...)`; то же для `app.rs:531-541` (дублирующий `secondary_action_risk` — оставить одну реализацию в `action.rs`).
4. Пометка «только через шлюз»: в `action.rs` завести `#[cfg(test)] static EXEC_COUNT` и тест, что каждый путь (`Action::run_with_policy`, `run_form_action`, вторичные действия, кнопки вью) инкрементирует счётчик ровно один раз.

**Тесты:** `gateway_denies_shell_without_capability`, `gateway_requires_confirmation_for_risk`, `secondary_actions_share_single_risk_table`.
**Критерий:** `grep -rn "run_execution_request" src/` даёт 1 совпадение (внутри `execute`); все существующие тесты зелёные.

**Риск:** механический рефактор на ~15 вызовов — делать отдельным коммитом без изменения поведения; тесты до и после должны совпадать.

---

### P1.2 — `ActionForm` несёт capabilities и риск → закрывает **B-1**, часть **B-6**

**Действия:**

1. `src/action.rs`: `ActionForm` получает `capabilities: CapabilitySet` и `risk: ActionRisk`.
2. `src/search/commands.rs:402-423`: в ветке формы прогонять тот же гейт, что и в ветке с аргументами:

   ```rust
   let capabilities = parse_capabilities(&entry.permissions);
   if matches!(entry.mode, CommandMode::Shell | CommandMode::Json)
       && !has_capability(&capabilities, Capability::Shell) {
       return Some(vec![blocked_action(entry, "lacks permissions = [\"shell\"]")]);
   }
   let form = ActionForm { capabilities, risk: ActionRisk::Shell, .. };
   ```

3. `src/app.rs:1025-1066` (`run_form_action`): вместо прямого `run_execution_request` — собрать `ExecutionRequest` из формы и вызвать `execute(request, &ExecutionTicket { policy, capabilities })`. При `NeedsConfirmation` вернуть решение наверх.
4. `src/ui/launcher.rs:2000-2045` (`show_form_for_action`): перед открытием формы проверить `action.risk`; подтверждение запрашивать **до** показа формы (иначе пользователь заполняет форму и не понимает, почему не выполнилось).
5. `src/search/scripts.rs:358-380`: форма скрипта получает `capabilities` из манифеста и `ActionRisk::Shell`.

**Тесты (это главный регресс-гард — сейчас у формы ноль тестов):**

```rust
#[test] fn shell_command_without_shell_permission_has_no_form() { … }
#[test] fn form_submission_requires_confirmation() { … }
#[test] fn extension_script_form_inherits_manifest_capabilities() { … }
```

**Критерий:** команда без `permissions` и с required-аргументом даёт `ActionKind::None` + «Blocked: …»; с `permissions=["shell"]` — форму, которая запрашивает подтверждение.

---

### P1.3 — Плейсхолдеры: контекст одинарных кавычек → закрывает **B-2**

**Действия (минимальный, проверяемый фикс):**

1. Вынести из текущего сканнера чистую функцию:

   ```rust
   enum ShellContext { Unquoted, Single, Double }
   fn shell_context(text: &str) -> ShellContext;
   ```

   `is_inside_unclosed_double_quotes` становится тонкой обёрткой (`matches!(shell_context(t), Double)`) — существующие тесты сканнера сохраняются как есть.
2. В `expand()`:

   ```rust
   let ctx = if shell_escape { shell_context(&output) } else { ShellContext::Unquoted };
   ```

   * `Unquoted` → `shell_quote(value)` (как сейчас);
   * `Double` → `double_quote_escape(value)` (как сейчас);
   * `Single` → `value.replace('\'', "'\\''")` **без** внешних кавычек.
3. Схлопывание «обёртки» `'{{x}}'` выполнять только когда после плейсхолдера стоит `}}'` **и** контекст `Unquoted`; при `Single` не схлопывать (иначе закрывающая кавычка автора «съест» хвост).
4. Обновить `README.md` («Placeholders») и `docs/security.md`: указать, что безопасны все три контекста, и привести пример `'prefix {{x}}'`.

**Тесты (добавить в `placeholders.rs::tests`):**

```rust
#[test] fn placeholder_inside_open_single_quote_run_is_literal() {
    let ctx = PlaceholderContext::new("", Some(&"$(touch /tmp/z)".into()));
    assert_eq!(
        expand_placeholders_shell("xdotool type 'clip: {{clipboard}}'", &ctx),
        "xdotool type 'clip: $(touch /tmp/z)'"
    );
}
#[test] fn single_quote_wrapper_still_collapses() { /* echo '{{query}}' → echo 'foo bar' */ }
#[test] fn apostrophe_value_in_single_quote_run_round_trips() { /* значение с ' → литерал */ }
```

Плюс property-тест: для набора «злых» значений (`$(…)`, `` `…` ``, `; rm`, `'`, `"`, `\`, `\n`) результат `expand_placeholders_shell` при передаче в `sh -c` даёт **ровно** одно слово = исходное значение (можно проверить тестом, запускающим `sh -c 'printf %s "$@"' _ <expanded>` и сравнивающим вывод).

**Критерий:** все старые тесты + новые зелёные; `sh`-round-trip не даёт исполнения в 3 контекстах.

---

### P1.4 — argv-режим и «trusted»-модель → закрывает остаток **B-6**

**Действия:**

1. `src/search/commands.rs:381-383,430-434`: argv-команды тоже требуют `Capability::Shell` **или** явного `permissions = ["exec"]`; в любом случае `ActionRisk::Shell` (подтверждение), кроме случая, когда программа в белом списке безопасных (`xdg-open`, `wl-copy`, `systemctl`-read-команды) — белый список можно ввести позже, на первом шаге достаточно подтверждения.
2. Убрать обоснование из `docs/security.md:41` и переписать раздел «Extension Permissions»: capability проверяется для shell/json/**argv**, форма наследует права, подтверждение — свойство действия, а не пути вызова.

**Тесты:** `argv_command_requires_confirmation`, `argv_sh_c_is_not_silent`.
**Критерий:** TOML `program="/bin/sh", args=["-c","…"]` без permissions → действие заблокировано либо требует подтверждения.

---

### P1.5 — Элементы расширений через общий гейт → закрывает **B-5**

**Действия:**

1. `src/search/extensions.rs:8-45`: `search_extension(path, name, query, capabilities)` — принимать capability манифеста; маппить элементы через тот же код, что `parse_json_action_kind` (`commands.rs`), либо вынести общий `to_action(item, capabilities)` в один модуль и переиспользовать в обоих местах.
2. `Launch`-элемент → `ActionRisk::Shell` (подтверждение), `OpenUrl` → требовать `open_url|network`, `copy_text` → `clipboard_write`.
3. `src/extensions.rs::manifest_paths`: поле `commands` — только `*.toml` (как в README); для исполняемых добавить отдельное явное поле `binaries = [...]`, требующее `capabilities = ["shell"]`.
4. `load_extension_manifests`: использовать `entry.file_type()` вместо `is_dir()` (не идти по symlink-каталогам).
5. Вынести `ExtensionsProvider` с UI-потока (см. P3.1) — до тех пор ограничить: не вызывать RPC, если расширений нет (уже так) и кэшировать по `(path, query)`.

**Тесты:** `extension_launch_item_requires_confirmation`, `manifest_commands_rejects_non_toml`, `manifest_binaries_require_shell`.
**Критерий:** `capabilities = []` + `commands = ["bin/tool"]` больше не исполняется; `Launch`-элемент показывает подтверждение.

---

### P1.6 — Путь скрипта без `sh -c` → закрывает **M-16**

**Действия:**

1. `src/search/scripts.rs:348-350`: заменить `ActionKind::Shell(ShellCommand::new(path))` на `ActionKind::Command(ProcessCommand::new(path, vec![]))` (argv, без shell) — как уже сделано в ветке формы. Для `.py/.js/.rb` без shebang сохранить существующий fallback (`scripts.rs::run_script_stdout_with_args` уже умеет выбирать интерпретатор).
2. Если `sh -c` всё-таки нужен: `pub(crate) fn shell_quote` из `placeholders.rs` → применить к пути.
3. `@raycast.needsConfirmation false` учитывать только для скриптов **без** `origin` (пользовательские), а для расширений игнорировать (манифест решает).

**Тесты:** `script_path_with_semicolon_is_not_shell_interpreted` (создать файл с `;` в имени в temp-каталоге).
**Критерий:** скрипт с метасимволами в имени выполняется как файл, инъекции нет.

---

### P1.7 — Валидация image-пути буфера обмена → закрывает **M-15**

**Действия:**

1. `src/services/clipboard_store.rs:50-51`: `clipboard_image_path` возвращает `Option<&str>` только если путь удовлетворяет:
   * `Path::new(p).starts_with(clipboard_cache_dir())` (после `canonicalize` обеих сторон, либо через `path.components()` без symlink-разрешения),
   * расширение `.png`,
   * нет компонент `..`.
2. Продублировать проверку в точке использования: `action.rs:621-630` (`copy_to_clipboard`) и `ui/launcher.rs:1719`, `views/clipboard_history.rs:194` — использовать один валидатор `validated_clipboard_image(value) -> Option<PathBuf>`.
3. `search/clipboard.rs::normalize_clipboard_text`: вырезать управляющий префикс `\u{1}` из **входящего** текста (`text.replace('\u{1}', "")` или отказ от записи, если строка начинается с префикса и путь невалиден) — иначе подделка остаётся в истории как «Image».
4. `services/clipboard_store.rs::ClipboardItem::parse` — использовать валидатор.

**Тесты:** `spoofed_image_entry_is_rejected`, `image_path_outside_cache_is_rejected`, `text_with_soh_prefix_is_stored_as_text`.
**Критерий:** текст `\x01zeshicast-image:/etc/passwd` сохраняется как текст и не читает файл.

---

### P1.8 — Capabilities как потолок, а не как добавка (MINOR из security-отчёта)

**Действия:** `CommandEntry::with_extension_origin` (`commands.rs:52-59`) сейчас **добавляет** манифестные права к правам команды. Инвертировать: `effective = declared ∩ manifest.capabilities`; если манифест пуст — команда не получает ничего (кроме `argv`-исключения из P1.4). Обновить README-пример `example.git-tools` (там `capabilities = ["shell","filesystem"]`, а команды объявляют свои — после инверсии пример продолжит работать).
**Тесты:** `command_cannot_self_grant_shell_inside_empty_manifest`.
**Критерий:** команда внутри расширения с `capabilities = []` не может объявить `permissions=["shell"]` в своём TOML.

---

## Фаза 2. Устойчивость и утечки ресурсов (1–2 дня)

### P2.1 — `extension_protocol`: гарантированное завершение детей → **B-3**

* `src/services/extension_protocol.rs:100-179`.
* Ввести guard:

  ```rust
  struct ChildGuard(Child);
  impl Drop for ChildGuard { fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); } }
  ```

  Все ветки (включая `Failed to write to stdin`) работают через guard; после получения строки — ожидание завершения с дедлайном:

  ```rust
  let deadline = Instant::now() + Duration::from_millis(GRACE_MS);
  while child.try_wait()?.is_none() { if Instant::now() > deadline { /* kill */ break } sleep(5ms) }
  ```

* Ограничить ответ: `read_line` → `read_until(b'\n', buf)` с `buf.capacity() <= 1 MiB` (иначе `Err("response too large")`).
* Тест: скрипт, пишущий валидный JSON и затем `sleep 60`, должен вернуть ошибку за ≤ таймаут+grace, а не висеть (сейчас — висит вечно).
* **Критерий:** ни один путь не оставляет живой процесс; время вызова ограничено сверху.

### P2.2 — Reaper для зомби → **M-9**

* В `src/bin/zeshicast-gtk.rs` (при `--daemon`) и в CLI-точке входа: `unsafe { libc::signal(libc::SIGCHLD, libc::SIG_IGN) }` — ядро само перестаёт держать зомби. `libc` уже доступен транзитивно через `rustix`; чтобы не тащить крейт, можно вызвать `rustix::process::` … либо объявить extern-функцию как уже делается в `osd.rs`.
* Альтернатива без `unsafe`-сигналов: хранить `Vec<Child>` и в 1-секундном GLib-таймере вызывать `try_wait()`.
* Проверить, что `child.wait()` в `copy_with` (`action.rs:646-662`) вызывается на всех путях.
* **Тест:** запустить из CLI 5 команд, `ps -o stat= -p <pid>` не должен показывать `Z` (интеграционный, можно оформить как shell-скрипт в `scripts/`).
* **Критерий:** дерево процессов демона не содержит `<defunct>` после N запусков.

### P2.3 — `preferences.toml` не теряется → **M-10**

* `src/config.rs:287-303`: `load_preferences` → `fn load_preferences(path) -> io::Result<HashMap<..>>` (ошибка парсинга/чтения — `Err`). Сохранить совместимость: добавить `load_preferences_or_default` для некритичных мест.
* `src/app.rs`: `load()` при `Err` — не считать настройки пустыми: сохранить файл как `preferences.toml.bad-<ts>`, залогировать, работать с дефолтами, **запретить** запись до успешного исправления (флаг `preferences_writable: bool`).
* `write_preferences`: экранировать `\n`, `\r`, `\t` и `< 0x20` (`\\n` и т.п.) — сейчас значение с переносом строки делает файл невалидным.
* Тесты: `corrupt_preferences_are_backed_up_not_overwritten`, `preference_value_with_newline_round_trips`, `set_preference_refuses_when_load_failed`.
* **Критерий:** любая последовательность «испортить файл → изменить настройку в UI» не приводит к потере данных.

### P2.4 — Импорт/экспорт → **M-11**, **M-12**

Импорт (`config.rs:114-208`):

* лимиты: `max_archive_bytes = 64 MiB`, `max_members = 4096`, `max_total_unpacked = 256 MiB`, счётчик распакованных байт через обёртку над ридером;
* один проход: валидировать и распаковывать из **одного** декодера, накапливая проверенные заголовки (устраняет двойное декодирование и TOCTOU);
* тесты: `import_rejects_oversized_archive`, `import_rejects_too_many_members`, `import_rejects_bomb` (архив с 0-байтными заголовками и большим полем размера).

Экспорт (`config.rs:64-101`):

* исключить из «безопасного» экспорта `zeshicast.db`, `zeshicast.db-wal/-shm`, `calc_history.json`, `[env]`-значения из `commands/*.toml`; вынести в отдельный флаг `--include-history` с явным предупреждением (и дефолтом «нет»);
* для `include_secrets`-пути заменить `append_dir_all` на обход, который **не** идёт по symlink (или сначала канонизировать и отвергать выход за пределы `config_dir`);
* тест: `safe_export_excludes_clipboard_db`, `export_does_not_follow_symlinks`.

### P2.5 — Недоверенный D-Bus → **M-14**

* `services/notifications.rs:96-118`: кап на `summary` (4 КиБ) и `body` (64 КиБ), обрезать по границе UTF-8;
* `ui/osd.rs:241-252`: `app_icon` грузить только если он либо имя иконки темы, либо существующий **обычный файл** внутри icon-каталогов пользователя; загрузку делать асинхронно (`gio::File::load_bytes_async`), чтобы FIFO/сеть не блокировали main loop;
* тест: `notify_with_huge_body_is_capped`, `app_icon_outside_icon_dirs_is_ignored`.

---

## Фаза 3. UI-поток, поиск и производительность (3–5 дней)

### P3.1 — Поиск: debounce + worker + generation → **M-1**

* `src/ui/launcher.rs:301-317`, `1315-1390`.
* Схема: `connect_changed` только запускает `glib::timeout_add_local_once(80ms)` (отменяя предыдущий таймер через `SourceId::remove`), затем `std::thread::spawn` с `(generation, query, snapshot нужных данных)`; результат возвращается по каналу, `glib::timeout_add_local` разбирает его и **отбрасывает, если `generation != current`**.
* Требуется сделать `Zeshicast::search` вызываемым на снимке данных без `&mut self` (сейчас он берёт `&self` — достаточно клонировать нужные `Rc`/`Arc`; индекс файлов и БД в поиске не мутируются).
* Тесты: `stale_results_are_discarded` (юнит-тест на функцию фильтрации по generation), `debounce_coalesces_keystrokes` (юнит на таймер-хелпер).
* **Критерий:** при быстром вводе 10 символов выполняется ≤2 поиска; длинный поиск не блокирует ввод.

### P3.2 — Кэш-поллеры: media / processes / compositor / thermal → **M-2**, **M-3**

* Расширить `src/services/poll_cache.rs` по образцу существующих audio/network:
  * `media` — интервал 1 с, **но** вызовы MPRIS с таймаутом 200–250 мс и подсчётом «залипших» игроков (пропускать после 2 таймаутов подряд, перепроверять раз в 30 с);
  * `processes` (`system_snapshot` + `top_processes_by_memory`) — интервал 1–2 с, только когда открыт Dashboard/SystemMonitor (см. P3.3);
  * `compositor` (`workspace_snapshot`) — интервал 2–3 с, кэш;
  * `thermal`/`battery` — 2–5 с.
* UI читает `cached_*()` — блокирующих вызовов на main loop не остаётся.
* Тест: юнит на «залипший игрок пропускается после N таймаутов» (мок-функция).
* **Критерий:** `grep -n "media_snapshot()\|system_snapshot()\|top_processes_by_memory\|workspace_snapshot()" src/ui/` не находит прямых вызовов внутри таймеров.

### P3.3 — Таймеры по видимости окна → **M-4**, F3/F14 ui-views

* `src/ui/launcher.rs:99,632,681,722` и все `views/*`: в начале колбэка `if !state.window.is_visible() { return glib::ControlFlow::Continue; }`.
* При `window.connect_visible_notify` — сбрасывать/взводить таймеры (полный стоп, когда окно скрыто и демон в idle).
* **Тест:** интеграционный ручной чек-лист: свернуть лончер на Dashboard → `top`/`powertop` не показывает периодических форков (можно проверить через `strace -f -e trace=process -p <pid>` в течение 10 с).
* **Критерий:** в скрытом состоянии демон не спавнит процессы (кроме clipboard-монитора с интервалом ≥150 мс).

### P3.4 — Kill по PID, а не по индексу → **M-7**

* `src/ui/views/system_monitor.rs:417-442`: убрать `list.select_row(row 0)`; сохранять выбранный PID при пересборке (сравнить список PID до/после, восстановить выделение по PID, если он ещё жив).
* `src/ui/launcher.rs:1620-1645` (`terminate_selected_system_process_or_confirm`): читать PID из строки (`row.widget_name()` или `Rc<ProcessSummary>` в `unsafe`-данных строки), а не из `top_processes_by_memory(8).get(index)`.
* Тот же паттерн применить к `copy_selected_network_value` (`launcher.rs:1645+`, индекс интерфейса).
* Тесты: `kill_uses_row_pid_not_index`, `selection_survives_refresh`.
* **Критерий:** клик по 4-й строке → подтверждение показывает именно тот процесс, который подсвечен.

### P3.5 — Модель «строка ↔ действие» и режим `=` → **B-4**

* `src/ui/launcher.rs:1327-1329,1428-1493,2433-2471`.
* Ввести перечисление отображаемых строк вместо параллельных `ListBox` + `results`:

  ```rust
  enum Row { Action(Action), Calculator { expr: String, result: String }, Header }
  ```

  `results: Rc<RefCell<Vec<Row>>>`; `action_index_for_row` удаляется, `selected_action` берёт `Row::Action`.
* `calc_result_row` → строить настоящий `Action::new("Calculator", format!("{expr} = {result}"), ActionKind::Copy(result), 1000)` и **добавлять его первым** в список строк. `evaluate_expr` удалить, использовать `crate::Calculator::new(expr).parse()` + `format_number`.
* Клавиши: Enter/Ctrl+Enter на calc-строке копируют результат; бейдж «⌃C» заменить на фактическое сочетание (Ctrl+Enter) или привязать Ctrl+C внутри строки.
* Тесты: `calc_row_is_the_first_action`, `calc_row_copies_result`, `row_index_maps_to_same_action` (параметризованный, включая вставленные невыделяемые заголовки).
* **Критерий:** `=2+2` показывает `4`; Enter копирует `4`; подсветка любой строки запускает именно её действие.

### P3.6 — Вывод процессов и таймауты → **M-5**

* `src/search/windows.rs:492-519`: читать stdout на отдельном потоке (или `output()` с внешним watchdog-таймером через `Child::kill`), дренировать stderr (`Stdio::null()` — ок), снизить `WINDOW_QUERY_TIMEOUT` с 200 мс до реалистичного и **кэшировать** снапшот окон на 300–500 мс (сейчас `search_windows` может форкать до 3 процессов на запрос ≥2 символов).
* Тест: команда, пишущая 1 МиБ и завершающаяся, должна дать полный вывод (сейчас — таймаут).
* **Критерий:** `swaymsg`-подобный большой JSON не теряется; ≤1 вызов на запрос.

### P3.7 — Стоимость файлового поиска → **M-6**

* `src/search/files.rs`: хранить `name_lower` в `FileEntry` (считать один раз при индексации), скорить без `to_lowercase()` на каждый запрос; в `FilesProvider` включать поиск только при `len >= 3` или явном префиксе `file`/`find`; в `Zeshicast::search` не запускать FilesProvider, если уже сработал явный провайдер.
* `load_file_index` для CLI — оставить синхронным, но добавить тест-бенчмарк на 10k записей (`cargo test -- --nocapture` с `Instant`).
* **Критерий:** поиск по 10k индексу ≤10 мс на запрос (замер в тесте).

### P3.8 — `browser_tabs` — починить или удалить

* Файл сессии, который читает провайдер, современным Firefox не пишется → ветка мертва. Решение: либо перейти на `recovery.jsonlz4` (с `lz4_flex`), либо удалить провайдер и упомянуть в CHANGELOG. Решение зафиксировать в коммите; до решения — исключить из горячего пути (кэш + TTL 2 с).

### P3.9 — Мелочи main-loop

* `src/ui/views/audio.rs:67,114`: слайдеры — debounce 120 мс + `wpctl` в фоне (сейчас `Command::output()` на каждое движение).
* `src/ui/views/media.rs`: in-flight guard на скраббер (не отправлять seek, пока предыдущий не завершился) — устраняет дублирующиеся/перескакивающие seek.
* `src/ui/views/ai_chat.rs` + `launcher_helpers.rs`: запретить второй запрос при активном стриме; чинить утечку сигнального хендлера; исправить «снятие не той записи истории» при ошибке.
* `src/ui/markdown.rs`: балансировать `**`/`*` (закрывать незакрытые спаны в конце блока) — сейчас пересечение даёт невалидную Pango-разметку и текст не рендерится.
* `src/ui/views/system_monitor.rs`: `net_iface` — определять активный интерфейс из `net_speed_mbps`/снапшота, а не `eth0`.
* `src/ui/navigation.rs`: `move_selection(0)` — ранний `return`.
* `src/ui/status_strip.rs`: учитывать `status_items` (сейчас 1-секундный таймер перезаписывает видимость).
* `expire_timeout_ms == 0` → «не истекает» (спека freedesktop), сейчас 8 с.

---

## Фаза 4. Эксплуатация, упаковка, CLI (1 день)

### P4.1 — Sandbox systemd и запуск приложений → **M-17**

* Вариант A (рекомендуемый): запускать всё, что выходит за пределы самого демона (`ActionKind::Launch`, `OpenUrl`, `OpenPath`, внешние утилиты), через `systemd-run --user --scope --collect --no-block -- <cmd>` — тогда `ProtectHome/PrivateTmp/seccomp` демона не наследуются. Реализовать одним хелпером в `action.rs::spawn_*` с fallback на прямой `Command` при отсутствии `systemd-run`.
* Вариант B: убрать `ProtectHome=read-only`/`ProtectSystem` из юнита и оставить только `NoNewPrivileges` + `PrivateTmp=false`; задокументировать, что запускатор приложений не может жить в жёстком sandbox.
* В любом случае: снять противоречие в `flake.nix:146-170` — комментарий «включать только после ручной проверки» и активные настройки несовместимы; либо проверить и переписать комментарий, либо отключить.
* Проверка: `systemctl --user start zeshicast` → запустить из палитры Firefox → проверить, что `~/.mozilla` пишется и `/tmp` общий (при варианте A).
* **Критерий:** приложения, запущенные из палитры, ведут себя как запущенные из обычного шелла.

### P4.2 — `.desktop` Exec и терминал

* `src/search/apps.rs:124-130,166`: парсить `Exec` в argv по правилам .desktop (учитывая кавычки, `%`-коды) и запускать через `ActionKind::Command`, а не `sh -c`. Учесть `env VAR=… cmd` (частая форма).
* `services/terminal.rs`: `launch_in_terminal` — оставить `sh -c` (терминалу нужен шелл), но убедиться, что внутрь попадает только уже собранная строка действия.
* Тест: `desktop_exec_with_quotes_splits_like_the_spec`.

### P4.3 — CLI-мелочи

* `src/main.rs:118-121`: не игнорировать `ExecutionDecision` — печатать «требуется подтверждение»/«запрещено».
* `src/cli.rs`: `--export`/`--import` распознавать только как первый аргумент (или требовать `--` для запроса) — сейчас `zeshicast shell --export x` уходит в экспорт.
* Ошибки `export/import` → `std::process::exit(1)`; сейчас код возврата 0.
* Тесты: `query_with_export_substring_is_a_query`, `export_failure_exits_nonzero`.

### P4.4 — Логирование

* Ввести `log` + `env_logger` (или `tracing`) под фичей/по умолчанию, заменить `eprintln!` в сервисах; `RUST_LOG=zeshicast=debug` для диагностики.
* Отдельно: заменить молчаливые `let _ =`/`.ok()` в местах, названных в отчёте (миграции `app.rs:78/88/96`, `chmod` в `storage.rs:22`, prune картинок) на логируемые ошибки.
* Тесты не требуются; критерий — `grep -c "let _ =" src/services/` заметно уменьшен, журнал в `journalctl --user -u zeshicast` читаем.

---

## Фаза 5. Архитектура и гигиена (фоном, 1–2 недели)

Порядок — от наибольшего эффекта на скорость разработки.

1. **P5.1 — Разделить фичи `desktop` и `gui`.** Сейчас `#[cfg(feature = "gui")]` рвёт CLI: не работают JSON-команды, `media`-действия молча ничего не делают (`services/media.rs:38-49`), нет захвата stdout скриптов. Ввести фичу `desktop` (gio/glib-сервисы + исполнение) и оставить `gui` только для виджетов; CLI собирать с `desktop`. Это чинит 3 латентных бага и снимает будущий дрейф.
2. **P5.2 — Расщепить `src/ui/launcher.rs` (3110 строк).** Предлагаемые модули: `launcher/{build,search_flow,actions,input,views_nav}.rs` + контекст-структура `LauncherUi { window, launcher, entry, list, results, navigation, … }`, которая убирает сигнатуры на 20–32 аргумента.
3. **P5.3 — Расщепить `src/app.rs` (1520 строк).** `app/{mod,model,clipboard,snippets,config_store,launch}.rs`; `Zeshicast` оставить данными + персистентностью, исполнение — в `action.rs`.
4. **P5.4 — Сократить `src/lib.rs` (188 re-export'ов).** Прямые пути модулей внутри крейта (`crate::search::apps::…`), публичная поверхность ~12 элементов; убрать `#[cfg(test)] pub(crate) use`-паттерн, заменив его на `#[cfg(test)]`-модули внутри самих файлов (они и так есть).
5. **P5.5 — Чистка.** Удалить `#[allow(dead_code)]`-хвосты (`JsonCommandAction`, `CAPABILITY_DENIED`, `initialize`/`list_commands`/`execute`, `SCORE_*`, `fa_icon`, `show_preferences_editor`/`show_extension_browser`), мёртвые вью-контролы (фильтр процессов, сортировки, per-row kill), `generate_simple_uuid` либо задокументировать в README.
6. **P5.6 — Docs-as-code.** Привести `docs/security.md` и README в соответствие после фазы 1 (это часть приёмки P1.4/P1.5/P1.8): убрать заявления, которые не подтверждаются кодом; добавить в CI проверку, что тесты для перечисленных в доке инвариантов существуют (например, ссылки по именам тестов).

---

## Сводная таблица: находка → шаг → тест

| ID ревью | Severity | Шаг | Ключевой тест |
| --- | --- | --- | --- |
| B-1 обход `shell` через форму | blocker | P1.2 | `shell_command_without_shell_permission_has_no_form` |
| B-2 инъекция в `'…{{x}}…'` | blocker | P1.3 | `placeholder_inside_open_single_quote_run_is_literal` |
| B-3 `wait()` без таймаута | blocker | P2.1 | `hanging_extension_times_out` |
| B-4 режим `=` | blocker | P3.5 | `calc_row_copies_result` |
| B-5 `commands`-бинарники | blocker | P1.5 | `manifest_commands_rejects_non_toml` |
| B-6 argv/скрипт-формы | blocker | P1.4 + P1.2 | `argv_command_requires_confirmation` |
| M-1 поиск на UI-потоке | major | P3.1 | `stale_results_are_discarded` |
| M-2 media D-Bus | major | P3.2 | `stalled_player_is_skipped` |
| M-3 `/proc`+niri на таймере | major | P3.2/P3.3 | ручной strace-чек-лист |
| M-4 таймеры без видимости | major | P3.3 | ручной чек-лист |
| M-5 pipe-deadlock | major | P3.6 | `large_output_is_not_lost` |
| M-6 стоимость files-поиска | major | P3.7 | бенч-тест |
| M-7 kill по индексу | major | P3.4 | `kill_uses_row_pid_not_index` |
| M-8 таймеры вью | major | P3.2/P3.9 | — |
| M-9 зомби | major | P2.2 | скрипт `scripts/check-zombies.sh` |
| M-10 потеря preferences | major | P2.3 | `corrupt_preferences_are_backed_up_not_overwritten` |
| M-11 импорт без лимитов | major | P2.4 | `import_rejects_bomb` |
| M-12 export тянет БД | major | P2.4 | `safe_export_excludes_clipboard_db` |
| M-13 prune/сниппеты | major | P3.9 + P4.4 | `snippet_db_error_is_not_swallowed` |
| M-14 D-Bus ввод | major | P2.5 | `notify_with_huge_body_is_capped` |
| M-15 image-путь | major | P1.7 | `spoofed_image_entry_is_rejected` |
| M-16 путь скрипта в `sh -c` | major | P1.6 | `script_path_with_semicolon_is_not_shell_interpreted` |
| M-17 systemd sandbox | major | P4.1 | ручная проверка запуска браузера |
| M-18 `.desktop` Exec | major | P4.2 | `desktop_exec_with_quotes_splits_like_the_spec` |
| MINOR (риск-таблица, CLI, мёртвый код, docs) | minor | P4.3, P5.5, P5.6 | по месту |

---

## Быстрые победы (можно сделать в первые 2 часа)

1. **P0.1** — `git add -f flake.lock` (одна строка в `.gitignore`) → локальная сборка GUI и CI перестают зависеть от сети.
2. **P1.3** — фикс контекста одинарных кавычек: ~15 строк + 3 теста, закрывает блокер.
3. **P2.1** — `ChildGuard` в `extension_protocol.rs`: ~25 строк, закрывает второй блокер (вечный фриз).
4. **P1.7** — валидация image-пути: ~10 строк, закрывает произвольное чтение файла в буфер.
5. **P3.4** — PID из строки вместо индекса: ~15 строк, устраняет убийство не того процесса.
6. **P3.9 (`move_selection(0)`)** и **`expire_timeout_ms == 0`** — по 1 строке.

---

## Критерии готовности (Definition of Done) для всего плана

* Все 6 блокеров закрыты и покрыты тестами, которые **падают** на исходном коде (проверить `git stash`-ом: тест должен краснеть до фикса).
* `docs/security.md`: каждое утверждение о границах доверия подтверждается либо тестом, либо явной ссылкой на код; раздел «Known Gaps» заполнен честно.
* `cargo clippy --all-targets --features gui,layer-shell -- -D warnings` + `cargo test --features gui,layer-shell` зелёные в CI.
* В скрытом состоянии демон не спавнит процессы и не делает блокирующих D-Bus вызовов (проверяется `strace`/`powertop` в течение 60 с).
* Запуск приложения из палитры эквивалентен запуску из шелла сессии (P4.1).
* Ручной чек-лист UI (10 пунктов из P3.4/P3.5/P3.9) пройден на niri и на fallback-режиме без layer-shell.

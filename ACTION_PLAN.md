# Action Plan — Security, Performance & Robustness

> **Superseded:** this 2026-06-18 review plan is retained for history. The
> active execution plan is `ACTION_PLAN_FULL_REVIEW.md` (2026-06-27). Major
> items from this file have since been closed in code: import hardening
> (`src/config.rs`), shell placeholder quoting and command capability
> enforcement (`src/search/commands.rs`, `src/placeholders.rs`), async/deferred
> JSON command execution (`src/search/commands.rs`, `src/ui/launcher.rs`),
> clipboard/usage SQLite pruning and migrations (`src/services/storage.rs`),
> clipboard image cache pruning (`src/app.rs`, `src/ui/launcher.rs`), privacy
> controls (`src/ui/preferences.rs`, `src/app.rs`), and warning/clippy cleanup.

> **Status (2026-06-18):** #2, #6, #7, #8, #9, #10, #11, #12 — done. #3 — done
> (network/audio moved off main thread). #4 — left view-gated by design (see note).
> #5 — bounded with a 1s timeout; full async-on-keystroke deferred. #1 — false
> positive. All builds warning-free (gui + default); 68 lib tests pass.


Сводка двух код-ревью (2026-06-18), сверена с кодом. Дубликаты объединены.
Порядок = приоритет исполнения. `[sec]` — безопасность, `[perf]` — производительность.

Состояние сборки на момент сверки: `cargo build --features gui` — 4 dead-code warning'а
(`write_frequencies`, `load_clipboard_timestamps`, `write_clipboard_timestamps`,
`parse_script_json_output`). Заявленные в ревью 7 warning'ов — артефакт сборки без `gui`.

---

## P0 — критично (деструктив / выход за пределы конфига) `[sec]`

### 1. Kill-процессы всплывают в общем поиске — ❌ FALSE POSITIVE
- **Где:** `src/search/mod.rs:204` (`impl SearchProvider for ProcessesProvider`)
- **Вердикт:** оба ревью ошиблись. `ProcessesProvider::search()` уже гейтит результаты по
  префиксу `proc `/`process ` (`mod.rs:208`) и без него возвращает пусто. `score + 240`
  применяется только к процессам, которые появляются лишь под префиксом — что и есть желаемое
  поведение. `firefox` без префикса Kill НЕ показывает. Чинить нечего.

### 2. `--import` распаковывает архив за пределы своего конфига
- **Где:** `src/config.rs:23` (`import_config`)
- **Суть:** `tar -xzf <src> -C ~/.config` принимает любой layout, `../`, симлинки →
  перезапись соседних конфигов (`autostart/`, `systemd/user/`).
- **Фикс:** распаковка в temp → валидация единственного root `zeshicast/`, запрет
  absolute/`..`/symlink → атомарный перенос в `config_dir`.
- **Acceptance:** архив с `../foo` или вторым root отклоняется, вне `config_dir` ничего не пишется.

---

## P1 — отзывчивость UI / расход батареи (блокировки main-треда) `[perf]`

### 3. Поллинг спавнит 7 подпроцессов в секунду
- **Где:** `src/ui/launcher.rs:510` и `:540`; `src/services/network.rs:41`, `src/services/audio.rs:37`
- **Суть:** при открытом launcher каждую секунду безусловно: `network_snapshot()` (4 процесса:
  `ip x2`, `nmcli x2`) + `audio_snapshot()` (3: `wpctl status` + `get-volume x2`) +
  media/battery. diff-guard стоит только на перерисовке списков, не на сборе данных —
  fork+exec летят всё равно. 7 подпроцессов/сек на main-треде → микро-стоттеры, разряд батареи.
- **Фикс:** собирать снапшоты в фоновом потоке (channel → main), либо кэш на 5–10с;
  где можно — нативные API (DBus NetworkManager, PipeWire/PulseAudio bindings) вместо CLI.
- **Acceptance:** при открытом launcher нет ежесекундного fork; данные обновляются из кэша/фона.

### 4. System monitor синхронно ходит по `/proc` каждую секунду — ⚠️ ОСТАВЛЕНО ПО ДИЗАЙНУ
- **Где:** `src/services/system_stats.rs` (`top_processes_by_memory`, `system_snapshot` форкает `df`)
- **Решение:** в отличие от #3 этот поллинг **view-gated** — работает только пока открыт
  экран Dashboard/SystemMonitor, а не всегда. Перенос в always-on воркер заставил бы
  форкать `df` и сканировать `/proc` постоянно (хуже для батареи в простое). Корректный,
  но более крупный вариант — воркер, активный только при видимом экране; пока не делаем.

### 5. JSON-команды выполняются синхронно во время поиска — ◑ ЧАСТИЧНО
- **Где:** `src/search/commands.rs` (`run_json_command`)
- **Сделано:** добавлен жёсткий watchdog-таймаут 1с (`JSON_COMMAND_TIMEOUT`) — зависший
  скрипт убивается и возвращает ошибку вместо бесконечной заморозки main-треда.
- **Осталось (follow-up):** полный async + debounce + cancel, либо исполнение только по Enter.
  Это меняет UX «search-as-you-type» JSON-провайдеров и требует отдельного согласования.

### 6. HttpCopy-action блокирует окно до 30с
- **Где:** `src/action.rs:175` (`execute_http_request`), `src/services/local_ai.rs:20`,
  `src/search/web.rs:63`
- **Суть:** перевод/OpenAI/Ollama по Enter исполняются блокирующим `ureq` на main-треде;
  медленный сервис вешает окно до таймаута (30с у OpenAI). `ask_local_ai` вообще без таймаута.
- **Фикс:** async-исполнение (`glib::spawn_future_local` или воркер+channel, как
  `ask_local_ai_streaming`); добавить timeout в `ask_local_ai`.
- **Acceptance:** запуск HTTP-action не морозит окно; недоступный endpoint падает по таймауту.

### 7. Индекс файлов строится синхронно на старте
- **Где:** `src/app.rs:245` (`load_file_index`), `src/search/files.rs:61`
- **Суть:** рекурсивный обход `$HOME` (до 10k entries, глубина 5) до показа окна →
  задержка в секунды на медленном диске / большом HOME.
- **Фикс:** строить индекс асинхронно после показа окна, либо on-demand по префиксу `file `
  (через `fd`/`locate`); кэш + stale-результаты до обновления.

---

## P2 — состояние / БД `[sec/perf]`

### 8. SQLite clipboard не прунится (медленная утечка диска)
- **Где:** `src/services/storage.rs:54` (`clipboard_insert`)
- **Суть:** чтение лимитировано `LIMIT 100`, но таблица растёт бесконечно.
- **Фикс:** после вставки удалять лишнее:
  ```sql
  DELETE FROM clipboard WHERE id NOT IN (
      SELECT id FROM clipboard ORDER BY added_at DESC LIMIT 100);
  ```

### 9. DND не персистится между перезапусками
- **Где:** `src/services/notifications.rs:110` (`toggle_dnd`)
- **Суть:** только thread-local `STATE`; рестарт демона сбрасывает в `false`.
- **Фикс:** сохранять в `preferences.toml` или SQLite, читать при старте.

---

## P3 — hardening `[sec]`

### 10. Placeholder-аргументы без экранирования → shell injection
- **Где:** `src/search/commands.rs:256`, `src/lib.rs:135`, `src/placeholders.rs:38`
- **Суть:** `{{query}}`/`{{clipboard}}`/`{{arg:*}}` подставляются сырыми в `sh -c`;
  буфер с `$(...)`/`; reboot` → произвольное исполнение.
- **Фикс:** argv-mode для команд **или** shell-escape значений; пометить shell-mode unsafe в доках.

---

## P4 — чистота кода `[low]`

### 11. Dead code (4 warning'а под `--features gui`)
- `write_frequencies`, `load_clipboard_timestamps`, `write_clipboard_timestamps` — `src/config.rs`
- `parse_script_json_output` — `src/search/scripts.rs`
- **Фикс:** удалить либо подключить.

### 12. `app_watcher.rs` — мёртвый модуль
- **Где:** `src/services/app_watcher.rs` (не зарегистрирован в `src/services/mod.rs`)
- **Фикс:** либо подключить (inotify hot-reload .desktop), либо удалить файл.

---

## Известные ограничения (не баг — не чинить)

- **Wayland clipboard в фоне.** `launcher.rs` `connect_changed` срабатывает только при фокусе
  окна; скрытый демон не читает буфер. Это модель безопасности Wayland — фоновый захват
  требует внешнего `wl-paste --watch`. Архитектурное решение, задокументировано.

---

## Порядок работ
1. **P0 #1, #2** — независимы, первыми (деструктив + traversal).
2. **P1 #3** — наибольший перф-эффект (поллинг), затем #4–#7.
3. **P2 #8, #9** — небольшие, надёжность.
4. **P3 #10** — escape/argv.
5. **P4 #11, #12** — уборка.

# Полное ревью кода — zeshicast

**Ревизия:** `15a72ec` («docs: product plan from the Vicinae comparison»), ветка `main`
**Дата:** 2026-10-04
**Объём:** ~32 600 строк Rust (без шрифтов), 2 бинарника, GTK4 + сервисный слой (D-Bus/MPRIS/PipeWire/NetworkManager/HTTP)
**Метод:** реальные запуски cargo-проверок + 6 параллельных доменных ревью (регрессия, core-безопасность, search, services, ui, ops/упаковка/доки), ключевые находки перепроверены автором ревью по коду; 4 находки воспроизведены рантайм-ом (PoC в копии репозитория, исходники проекта не изменялись).

**Что изменилось с прошлого ревью (2026-09-11):** все 6 блокеров и 15/18 major-находок закрыты (раздел 4); архитектура заметно улучшилась (`lib.rs` — 3 публичных re-export вместо 188, `app.rs`/`launcher.rs` разбиты, единая точка исполнения `execute()` с `ExecutionTicket`, ребёнок-ребёпер в `process.rs`). Новые находки — в основном обходы/остатки в *новой* логике (heredoc-эскейп, collapse-эвристика, приватный режим картинок) и эксплуатационные дыры (pipe-deadlock JSON-команд).

---

## 0. Базовая линия (что реально запускалось здесь)

| Проверка | Результат |
| --- | --- |
| Рабочая копия | ⚠️ **грязная**: `M src/search/scripts.rs` (незакоммиченная правка import-порядка, строки 432) |
| `cargo fmt --check` | ⚠️ **падает** — ровно из-за этой правки в `src/search/scripts.rs:432` (import-порядок `execute` перед `ActionRisk`); файл на HEAD rustfmt-чист (проверено `git show HEAD:src/search/scripts.rs \| rustfmt --edition 2024 --check`) |
| `cargo check --all-targets` | ✅ чисто |
| `cargo clippy --all-targets -- -D warnings` | ✅ чисто |
| `cargo test` (без фич) | ✅ **256 passed, 0 failed** (252 lib + 4 CLI), 3.4 s |
| `cargo clippy/test --features desktop` | ❌ **не собирается в песочнице**: `gio`/`glib` требуют `glib-2.0.pc`, dev-файлов нет |
| `cargo check --features gui,layer-shell` | ❌ **не собирается в песочнице**: нет `gtk4.pc` (та же причина, что и в ревью 2026-09-11) |
| `cargo deny check` | ❌ **не запускался**: `cargo-deny` не установлен в песочнице (`no such command: deny`); в CI гоняется, но — см. N-11 |
| `git ls-files flake.lock` | ✅ файл закоммичен (регрессия MINOR-18 закрыта) |

Все GUI-находки ниже — **статический анализ** (GUI не собирается здесь); там, где нужен рантайм, это явно помечено `HYPOTHESIS`.

---

## 1. 🔴 БЛОКЕРЫ / HIGH (новые)

### N-1. 🔴 BLOCKER (PoC) — инъекция через кавычку в **закрытом** теле heredoc

`src/placeholders.rs:436-477` (`shell_context`), `:79-80` (выбор контекста). CWE-78. **CONFIRMED (PoC, рантайм).**

`open_heredoc` (`placeholders.rs:228`) корректно пропускает тела heredoc при поиске «открытого» heredoc, но `shell_context` — **нет**: он считает кавычки по всему тексту, включая закрытые тела. Кавычка внутри тела ломает классификацию контекста для всех последующих плейсхолдеров.

PoC (воспроизведено в копии репозитория, `expand_placeholders_shell` + `sh -c`):

```rust
// шаблон из пользовательского commands.toml
let template = "cat <<EOF\nit's\nEOF\necho {{clipboard}}";
// буфер обмена (недоверенный): "$(touch /tmp/zeshicast_poc_f1)"
```

Результат раскрытия:

```
cat <<EOF
it's
EOF
echo $(touch /tmp/zeshicast_poc_f1)
```

Значение вставлено **без квотирования** (контекст классифицирован как `Single` из-за апострофа в `it's`) → `$(…)` из буфера обмена исполняется. Контрольный случай `echo '{{clipboard}}'` работает корректно — сломан только heredoc-путь. Инвариант «values are shell-quoted so untrusted input cannot break out» (`placeholders.rs:53-56`) нарушен. Тесты heredoc (`placeholders.rs:828-895`) не покрывают кавычки в *закрытом* теле.

**Фикс:** `shell_context` должна пропускать тела heredoc так же, как `open_heredoc` (или считать контекст после последней закрытой пары).

### N-2. 🟠 HIGH (PoC) — collapse-эвристика `'{{x}}'` оставляет значение в unquoted-контексте

`src/placeholders.rs:109-117`, `:130-141` (вставка через `escape_for_context`). CWE-78. **CONFIRMED (PoC, рантайм).**

Эвристика «схлопнуть авторскую обёртку `'{{x}}'`» срабатывает, когда `'` перед плейсхолдером **закрывает** предыдущий run кавычек (контекст `Unquoted`): она убирает эту кавычку из вывода и пропускает первую `'` после `}}`, а значение вставляет через `shell_quote` — чья открывающая кавычка лишь закрывает остаток run'а. Значение остаётся в реальном unquoted-контексте.

PoC (воспроизведено):

```rust
let template = "echo 'x'{{clipboard}}'y'";
// буфер обмена: "$(touch /tmp/zeshicast_poc_f2)"
// раскрытие: echo 'x'$(touch /tmp/zeshicast_poc_f2)'y'  → команда исполняется
```

Кавычки при этом сбалансированы — синтаксической ошибки нет, подмена незаметна. Контроль `echo '{{clipboard}}'` → `echo '$(…)'` — безопасен (тест `single_quote_wrapper_still_collapses` покрывает именно этот, безопасный путь; Unquoted-ветка эвристики не покрыта evil-значениями). Побочно: в не-shell режиме (`expand`, `placeholders.rs:82` форсирует `Unquoted`) та же эвристика теряет литеральную кавычку шаблона (`x'{{query}}'y` → `x'VALUEy`) и портит argv/env/URL.

**Фикс:** collapse только если удаляемая кавычка действительно открывала обёртку плейсхолдера (проверять парность), либо удалить эвристику.

### N-3. 🟠 MEDIUM (**уточнено при верификации**) — приватный режим и кэш изображений

`src/ui/clipboard_capture.rs:34,101` (фоновый `wl-paste`-наблюдатель и запись PNG). CWE-359. **CONFIRMED (код).**

> **Поправка к первичной формулировке.** В черновике этот пункт стоял как High «приватный режим не действует на изображения». При перепроверке по коду выяснилось, что это **ложноположительное** в главной части: `clipboard_capture_images()` (`src/app/preferences.rs:147-150`) уже включает `&& self.clipboard_history_enabled() && !self.clipboard_private_mode()`, поэтому `add_clipboard_image` (`src/app/clipboard.rs:81`) через эту проверку **не пишет историю** в приватном режиме. Тест `private_mode_does_not_record_clipboard` (`src/app/mod.rs:675`) покрывает текст, но логика для картинок эквивалентна.

Что осталось реального: `wl-paste --watch`-наблюдатель запускается один раз при старте (`clipboard_capture.rs:34`) и `watch_clipboard_image` пишет PNG в кэш (`save_clipboard_image`, строка ~101) **до** любой проверки. Если пользователь включает приватный режим уже после старта, поток продолжает писать PNG в `~/.cache/zeshicast/clipboard`; запись в историю блокируется на стороне UI (`clipboard_capture_images()` == false), но сами изображения остаются в кэше — это и есть утечка приватного режима.

**Фикс (сделан):** потокобезопасный флаг `IMAGE_CAPTURE_ALLOWED` (atomic), который главный поток обновляет из `clipboard_capture_images()` в тике наблюдателя, а `watch_clipboard_image_with` проверяет перед `save_clipboard_image` (`clipboard_capture.rs`). Тест `disabled_image_capture_writes_nothing` (gui-конфигурация) — при выключенном флаге ни один путь не форвардится и PNG не пишется.

### N-4. 🟠 HIGH (PoC-паттерн) — `run_json_command` душит собственного ребёнка: лимит 512 КиБ недостижим

`src/search/commands.rs:582-625` (цикл `try_wait()` + `sleep`, чтение stdout **после** выхода), константы `commands.rs:575-577`. CWE-833. **CONFIRMED (код + паттерн-PoC).**

`Stdio::piped()` для stdout/stderr, но никто не читает их до завершения процесса. Буфер канала Linux ≈64 КиБ: команда с бо́льшим выводом блокируется в `write`, не завершается и убивается таймаутом 1 с (`"json command timed out"`). Заявленный `JSON_STDOUT_LIMIT_BYTES = 512 * 1024` недостижим по построению. Воспроизведение точной копии цикла (функция за фичей `gui`, в песочнице не собирается — воспроизводился паттерн):

```
small(≈10KiB): Ok(8893)
big(≈170KiB, limit says 512KiB): Err("json command timed out")
```

Легитимные «тяжёлые» JSON-команды расширений молча ломаются. Эталонный фикс уже есть в этом же репо: reader-поток в `src/search/windows.rs:505-512` (M-5).

---

## 2. 🟠 MAJOR (новые)

| # | Где | Корень | CWE / статус |
| --- | --- | --- | --- |
| N-5 | `src/services/extension_protocol.rs:135-141` | «С таймаутом» вызов виснет навечно: `drop(child); reader_handle.join()` — `ChildGuard` убивает только прямого ребёнка (`process.rs:60-73`), а **внук**, унаследовавший stdout, держит write-end пайпа → `read_until` не возвращается → `join()` виснет. Косвенное доказательство — их же тест `extension_protocol.rs:258-272` (внук `sleep 2` переживает kill, `join()` ждёт). Ошибки поиска не кэшируются → каждый ввод порождает висящий поток + утечку fd. Фикс: kill группы процессов (`process_group(0)`), либо не `join()`-ить на error-путях | CWE-400, CONFIRMED |
| N-6 | `src/services/notifications.rs:57-62` (`thread_local! STATE`) + `src/action.rs:956,972` | `push_notification` из `std::thread` (воркеры extension-exec) пишет в **thread-local** состояние, которое главный поток никогда не читает → `notify`/`output` расширений и «Extension Item Failed» молча теряются. HTTP-путь корректно делает `glib::idle_add_once` (`action.rs:892`) — контраст | CONFIRMED |
| N-7 | `src/ui/views/media.rs:305-325`, источник `src/services/media.rs:303` | Недоверенный `mpris:artUrl` из D-Bus (любой процесс session-bus может зарегистрировать плеер): `file://` читается и декодируется **синхронно на главном цикле** без конфайнмента путей (контраст `safe_icon_path_in`, `osd.rs:284-296`); `http(s)` — `read_to_end` без лимита размера, поток на смену URL | CWE-73/400/918, путь CONFIRMED, масштаб фриза HYPOTHESIS |
| N-8 | `src/config.rs:275-297` | Импорт — полная замена `config_dir`; `let _ = fs::remove_dir_all(&backup)` сразу удаляет старый конфиг → импорт «safe»-экспорта (без истории) **безвозвратно стирает** `zeshicast.db` (история буфера) и прочее локальное состояние | Потеря данных, CONFIRMED |
| N-9 | `src/config.rs:189-191` | «Безопасный» экспорт вырезает `[env]`-секции по `starts_with("[env]")`: inline-таблица `env = { API_KEY = "…" }` не вырезается вовсе, а `[ env ]`/`[env ]` (легальные TOML v1.0) не распознаются → токены уезжают в экспорт, который пользователь считает безопасным | CWE-200, CONFIRMED |
| N-10 | `src/config.rs:493` (+ `src/app/preferences.rs:162-179`) | Ключи `preferences.toml` пишутся **без** экранирования (значения экранированы, `config.rs:502`). Ключ с `"`/`\n` из импортированного файла при любом сохранении из GUI внедряет произвольные строки — например, флипает дефолт `export_include_secrets`, либо ломает TOML (режим M-10) | CWE-74, CONFIRMED |
| N-11 | `.github/workflows/rust.yml:94` | `cargo-deny` гоняет `check licenses bans sources` — секция **`advisories` исключена**, хотя `deny.toml:3-4` задаёт политику, а `docs/development.md:36` рекомендует `cargo deny check`. Известные CVE/RUSTSEC в зависимостях не блокируют CI | CWE-1035, CONFIRMED |
| N-12 | `src/search/scripts.rs:292-315` | `run_script_stdout_with_args`: `Command::output()` без таймаута и без лимита вывода (JSON-путь ограничен 1 с/512 КиБ, windows — 500 мс). Зависший скрипт навсегда занимает воркер; гигабайтный вывод — OOM. Вызывается из `ui/launcher/scripts.rs:64` | CWE-400, CONFIRMED |
| N-13 | `src/search/calculator.rs:121-133`, горячий путь `src/search/snapshot.rs:98-102` | Рекурсивный спуск без ограничения глубины: каждый `-` и каждая скобка добавляют кадры. PoC (воспроизведено): 50 000 скобок → `thread … has overflowed its stack / fatal runtime error: stack overflow, aborting` — **падение всего процесса лаунчера** на пасте большого текста (`looks_like_expression` пропускает такой ввод) | CWE-674, CONFIRMED (PoC) |
| N-14 | `src/app/preferences.rs:55-68` + дефолт `src/ui/preferences.rs:32` | `script_dirs` не раскрывает литеральный `~` → сохранённый дефолт `~/.config/zeshicast/scripts` превращается в **CWD-относительный** путь: скрипты пользователя молча не грузятся, а каталог с именем `~` в рабочем каталоге попадает в палитру как исполняемые строки | CWE-73, CONFIRMED |
| N-15 | `src/search/commands.rs:587-590` (`run_json_command`), `src/search/scripts.rs:295-296`, `src/services/text_input.rs:19` | Три spawn-пути мимо документированной единой точки исполнения (`action.rs:848-854`): `sh -c` напрямую, `Command::output()` напрямую, `spawn_detached` напрямую. Сейчас закрыто дублирующими проверками на этапе построения Action (blocked → `ActionKind::None`), т.е. не эксплуатируется, но нарушает инвариант «каждый путь проходит `execute()`» — следующий producer станет дырой | CWE-693, CONFIRMED |

---

## 3. 🟡 MINOR / NIT (новые)

* **`replaces_id` без привязки к отправителю** (`src/ui/notify_server.rs:106-121`, `services/notifications.rs:126-133`): любой клиент session-bus может подменить/удалить чужое уведомление по id. Также `_sender` игнорируется — `app_name`/`app_icon` спуфабельны (спецификация это допускает), а `CloseNotification` (`notify_server.rs:136-140`) закрывает текущий тост любого клиента. CWE-863, Low.
* **xclip-фолбэк копит зомби** (`src/services/clipboard_store.rs:335-341`): `.spawn().is_ok()` без `wait()`/регистрации — единственный spawn-сайт мимо ребёпера `process.rs`. Фикс: `spawn_detached` (1 строка). CWE-404, Low (остаток M-9).
* **Успех вставки в буфер превращается в `Err`** (`src/app/clipboard.rs:33`): `prune_clipboard_image_cache(...)?` после успешного insert — суть M-13 не закрыта до конца. Low (остаток M-13).
* **Ошибки БД сниппетов глотаются** (`src/app/snippets.rs:36,43,57,84,86` — `.ok()`), а `snippets.txt` перезаписывается из памяти; формат файла ломается, если имя/значение содержат `\n` или ` = ` (`snippets.rs:112-119` без экранирования). Low (остаток M-13).
* **`font_desc` без экранирования** (`src/ui/fonts.rs:237-241`): `replace('"', "")` вместо `glib::markup_escape_text`; `<`/`&` из `fc-list` ломают Pango-разметку (выхода из атрибута нет — просто пустой предпросмотр). CWE-116, Nit/Low.
* **Декод иконки уведомления синхронно на главном цикле** (`src/ui/osd.rs:331`): чтение асинхронное, декод — нет; размер файла не ограничен (пути конфайнены `safe_icon_path_in` — ок). CWE-400, Low/HYPOTHESIS (заморозка).
* **`copy_with` — `wait()` без таймаута** (`src/action.rs:1178-1182`): зависший `wl-copy`/`xclip` блокирует вызывающий поток (для `ExecutionRequest::Copy` — главный). HYPOTHESIS.
* **Ответы AI/перевода читаются без лимитов** (`src/services/local_ai.rs:56,88,146-148`, `src/search/web.rs:23-54`): `into_json` без капа тела, `lines()` без капа строки, `STREAM_READ_TIMEOUT` — per-read, не общий дедлайн. Эндпоинт задаёт пользователь → HYPOTHESIS.
* **Права WAL/SHM не закреплены** (`src/services/storage.rs:14-29`): 0600 ставится только на `zeshicast.db`; `-wal`/`-shm` никогда не chmod-ятся (тест `storage.rs:407-410` проверяет только основной файл). HYPOTHESIS (нужен `ls -l` после первой вставки).
* **`web.rs:83,156,158`** — срезы по исходной строке при guard на `to_lowercase()`: возможное смещение среза (Unicode-панику сконструировать не удалось). CWE-176, Low/HYPOTHESIS.
* **`notify_server.rs:109,136`** — `params.child_value(n)` может паниковать на коротком кортеже, если GDBus не превалидирует сигнатуру; unwind через FFI → abort демона. HYPOTHESIS (нужен рантайм-тест `gdbus call`). Фикс дёшев: проверка `n_children()`.
* **Скрипт сам отключает себе подтверждение** (`src/search/scripts.rs:68-72`): `# @raycast.needsConfirmation false` даёт `ActionRisk::Normal` для локального скрипта без манифеста; симлинки в `script_dirs` не фильтруются (`scripts.rs:113-127`). Дизайнерское решение, но ослабляет «confirm на каждой ветке» (CWE-862).
* **Установочный скрипт:** `scripts/install-user.sh:88-90` собирает `--features gui` без `layer-shell` (README.md:651 рекламирует оверлей; в Non-Nix секции README.md:231 оговорки нет) и через `shell.nix:1` → непиненый `<nixpkgs>` вместо flake-закреплённого devShell.
* **CI-мелочи:** clippy для дефолтного набора фич нигде не гоняется (`rust.yml:25-30`); джобы `gui-check` (`rust.yml:55`) и `layer-shell-check` (`rust.yml:61`) выполняют идентичные команды; `magic-nix-cache-action` (`rust.yml:83`) — сервис объявлялся deprecated (HYPOTHESIS).
* **flake.nix:** комментарий `flake.nix:121` ссылается на `github:blackzeshi/zeshicast`, везде остальное — `zeshi09/zeshicast`; заявление «Зеркален packaging/zeshicast-gtk.service» (`flake.nix:147-148`) неточно: `ExecStop` есть только во flake (`flake.nix:158,197`), описания расходятся (поведенческий эффект минимален — SIGTERM обрабатывается, `src/bin/zeshicast-gtk.rs:102-105`).
* **XDG игнорируется:** `~/.config/zeshicast` захардкожен (`src/app/mod.rs:232`), `home_dir()`-fallback `PathBuf::from(".")` (`config.rs:436-439`) — с пустым `HOME` эксп/импорт работают относительно CWD. Согласовано с `docs/privacy.md`, т.е. портируемость.
* **Поведенческие:** JSON-mode команда при fuzzy-совпадении выполняется как обычный `sh -c`, JSON-результат отбрасывается (`commands.rs:393-431`); `windows.rs:518-520` — ветка `Err` от `try_wait` не убивает ребёнка; `markdown.rs:103` — пользовательский U+E000 превращается в `**` (порча текста, не инъекция); `search_flow.rs:124` — нет in-flight гарда (поток на каждый отведебаунсенный запрос); закладки Chromium показываются как вкладки (`browser_tabs.rs:174`); `format_number` молча сатурирует (`calculator.rs:13`); kill без проверки start-time процесса (переиспользование PID); `usage`-таблица растёт неограниченно (`storage.rs:190-198`); `extension.toml` читается без лимита размера (`extensions.rs:40`).

---

## 4. Регрессия ревью 2026-09-11 (вердикты по текущему коду)

### Блокеры — все закрыты

| Находка | Вердикт | Подтверждение (проверено автором ревью) |
| --- | --- | --- |
| **B-1** обход `shell` через форму | ✅ **FIXED** | Форма без capability вообще не создаётся: `capability_denial` (`search/commands.rs:465-475`) вызывается до ветки формы (`commands.rs:387-389` → `ActionKind::None`); форма несёт `capabilities` + `ActionRisk::Shell` (`commands.rs:435-437,446-452`); сабмит идёт через `ExecutionTicket::for_action` → `execute()` → `preflight` (capability ceiling → confirmation, fail-closed; `action.rs:238-251,855-863`, `app/launch.rs:237-282`). Тесты `shell_command_without_shell_permission_has_no_form`, `form_without_capabilities_is_denied_even_when_confirmed` |
| **B-2** инъекция в `'…{{x}}…'` | ✅ **FIXED** (но см. N-1/N-2 — новые варианты той же механики) | `shell_context` различает Single/Double/Comment/Unquoted (`placeholders.rs:436-477`); Single — только `'\''`-замена, Comment — литерал (`:119-124`); тесты `:589-604` воспроизводят B-2-сценарий |
| **B-3** `wait()` без таймаута | ✅ **FIXED** (но см. N-5 — join() на error-пути) | `ChildGuard` (`extension_protocol.rs:106`), bounded `reap` (`process.rs:60-74`), лимит ответа 1 МиБ (`:82,127,167`); тесты timeout/oversized/stdin-failure |
| **B-4** режим `=` калькулятора | ✅ **FIXED** | `calc_row_action` → настоящий `calculator::calc_action` (`ui/launcher/results.rs:225-228`), строка в `results[0]` (`:156`), индексация по тегам (`action_index_from_tag`); тесты `calc_row_copies_the_result`, `an_unregistered_row_never_runs_a_neighbour_action` |
| **B-5** `commands`-бинарники расширений | ✅ **FIXED** | `.toml`-фильтр + binaries только при shell-capability (`extensions.rs:54-64`); элементы через `gate_action_intent` → JSON-RPC `execute`; spawn на каждое нажатие смягчён TTL-кэшем (`extensions.rs:73-89`) + debounce |
| **B-6** argv/скрипт-формы | ✅ **FIXED** (см. оговорку) | `ActionKind::Command` → `ActionRisk::Shell` (`commands.rs:446-452`); extension-argv требует manifest `shell` (`:477`); `confirmation_risk()` (`scripts.rs:68-74`) — extension-скрипт всегда Shell. Оговорка (design decision): локальный скрипт с `needsConfirmation false` остаётся `Normal` — тот же уровень доверия, что и свои commands |

### Major

| # | Вердикт | Подтверждение |
| --- | --- | --- |
| M-1 поиск на UI-потоке | ✅ FIXED | debounce 80 мс + generation-счётчик + worker с panic-catch + стейл-гард (`ui/search_flow.rs:24-159`) |
| M-2 media D-Bus | ✅ FIXED | `CALL_TIMEOUT_MS=250`, `STUCK_AFTER`, HealthBook (`services/media.rs:76-121`), снапшот в poller-потоке |
| M-3 `/proc`+niri на таймере | ✅ FIXED (код) | poll_cache: system/processes каждый 2-й тик, остальное — 3-й, всё в фоне (`services/poll_cache.rs:66-80`); UI читает кэш. Рантайм-strace — всё ещё «owed by a human» (HYPOTHESIS) |
| M-4 таймеры без видимости | ✅ FIXED (код) | `hidden()` + `track_window_visibility` → `poll_cache::set_active` (`ui/mod.rs:40-56`), таймеры гейтятся (`build.rs:645`). Рантайм-strace owed (HYPOTHESIS) |
| M-5 pipe-deadlock | ✅ FIXED | reader-поток дренирует stdout + watchdog (`search/windows.rs:497-552`) |
| M-6 стоимость files-поиска | ✅ FIXED | предвычисленный `name_lower`, explicit-запросы не сканируют индекс (`search/mod.rs:207,261-268`) |
| M-7 kill по индексу | ✅ FIXED | kill по `row_process` (PID из строки) (`ui/launcher/views.rs:176-181`, `views/system_monitor.rs:404-406`); тест `kill_uses_row_pid_not_index` |
| M-8 таймеры вью | ✅ FIXED | `VOLUME_DEBOUNCE=120ms`, in-flight гарды (AI `can_start_request`), тесты на drag/scrubber |
| M-9 зомби | ⚠️ **PARTIAL** | Ядро закрыто: `ChildGuard` + registry + `reap_finished` (`process.rs:47-152`), все горячие точки мигрированы. **Остаток:** `clipboard_store.rs:335-341` — xclip-фолбэк без wait (см. Minor) |
| M-10 потеря preferences | ✅ FIXED | `load_preferences → io::Result`, бэкап `.bad-<ts>`, `escape_toml_string`, отказ записи (`config.rs:525-610`, `app/preferences.rs:162-172`) |
| M-11 импорт без лимитов | ✅ FIXED | один проход «validate and write» + счётчик декомпрессии + `ImportLimits`, TOCTOU устранён (`config.rs:239-347`) |
| M-12 export тянет БД | ✅ FIXED | `HISTORY_FILES` исключены, `strip_env_table`, `append_tree` без follow-симлинков (`config.rs:70-203`) — **но см. N-9**: `strip_env_table` обходится |
| M-13 prune/сниппеты | ⚠️ **PARTIAL** | chmod-ошибка логируется (тест есть). Остатки: `app/clipboard.rs:33` `?` после insert; snippets `.ok()` (см. Minor) |
| M-14 D-Bus ввод | ✅ FIXED | каппы длин + UTF-8-safe truncate (`notifications.rs:65-85`); `safe_icon_path` + async-декод с generation-проверкой (`osd.rs:296-320`) — **но см. Minor**: декод по-прежнему на главном цикле |
| M-15 image-путь | ✅ FIXED | `validated_image_path`: absolute, без `..`, внутри cache-dir, `.png`, canonicalize (`clipboard_store.rs:27-60,315-317`); входящий текст санитизируется (`search/clipboard.rs:28`) |
| M-16 путь скрипта в `sh -c` | ✅ FIXED | argv-запуск: `ProcessCommand::new(path, [])` (`scripts.rs:363-372`), тест `script_path_with_semicolon_is_not_shell_interpreted` |
| M-17 systemd sandbox | ✅ FIXED (осознанный trade-off) | Юниты оставляют только child-safe набор (6 директив + `NoNewPrivileges`), инвариант-тест `the_daemon_unit_does_not_restrict_what_it_launches` (`lib.rs:1452-1492`) патрулирует оба юнита. Цена задокументирована (`systemd-analyze` 3.4 → 8.1); ручная проверка Firefox — owed |
| M-18 `.desktop` Exec | ✅ FIXED | `desktop_exec_argv` + `split_desktop_exec` + `expand_field_codes` → `ActionKind::Command` (`apps.rs:159-316`), тесты на кавычки/`\`/field-codes |

### MINOR (раздел 3 прошлого ревью)

fixed: дублирование `secondary_action_risk` (единственная `action.rs:607`, RunInTerminal→Shell); `run_action_menu` учитывает решение (`main.rs:209-212`); `--export` в тексте запроса (`cli.rs:138`); `lib.rs` god-module (3 re-export); мёртвый код/`#[allow(dead_code)]` подчищены; `status_items` работает (`status_strip.rs:24-25`); NET больше не `eth0`; Ctrl+Shift+F → «Show in Files»; `move_selection(0)` не крутится; `expire_timeout_ms == 0` = «никогда» (`osd.rs:146-148`); Markdown-спаны (`STAR_GUARD`, `markdown.rs:96-105`); мёртвая ветка `recovery.js` (теперь `recovery.jsonlz4`); log-фасад (`src/logging.rs`); export/import с кодом выхода (`main.rs:357-368`); `flake.lock` закоммичен и не в `.gitignore`.

**Остаются открытыми:** фильтр/сортировки процессов и per-row kill в System Monitor (`views/system_monitor.rs:259` — кнопка скрыта; UX-недоделки); футер без невыделяемых строк (не проверено); `font_desc`-экранирование (→ Nit выше); закладки как вкладки (`browser_tabs.rs:174`); `format_number`-сатурация; PID-reuse при kill (нет проверки start-time); `~` в `script_dirs` (→ N-14); `deny.toml` остался минимальным (`ignore = []`, `multiple-versions = "warn"` — терпимо).

---

## 5. ✅ Что сделано хорошо (контекст для оценки)

* **Модель исполнения — настоящий choke point.** `execute()` → `preflight()` (capability ceiling → confirmation, fail-closed на отсутствующий request), `ExecutionTicket` нельзя подделать снаружи крейта; «манифест — потолок» (`with_extension_origin`); `ExtensionExec` не превращается в командную строку; эффекты ответа расширения re-gated (`gate_action_intent`, `file://` отрезан). Это закрывает целый класс прошлых блокеров — и N-15 показывает, что инвариант уже стоит того, чтобы держать его строго.
* **Плейсхолдеры** (за вычетом N-1/N-2): `shell_quote`/`double_quote_escape` сбалансированы на краевых значениях (`'`, `\`, `"` в конце), heredoc-тела и комментарии обработаны, argv-расширение без shell-квотирования — есть регрессионные тесты на прошлые находки.
* **Импорт/export tar**: traversal/absolute/symlink/hardlink отрезаны, лимиты (64 МиБ/4096 членов/256 МиБ декомпрессии), один проход в staging, атомарный rename с откатом, `mode & 0o777` без setuid.
* **SQLite**: только параметризованные запросы (динамического SQL нет), транзакционные миграции fail-closed на более новой схеме, `busy_timeout` у rusqlite 0.32, файл БД 0600.
* **Процессы**: `ChildGuard` (bounded kill+reap в Drop), registry + `reap_finished`, ETXTBSY-retry с ограниченной грацией (диагностика #46 — образцовая).
* **Парсинг недоверенных данных**: mozlz4 — строгий (лимит 64 МиБ, bounds-checked); `.desktop` Exec разбирается в argv по спеке; serde-лимиты глубины; длинные D-Bus-строки капаются; `safe_icon_path_in` — канонизация + отклонение symlink/FIFO.
* **Документация = код**: `docs/security.md`/`privacy.md`/`development.md` сверены с источником и совпадают (включая машинную проверку имён тестов `documented_invariants_have_the_tests_they_name`, `lib.rs:1493`).
* **Опыт ремедиации**: `docs/remediation-acceptance.md` — редкого качества запись приёмки; её утверждения при независимой проверке подтвердились (расхождения только в остатках M-9/M-13).

---

## 6. Архитектурный вердикт

* Проект перешёл из состояния «системные проблемы» (ревью 2026-08-23) через «нет единой точки исполнения» (2026-09-11) к **зрелой модели исполнения с capability-потолком и fail-closed гейтом**. Основной технический долг прошлого ревью оплачен: god-module разобран, UI-поток разгружен (debounce + poll_cache + worker), процессы под ребёпером.
* Оставшиеся проблемы сгруппированы в две темы: (1) **хвосты механики квотирования/экранирования** — N-1/N-2 показывают, что ручной shell-лексер (`shell_context`) неизбежно будет давать обходы на нестандартных конструкциях; долгосрочная стратегия — минимизировать `sh -c` (argv везде) или генерировать скрипт через безопасный шаблонизатор, а не собирать строку; (2) **новые пути ввода без той же дисциплины лимитов** (JSON-вывод, вывод скриптов, artUrl, ответы AI) — лимиты/таймауты надо добавить одним общим helper'ом (capped reader + deadline), чтобы следующий провайдер не забывал их.
* Инвариант «всё через `execute()`» (N-15) стоит закрепить тестом-гвардом (grep-style тест на прямые `Command::new` вне `process.rs`/`action.rs`), как уже сделано для systemd-директив.
* CLI и GUI перестали расходиться по рискам (`secondary_action_risk` единая), но `run_json_command` остался за `#[cfg(feature = "gui")]` — headless-сборка по-прежнему не выполняет JSON-команды (задокументировано, но это ломает заявление README о паритете CLI).

---

## 7. План исправлений (приоритизированный)

| # | Исправление | Критерии приёмки | Трудоёмкость |
| --- | --- | --- | --- |
| P1 | **N-1**: `shell_context` пропускает тела heredoc | тест: шаблон `cat <<EOF\nit's\nEOF\necho {{clipboard}}` со значением `$(…)` даёт литерал; PoC-маркер не создаётся | S |
| P2 | **N-2**: collapse только для настоящих обёрток (или удалить) | тесты: `echo 'x'{{c}}'y'` и `printf ''{{c}}''` не исполняют значение; `echo '{{c}}'` по-прежнему схлопывается; не-shell-режим не теряет кавычку | S |
| P3 | **N-3**: `clipboard_private_mode()` в `add_clipboard_image` + до записи PNG в наблюдателе | тест: в приватном режиме картинка не попадает ни в историю, ни в кэш | XS |
| P4 | **N-4**: reader-поток в `run_json_command` (эталон `windows.rs:505-512`) | тест: JSON-команда с выводом 300 КиБ возвращается целиком (в пределах 512 КиБ) и не таймаутится | S |
| P5 | **N-5**: kill группы процессов / безусловный `join()` убрать с error-путей | тест: расширение с внуком, держащим stdout, не вешает вызов дольше таймаута + fd не растут | M |
| P6 | **N-6**: `push_notification` через `glib::idle_add_once` (или глобальный `Mutex`) | тест: уведомление из worker-потока видно в `notification_snapshot` на главном | XS |
| P7 | **N-13**: лимит глубины рекурсии калькулятора (напр. 64) | тест: 50 000 скобок → ошибка, не abort; PoC перестаёт падать | XS |
| P8 | **N-11**: `check advisories licenses bans sources` в CI | в CI есть advisory-прогон; `cargo deny check advisories` чист | XS |
| P9 | **N-14**: раскрытие `~/` в `script_dirs` | тест: `~/…` резолвится в `$HOME`, CWD-каталог `~` не читается | XS |
| P10 | **N-9/N-10**: нормализация заголовков `[ env ]` + вырезание inline `env = {…}`; экранирование ключей preferences | тесты: safe-export без токенов во всех трёх формах записи `env`; ключ с `"`/`\n` не ломает файл и не флипает `export_include_secrets` | S |
| P11 | **N-12**: capped reader + таймаут для `run_script_stdout_with_args` | тест: висящий скрипт убивается по таймауту, 100 МиБ вывода не буферизуются | S |
| P12 | **N-7**: конфайнмент `file://` для artUrl (канонизация + cache/allow-list), лимит и таймаут для http | тест: `file:///etc/passwd` и 100 МиБ по http не роняют/не вешают UI | M |
| P13 | **N-8**: импорт не удаляет backup автоматически (или мержит историю) + документация | тест: импорт safe-экспорта сохраняет `zeshicast.db` или явно предупреждает; backup доступен после импорта | M |
| P14 | **N-15**: три «диких» spawn-пути через `execute()`/`preflight` + grep-гвард-тест на прямые `Command::new` вне process/action | тест: новый прямой spawn роняет `cargo test` | M |
| P15 | Minor-пакет: xclip `spawn_detached`; `prune`-`?` → warn; snippets — ошибки не глотать + экранирование формата; `font_desc` через `markup_escape_text`; кап декода иконок; `wait()` с таймаутом в `copy_with`; лимиты `local_ai`/web; chmod WAL/SHM; `replaces_id` → привязка к отправителю; `n_children()`-проверка в notify_server | по тесту на каждый пункт | S–M (пакет) |
| P16 | Ops-пакет: clippy для дефолтных фич; убрать дубль джобы; `install-user.sh` → flake-devShell + опция `layer-shell`; README-оговорка для Non-Nix; правка URL во flake-комментарии | CI зелёный, README соответствует сборке | S |
| P17 | UX-долг (из прошлого MINOR): фильтр/сортировки процессов, per-row kill, футер, PID start-time при kill | по чек-листу | M |

Порядок: P1–P4 — до любого релиза (PoC-подтверждённые дыры/поломки), P5–P11 — текущий спринт, остальное — планово.

---

## 8. Ограничения ревью

* **GUI (`gui,layer-shell`) и `desktop`-фичи не собирались** в этой песочнице (нет `gtk4.pc`/`glib-2.0.pc`): все UI/D-Bus-выводы — статический анализ. GUI-тесты (заявленные 318 в acceptance) здесь не гонялись — их прохождение остаётся HYPOTHESIS (набор тестов существования гарантирует `documented_invariants_have_the_tests_they_name`).
* **`cargo deny check` не запускался** (cargo-deny не установлен); политика `deny.toml` оценена статически.
* **PoC N-4** — воспроизведение паттерна (сама функция за `gui`-фичей); PoC N-1/N-2/N-13 — рантайм, в копии репозитория (`/tmp/zc-poc`), исходники проекта не изменялись.
* **HYPOTHESIS, требующие рантайма на живой машине:** паника `child_value` на кривой сигнатуре D-Bus (нужен `gdbus call`); hang `join()` с внуком (нужно расширение с фоновым процессом); права `-wal`/`-shm` (нужен `ls -l` после вставки); фриз/OOM от `artUrl`; ручные проверки M-3/M-4 (strace простоя) и M-17 (Firefox из палитры) из прошлого плана — **всё ещё owed**.
* Незакоммиченная правка в `src/search/scripts.rs` (ломает `cargo fmt --check`) не анализировалась как изменение кода — это ровно переупорядочение import'ов; всё ревью сделано по рабочей копии как она есть.

---

## 9. Ход резолвинга (addendum от 2026-10-04)

После ревью план P1–P17 превращён в `docs/remediation-plan-2026-10-04.md` и начата реализация. Готово в этой сессии:

| Шаг | Находка | Статус | Где / чем держится |
| --- | --- | --- | --- |
| P1 | N-1 (heredoc-контекст) | ✅ **исправлено** | `src/placeholders.rs`: единый `scan_shell` пропускает тела закрытых heredoc; тесты `apostrophe_in_a_closed_heredoc_body_does_not_unquote_a_later_value`, `heredoc_body_quote_does_not_let_a_value_execute` |
| P2 | N-2 (collapse-эвристика) | ✅ **исправлено** | `src/placeholders.rs`: эвристика удалена — `escape_for_context` уже безопасен во всех трёх контекстах; тесты `author_quotes_beside_a_placeholder_keep_the_value_quoted`, `quotes_beside_a_placeholder_do_not_let_a_value_execute`, `non_shell_expansion_keeps_literal_quotes` |
| P3 | N-3 (кэш изображений) | ✅ **исправлено** (после уточнения) | `src/ui/clipboard_capture.rs`: atomic-флаг `IMAGE_CAPTURE_ALLOWED`, проверка до `save_clipboard_image`; тест `disabled_image_capture_writes_nothing` |
| P4 | N-4 (pipe-deadlock JSON) | ✅ **исправлено** | новый `process::run_capped` (дренаж пайпов + дедлайн + капы) → `run_json_command`; тесты `a_large_json_command_stdout_does_not_deadlock`, `a_json_command_over_the_cap_is_rejected_cleanly` + `process::tests::run_capped_*` |
| P11 (часть) | N-12 (скрипты без таймаута) | ✅ **исправлено** | `run_script_stdout_with_args` через `run_capped` (30 с, 1 МиБ); тест `script_stdout_larger_than_the_pipe_buffer_is_captured` |
| P7 | N-13 (рекурсия калькулятора) | ✅ **исправлено** | `src/search/calculator.rs`: `MAX_DEPTH = 64`; тест `calculator_rejects_deeply_nested_expressions` |
| P8 | N-11 (CI без advisories) | ✅ **исправлено** | `.github/workflows/rust.yml`: `check advisories …`; нашёл `rustls 0.23.40` (RUSTSEC-2026-0285) → `cargo update -p rustls` → `0.23.45`; `cargo deny check` — всё ok |
| P6 | N-6 (уведомления из воркеров) | ✅ **исправлено** | `notifications.rs`: process-global `Mutex` вместо `thread_local`; тест `a_worker_thread_push_is_visible_from_another_thread` |
| P9 | N-14 (`~` в `script_dirs`) | ✅ **исправлено** | `app/preferences.rs`: `expand_home_dir`; тест `script_dirs_expand_a_leading_tilde` |
| P10 | N-9/N-10 (env-strip + TOML-ключи) | ✅ **исправлено** | `config.rs`: `strip_env_table` ловит все формы `env`; `toml_key` экранирует ключи; тесты `strip_env_table_covers_all_spellings`, `preference_keys_are_escaped_not_injected` |

**Новая находка, сделанная при резолвинге (нужна отдельная правка):**

* **N-16 | Medium | тестовая изоляция.** `search::browser_tabs::tests::tab_collection_is_cached_between_queries` (`src/search/browser_tabs.rs:355`) опирался на пустой процесс-глобальный кэш `tab_cache()`; любой другой тест, дергающий поисковый пайплайн, успевал его заполнить, и проверка `calls == 1` падала **недетерминированно** — воспроизведено и на чистом HEAD `15a72ec` (`cargo test`: то 252 passed, то 251 passed + 1 failed). Это делало `cargo test` мигающим на CI. **Исправлено:** `#[cfg(test)] reset_tab_cache()` в начале теста. Проверка: `cargo test` дважды подряд — 260 passed, 0 failed.

**Итоговая базовая линия после правок (все конфигурации, включая те, что не собирались на момент ревью):**

| Проверка | Результат |
| --- | --- |
| `cargo fmt --check` | ✅ чисто (в т.ч. снят предсуществующий fmt-дефект в `scripts.rs`) |
| `cargo check --all-targets` | ✅ |
| `cargo clippy --all-targets -- -D warnings` | ✅ |
| `cargo test` | ✅ 260 + 4 CLI, стабильно |
| `cargo clippy/test --features desktop` | ✅ 264 + 4 (через `nix develop`) |
| `cargo clippy/test --features gui,layer-shell` | ✅ 325 + 4 + 1 (через `nix develop`) |

GUI/desktop теперь **реально собраны и протестированы** в `nix develop` (dev-shell с GTK4/glib доступен) — ограничение из §8 снято для будущих прогонов; PoC N-4 подтверждён настоящим `run_json_command`, не только паттерном.

Остаются не сделанными пункты P5, P12–P17 (см. план).

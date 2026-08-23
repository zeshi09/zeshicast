# Полное ревью zeshicast — 2026-08-23

Методика: 6 параллельных fresh-context ревьюеров (read-only) с разными углами атаки

+ супервизорская верификация спорных находок и запуск тестов. Спорные пункты
разрешены чтением кода и эмпирической проверкой; все находки снабжены файл:строка.
Сырые отчёты лейнов: `/tmp/zeshicast-review/*.md` (sec-docs, privacy-readme,
architecture, security-code, tests-ci, parity-packaging).

Проверено исполнением:

+ `cargo check --no-default-features` — ✅ собирается (headless CLI без gtk);
+ `cargo test --features gui` — ✅ **119 passed, 0 failed**;
+ эмпирическая проверка shell-инъекции через двойные кавычки на `sh` — инъекция подтверждена.

---

## 1. Executive summary

| # | Sev | Находка |
| --- | ----- | --------- |
| 1 | **P1** | Обход shell-quoting через двойные кавычки шаблона команды (`src/placeholders.rs:66-75`): `cmd "{{query}}"` → инъекция произвольного кода. Противоречит главному обещанию docs/security.md |
| 2 | **P1** | CI никогда не запускает `cargo test --features gui`: ~17 написанных GUI-тестов мертвы в CI, при этом docs/development.md прямо требует эту команду перед коммитом |
| 3 | P2 | Скрипты расширений исполняются без `ActionRisk::Shell` и без проверки capabilities манифеста (`src/search/scripts.rs:198-206`) — без подтверждения даже при `capabilities = []`, вопреки security.md |
| 4 | P2 | Деструктивные операции с буфером (clear/delete) в CLI REPL минуют ExecutionPolicy (`src/app.rs:707-722`) — executor-гарантия из security.md не соблюдается для secondary actions |
| 5 | P2 | AI-стриминг без read-timeout: зависший сервер = утечка потока + вечное ожидание в UI, флаг отмены не прерывает блокированное чтение (`src/services/local_ai.rs:76-83`) |
| 6 | P2 | Блокирующие подпроцессы на GTK main thread: `wpctl set-default` по клику (`ui/views.rs:2095`), `fc-list` при старте окна (`ui/views.rs:2939`), пользовательский скрипт синхронно `.output()` без таймаута (`ui/launcher.rs:3037`) |
| 7 | P2 | systemd unit вообще без hardening (`packaging/zeshicast-gtk.service`) |
| 8 | P2→док | Палитра из docs/DESIGN.md и design-system-plan.md не существует в коде ни одним hex'ом; окно 900×760 против заявленных 860×600. **По git-истории это не дефект, а отставание доков от осознанного редизайна** (32b435e «Implement Raycast v2 design handoff» → серия align/polish коммитов); фикс — только обновить документацию |
| 9 | P2 | Мёртвый модуль `src/search/ssh.rs` (не объявлен в `search/mod.rs`) + SSH-бейдж в UI, который никогда ничего не вернёт; там же неэкранированный хост в `sh -c` |
| 10 | P2 | Roadmap/планы системно устарели: M3/M4/M5 в основном реализованы, но не помечены; Phase D описывает playerctl/swaync, которых давно нет |

**Общий вывод.** Ядро безопасности реализовано добротно: quoting по одинарным
кавычкам корректен, capability-модель JSON-действий работает, запись конфигов
атомарна (tempfile+fsync+rename), миграции SQLite в транзакциях и протестированы,
секреты маскируются и не утекают в логи. Найдены один реальный обход quoting (P1),
дыра CI (P1) и систематический дрейф документации от кода. Для локального
single-user приложения релиз-блокеров нет, кроме пунктов 1–2.

---

## 2. Сводка «Документация ↔ Код»

| Claim (документация) | Факт (код) | Статус |
| --- | --- | --- |
| security.md: плейсхолдеры POSIX-quoted, инъекция невозможна | Верно только вне двойных кавычек шаблона | ❌ обход найден |
| security.md: shell-действия проходят confirmation | Команды/JSON-продюсеры — да; скрипты расширений — нет; clipboard clear/delete вне GTK UI — нет | ⚠️ частично |
| security.md: undeclared capabilities блокируются | Подтверждено (`commands.rs:682-734`), кроме скриптов расширений | ✅ / ⚠️ |
| security.md: «UI не вызывает сырой process-spawn» | `views.rs:2066-2100` дергает wpctl напрямую | ❌ |
| privacy.md: таблица Stored Data полна | Нет `~/.config/zeshicast/dnd`, `~/.local/share/fonts/zeshicast/`, legacy `frequencies.txt`, WAL-sidecar'ов | ⚠️ неполна |
| privacy.md: retention контролирует хранение строк | Prune только при вставке/смене настройки через UI; ручная правка preferences.toml + рестарт оставляет лишние строки до первой записи | ⚠️ |
| privacy.md/UI: тумблер «Export secrets» влияет на экспорт | Мёртвый контроль: CLI использует только флаг `--include-secrets` (`main.rs:21`); `export_config` — мёртвый код | ❌ |
| README: установка flake/NixOS/systemd/niri-binding | Полностью соответствует flake.nix + packaging + scripts/install-user.sh | ✅ |
| README: дефолтная сборка включает notify daemon / MPRIS / clipboard text+image | Подтверждено (`buildFeatures = ["gui" "layer-shell"]`, дефолты preference'ов true) | ✅ |
| README: headless CLI без GUI-зависимости | Статически подтверждено (gtk optional за feature gui; все импорты cfg-гейтнуты) + cargo check | ✅ |
| DESIGN.md / design-system-plan.md: палитра (#111216, #8ab4f8, success #6dd58c, warning #f4c76b…) | Ни одного совпадения hex в src/; реальные токены — GTK-тема + ac_purple #8B7CF8 / ac_amber #F5A623 / ac_green #4BD98A / ac_red #FF6B5F (`style.rs:37-40`), источник истины — design_handoff CSS | ❌ палитра |
| DESIGN.md: окно 860×600, радиусы 12/8px, футер 40px | Факт: 900×760 (`launcher.rs:143-144`), радиусы 14/10px, футер 38px | ⚠️ геометрия |
| DESIGN.md: шрифтовый стек Outfit/Inter/Noto, типографика | Совпадает (`style.rs:17-31`) | ✅ |
| roadmap: «поведение сосредоточено в lib.rs и zeshicast-gtk.rs» | Устарело: lib.rs — фасад (~216 строк кода + тесты); жирные файлы теперь launcher.rs/views.rs/app.rs | ❌ устарело |
| roadmap: M3 SQLite для clipboard/usage + миграции | Реализовано (`storage.rs`), маркеров Done нет; snippets всё ещё txt | 📄 не отражено / частично |
| roadmap: M4 Raycast script metadata | Реализовано частично (`scripts.rs:96-131`; mode парсится, но не используется; arguments/preferences скриптов нет) | ⚠️ |
| roadmap: M5 views (clipboard history, snippets, emoji, font browser, calc history, window switcher, status strip) | Почти всё реализовано и шире плана; маркеров нет | 📄 не отражено |
| linux-command-center-plan Phase D: media через playerctl; детект swaync/dunst | Нативный MPRIS D-Bus без playerctl; zeshicast сам notification daemon | ❌ устарело |
| raycast-linux-features.md: emoji picker «не делать» | Полностью реализован (`search/emoji.rs`, view, `--emoji`) | ❌ противоречие |
| development.md: команды сборки/теста | Актуальны, кроме того что требуемый `cargo test --features gui` отсутствует в CI | ⚠️ |

Двусторонний drift (код без документации): OSD пилюля раскладки клавиатуры
(`ui/osd.rs`), markdown-рендер ответов AI (`ui/markdown.rs`) — нигде не описаны.

---

## 3. Детальные находки

### 3.1 Безопасность и надёжность кода

**P1 · Обход shell-экранирования внутри двойных кавычек шаблона**
`src/placeholders.rs:66-75`. `expand()` детектирует только обёртку в одинарные
кавычки. Для шаблона автора `notify-send "Result" "{{query}}"` значение
подставляется как `'$(reboot)'` внутрь двойных кавычек, где одинарные кавычки —
литеральные символы, а command substitution исполняется. Проверено эмпирически на
`sh`. Буфер обмена — недоверенные данные из любого приложения ⇒ любой кастомный
шаблон вида `cmd "{{clipboard}}"` становится code-exec, вопреки docs/security.md.
Фикс: детектировать незакрытые `"` в накопленном output и применять
double-quote-safe экранирование (`\" \$ \` \\`), либо отклонять/предупреждать о
шаблонах с`"{{…}}"` при загрузке манифеста. Тест:
`expand_placeholders_shell("echo \"{{query}}\"", …)` не должен позволять `$()` исполниться.

**P2 · Скрипты расширений без risk/capabilities** — `scripts.rs:198-206` создаёт
`ActionKind::Shell(...)` без `.with_risk(ActionRisk::Shell)`; capabilities из
`extension.toml` для скриптов не сверяются вовсе (только для команд через
`CommandEntry::with_extension_origin`). Запуск скрипта идёт без диалога
подтверждения даже при пустом списке capabilities. Фикс: добавить risk либо явно
задокументировать скрипты как trusted extension code.

**P3 · Неэкранированные подстановки в sh -c**: `ssh.rs:108` — хост/пользователь
сырыми в `format!("{} ssh {}", …)` (сейчас модуль мёртв — починить при подключении);
`scripts.rs:199` — путь скрипта сырым (пробелы ломают, `a$(reboot).sh` исполняется).
Фикс: `shell_quote()` (уже есть в placeholders.rs:96) или argv-запуск.

**P3 · Права кэша изображений буфера**: `launcher.rs:1368-1371` — каталог/файлы
создаются с umask (0644) вместо 0700/0600, в отличие от текстовой истории и
preferences.toml. Скриншоты из буфера — потенциально чувствительны.

**P3 · SSID как аргумент nmcli**: `launcher.rs:2384` — SSID `-w`/`--ask`
разберётся как опция. Фикс: вставить `--` перед ssid.

**P3 · Прочее**:

+ poisoned-mutex каскад: `network.rs:67` `lock().unwrap()` → `unwrap_or_else(|p| p.into_inner())`;
+ небуферизованное чтение буфера `launcher.rs:1291` (гигантская копия целиком в памяти) → `.take(MAX+N)`;
+ `lib.rs:186` печатает копируемый текст в stdout при отсутствии wl-copy/xclip → секреты в journald;
+ CLI REPL молча игнорирует NeedsConfirmation (пользователь выбирает Power Off — ничего не происходит без объяснения).

**Проверено, проблем нет** (подтверждено чтением):

+ Все SQL-запросы параметризованы; миграции в транзакции с pragma_update, версия новее поддерживаемой отвергается; права БД 0600 закреплены и протестированы.
+ Атомарная запись: tempfile в том же каталоге → fsync → chmod 0600 → rename → fsync каталога; тест на symlink-атаку (`config.rs:449-471`).
+ Импорт архива: валидация членов до распаковки, запрет абсолютных путей/`..`/symlink/hardlink, staging-dir + backup + откат.
+ Инъекции через процессы: композиторы — argv с числовыми id/адресами из ответов самих композиторов; kill — строго u32/u64 pid + проверка uid владельца + confirmation; плейсхолдеры по одинарным кавычкам нейтрализуют `$(rm -rf ~); reboot` (тест lib.rs:784-811).
+ Аудит паник: unwrap/expect вне тестов почти отсутствуют; внешние данные парсятся Option-цепочками (D-Bus, MPRIS, .desktop, JSON продюсеров, nmcli).
+ Сеть: таймауты на всех вызовах (кроме стрима, см. ниже); ключи не попадают в ошибки/логи (проверен grep всех println/eprintln).
+ notify_server: толерантный разбор мусора, MAX_HISTORY=100, checked_add для id.

### 3.2 Архитектура

**Блокировки UI-потока (P2×3)**:

+ `views.rs:2095-2104` — синхронный `wpctl set-default .status()` + свежий `audio_snapshot()` (4 fork+exec) в обработчике клика;
+ `views.rs:2939-2956` — `fc-list .output()` в build_ui → задержка первого показа окна;
+ `launcher.rs:3037-3050` — пользовательский скрипт синхронно без таймаута в connect_row_activated (для JSON-команд той же ветки уже есть поток+канал).
Фикс: существующий паттерн worker-поток + mpsc + `glib::timeout_add_local`.

**Слои**:

+ search → UI: утечек нет (grep чистый) ✅;
+ services → gtk: единственное место `services/media.rs:55-70` (mpris через gio/glib), спрятано за cfg(gui) — задокументировать как осознанное исключение или выделить чистый D-Bus клиент;
+ прямые вызовы утилит из UI: `views.rs:2065-2072, 2097, 2940`, `fonts.rs:51`, `launcher.rs:2411-2420, 1275` — разнести по services/audio, services/fonts.

**Против Target Architecture (roadmap)**: выполнено — SearchProvider trait (14 провайдеров), services/, ui/ модули, lib.rs-фасад. Не выполнено: `services/clipboard.rs` (код в app.rs), `services/file_index.rs` (в search/files.rs), `services/preferences.rs` (в config.rs), абстракция window_manager (три похожих провайдера Niri/Hyprland/Sway), критерий «core files small enough» для launcher.rs/views.rs.

**Прочее (P3)**:

+ `app.rs:441` — clone всех preferences на каждое нажатие клавиши поиска;
+ `notify_server.rs:64-70` — name_lost только печатает; `STATE.running` остаётся true → призрачный индикатор активного бэкенда;
+ нет обработки SIGTERM/SIGINT → graceful shutdown неявный;
+ дублирование CLI/GUI: config-dir вычисляется инлайн, два независимых help-текста, канонические имена view продублированы;
+ `ui/style.rs` — 1365 строк CSS в Rust-строке → resources/style.css + include_str!.

**План декомпозиции монолитов** (шаги «чистого переноса», barrel-реэкспорты, после каждого шага `cargo test && cargo check --features gui`):

1. `lib.rs` (~216 строк): fuzzy_score → search/mod.rs; spawn_*/copy_* → action.rs (exec); lib.rs — чистый фасад.
2. `app.rs` (1482): → `services/clipboard_store.rs` (ClipboardItem, классификация, PNG-cache, retention, методы add/delete/clear) + `services/text_input.rs` (wtype).
3. `ui/views.rs` (3757) → каталог `ui/views/` по доменам (audio, dashboard, system_monitor, media, network, notifications, clipboard_history, extensions, snippets); preferences_view объединить с ui/preferences.rs; font browser → ui/fonts.rs.
4. `ui/launcher.rs` (3227) → `ui/clipboard_capture.rs`, `ui/action_panel_controller.rs`, `ui/keybindings.rs` (~430 строк handle_key), `ui/execute.rs`; остаются GuiState/ensure_ui/build_ui.
5. `ui/style.rs` → CSS-ресурс.

**Проверено, проблем нет**: весь HTTP вне UI-потока; poll_cache идемпотентен (OnceLock); поиск окон с TTL-кэшем 2 c и таймаутом 200 мс; single-instance GApplication корректен; гонка владения org.freedesktop.Notifications обработана (REPLACE без ALLOW_REPLACEMENT, восстановление после смерти чужого демона); вотчеры переподключаются с liveness-пробой; индекс файлов строится в worker-потоке.

### 3.3 Тесты и CI

+ **P1 · CI не запускает GUI-тесты**: в rust.yml есть fmt/check/test без фич и nix-job с gui-check/gui-clippy/layer-shell-check, но ни одного `cargo test --features gui`. ~17 GUI-тестов мертвы. Поправка к исходной гипотезе: GUI в CI **компилируется**, cargo-deny **используется** (supply-chain job, EmbarkStudios/cargo-deny-action, --all-features). Фикс — одна строка YAML в nix-job.
+ **P2 · Главная ветка shell_quote не покрыта тестами**: апостроф внутри значения → `'\''` — единственная содержательная ветка функции, защищающей от выламывания из кавычек; все инъекционные тесты используют payload'ы без апострофов.
+ **P2 · Нет тестов retention-prune строк БД и data-миграций legacy txt→SQLite** (схемные миграции при этом протестированы хорошо).
+ **P2 · Нет Swatinem/rust-cache в Rust-job** (bundled SQLite пересобирается каждый прогон; Nix-job кэширован).
+ **P3** флакующий тайминговый тест `windows.rs:684` (жёсткий потолок 500 ms на elapsed); views.rs (3757 строк) — крупнейший файл без единого теста; osd/notify_server/style/widgets/preferences, search/emoji, search/web, services/local_ai — без тестов.
+ Качество существующих тестов высокое: поведенческие, на реальных фикстурах вывода wpctl/nmcli/desktop-файлов, edge cases (невалидный UTF-8, wildcard SSH hosts, symlink-атака). Соотношение тест/код ≈ 9.5%.
+ Мной прогнано: `cargo check --no-default-features` ✅; `cargo test --features gui` → **119 passed, 0 failed** ✅ (без gui — 102 passed).

### 3.4 Приватность и упаковка

+ **P2 · Мёртвый тумблер export_include_secrets** (см. таблицу). Фикс: читать preference в main.rs при отсутствии явного флага либо убрать тумблер из Privacy-секции.
+ **P3 · Импорт может оставить бэкап с секретами**: `config.rs:139-152` игнорирует ошибку `remove_dir_all` бэкапа `.zeshicast-backup-*`.
+ **P3 · Текст Privacy-панели «Stores last 50» vs дефолт 100** (`views.rs:3465-3467`).
+ **P3 · README-примеры расходятся с дефолтами** (ollama_model, ai_endpoint, translate_endpoint); комментарий flake.nix:38 указывает не на тот репозиторий.
+ **P3 · desktop-файлы без StartupWMClass=dev.zeshi.Zeshicast**, обобщённая иконка system-search-symbolic, flake не ставит собственную иконку.
+ **P3 · install-user.sh vs NixOS-модуль**: разные имена юнитов (zeshicast-gtk.service vs zeshicast.service) без защиты — возможен запуск двух демонов с гонкой за org.freedesktop.Notifications и clipboard-capture.
+ **P3 · Мелкий drift настроек**: dashboard_poll_interval_ms дефолт 1000 (план говорит 2000); notifications_enabled дефолт true (план — false); roadmap: «gtk-rs 0.22» (факт gtk4 crate 0.11.3), «100 tests» (факт ~120).
+ Проверено, проблем нет: flake↔packaging unit parity; wrapGAppsHook4 + wl-clipboard wrapper; оба бинарника собираются с правильными фичами; install-user.sh с set -euo pipefail, идемпотентен; deny.toml осмысленный (licenses allow-list, bans, sources, advisories gate); PNG prune/delete/clear полностью покрыты тестами; private mode переключается на лету мгновенно; маскирование секретов в UI/экспорте/логах подтверждено.

### 3.5 Паритет планов развития

Roadmap-паритет (выборочно, полный — в отчёте лейна parity-packaging):

| Пункт roadmap | Статус в доке | Факт |
| --- | --- | --- |
| M1: SearchProvider trait, provider-тесты | частично done | ✅ полностью |
| M3: SQLite clipboard/usage + миграции | будущее | ✅ реализовано, не отмечено |
| M3: SQLite snippets | будущее | ❌ всё ещё snippets.txt |
| M4: Raycast/Vicinae script metadata | будущее | ⚠️ @raycast.* парсится; mode не используется; arguments/preferences нет |
| M5: views (clipboard/snippets/emoji/fonts/calc/windows/status strip) | будущее / started | ✅ почти всё реализовано, часто шире плана |
| M6: extension manifest/registry | pragmatic path | ⚠️ частично (manifest + capabilities + Extension Browser); JS runtime корректно отсутствует |

---

## 4. Проверено и подтверждено закрытым (архивные аудиты)

TOCTOU/атомарные записи (tempfile+fsync+rename+symlink-тест), двойное экранирование,
дрейф таймера — реально исправлены в коде, а не только отмечены выполненными.
Нарушений заявлений архивных аудитов не найдено.

## 5. План действий

**S (до часа каждая, сделать сейчас):**

1. Строка `gui-test` (nix develop --command cargo test --features gui) в CI + Swatinem/rust-cache в Rust-job.
2. Фикс double-quote quoting в placeholders.rs (+2 теста: apostrophe-branch и `"{{query}}"`).
3. `.with_risk(ActionRisk::Shell)` для скриптов расширений.
4. `--` перед SSID в nmcli; 0700/0600 на PNG-кэш; `unwrap_or_else(into_inner)` в network.rs.
5. Решить судьбу ssh.rs: подключить провайдер или удалить модуль вместе с SSH-бейджем.
6. Починить или убрать тумблер Export secrets.
7. StartupWMClass=dev.zeshi.Zeshicast в оба desktop-файла.
8. Волна правок документации: roadmap-маркеры Done, Phase D в linux-command-center-plan.md, emoji в raycast-linux-features.md, таблица Stored Data в privacy.md (+dnd, fonts, frequencies.txt, WAL), README-примеры, текст «last 50», guard от двух демонов в install-user.sh. Дизайн-доки (DESIGN.md, design-system-plan.md) — по решению автора: либо синхронизировать с фактическим состоянием после редизайна (источник истины — design_handoff), либо пометить архивными; код дизайна НЕ трогать.

**M (по полдня):**
9. Read-timeout AI-стрима + прерывание отменой.
10. Вынос wpctl/fc-list/user-scripts в worker-потоки (паттерн уже в коде).
11. ExecutionPolicy для destructive secondary actions (единая точка в run_secondary_action).
12. systemd hardening (NoNewPrivileges, PrivateTmp, ProtectSystem=full, ReadWritePaths, SystemCallFilter=@system-service…).
13. Тесты: retention-prune строк, migrate_clipboard/migrate_usage, shell_quote apostrophe.

**L (рефакторинг):**
14. Декомпозиция по 5-шаговому плану (раздел 3.2).
15. Отвязка services от gio (чистый D-Bus клиент для MPRIS).

## 6. Что осталось непроверенным

+ Runtime-поведение под Wayland: layer-shell, реальные D-Bus-гонки, reconnect niri event-stream — только статический анализ.
+ Права WAL-sidecar файлов SQLite (рекомендуется разово проверить `ls -la ~/.config/zeshicast/`).
+ Фактическая ассоциация окно↔desktop-файл на реальных Wayland-лаунчерах.

---

## 7. Дельта к существующей документации: новое / уже признано / регрессия закрытых пунктов

Этот раздел классифицирует находки по отношению к тому, что проект **сам уже
зафиксировал** в активной документации и архивных аудитах. Ключевой контекст:
`docs/archive/ACTION_PLAN_FULL_REVIEW.md` помечен «🟢 ВСЕ ЭТАПЫ ВЫПОЛНЕНЫ И В АРХИВЕ»
с чеклистом `[x]` по P0–EXT, а `docs/README.md` утверждает, что все исторические
аудиты «на 100% выполнены в коде». Часть находок — это не новые проблемы, а
**невыполненные или неполностью выполненные пункты, отмеченные как закрытые**.

### 7.1 Регрессии / неполное закрытие пунктов, отмеченных `[x]`

| Находка из этого ревью | Пункт архива, которому противоречит |
| --- | --- |
| Скрипты расширений без `ActionRisk::Shell` и без проверки capabilities (`scripts.rs:198-206`) | **P0-3 «capabilities enforced» [x]** — план явно перечисляет `src/search/scripts.rs`, требует `Script { path, capabilities }` и гейт JSON/shell действий. Для команд реализовано, для скриптов — нет |
| Мёртвый тумблер `export_include_secrets` | **P2-2 «privacy controls implemented» [x]** — preference введён планом, но в CLI-путь экспорта не подключён (`export_config` — мёртвый код) |
| `fc-list .output()` синхронно в `build_ui` | **P1-3 «startup subprocess work removed» [x]** — удалены одни стартовые fork'и, но блокирующий вызов при первом показе окна остался |
| Пользовательский скрипт исполняется синхронно без таймаута на UI-потоке (`launcher.rs:3037`) | **P1-4 «JSON commands async/cancellable» [x]** — асинхронизацию получили только JSON-команды; соседняя ветка Script в том же обработчике осталась синхронной |
| Массовый drift документации (палитра, геометрия, roadmap-маркеры, Phase D, README-примеры) | **P2-5 «docs synchronized» [x]** — синхронизация коснулась не всех документов; DESIGN.md описывает палитру, которой нет ни одним hex'ом |
| Разные имена юнитов install-user.sh vs NixOS-модуля без guard'а | **PKG-2 «systemd targets aligned» [x]** — target'ы выровнены, но ко existence двух демонов защиты нет |

Вывод: архивация аудита со статусом «всё закрыто» создала ложное ощущение
завершённости — как минимум 6 пунктов закрыты частично. Рекомендация: при
следующем аудите сверять каждый `[x]` не с текстом плана, а с acceptance-
критериями раздела.

### 7.2 Уже признано документацией (не новые проблемы)

Эти пункты документация честно фиксирует как осознанные решения или известные
ограничения — ревью подтверждает соответствие, новых действий не требует:

+ Отсутствие песочницы команд/расширений — security.md прямо declares «user-trusted
  extension code»; находка про скрипты частично покрыта этим допущением (но обещание
  confirmation для shell-действий всё равно нарушено — см. 7.1);
+ Отправка промптов и API-ключей настроенному endpoint'у — security.md, Trust Boundaries;
+ Слабость Wayland-инъекции ввода (snippets только copy/wtype) — raycast-linux-features.md;
+ Синхронная природа исполнения в search-path как остаточный риск — FULL_REVIEW.md
  («Remaining performance risk is mostly sync execution…») — что не отменяет
  конкретные точки блокировки из раздела 3.2;
+ Ограниченный DoS-поверхность notify_server (без rate-limit) — оценена в ревью как
  приемлемая для session-bus, в доках не зафиксирована (мелочь).

### 7.3 Полностью новые находки (нигде не зафиксированы)

Ни один из этих пунктов не упоминается в активных доках, бэклоге или архиве:

+ **P1 · Обход quoting через двойные кавычки** (`placeholders.rs:66-75`). Особо:
  security.md в разделе Reporting Security Issues прямо просит документировать
  именно такие обходы («If you find a path where untrusted input bypasses …
  placeholder quoting, document the exact command/config needed to reproduce it»)
  — находка попадает точно в этот запрос;
+ **P1 · CI без `cargo test --features gui`**;
+ AI-стриминг без read-timeout, отмена не прерывает заблокированное чтение;
+ clipboard clear/delete через CLI REPL минует ExecutionPolicy;
+ мёртвый модуль search/ssh.rs + «пустой» SSH-бейдж в UI;
+ палитра DESIGN.md отсутствует в коде; окно 900×760 vs 860×600;
+ name_lost не снимает индикатор активного notification-бэкенда;
+ нет обработки SIGTERM/graceful shutdown;
+ права PNG-кэша по umask; SSID без `--` в nmcli; println копируемого текста при
  отсутствии wl-copy; poisoned-mutex unwrap;
+ retention-prune не выполняется при ручной правке preferences.toml + рестарт;
+ бэкап импорта с секретами может пережить сбой удаления;
+ desktop-файлы без StartupWMClass; README-примеры расходятся с дефолтами;
+ нет rust-cache в Rust-job; флакующий тайминговый тест windows.rs:684;
+ главная ветка shell_quote (апостроф) не покрыта тестами; retention-prune строк
  БД и data-миграции legacy txt→SQLite без тестов;
+ clone всех preferences на каждое нажатие клавиши (app.rs:441);
+ недокументированные поверхности: OSD раскладки, markdown-рендер AI.

### 7.4 Доп. проход: полное покрытие оставшихся активных документов

После раздела 7 проведён целевой проход по двум документам, покрытым ранее лишь
частично: design-system-plan.md (шаблоны видов и карточек) и development.md
(правила как обязательные практики). Полный отчёт: `/tmp/zeshicast-review/docs-deep.md`.

> **Важная оговорка про дизайн (по решению автора, подтверждено git-историей):**
> расхождения кода с DESIGN.md / design-system-plan.md — не дефекты. История
> коммитов (`32b435e` «Implement Raycast v2 design handoff: new CSS, all views
> redesigned» → `c19152b`, `d5cf46a`, `fc5aec3`, `f634a2d`, `c80625c`) показывает
> длительную осознанную эволюцию дизайна от исходных документов к макетам
> design_handoff. Все дизайн-находки этого ревью (палитра, геометрия окна,
> радиусы/паддинги карточек, футер, шаблоны видов, settings search, DNS/Output
> секции и т.д.) следует читать как **задачи синхронизации документации**, а не
> как правки кода: либо привести DESIGN.md/design-system-plan.md к фактическому
> состоянию, либо явно пометить их архивными с указанием на design_handoff как
> источник истины.

Новые находки:

| Sev | Документ | Суть |
| --- | --- | --- |
| P2 | development.md «Adding A Provider» п.4 | `ProcessesProvider` читает `/proc` (read_dir + stat/cmdline каждого PID) синхронно на каждый запрос `proc …` без кэша (`processes.rs:19-24, 67+`) — нарушение собственного правила «cached service snapshot»; кэшированный сервис уже существует (`top_processes_by_memory`) |
| P2 | design-system-plan :381-398, :452 | Settings search заявлен планом («Add settings search entry») — не реализован (`views.rs:3225-3228`); замена Save/Cancel на autosave не отражена в плане |
| P2 | design-system-plan :201-206, :448 | Карточки: факт radius 10px/padding 16px против плана «≤8px/12px» (`style.rs:594-599, 747-751, 821-826`) |
| P2 | design-system-plan :441-446 | Футер root-экрана остался иконным против требования текстового «Run Enter · Actions Ctrl+K · Copy Ctrl+Enter» (`launcher.rs:2541-2560`) |
| P3 | design-system-plan :131-137 | Строка результата: title над subtitle (stacked) вместо одной базовой линии (`widgets.rs:163-190`) — осознанный уход под макет, в плане не отражён |
| P3 | design-system-plan :278-303 | Network View: секции DNS нет, Copy IP/MAC скрыты (`views.rs:1199-1216`, кнопки созданы и set_visible(false)) |
| P3 | design-system-plan :295-307 | Media View: секция Output отсутствует (громкость вынесена в Audio View, план не отражает) |
| P3 | design-system-plan :180-182 | Action Panel: hotkey-назначение не реализовано (только Set Alias) |
| P3 | design-system-plan :215 | Dashboard: ряд быстрых кнопок скрыт, навигация через кликабельные карточки (`views.rs:851-871`) |
| P3 | токены preferences | Внутреннее противоречие: дефолт ui_density = "compact" (`preferences.rs:4`), описание говорит «comfortable (default)» (`preferences.rs:39-41`) |
| P3 | development.md «tests near provider» | Большинство тестов централизовано в lib.rs, а не рядом с провайдерами |
| P3 | development.md «startup minimal» | `fc-cache -f` синхронно при старте (`fonts.rs:51`) — полная пересборка кэша может длиться секунды |

Подтверждено соответствие (выборочно): группировка root-экрана, структура Action
Panel, StatusChip по спецификации, GTK Implementation Plan п.1-2/6-7 выполнен,
Non-Goals не нарушены, все 18 провайдеров зарегистрированы и реализуют контракт,
HTTP вне main thread, windows-провайдеры с TTL-кэшем соответствуют разрешённому
паттерну, Build Commands/Test Strategy точны.

Итог по полноте: теперь покрыты все активные документы (README, docs/README,
security, privacy, DESIGN, design-system-plan, development, roadmap,
linux-command-center-plan, raycast-linux-features) плюс design_handoff CSS как
источник палитры.

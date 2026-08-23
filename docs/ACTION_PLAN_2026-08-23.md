# Action Plan — 2026-08-23 (по итогам полного ревью)

Источник находок: [full-review-2026-08-23.md](full-review-2026-08-23.md).
Формат: фазы по приоритету, внутри — порядок выполнения. Каждая задача:
где · что делать · критерий приёмки. Чекбоксы обновлять по мере закрытия.
После закрытия фаз P0–P1 снять соответствующие пункты с Known Gaps в security.md.

---

## Phase P0 — безопасность и дыра в CI

### P0-1. Закрыть обход shell-quoting через двойные кавычки плейсхолдеров

- **Where:** `src/placeholders.rs` (`expand()`, блок `is_wrapped_in_single_quotes`)
- **Problem:** шаблон автора вида `cmd "{{query}}"` подставляет значение как `'…'`
  внутрь двойных кавычек, где одинарные кавычки литеральны, а `$()` / `` ` `` /
  `$VAR` интерпретируются shell'ом → инъекция произвольного кода из буфера
  обмена/поиска (подтверждено эмпирически на sh).
- **Do:** при `shell_escape = true` детектировать незакрытые `"` в уже
  накопленном `output` (аналогично одинарным кавычкам) и для этого контекста
  применять double-quote-safe экранирование (`\" \$ \` \\`). Альтернатива
  (минимальная): при загрузке command TOML отклонять/предупреждать о шаблонах
  с`"{{…}}"`.
- **Accept:** тест `expand_placeholders_shell("echo \"{{query}}\"", ctx)` с
  query=`$(reboot)` не допускает исполнения подстановки (проверить через
  фактический `sh -c` в тесте или точное сравнение строки); существующие
  single-quote тесты зелёные.

### P0-2. Запускать GUI-тесты в CI

- **Where:** `.github/workflows/rust.yml`, nix-job
- **Problem:** нет ни одного `cargo test --features gui` — ~17 GUI-тестов мертвы
  в CI; development.md требует эту команду перед коммитом.
- **Do:** добавить шаг `nix develop --command cargo test --features gui`;
  в Rust-job добавить `Swatinem/rust-cache@v2` (bundled SQLite пересобирается
  каждый прогон).
- **Accept:** CI зелёный на main; GUI-тесты реально выполняются (видны в логе).

### P0-3. Риск и capabilities для скриптов расширений

- [x] **Done.** Где: `src/search/scripts.rs` + `src/ui/launcher.rs`. Проблема:
  скрипты создавали `ActionKind::Shell(...)` без `.with_risk(ActionRisk::Shell)`
  и без сверки с capabilities манифеста, а ветка активации Script в лаунчере
  исполняла скрипт через `run_script_capture` до всякого подтверждения.
  Решение: все исполняемые скрипты помечены `ActionRisk::Shell`; extension-скрипты
  сверяются с `capabilities` манифеста (без `shell` — блокировка `ActionKind::None`
  с поясняющим subtitle, по образцу `CommandEntry::with_extension_origin`);
  capture-путь в лаунчере выполняется только после подтверждения в панели.
  Bullet снят с Known Gaps в security.md.

### P0-4. Деструктивные операции с буфером через ExecutionPolicy на всех путях

- [x] **Done.** Где: `src/app.rs` (`run_secondary_action`), CLI REPL в `src/main.rs`.
  Проблема: clear/delete clipboard истории миновали ExecutionPolicy вне GTK UI;
  в CLI REPL выполнялись немедленно. Решение: единая точка защиты в `app.rs` —
  `run_secondary_action` для рискованных secondary-операций
  (`DeleteClipboardItem`, `ClearClipboardHistory`) без подтверждения возвращает
  `ExecutionDecision::NeedsConfirmation(risk)` и не исполняет операцию;
  подтверждённый путь — `run_secondary_action_confirmed`. GUI-лаунчер передаёт
  подтверждение после своей панели; CLI REPL печатает «confirmation required;
  use the GTK UI to run this action» вместо исполнения. Нерискованные secondary
  действия работают как раньше. Bullet снят с Known Gaps в security.md.

---

## Phase P1 — надёжность и быстрые фиксы (каждый ≤ часа)

### P1-1. Read-timeout для AI-стрима

- **Where:** `src/services/local_ai.rs:76-83`
- **Do:** `timeout_read(30s)` у ureq-agent (таймаут между чанками, не на весь
  ответ); убедиться, что отмена прерывает ожидание чтения.
- **Accept:** замолчавший сервер не оставляет вечного «ждём ответ»; отмена
  освобождает поток.

### P1-2. Вынос блокирующих вызовов с GTK main thread

- **Where:** `src/ui/views.rs:2095-2104` (wpctl set-default + audio_snapshot),
  `src/ui/views.rs:2939-2956` (fc-list в build_ui),
  `src/ui/launcher.rs:3037-3050` (пользовательский скрипт синхронно)
- **Do:** паттерн уже в коде — worker-поток + mpsc +
  `glib::timeout_add_local` (как `run_json_command_action_async`,
  `launcher.rs:2776-2823`; отложенный файловый индекс `launcher.rs:105-126`).
- **Accept:** клик по аудиоустройству и активация скрипта не блокируют отрисовку;
  окно появляется до завершения fc-list.
- **Also (P2 из валидации P0-3):** устранить двойное исполнение скриптов с
  пустым stdout — `run_script_capture` уже исполнил скрипт, а fallback
  `run_action_confirmed` спавнит его повторно (`launcher.rs` on_confirm) —
  различать «выполнен без вывода» и «не выполнялся».

### P1-3. Мелкие security/hygiene фиксы (один коммит)

- `src/ui/launcher.rs:2384`: вставить `--` перед SSID в nmcli connect.
- `src/ui/launcher.rs:1368-1371`: каталог PNG-кэша `0o700`, файлы через
  `write_file_atomic(…, 0o600)` (функция уже есть).
- `src/services/network.rs:67`: `lock().unwrap_or_else(|p| p.into_inner())`.
- `src/ui/launcher.rs:1291`: ограничить чтение буфера `.take(MAX_CLIPBOARD_TEXT_BYTES + N)`.
- `src/lib.rs:163,186`: не печатать копируемый текст/полную команду в stdout
  демона (только имя операции/ошибку IO).
- **Accept:** `cargo test --features gui` зелёный; права кэша проверить `ls -la`.

### P1-4. Решить судьбу мёртвого search/ssh.rs

- **Where:** `src/search/ssh.rs` (не объявлен в `search/mod.rs`),
  `src/ui/launcher.rs:316-318` (SSH-бейдж)
- **Do:** вариант A — подключить провайдер (перенеся определение терминала из
  синхронных `which <term>` спавнов в кэш/конфиг, экранируя хост через
  `shell_quote`); вариант B — удалить модуль и бейдж.
- **Accept:** либо SSH-режим выдаёт результаты, либо бейджа нет; мёртвого файла нет.

### P1-5. Починить или убрать тумблер export_include_secrets

- **Where:** `src/main.rs:21` vs `src/config.rs:7-10` (`export_config` — мёртвый
  код), тумблер в `ui/preferences.rs:157`
- **Do:** при отсутствии явного `--include-secrets` читать preference; либо
  убрать строку из PREFERENCE_SECTIONS.
- **Accept:** поведение экспорта предсказуемо и совпадает с UI.

### P1-6. Packaging мелочи

- `packaging/*.desktop`: добавить `StartupWMClass=dev.zeshi.Zeshicast`.
- `scripts/install-user.sh`: предупреждение/аборт если уже установлен юнит
  `zeshicast.service` (NixOS-модуль) — защита от двух демонов.
- **Accept:** окно ассоциируется с .desktop в лаунчерах Wayland; повторная
  установка предупреждает о конфликте.

### P1-7. Тесты на security-контракты

- `placeholders.rs` / `lib.rs`: ветка апострофа в `shell_quote`
  (`it'; rm -rf ~` → `'it'\''; rm -rf ~'`) и P0-1 double-quote кейс.
- `services/storage.rs`: retention-prune (вставить 105 строк при limit=100 →
  COUNT==100 новейших); `migrate_clipboard` дедупликация.
- `src/search/windows.rs:684`: убрать флакующий потолок elapsed 500ms
  (первая ассершня `is_none()` достаточна).
- **Accept:** новые тесты ловят регрессии перечисленных контрактов.

---

## Phase P2 — производительность и UX-надёжность

### P2-1. Кэшировать ProcessesProvider

- **Where:** `src/search/processes.rs:19-24,67+` (чтение /proc на каждый запрос),
  готовый кэш: `top_processes_by_memory`
- **Do:** TTL-снапшот по образцу windows.rs (TTL 2s, таймаут 200ms).
- **Accept:** ввод `proc …` не читает /proc чаще раза в TTL.

### P2-2. fc-cache только по необходимости

- **Where:** `src/ui/fonts.rs:51` (`fc-cache -f` синхронно при старте через install_css)
- **Do:** запускать только при первом создании каталога шрифтов либо в фоне.
- **Accept:** рестарт демона не гоняет полную пересборку кэша шрифтов.

### P2-3. Корректность индикатора notification-бэкенда

- **Where:** `src/ui/notify_server.rs:64-70` (name_lost только печатает),
  `services/notifications.rs:146-151`
- **Do:** в name_lost вызвать новый `mark_server_inactive()`.
- **Accept:** при потере имени статус-зона не показывает активный бэкенд.

### P2-4. Graceful shutdown

- **Where:** `bin/zeshicast-gtk.rs` / launcher
- **Do:** `glib::unix_signal_add_local` на SIGTERM/SIGINT → `app.quit()`;
  задокументировать поведение вотчеров (EPIPE/EOF-reconnect уже работает).
- **Accept:** `systemctl --user stop zeshicast-gtk` завершает процесс аккуратно.

### P2-5. Убрать clone preferences на каждое нажатие клавиши

- **Where:** `src/app.rs:441` (`context.with_preferences(self.preferences.clone())`
  в `Zeshicast::search`)
- **Do:** параметризовать `PlaceholderContext` временем жизни / передавать ссылку.
- **Accept:** профайлинг/код-ревью: аллокаций map на keypress нет.

### P2-6. systemd hardening

- **Where:** `packaging/zeshicast-gtk.service` + юнит во flake.nix
- **Do:** начать с безопасного набора: NoNewPrivileges=true,
  PrivateTmp=true, ProtectSystem=full, RestrictSUIDSGID=true,
  LockPersonality=true, CapabilityBoundingSet=; затем ReadWritePaths=%h/.config/zeshicast %h/.cache/zeshicast,
  ProtectKernel*, SystemCallFilter=@system-service. Проверять каждую опцию на
  живой сессии (D-Bus, Wayland-сокет, clipboard, MPRIS, сеть).
- **Accept:** демон работает под юнитом с hardening; clipboard/notify/MPRIS живы.

---

## Phase P3 — рефакторинг (L, после стабилизации P0–P2)

Декомпозиция «чистыми переносами», barrel-реэкспорты, после каждого шага
`cargo test && cargo check --features gui`:

- [ ] **P3-1.** `lib.rs` → чистый фасад: `fuzzy_score` → `search/mod.rs`;
      `spawn_shell/spawn_command/copy_to_clipboard/copy_with` → `action.rs` (exec).
- [ ] **P3-2.** `app.rs` → `services/clipboard_store.rs` (ClipboardItem,
      классификация, PNG-cache, retention, add/delete/clear) +
      `services/text_input.rs` (wtype).
- [ ] **P3-3.** `ui/views.rs` (3757) → каталог `ui/views/` по доменам (audio,
      dashboard, system_monitor, media, network, notifications,
      clipboard_history, extensions, snippets); preferences_view → ui/preferences.rs;
      font browser → ui/fonts.rs.
- [ ] **P3-4.** `ui/launcher.rs` (3227) → ui/clipboard_capture.rs,
      ui/action_panel_controller.rs, ui/keybindings.rs (~430 строк handle_key),
      ui/execute.rs; остаются GuiState/ensure_ui/build_ui.
- [ ] **P3-5.** `ui/style.rs` (1365) → resources/style.css + include_str!.
- [ ] **P3-6.** (опционально) отвязка services от gtk/gio: чистый D-Bus клиент
      для MPRIS, адаптер остаётся за feature gui.

---

## Финальная верификация

- [ ] `cargo check --no-default-features` (headless CLI)
- [ ] `cargo clippy --all-targets --features gui -- -D warnings`
- [ ] `cargo test --features gui` (все, включая новые)
- [ ] CI зелёный, включая новый gui-test шаг
- [ ] security.md Known Gaps сняты (если P0-3/P0-4 закрыты кодом)
- [ ] Ручной smoke: демон + niri биндинг, clipboard text+image, уведомления,
      MPRIS, Wi-Fi подключение с SSID начинающимся с `-`

## Внеполосные заметки (не входят в план)

- `docs/linux-command-center-plan.md` Core Surfaces всё ещё описывает Volume
  внутри Media View (аспирационно); громкость теперь в Audio View — поправить
  при следующей ревизии плана.
- Текст preferences «Row density: comfortable (default)» vs дефолт compact
  (`preferences.rs:39-41`) и UI-текст «Stores last 50» (`views.rs:3465`) —
  поправить вместе с ближайшим коммитом вокруг этих мест.

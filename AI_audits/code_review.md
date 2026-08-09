# Отчет о Code Review: zeshicast

## 🌟 Общее впечатление и что сделано отлично (Praise)
Репозиторий `zeshicast` представляет собой отлично структурированный проект лаунчера на Rust. Код разбит на логичные и изолированные модули (`search`, `services`, `ui`, `action`), что сильно упрощает навигацию и поддержку.

Особенно хочется отметить:
- **Внимание к безопасности при импорте конфигов:** В [`src/config.rs`](file:///home/blackzeshi/Code/Rust/zeshicast/src/config.rs) реализована хорошая защита от распаковки архивов (проверки на path traversal `../`, запрет абсолютных путей и предотвращение атак через symlink-и).
- **Экранирование шелл-команд:** В [`src/placeholders.rs`](file:///home/blackzeshi/Code/Rust/zeshicast/src/placeholders.rs) реализована умная функция `expand_placeholders_shell`, которая корректно оборачивает параметры в POSIX-кавычки, предотвращая базовые RCE-инъекции.
- **Отзывчивость UI:** Грамотное использование [`src/services/poll_cache.rs`](file:///home/blackzeshi/Code/Rust/zeshicast/src/services/poll_cache.rs) для вынесения тяжелых системных вызовов (сбор статистики сети, аудио через `ip` и `wpctl`) в фоновый поток, чтобы не блокировать главный цикл GTK.

---

## 🔴 Blockers (Критичные проблемы)

### 1. Security: Race Condition / TOCTOU при сохранении конфигов
- **Location:** [`src/config.rs`](file:///home/blackzeshi/Code/Rust/zeshicast/src/config.rs) (функция `write_file_atomic`)
- **Проблема:** В функции `write_file_atomic` сначала создается временный файл, и лишь затем меняются его права доступа:
  ```rust
  let mut file = fs::OpenOptions::new()
      .write(true)
      .create_new(true)
      .open(&temp_path)?;
  set_file_mode(&file, mode)?; // Устанавливается 0o600
  ```
  Файл изначально создается с системным `umask` (часто `0644` или `0666`), и только после этого его права урезаются до `0600`. Это создает TOCTOU (Time-of-check to time-of-use) уязвимость: в этот короткий промежуток времени другой локальный процесс может прочитать содержимое файла. Учитывая, что туда сохраняется `preferences.toml` с ключами `ai_api_key` и `translate_api_key`, это критично.
- **Решение:** Использовать `std::os::unix::fs::OpenOptionsExt`, чтобы задавать режим прав доступа *до* создания файла:
  ```rust
  use std::os::unix::fs::OpenOptionsExt;
  let mut file = fs::OpenOptions::new()
      .write(true)
      .create_new(true)
      .mode(mode) // Устанавливаем 0o600 сразу при создании
      .open(&temp_path)?;
  ```

---

## 🟡 Suggestions (Предложения по улучшению)

### 1. Performance: Системные вызовы вместо чтения процессов через `fork+exec`
- **Location:** [`src/services/system_stats.rs`](file:///home/blackzeshi/Code/Rust/zeshicast/src/services/system_stats.rs) (`read_root_disk_usage`)
- **Проблема:** Вызывается внешний процесс `Command::new("df").args(["-kP", "/"])`. Использование `fork+exec` для получения простой статистики диска — тяжелая операция по сравнению с нативными системными вызовами.
- **Решение:** Использовать системный вызов `statvfs` (напрямую через крейт `libc` или обертку `rustix::fs::statvfs`), что работает быстрее и не требует порождения процессов.

### 2. Security: Защита от "двойного экранирования" плейсхолдеров
- **Location:** [`src/placeholders.rs`](file:///home/blackzeshi/Code/Rust/zeshicast/src/placeholders.rs) (`expand_placeholders_shell`)
- **Проблема:** Функция `expand_placeholders_shell` оборачивает аргументы в одинарные кавычки (`'`). Однако, если пользователь в конфигурационном файле сам обернет плейсхолдер в кавычки (например: `command = "echo '{{query}}'"`), итоговая строка раскроется как `echo ''пользовательский_ввод''`, выходя из кавычек.
- **Решение:** Добавить проверки при парсинге команд или предупреждение для авторов расширений.

### 3. Performance: Устранение дрифта таймера в фоновом поллинге
- **Location:** [`src/services/poll_cache.rs`](file:///home/blackzeshi/Code/Rust/zeshicast/src/services/poll_cache.rs)
- **Проблема:** Цикл фонового потока вызывает `std::thread::sleep(Duration::from_secs(1))` *после* выполнения работы (которая включает вызовы `wpctl` и `ip`). Если работа занимает 200 мс, реальный интервал составит 1.2 с.
- **Решение:** Вычислять оставшееся время `1s - elapsed` или использовать каналы с регулярным таймером.

### 4. Maintainability: Вызов бинарников напрямую вместо shell-строк
- **Location:** [`src/search/windows.rs`](file:///home/blackzeshi/Code/Rust/zeshicast/src/search/windows.rs)
- **Проблема:** Команды фокуса окон формируются как shell-строки (`ActionKind::Shell`) с пробросом в `sh -c`.
- **Решение:** Концептуально чище использовать прямые вызовы процессов `ProcessCommand::new("hyprctl", vec!["dispatch", ...])`, избавляя от вызова интерпретатора `sh`.

---

## 💭 Nits (Замечания и мелочи)

- **Платформозависимость:** Проект нацелен на Linux. Использование атрибутов `#[cfg(unix)]` в некоторых местах избыточно общее — его можно уточнить до `#[cfg(target_os = "linux")]`.
- **Константы:** Параметры лимитов (например, `MAX_CLIPBOARD_ENTRIES`) вынести в единый модуль констант `constants.rs`.

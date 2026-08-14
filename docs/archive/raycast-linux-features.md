# Raycast v2 → Linux porter: feature map

> Стек: Rust + GTK4 + gtk4-layer-shell | NixOS + Niri (Wayland) | личный проект

## MVP (реализовать первым)

| Фича | Raycast v2 | Linux-реализация | Сложность |
|------|-----------|-----------------|-----------|
| **Root Search — приложения** | Поиск+запуск по всем .app | Парсинг `.desktop` из XDG-каталогов + nix-store пути | 🟢 Низкая |
| **Root Search — файлы** | Файлы и папки прямо в root, Rust-индексатор | `fd` как backend или свой inotify-вотчер | 🟡 Средняя |
| **Калькулятор** | Встроенный, с подсветкой синтаксиса, единицы/валюты/время | Биндинг к `libqalculate` или `evalexpr` | 🟢 Низкая |
| **Буфер обмена (история)** | Event-based capture, группировка, переименование | `wl-clipboard` watcher + SQLite; или reuse `cliphist` | 🟢 Низкая |
| **Вызов окна по хоткею** | Глобальный хоткей | Биндинг в конфиге Niri → `niri msg action spawn` | 🟢 Низкая |
| **Layer-shell окно** | Всегда поверх, без декораций | `gtk4-layer-shell` crate | 🟡 Средняя |

## Второй приоритет

| Фича | Raycast v2 | Linux-реализация | Сложность |
|------|-----------|-----------------|-----------|
| **Сниппеты** | Expand при вводе суффикса, теги | Демон + `zwp_virtual_keyboard_v1`; или делегировать `espanso` | 🔴 Высокая |
| **Quicklinks** | Пиннинг, теги, prefer existing tabs | Открыть URL через `xdg-open`; хранить в SQLite | 🟢 Низкая |
| **Translator** | Inline с суффиксом `in`, язык по умолчанию | HTTP к DeepL/LibreTranslate API | 🟢 Низкая |
| **AI Chat** | Agents, Memory, Skills, ветвление чатов | HTTP к LiteLLM-прокси (уже есть у тебя в Kontur/дома) | 🟢 Низкая |
| **Управление окнами** | Switch Spaces, resize | `niri msg` IPC — Niri-специфично, непереносимо | 🟡 Средняя |

## Пропустить / не реализовывать

| Фича | Причина |
|------|---------|
| Cloud Sync | Экосистема, не код |
| Store расширений | Годы работы команды |
| Dictation (Auto Styling) | Зависит от macOS Speech framework |
| Raycast Focus | Специфика macOS |
| Inline Emoji Picker | Есть `smile` / системный picker |
| Hyper Key | Лучше через Niri/Karabiner-аналоги |

## Подводные камни (Wayland + Niri)

- **Глобальных хоткеев нет** — вызов только через биндинг в конфиге композитора
- **Сниппеты сложны** — Wayland намеренно блокирует инъекцию ввода; `zwp_virtual_keyboard_v1` работает не во всех приложениях
- **Управление окнами = Niri IPC** — фича непереносима на другие композиторы
- **layer-shell обязателен** — обычное GTK-окно нельзя показать поверх всего без него

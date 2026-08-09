# Briefing: Raycast-clone для Linux

## Проект
Пишу launcher-приложение — аналог Raycast v2 под Linux (NixOS + Niri + Wayland).
Стек: **Rust + GTK4 + gtk4-layer-shell**.

## Контекст
- ОС: NixOS, compositor: Niri (Wayland)
- Есть опыт с Rust (rmcp SDK, Exercism, async)
- Raycast v2 — реальный референс: изучил официальный мануал и таблицу фич

## MVP (в порядке приоритета)
1. **Layer-shell окно** — поверх всего, без декораций (`gtk4-layer-shell` crate)
2. **Root Search** — запуск приложений (парсинг `.desktop` из XDG + nix-store пути)
3. **File Search** — файлы/папки в том же поиске (`fd` backend или inotify-вотчер)
4. **Калькулятор** — inline, `libqalculate` или `evalexpr`
5. **Буфер обмена** — история через `wl-clipboard` watcher + SQLite
6. **Вызов по хоткею** — биндинг в конфиге Niri → `niri msg action spawn`

## Важные ограничения Wayland
- Глобальных хоткеев нет — вызов только через Niri config (`bind`)
- Сниппеты сложны: нужен `zwp_virtual_keyboard_v1`, работает не везде
- Управление окнами = `niri msg` IPC, непереносимо

## Что пропускаем в v1
Cloud Sync, Store расширений, Dictation, Focus, Inline Emoji Picker.

## Текущее состояние
Проект начат, базовый каркас на Rust + GTK4 уже пишется.

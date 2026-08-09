# Zeshicast Documentation Index

Добро пожаловать в единую директорию документации проекта **Zeshicast** — клавиатурного лаунчера нового поколения для Linux (Wayland / GTK4).

---

## 📚 Актуальная документация (Active Documentation)

### 🏗 Архитектура и Разработка
- [📄 Architecture & UI Design (`DESIGN.md`)](DESIGN.md) — Описание архитектурных решений ядра лаунчера, обработчиков действий и GTK4 UI.
- [📄 Developer Guide (`development.md`)](development.md) — Инструкция по сборке, тестированию (`cargo test --features gui`), линтингу и локальной разработке.
- [📄 GTK4 Design System (`design-system-plan.md`)](design-system-plan.md) — Полный план дизайн-системы, токенов CSS и компонентов UI.

### 🔒 Безопасность и Приватность
- [📄 Security Policy & Threat Model (`security.md`)](security.md) — Модель угроз, безопасное ограничение прав расширений и защита атомарных файлов.
- [📄 Privacy & Data Retention (`privacy.md`)](privacy.md) — Политики сохранения истории буфера обмена, фильтрация приватного режима и маскирование секрета.

### 🗺 План развития (Roadmaps & Features)
- [📄 Vicinae Parity Roadmap (`vicinae-parity-roadmap.md`)](vicinae-parity-roadmap.md) — Дорожная карта паритета с командными центрами (Vicinae/Raycast), метрики покрытия тестами.
- [📄 Linux Command Center Vision (`linux-command-center-plan.md`)](linux-command-center-plan.md) — Концепция превращения Zeshicast в полноценный системный командный центр.
- [📄 Raycast Linux Feature Matrix (`raycast-linux-features.md`)](raycast-linux-features.md) — Сравнительная матрица возможностей Zeshicast и Raycast.

---

## 🗄 Архив проверок и аудитов (Archive & Completed Audits)

Все исторические отчеты, аудиты кода, планы рефакторинга и проверки безопасности были на 100% выполнены в коде и перенесены в архив `docs/archive/`:

- [📁 docs/archive/ai_audits/](archive/ai_audits/README.md) — **Завершённые AI-аудиты**
  - [📄 code_review.md](archive/ai_audits/code_review.md) — Исправление TOCTOU, `statvfs`, двойного экранирования и дрифта таймера.
  - [📄 refactoring_audit.md](archive/ai_audits/refactoring_audit.md) — Завершённый рефакторинг (`RUST-001` — `RUST-006`).
- [📄 FULL_REVIEW.md](archive/FULL_REVIEW.md) — Полный технический обзор (2026-06-27).
- [📄 ACTION_PLAN_FULL_REVIEW.md](archive/ACTION_PLAN_FULL_REVIEW.md) — Полный исполнительный план действий (2026-06-27) — Все этапы P0–EXT закрыты.
- [📄 ACTION_PLAN_2026-06-18.md](archive/ACTION_PLAN_2026-06-18.md) — Ранний план безопасности и производительности (2026-06-18) — Закрыт.
- [📄 BACKLOG.md](archive/BACKLOG.md) — Исторический бэклог задач.

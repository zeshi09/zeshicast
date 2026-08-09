# Backlog — feature requests

Feature requests. (Security/perf review items live in
[ACTION_PLAN.md](ACTION_PLAN.md).)

---

## AI-1. Dynamic Ollama model list + on-screen switching — ✅ DONE (2026-06-25)
Implemented: `list_models` (`GET /api/tags`, `src/services/local_ai.rs`) fetched
off-thread; `ai_chat_view` model bar filled dynamically with a live refresh
button; clicking a model persists `ollama_model` via `set_preference`; falls back
to the first installed model when the configured one is missing.

- **Severity:** feature · **Recorded:** 2026-06-24
- **Where:** `src/ui/views.rs:303` (hardcoded list), `src/ui/views.rs:42`
  (`output`/bar), `src/services/local_ai.rs`, `src/search/web.rs:130`
- **Problem:** the AI screen renders a hardcoded model bar
  `["llama3.2:3b", "mistral:7b", "phi3:mini"]` with `llama3.2:3b` forced active,
  and the buttons have **no click handlers** — they don't reflect installed
  models and don't switch anything. The active model is hardcoded in the config
  (`ollama_model`).
- **Goal:**
  - Fetch the real installed models from Ollama: `GET {ollama_endpoint}/api/tags`
    → `{"models":[{"name":"…"}, …]}`. (For the OpenAI-compatible provider, the
    equivalent is `GET {ai_endpoint}/models` → `{"data":[{"id":"…"}]}`.)
  - Render the actual models in the bar (buttons or a dropdown); clicking one
    switches the active model live, no config edit required.
  - Default selection = currently configured model if present, else the first
    available; persist the choice back to the `ollama_model` preference.
  - Fetch off the main thread (like the layout/clipboard watchers) so the UI
    never blocks; handle "Ollama unreachable / no models" gracefully.
- **Acceptance:** opening the AI screen lists the models actually installed in
  Ollama; clicking one makes the next query use it; no model is hardcoded.

## AI-2. Markdown rendering for model messages — ✅ DONE (2026-06-25)
Implemented: lightweight Markdown → Pango-markup converter (`src/ui/markdown.rs`,
no new deps) covering bold/italic, inline + fenced code, headings and bullets;
the AI view streams plain text then re-renders the completed reply formatted via
`Label::set_markup`. Underscores left literal to spare `snake_case`.

- **Severity:** feature · **Recorded:** 2026-06-24
- **Where:** `src/ui/views.rs:42` (`output: gtk::Label`), set as plain text;
  responses stream via `ask_local_ai_streaming` (`src/services/local_ai.rs`).
- **Problem:** model replies are shown as raw plain text — `**bold**`,
  `` `code` ``, lists, headings and code fences are not formatted.
- **Goal:** render Markdown in the AI output. Lightweight path: convert Markdown
  → Pango markup (GtkLabel already supports Pango markup) covering bold/italic,
  inline code, code blocks (monospace), headings and lists. Decide how to handle
  **streaming**: either re-render the accumulated text on each token, or render
  formatted only once the response completes (plain text while streaming).
- **Acceptance:** a reply containing bold, inline code, a fenced code block and a
  list renders formatted rather than as literal Markdown.

---

### Notes
- AI-1 and AI-2 touch the same view (`ai_chat_view` in `views.rs`); reasonable
  to do together.
- Both should keep working with the existing streaming output path.

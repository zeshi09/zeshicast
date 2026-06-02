pub(crate) const KNOWN_PREFERENCES: &[(&str, &str)] = &[
    ("ui_font_family", "UI font family (restart required)"),
    ("ui_font_size", "UI base font size 12-22 (restart required)"),
    ("show_status_strip", "Show status strip (true/false)"),
    (
        "status_items",
        "Status strip items (clock,date,network,battery,audio,media)",
    ),
    ("dashboard_enabled", "Dashboard views enabled (true/false)"),
    ("network_enabled", "Network features enabled (true/false)"),
    ("media_enabled", "Media features enabled (true/false)"),
    (
        "notifications_enabled",
        "Notification features enabled (true/false)",
    ),
    ("ai_enabled", "AI features enabled (true/false)"),
    (
        "dashboard_poll_interval_ms",
        "Dashboard refresh interval (ms)",
    ),
    ("ai_provider", "AI provider (ollama/openai)"),
    ("ai_endpoint", "AI endpoint (OpenAI-compatible URL)"),
    ("ai_model", "AI model"),
    ("ai_api_key", "AI API key"),
    ("ollama_endpoint", "Ollama endpoint"),
    ("ollama_model", "Ollama model"),
    (
        "translate_endpoint",
        "Translate endpoint (LibreTranslate URL)",
    ),
    ("translate_api_key", "Translate API key"),
    (
        "translate_target",
        "Translate target language (e.g. en, ru, de)",
    ),
];

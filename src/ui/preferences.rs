pub(crate) const PREFERENCE_DEFAULTS: &[(&str, &str)] = &[
    ("ui_font_family", "Outfit, Inter, Noto Sans, sans-serif"),
    ("ui_font_size", "15"),
    ("ui_density", "compact"),
    ("ui_theme", "system"),
    ("show_status_strip", "true"),
    (
        "status_items",
        "clock,date,network,battery,audio,media,layout",
    ),
    ("dashboard_enabled", "true"),
    ("dashboard_poll_interval_ms", "1000"),
    ("network_enabled", "true"),
    ("media_enabled", "true"),
    ("notifications_enabled", "true"),
    ("notifications_history_enabled", "true"),
    ("clipboard_history_enabled", "true"),
    ("clipboard_private_mode", "false"),
    ("clipboard_retention", "100"),
    ("clipboard_capture_images", "true"),
    ("export_include_secrets", "false"),
    ("ai_enabled", "true"),
    ("ai_provider", "ollama"),
    ("ollama_endpoint", "http://localhost:11434"),
    ("ollama_model", "llama3.2:3b"),
    ("ai_endpoint", "https://api.openai.com/v1"),
    ("ai_model", "gpt-4o-mini"),
    ("ai_api_key", ""),
    ("translate_endpoint", "http://localhost:5000"),
    ("translate_api_key", ""),
    ("translate_target", "en"),
    ("script_dirs", "~/.config/zeshicast/scripts"),
];

pub(crate) const KNOWN_PREFERENCES: &[(&str, &str)] = &[
    ("ui_font_family", "UI font family (restart required)"),
    ("ui_font_size", "UI base font size 12-22 (restart required)"),
    (
        "ui_density",
        "Row density: comfortable (default) or compact",
    ),
    ("ui_theme", "Theme: system (default), dark, or light"),
    ("show_status_strip", "Show status strip (true/false)"),
    (
        "status_items",
        "Status strip items (clock,date,network,battery,audio,media,layout)",
    ),
    ("dashboard_enabled", "Dashboard views enabled (true/false)"),
    ("network_enabled", "Network features enabled (true/false)"),
    ("media_enabled", "Media features enabled (true/false)"),
    (
        "notifications_enabled",
        "Notification features enabled (true/false)",
    ),
    (
        "notifications_history_enabled",
        "Notification history enabled (true/false)",
    ),
    (
        "clipboard_history_enabled",
        "Clipboard history enabled (true/false)",
    ),
    (
        "clipboard_private_mode",
        "Pause clipboard capture without clearing existing history (true/false)",
    ),
    (
        "clipboard_retention",
        "Clipboard history retention count (1-1000)",
    ),
    (
        "clipboard_capture_images",
        "Capture clipboard images into local cache (true/false)",
    ),
    (
        "export_include_secrets",
        "Include API keys and secrets in config export (true/false)",
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
    ("script_dirs", "Script directories (comma-separated paths)"),
];

pub(crate) struct PrefSection {
    pub(crate) name: &'static str,
    pub(crate) keys: &'static [(&'static str, &'static str)],
}

pub(crate) const PREFERENCE_SECTIONS: &[PrefSection] = &[
    PrefSection {
        name: "General",
        keys: &[
            ("show_status_strip", "Show status strip"),
            ("status_items", "Status items"),
            ("dashboard_enabled", "Dashboard enabled"),
            ("dashboard_poll_interval_ms", "Refresh interval (ms)"),
        ],
    },
    PrefSection {
        name: "Appearance",
        keys: &[
            ("ui_font_family", "Font family"),
            ("ui_font_size", "Font size"),
            ("ui_density", "Row density"),
            ("ui_theme", "Theme"),
        ],
    },
    PrefSection {
        name: "Keyboard",
        keys: &[],
    },
    PrefSection {
        name: "Extensions",
        keys: &[
            ("network_enabled", "Network features"),
            ("media_enabled", "Media features"),
            ("notifications_enabled", "Notification features"),
            ("ai_enabled", "AI features"),
            ("ai_provider", "AI provider"),
            ("ollama_endpoint", "Ollama endpoint"),
            ("ollama_model", "Ollama model"),
            ("ai_endpoint", "AI/OpenAI endpoint"),
            ("ai_model", "AI model"),
            ("ai_api_key", "AI API key"),
            ("script_dirs", "Script directories"),
            ("translate_endpoint", "Translate endpoint"),
            ("translate_api_key", "Translate API key"),
            ("translate_target", "Translate target language"),
        ],
    },
    PrefSection {
        name: "Privacy",
        keys: &[
            ("clipboard_history_enabled", "Clipboard history"),
            ("clipboard_private_mode", "Pause clipboard capture"),
            ("clipboard_retention", "Clipboard retention"),
            ("clipboard_capture_images", "Clipboard images"),
            ("notifications_history_enabled", "Notification history"),
            ("export_include_secrets", "Export secrets"),
        ],
    },
    PrefSection {
        name: "About",
        keys: &[],
    },
];

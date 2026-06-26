use std::collections::HashMap;

use crate::{Action, ActionKind, HttpRequest};

pub(crate) fn execute_http_request(request: &HttpRequest) -> Option<String> {
    match request {
        HttpRequest::Translate {
            endpoint,
            text,
            target,
            api_key,
        } => {
            let agent = ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(5))
                .build();
            let body = serde_json::json!({
                "q": text,
                "source": "auto",
                "target": target,
                "api_key": api_key,
                "format": "text"
            });
            let resp = agent
                .post(&format!("{endpoint}/translate"))
                .set("Content-Type", "application/json")
                .send_json(body)
                .ok()?;
            let value: serde_json::Value = resp.into_json().ok()?;
            value
                .get("translatedText")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        }
        HttpRequest::AiChat {
            endpoint,
            model,
            query,
            api_key,
        } => {
            let agent = ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(30))
                .build();
            let body = serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": query}],
                "max_tokens": 500
            });
            let resp = agent
                .post(&format!("{endpoint}/chat/completions"))
                .set("Content-Type", "application/json")
                .set("Authorization", &format!("Bearer {api_key}"))
                .send_json(body)
                .ok()?;
            let value: serde_json::Value = resp.into_json().ok()?;
            value
                .get("choices")
                .and_then(|v| v.get(0))
                .and_then(|v| v.get("message"))
                .and_then(|v| v.get("content"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        }
        HttpRequest::LocalAiGenerate {
            endpoint,
            model,
            query,
        } => crate::ask_local_ai(
            &crate::LocalAiConfig {
                endpoint: endpoint.clone(),
                model: model.clone(),
            },
            query,
        )
        .ok(),
    }
}

pub(crate) fn search_ai(query: &str, preferences: &HashMap<String, String>) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    if !lower.starts_with("ai ") {
        return Vec::new();
    }
    let text = query.trim()[3..].trim();
    if text.is_empty() {
        return Vec::new();
    }
    let preview = if text.chars().count() > 40 {
        format!("{}...", text.chars().take(37).collect::<String>())
    } else {
        text.to_string()
    };

    let provider = preferences
        .get("ai_provider")
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| "ollama".to_string());

    if provider == "openai" {
        let endpoint = preferences
            .get("ai_endpoint")
            .cloned()
            .unwrap_or_else(|| "http://localhost:11434/v1".to_string());
        let model = preferences
            .get("ai_model")
            .cloned()
            .unwrap_or_else(|| "gemma4:e4b".to_string());
        let api_key = preferences.get("ai_api_key").cloned().unwrap_or_default();
        return vec![
            Action::new(
                "AI",
                format!("Ask AI: {preview}"),
                ActionKind::HttpCopy(HttpRequest::AiChat {
                    endpoint,
                    model: model.clone(),
                    query: text.to_string(),
                    api_key,
                }),
                980,
            )
            .with_subtitle(format!("{model} - response copied to clipboard"))
            .with_icon("dialog-question-symbolic"),
        ];
    }

    let endpoint = preferences
        .get("ollama_endpoint")
        .or_else(|| preferences.get("local_ai_endpoint"))
        .cloned()
        .unwrap_or_else(|| "http://localhost:11434".to_string());
    let model = preferences
        .get("ollama_model")
        .or_else(|| preferences.get("local_ai_model"))
        .or_else(|| preferences.get("ai_model"))
        .cloned()
        .unwrap_or_default();

    vec![
        Action::new(
            "AI",
            format!("Ask AI: {preview}"),
            ActionKind::HttpCopy(HttpRequest::LocalAiGenerate {
                endpoint,
                model: model.clone(),
                query: text.to_string(),
            }),
            980,
        )
        .with_subtitle(format!("{model} - response copied to clipboard"))
        .with_icon("dialog-question-symbolic"),
    ]
}

pub(crate) fn search_translate(query: &str, preferences: &HashMap<String, String>) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let text_part = if lower.starts_with("translate ") {
        query.trim()[10..].trim()
    } else if lower.starts_with("trans ") {
        query.trim()[6..].trim()
    } else {
        return Vec::new();
    };

    if text_part.is_empty() {
        return Vec::new();
    }

    let default_target = preferences
        .get("translate_target")
        .cloned()
        .unwrap_or_else(|| "en".to_string());

    let (text, target) = if let Some(pos) = text_part.rfind(" in ") {
        let suffix = &text_part[pos + 4..];
        let lang_len = suffix.chars().count();
        if (2..=3).contains(&lang_len) && suffix.chars().all(|c| c.is_ascii_alphabetic()) {
            (&text_part[..pos], suffix.to_string())
        } else {
            (text_part, default_target)
        }
    } else {
        (text_part, default_target)
    };

    if text.trim().is_empty() {
        return Vec::new();
    }

    let endpoint = preferences
        .get("translate_endpoint")
        .cloned()
        .unwrap_or_else(|| "https://libretranslate.com".to_string());
    let api_key = preferences
        .get("translate_api_key")
        .cloned()
        .unwrap_or_default();

    vec![
        Action::new(
            "Translate",
            format!("Translate: {text}"),
            ActionKind::HttpCopy(HttpRequest::Translate {
                endpoint,
                text: text.to_string(),
                target: target.clone(),
                api_key,
            }),
            980,
        )
        .with_subtitle(format!("to {target} \u{2014} result copied to clipboard"))
        .with_icon("preferences-desktop-locale-symbolic"),
    ]
}

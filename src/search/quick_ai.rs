//! Quick AI on the selected text (Q8.1): fix, translate or summarize what is
//! selected in any window, and paste the result over it.
//!
//! The row only names the task and the endpoint. The selection (Wayland
//! PRIMARY) is read when the row runs, never while searching, and is sent to
//! the endpoint configured for AI (Ollama, or any OpenAI-compatible one).

use std::collections::HashMap;

use crate::action::{Action, ActionKind, HttpRequest};
use crate::search::fuzzy_score;

/// Longest selection sent; a longer one is cut (a model would truncate it
/// anyway, and a stray select-all should not upload a whole document).
pub(crate) const MAX_SELECTION_CHARS: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickAiTask {
    Fix,
    Translate(String),
    Summarize,
}

impl QuickAiTask {
    fn title(&self) -> String {
        match self {
            Self::Fix => "AI: Fix Selected Text".to_string(),
            Self::Translate(language) => format!("AI: Translate Selection to {language}"),
            Self::Summarize => "AI: Summarize Selected Text".to_string(),
        }
    }

    fn keywords(&self) -> &'static str {
        match self {
            Self::Fix => "fix correct spelling grammar typo rewrite selection",
            Self::Translate(_) => "translate translation selection",
            Self::Summarize => "summarize summary tldr shorten selection",
        }
    }

    /// The prompt for `selection`, cut to [`MAX_SELECTION_CHARS`].
    pub(crate) fn prompt(&self, selection: &str) -> String {
        let text: String = selection.chars().take(MAX_SELECTION_CHARS).collect();
        let instruction = match self {
            Self::Fix => "Fix the spelling, grammar and punctuation of the text below. Keep its \
                          language, meaning, tone and line breaks. Reply with the corrected text \
                          only, without quotes or comments."
                .to_string(),
            Self::Translate(language) => format!(
                "Translate the text below into {language}. Keep its meaning, tone and line \
                 breaks. Reply with the translation only, without quotes or comments."
            ),
            Self::Summarize => "Summarize the text below in a few sentences, in the same \
                                language. Reply with the summary only."
                .to_string(),
        };
        format!("{instruction}\n\n{text}")
    }
}

/// The request for the configured endpoint, with an empty query to fill in
/// at run time; why not, when AI is not set up enough to answer.
pub(crate) fn request_template(
    preferences: &HashMap<String, String>,
) -> Result<HttpRequest, String> {
    let get = |key: &str| {
        preferences
            .get(key)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    let provider = get("ai_provider").unwrap_or_else(|| "ollama".to_string());
    if provider.eq_ignore_ascii_case("openai") {
        let endpoint = get("ai_endpoint").ok_or("set ai_endpoint in Settings → AI")?;
        let model = get("ai_model").ok_or("set ai_model in Settings → AI")?;
        return Ok(HttpRequest::AiChat {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            model,
            query: String::new(),
            api_key: get("ai_api_key").unwrap_or_default(),
        });
    }
    let model = get("ollama_model")
        .or_else(|| get("local_ai_model"))
        .ok_or("set ollama_model in Settings → AI (or ai_provider = openai)")?;
    Ok(HttpRequest::LocalAiGenerate {
        endpoint: get("ollama_endpoint")
            .or_else(|| get("local_ai_endpoint"))
            .unwrap_or_else(|| "http://localhost:11434".to_string()),
        model,
        query: String::new(),
    })
}

/// `template` with `prompt` as its query.
pub(crate) fn with_prompt(template: &HttpRequest, prompt: String) -> HttpRequest {
    let mut request = template.clone();
    match &mut request {
        HttpRequest::AiChat { query, .. } | HttpRequest::LocalAiGenerate { query, .. } => {
            *query = prompt;
        }
        HttpRequest::Translate { text, .. } => *text = prompt,
    }
    request
}

fn model_of(request: &HttpRequest) -> &str {
    match request {
        HttpRequest::AiChat { model, .. } | HttpRequest::LocalAiGenerate { model, .. } => model,
        HttpRequest::Translate { .. } => "",
    }
}

/// Rows for `query`: the tasks whose words it matches (`fix`, `translate`,
/// `summarize`, or anything after `ai `).
pub(crate) fn search_quick_ai(query: &str, preferences: &HashMap<String, String>) -> Vec<Action> {
    let trimmed = query.trim();
    let needle = trimmed
        .strip_prefix("ai ")
        .or_else(|| trimmed.strip_prefix("AI "))
        .unwrap_or(trimmed)
        .trim();
    if needle.chars().count() < 3 {
        return Vec::new();
    }
    let language = preferences
        .get("quick_ai_language")
        .or_else(|| preferences.get("translate_target"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "English".to_string());
    let tasks = [
        QuickAiTask::Fix,
        QuickAiTask::Translate(language),
        QuickAiTask::Summarize,
    ];
    let template = request_template(preferences);
    tasks
        .into_iter()
        .filter_map(|task| {
            let score = fuzzy_score(&format!("{} {}", task.title(), task.keywords()), needle)?;
            let title = task.title();
            let action = match &template {
                Ok(template) => Action::new(
                    "AI",
                    title,
                    ActionKind::QuickAi {
                        task,
                        request: template.clone(),
                    },
                    score + 120,
                )
                .with_subtitle(format!(
                    "{} · pastes the result over the selection",
                    model_of(template)
                )),
                Err(reason) => Action::new("AI", title, ActionKind::None, score + 120)
                    .with_subtitle(format!("AI is not set up: {reason}")),
            };
            Some(action.with_icon("tools-check-spelling-symbolic"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefs(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn prompts_name_the_task_and_carry_the_selection() {
        let prompt = QuickAiTask::Fix.prompt("teh text");
        assert!(prompt.starts_with("Fix the spelling"));
        assert!(prompt.ends_with("\n\nteh text"));
        assert!(
            QuickAiTask::Translate("Russian".into())
                .prompt("x")
                .contains("into Russian")
        );
        let long = "ж".repeat(MAX_SELECTION_CHARS + 50);
        let cut = QuickAiTask::Summarize.prompt(&long);
        assert_eq!(
            cut.chars().filter(|c| *c == 'ж').count(),
            MAX_SELECTION_CHARS
        );
    }

    #[test]
    fn the_request_follows_the_configured_provider() {
        let ollama = request_template(&prefs(&[("ollama_model", "qwen3:8b")])).unwrap();
        assert!(matches!(
            &ollama,
            HttpRequest::LocalAiGenerate { endpoint, model, .. }
                if endpoint == "http://localhost:11434" && model == "qwen3:8b"
        ));
        let openai = request_template(&prefs(&[
            ("ai_provider", "openai"),
            ("ai_endpoint", "https://api.example.com/v1/"),
            ("ai_model", "gpt-x"),
            ("ai_api_key", "sk-1"),
        ]))
        .unwrap();
        let filled = with_prompt(&openai, "hello".to_string());
        assert!(matches!(
            filled,
            HttpRequest::AiChat { endpoint, query, api_key, .. }
                if endpoint == "https://api.example.com/v1" && query == "hello" && api_key == "sk-1"
        ));
    }

    #[test]
    fn without_a_model_the_row_says_what_to_set() {
        assert!(
            request_template(&HashMap::new())
                .unwrap_err()
                .contains("ollama_model")
        );
        let rows = search_quick_ai("fix", &HashMap::new());
        assert_eq!(rows.len(), 1);
        assert!(matches!(rows[0].kind, ActionKind::None));
        assert!(
            rows[0].subtitle.contains("ollama_model"),
            "{}",
            rows[0].subtitle
        );
        assert!(
            request_template(&prefs(&[("ai_provider", "openai"), ("ai_model", "m")]))
                .unwrap_err()
                .contains("ai_endpoint")
        );
    }

    #[test]
    fn rows_name_the_task_not_the_text() {
        let preferences = prefs(&[("ollama_model", "m"), ("translate_target", "German")]);
        let rows = search_quick_ai("translate", &preferences);
        assert_eq!(rows[0].title, "AI: Translate Selection to German");
        // Searching never reads the selection: the row holds an empty query.
        let ActionKind::QuickAi { task, request } = &rows[0].kind else {
            panic!("a runnable row");
        };
        assert_eq!(*task, QuickAiTask::Translate("German".into()));
        assert!(matches!(request, HttpRequest::LocalAiGenerate { query, .. } if query.is_empty()));
        assert!(
            search_quick_ai("ai summ", &preferences)[0]
                .title
                .contains("Summarize")
        );
        assert!(search_quick_ai("fi", &preferences).is_empty());
    }
}

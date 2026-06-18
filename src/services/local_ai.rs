use std::io::{self, BufRead, BufReader};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[derive(Debug, Clone)]
pub struct LocalAiConfig {
    pub endpoint: String,
    pub model: String,
}

pub fn ask_local_ai(config: &LocalAiConfig, prompt: &str) -> io::Result<String> {
    if config.model.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "local AI model is not configured",
        ));
    }

    let endpoint = config.endpoint.trim_end_matches('/');
    let url = format!("{endpoint}/api/generate");
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(60))
        .build();
    let response = agent
        .post(&url)
        .send_json(serde_json::json!({
            "model": config.model,
            "prompt": prompt,
            "stream": false,
        }))
        .map_err(|error| io::Error::other(error.to_string()))?;

    let value: serde_json::Value = response
        .into_json()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;

    value
        .get("response")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing AI response"))
}

/// Streaming version: spawns a background thread that sends token chunks via `sender`.
/// Returns a cancel handle — set it to `true` to abort early.
pub fn ask_local_ai_streaming(
    config: LocalAiConfig,
    prompt: String,
    sender: std::sync::mpsc::SyncSender<StreamChunk>,
) -> Arc<AtomicBool> {
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = Arc::clone(&cancel);

    std::thread::spawn(move || {
        let endpoint = config.endpoint.trim_end_matches('/').to_string();
        let url = format!("{endpoint}/api/generate");

        // Connect timeout only — the response body is a long token stream, so we
        // must not impose an overall read timeout on it.
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(30))
            .build();
        let response = match agent.post(&url).send_json(serde_json::json!({
            "model": config.model,
            "prompt": prompt,
            "stream": true,
        })) {
            Ok(r) => r,
            Err(e) => {
                sender.send(StreamChunk::Error(e.to_string())).ok();
                return;
            }
        };

        let reader = BufReader::new(response.into_reader());
        for line in reader.lines() {
            if cancel_clone.load(Ordering::Relaxed) {
                sender.send(StreamChunk::Cancelled).ok();
                return;
            }
            let Ok(line) = line else { break };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if let Some(token) = value.get("response").and_then(|v| v.as_str()) {
                if !token.is_empty() {
                    sender.send(StreamChunk::Token(token.to_string())).ok();
                }
            }
            if value.get("done").and_then(|v| v.as_bool()).unwrap_or(false) {
                break;
            }
        }
        sender.send(StreamChunk::Done).ok();
    });

    cancel
}

#[derive(Debug)]
pub enum StreamChunk {
    Token(String),
    Done,
    Cancelled,
    Error(String),
}

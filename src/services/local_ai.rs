use std::io;

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
    let response = ureq::post(&url)
        .send_json(serde_json::json!({
            "model": config.model,
            "prompt": prompt,
            "stream": false,
        }))
        .map_err(|error| io::Error::new(io::ErrorKind::Other, error.to_string()))?;

    let value: serde_json::Value = response
        .into_json()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;

    value
        .get("response")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing AI response"))
}

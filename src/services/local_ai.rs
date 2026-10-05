use std::io::{self, BufRead, BufReader};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct LocalAiConfig {
    pub endpoint: String,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }
}

/// Cap for a JSON body read from a user-configured endpoint (N-7).
const MAX_AI_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;

/// Read and parse a JSON body under `cap` bytes. A user-configured endpoint
/// must not be able to make us buffer an unbounded response.
pub(crate) fn read_json_body(response: ureq::Response, cap: u64) -> Option<serde_json::Value> {
    let mut reader = response.into_reader();
    let body = crate::services::media::read_capped(&mut reader, cap).ok()?;
    if body.len() as u64 > cap {
        return None;
    }
    serde_json::from_slice(&body).ok()
}

/// List the models installed on an Ollama server (`GET {endpoint}/api/tags`).
/// Blocking — call off the UI thread. Returns an empty list if unreachable.
pub fn list_models(endpoint: &str) -> Vec<String> {
    let endpoint = endpoint.trim_end_matches('/');
    let url = format!("{endpoint}/api/tags");
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(5))
        .build();
    let Ok(response) = agent.get(&url).call() else {
        return Vec::new();
    };
    let Some(value) = read_json_body(response, MAX_AI_RESPONSE_BYTES) else {
        return Vec::new();
    };
    value["models"]
        .as_array()
        .map(|models| {
            models
                .iter()
                .filter_map(|model| model["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
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

    let value: serde_json::Value =
        read_json_body(response, MAX_AI_RESPONSE_BYTES).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "AI response missing or too large",
            )
        })?;

    value
        .get("response")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing AI response"))
}

/// Maximum silence between stream chunks before the reader gives up.
/// This is a per-socket-read timeout (ureq `timeout_read`), NOT a cap on the
/// total response duration, so long generations are unaffected. It bounds how
/// long the streaming thread can stay blocked when the server stops sending,
/// which in turn bounds how late a cancel request can be noticed.
const STREAM_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Multi-turn streaming chat version targeting Ollama `POST {endpoint}/api/chat`.
pub fn chat_local_ai_streaming(
    config: LocalAiConfig,
    messages: Vec<ChatMessage>,
    sender: std::sync::mpsc::SyncSender<StreamChunk>,
) -> Arc<AtomicBool> {
    chat_local_ai_streaming_with_timeout(config, messages, sender, STREAM_READ_TIMEOUT)
}

pub(crate) fn chat_local_ai_streaming_with_timeout(
    config: LocalAiConfig,
    messages: Vec<ChatMessage>,
    sender: std::sync::mpsc::SyncSender<StreamChunk>,
    read_timeout: std::time::Duration,
) -> Arc<AtomicBool> {
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = Arc::clone(&cancel);

    std::thread::spawn(move || {
        let endpoint = config.endpoint.trim_end_matches('/').to_string();
        let url = format!("{endpoint}/api/chat");

        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(30))
            .timeout_read(read_timeout)
            .build();
        let response = match agent.post(&url).send_json(serde_json::json!({
            "model": config.model,
            "messages": messages,
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
            let Ok(line) = line else {
                sender
                    .send(StreamChunk::Error(
                        "AI stream interrupted: the server stopped responding or the connection dropped"
                            .to_string(),
                    ))
                    .ok();
                return;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let token = value
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .or_else(|| value.get("response").and_then(|v| v.as_str()));
            if let Some(token) = token
                && !token.is_empty()
            {
                sender.send(StreamChunk::Token(token.to_string())).ok();
            }
            if value.get("done").and_then(|v| v.as_bool()).unwrap_or(false) {
                break;
            }
        }
        sender.send(StreamChunk::Done).ok();
    });

    cancel
}

#[derive(Debug, PartialEq, Eq)]
pub enum StreamChunk {
    Token(String),
    Done,
    Cancelled,
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::time::Duration;

    #[test]
    fn chat_stream_delivers_tokens_from_chat_api() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let server = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf); // drain the request head
            sock.write_all(
                b"HTTP/1.1 200 OK\r\n\
                  Content-Type: application/x-ndjson\r\n\
                  Transfer-Encoding: chunked\r\n\
                  \r\n",
            )
            .expect("write headers");

            let frame1 =
                b"{\"message\":{\"role\":\"assistant\",\"content\":\"hello \"},\"done\":false}\n";
            write!(sock, "{:x}\r\n", frame1.len()).expect("write chunk header");
            sock.write_all(frame1).expect("write frame1");
            sock.write_all(b"\r\n").expect("terminate frame1");

            let frame2 =
                b"{\"message\":{\"role\":\"assistant\",\"content\":\"world\"},\"done\":true}\n";
            write!(sock, "{:x}\r\n", frame2.len()).expect("write chunk header");
            sock.write_all(frame2).expect("write frame2");
            sock.write_all(b"\r\n").expect("terminate frame2");

            sock.write_all(b"0\r\n\r\n").expect("terminate chunked");
            sock.flush().expect("flush");
            let _ = sock.shutdown(std::net::Shutdown::Write);
            let mut drain = Vec::new();
            let _ = sock.read_to_end(&mut drain);
        });

        let config = LocalAiConfig {
            endpoint: format!("http://{addr}"),
            model: "chat-model".to_string(),
        };
        let messages = vec![ChatMessage::user("hi")];
        let (tx, rx) = std::sync::mpsc::sync_channel(64);
        let cancel =
            chat_local_ai_streaming_with_timeout(config, messages, tx, Duration::from_secs(5));

        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(StreamChunk::Token(token)) => assert_eq!(token, "hello "),
            other => panic!("expected token 1, got {other:?}"),
        }
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(StreamChunk::Token(token)) => assert_eq!(token, "world"),
            other => panic!("expected token 2, got {other:?}"),
        }
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(StreamChunk::Done) => {}
            other => panic!("expected Done, got {other:?}"),
        }

        drop(cancel);
        server.join().expect("server thread");
    }
}

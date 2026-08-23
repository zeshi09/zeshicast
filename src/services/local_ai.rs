use std::io::{self, BufRead, BufReader};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone)]
pub struct LocalAiConfig {
    pub endpoint: String,
    pub model: String,
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
    let Ok(value) = response.into_json::<serde_json::Value>() else {
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

    let value: serde_json::Value = response
        .into_json()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;

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

/// Streaming version: spawns a background thread that sends token chunks via `sender`.
/// Returns a cancel handle — set it to `true` to abort early.
pub fn ask_local_ai_streaming(
    config: LocalAiConfig,
    prompt: String,
    sender: std::sync::mpsc::SyncSender<StreamChunk>,
) -> Arc<AtomicBool> {
    ask_local_ai_streaming_with_timeout(config, prompt, sender, STREAM_READ_TIMEOUT)
}

fn ask_local_ai_streaming_with_timeout(
    config: LocalAiConfig,
    prompt: String,
    sender: std::sync::mpsc::SyncSender<StreamChunk>,
    read_timeout: std::time::Duration,
) -> Arc<AtomicBool> {
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = Arc::clone(&cancel);

    std::thread::spawn(move || {
        let endpoint = config.endpoint.trim_end_matches('/').to_string();
        let url = format!("{endpoint}/api/generate");

        // Connect timeout plus a per-read idle timeout. The response body is a
        // long token stream, so we must not impose an overall timeout on it;
        // `timeout_read` applies to each individual socket read, which aborts a
        // stream whose server has gone silent mid-response.
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(30))
            .timeout_read(read_timeout)
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
            // A read failure here means the connection broke or the server went
            // silent past the read timeout — surface it as an error so the UI
            // does not mistake a truncated answer for a complete one.
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
            if let Some(token) = value.get("response").and_then(|v| v.as_str())
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

#[derive(Debug)]
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

    /// Serves one HTTP response with a single NDJSON chunk, then stalls with the
    /// connection open. Verifies the read timeout turns the stall into a
    /// `StreamChunk::Error` (not `Done`) without any external network access.
    #[test]
    fn stalled_stream_yields_error_chunk() {
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
            let frame = b"{\"response\":\"hi\",\"done\":false}\n";
            write!(sock, "{:x}\r\n", frame.len()).expect("write chunk header");
            sock.write_all(frame).expect("write frame");
            sock.write_all(b"\r\n").expect("terminate frame");
            sock.flush().expect("flush");
            // Hold the connection open (silent server) until the client times out.
            std::thread::sleep(Duration::from_secs(5));
        });

        let config = LocalAiConfig {
            endpoint: format!("http://{addr}"),
            model: "test-model".to_string(),
        };
        let (tx, rx) = std::sync::mpsc::sync_channel(64);
        let cancel = ask_local_ai_streaming_with_timeout(
            config,
            "ping".to_string(),
            tx,
            Duration::from_millis(300),
        );

        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(StreamChunk::Token(token)) => assert_eq!(token, "hi"),
            other => panic!("expected first token chunk, got {other:?}"),
        }
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(StreamChunk::Error(message)) => {
                assert!(message.contains("interrupted"), "unexpected message: {message}");
            }
            other => panic!("expected Error chunk after read timeout, got {other:?}"),
        }

        drop(cancel);
        server.join().expect("server thread");
    }
}

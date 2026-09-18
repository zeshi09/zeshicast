use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::process::{CHILD_EXIT_GRACE, ChildGuard};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Value,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionSearchResultItem {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<String>,
    pub score: Option<i32>,
    pub open_url: Option<String>,
    pub copy_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionExecuteResult {
    pub success: bool,
    pub output: Option<String>,
    pub open_url: Option<String>,
    pub copy_text: Option<String>,
    pub notify: Option<String>,
}

/// Executes a JSON-RPC 2.0 request against an external executable with stdin/stdout isolation
/// and a strict timeout guard.
pub fn call_json_rpc(
    binary_path: &Path,
    method: &str,
    params: Value,
    timeout_ms: u64,
) -> Result<Value, String> {
    call_json_rpc_with_args(binary_path, &[], method, params, timeout_ms)
}

/// Upper bound on a single JSON-RPC response line (B-3: an extension must not be
/// able to exhaust our memory with one "line").
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// Executes a JSON-RPC 2.0 request against an external command with arguments,
/// stdin/stdout pipes, and a timeout guard that terminates the child process on
/// timeout.
///
/// The call is bounded from above: reading the response is capped at
/// [`MAX_RESPONSE_BYTES`] and at `timeout_ms`, and the child is always killed and
/// reaped before returning.
pub fn call_json_rpc_with_args(
    executable: &Path,
    args: &[&str],
    method: &str,
    params: Value,
    timeout_ms: u64,
) -> Result<Value, String> {
    let child = Command::new(executable)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", executable.display(), e))?;
    // From here on every exit path goes through the guard (see `crate::process`).
    let mut child = ChildGuard::new(child);

    let request = JsonRpcRequest::new(1, method, params);
    let request_json =
        serde_json::to_string(&request).map_err(|e| format!("Failed to serialize request: {e}"))?;

    if let Some(mut stdin) = child.child_mut().stdin.take() {
        writeln!(stdin, "{request_json}").map_err(|e| format!("Failed to write to stdin: {e}"))?;
        let _ = stdin.flush();
    }

    let stdout = child
        .child_mut()
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture stdout".to_string())?;

    let (tx, rx) = mpsc::channel();
    let reader_handle = std::thread::spawn(move || {
        // `take` bounds the allocation: a child that never sends a newline can
        // only make us read `MAX_RESPONSE_BYTES + 1` bytes.
        let mut reader = BufReader::new(stdout).take((MAX_RESPONSE_BYTES + 1) as u64);
        let mut buf = Vec::new();
        let res = reader.read_until(b'\n', &mut buf).map(|_| buf);
        let _ = tx.send(res);
    });

    let effective_timeout = if timeout_ms == 0 { 500 } else { timeout_ms };
    let line_result = rx.recv_timeout(Duration::from_millis(effective_timeout));

    let line = match line_result {
        Ok(Ok(line)) => line,
        Ok(Err(e)) => {
            // Kill first, then join: the reader only finishes once the pipe is
            // closed, which the child's death guarantees.
            drop(child);
            let _ = reader_handle.join();
            return Err(format!("Failed to read response: {e}"));
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            drop(child);
            let _ = reader_handle.join();
            return Err(format!(
                "JSON-RPC call to '{}' timed out after {}ms",
                executable.display(),
                effective_timeout
            ));
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            drop(child);
            let _ = reader_handle.join();
            return Err("Process closed stdout pipe without sending response".to_string());
        }
    };

    // The protocol is one-shot: the extension answers and exits. Give it a short
    // grace period and kill it otherwise — a lingering child must not keep the
    // caller (or an unreaped zombie) alive (B-3).
    child.reap(CHILD_EXIT_GRACE);
    let _ = reader_handle.join();

    if line.len() > MAX_RESPONSE_BYTES {
        return Err(format!(
            "Response too large from '{}' (> {MAX_RESPONSE_BYTES} bytes)",
            executable.display()
        ));
    }

    let line = String::from_utf8_lossy(&line);
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err("Empty response from extension".to_string());
    }

    let response: JsonRpcResponse = serde_json::from_str(trimmed)
        .map_err(|e| format!("Invalid JSON-RPC response: {e} (got: {trimmed})"))?;

    if let Some(err) = response.error {
        return Err(format!("RPC error {}: {}", err.code, err.message));
    }

    response
        .result
        .ok_or_else(|| "Empty RPC result".to_string())
}

pub fn search(
    binary_path: &Path,
    query: &str,
    timeout_ms: u64,
) -> Result<Vec<ExtensionSearchResultItem>, String> {
    let params = serde_json::json!({
        "query": query,
    });
    let result = call_json_rpc(binary_path, "search", params, timeout_ms)?;
    serde_json::from_value(result).map_err(|e| format!("Failed to parse search result: {e}"))
}

pub fn execute(
    binary_path: &Path,
    id: &str,
    arguments: Option<Value>,
    timeout_ms: u64,
) -> Result<ExtensionExecuteResult, String> {
    let params = serde_json::json!({
        "id": id,
        "arguments": arguments.unwrap_or(serde_json::json!({})),
    });
    let result = call_json_rpc(binary_path, "execute", params, timeout_ms)?;
    serde_json::from_value(result).map_err(|e| format!("Failed to parse execute result: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_call_json_rpc_with_args_echo() {
        let res = call_json_rpc_with_args(
            Path::new("sh"),
            &[
                "-c",
                "read line; echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"status\":\"ok\"}}'",
            ],
            "ping",
            serde_json::json!({}),
            1000,
        )
        .unwrap();

        assert_eq!(res["status"], "ok");
    }

    #[test]
    fn test_call_json_rpc_with_args_error() {
        let res = call_json_rpc_with_args(
            Path::new("sh"),
            &[
                "-c",
                "read line; echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32601,\"message\":\"Method not found\"}}'",
            ],
            "unknown",
            serde_json::json!({}),
            1000,
        );

        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Method not found"));
    }

    #[test]
    fn test_call_json_rpc_with_args_timeout() {
        let res = call_json_rpc_with_args(
            Path::new("sh"),
            &[
                "-c",
                "read line; sleep 2; echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}'",
            ],
            "slow",
            serde_json::json!({}),
            50,
        );

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.contains("timed out after 50ms"));
    }

    fn temp_pid_file(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "zeshicast-rpc-{tag}-{}-{nanos}.pid",
            std::process::id()
        ))
    }

    fn wait_for_pid(path: &Path) -> i32 {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(text) = std::fs::read_to_string(path)
                && let Ok(pid) = text.trim().parse::<i32>()
            {
                return pid;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("child never wrote its pid to {}", path.display());
    }

    fn pid_is_alive(pid: i32) -> bool {
        Path::new(&format!("/proc/{pid}")).exists()
    }

    #[test]
    fn lingering_child_is_reaped_after_answering() {
        // Answers correctly, then keeps running: the call must return promptly
        // and must not leave the process behind (B-3). `exec` keeps sh's pid so
        // the liveness check is exact.
        let pid_file = temp_pid_file("linger");
        let script = format!(
            "echo $$ > {}; read line; echo '{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"status\":\"ok\"}}}}'; exec sleep 60",
            pid_file.display()
        );

        let started = Instant::now();
        let res = call_json_rpc_with_args(
            Path::new("sh"),
            &["-c", &script],
            "ping",
            serde_json::json!({}),
            1000,
        )
        .expect("a valid response is still returned");
        let elapsed = started.elapsed();

        assert_eq!(res["status"], "ok");
        assert!(
            elapsed < Duration::from_secs(5),
            "call was not bounded: {elapsed:?}"
        );
        let pid = wait_for_pid(&pid_file);
        assert!(!pid_is_alive(pid), "child {pid} is still alive");
        std::fs::remove_file(&pid_file).ok();
    }

    #[test]
    fn oversized_response_is_rejected() {
        // One "line" well over the cap, with no newline at all: the reader must
        // stop at the cap instead of buffering it (M-9 style memory guard).
        let res = call_json_rpc_with_args(
            Path::new("sh"),
            &["-c", "read line; head -c 1200000 /dev/zero | tr '\\0' x"],
            "ping",
            serde_json::json!({}),
            5000,
        );

        let err = res.unwrap_err();
        assert!(err.contains("too large"), "unexpected error: {err}");
    }

    #[test]
    fn stdin_write_failure_still_reaps_the_child() {
        // The child closes its stdin and outlives the call: whichever error hits
        // (write failure or timeout) the process must be gone afterwards.
        let pid_file = temp_pid_file("nostdin");
        let script = format!("echo $$ > {}; exec 0<&-; exec sleep 60", pid_file.display());

        let res = call_json_rpc_with_args(
            Path::new("sh"),
            &["-c", &script],
            "ping",
            serde_json::json!({}),
            300,
        );

        let err = res.unwrap_err();
        assert!(
            err.contains("Failed to write to stdin") || err.contains("timed out"),
            "unexpected error: {err}"
        );
        let pid = wait_for_pid(&pid_file);
        assert!(!pid_is_alive(pid), "child {pid} is still alive");
        std::fs::remove_file(&pid_file).ok();
    }

    #[test]
    fn test_protocol_types_serde() {
        let exec_res: ExtensionExecuteResult = serde_json::from_str(
            r#"{
            "success": true,
            "output": "all good",
            "open_url": null,
            "copy_text": null,
            "notify": "done"
        }"#,
        )
        .unwrap();

        assert!(exec_res.success);
        assert_eq!(exec_res.output.as_deref(), Some("all good"));
        assert_eq!(exec_res.notify.as_deref(), Some("done"));
    }

    #[test]
    fn execute_asks_the_extension_over_its_own_protocol() {
        use std::process::Command;
        use std::sync::atomic::{AtomicUsize, Ordering};

        // The item contract: the id is sent to the extension's own `execute`
        // method, with the id in the params -- an id is never a command line.
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/extension-protocol-tests");
        std::fs::create_dir_all(&dir).unwrap();
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let stamp = NEXT.fetch_add(1, Ordering::Relaxed);
        let script = dir.join(format!("fake-extension-{}-{stamp}.sh", std::process::id()));
        let seen = dir.join(format!("seen-{}-{stamp}.txt", std::process::id()));

        let reply_line = r#"{"jsonrpc":"2.0","id":1,"result":{"success":true,"output":"ran","open_url":null,"copy_text":null,"notify":null}}"#;
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nread -r line\nprintf '%s\\n' \"$line\" > {seen}\nprintf '%s\\n' '{reply_line}'\n",
                seen = seen.display()
            ),
        )
        .unwrap();
        Command::new("chmod")
            .args(["+x", script.to_str().unwrap()])
            .status()
            .unwrap();

        let reply = execute(&script, "deploy", None, 5_000).expect("the extension answered");
        assert!(reply.success);
        assert_eq!(reply.output.as_deref(), Some("ran"));

        let request = std::fs::read_to_string(&seen).expect("the request was recorded");
        assert!(request.contains(r#""method":"execute""#), "got {request}");
        assert!(
            request.contains(r#""deploy""#),
            "the id must travel in the request: {request}"
        );
    }
}

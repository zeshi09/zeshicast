use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[allow(dead_code)]
pub const PARSE_ERROR: i64 = -32700;
#[allow(dead_code)]
pub const INVALID_REQUEST: i64 = -32600;
#[allow(dead_code)]
pub const METHOD_NOT_FOUND: i64 = -32601;
#[allow(dead_code)]
pub const INVALID_PARAMS: i64 = -32602;
#[allow(dead_code)]
pub const INTERNAL_ERROR: i64 = -32603;
#[allow(dead_code)]
pub const TIMEOUT_ERROR: i64 = -32000;
#[allow(dead_code)]
pub const CAPABILITY_DENIED: i64 = -32001;

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

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionInitParams {
    pub client: String,
    pub version: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionInitResult {
    pub id: String,
    pub name: String,
    pub version: String,
    pub protocol_version: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionCommandInfo {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub mode: Option<String>,
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

/// Verify that all `required` capabilities exist within `granted`.
#[allow(dead_code)]
pub fn check_capabilities(granted: &[String], required: &[String]) -> Result<(), String> {
    for req in required {
        if !granted.iter().any(|g| g.eq_ignore_ascii_case(req)) {
            return Err(format!(
                "Capability check failed: missing required capability '{req}'"
            ));
        }
    }
    Ok(())
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

/// Executes a JSON-RPC 2.0 request against an external command with arguments,
/// stdin/stdout pipes, and a timeout guard that terminates the child process on timeout.
pub fn call_json_rpc_with_args(
    executable: &Path,
    args: &[&str],
    method: &str,
    params: Value,
    timeout_ms: u64,
) -> Result<Value, String> {
    let mut child = Command::new(executable)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", executable.display(), e))?;

    let request = JsonRpcRequest::new(1, method, params);
    let request_json = serde_json::to_string(&request)
        .map_err(|e| format!("Failed to serialize request: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        writeln!(stdin, "{request_json}")
            .map_err(|e| format!("Failed to write to stdin: {e}"))?;
        let _ = stdin.flush();
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture stdout".to_string())?;

    let (tx, rx) = mpsc::channel();
    let reader_handle = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let res = reader.read_line(&mut line).map(|_| line);
        let _ = tx.send(res);
    });

    let effective_timeout = if timeout_ms == 0 { 500 } else { timeout_ms };
    let line_result = rx.recv_timeout(Duration::from_millis(effective_timeout));

    let line = match line_result {
        Ok(Ok(line)) => {
            let _ = child.wait();
            let _ = reader_handle.join();
            line
        }
        Ok(Err(e)) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader_handle.join();
            return Err(format!("Failed to read response: {e}"));
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader_handle.join();
            return Err(format!(
                "JSON-RPC call to '{}' timed out after {}ms",
                executable.display(),
                effective_timeout
            ));
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = child.wait();
            let _ = reader_handle.join();
            return Err("Process closed stdout pipe without sending response".to_string());
        }
    };

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

#[allow(dead_code)]
pub fn initialize(
    binary_path: &Path,
    client_version: &str,
    capabilities: &[String],
    timeout_ms: u64,
) -> Result<ExtensionInitResult, String> {
    let params = serde_json::json!({
        "client": "zeshicast",
        "version": client_version,
        "capabilities": capabilities,
    });
    let result = call_json_rpc(binary_path, "initialize", params, timeout_ms)?;
    serde_json::from_value(result).map_err(|e| format!("Failed to parse initialize result: {e}"))
}

#[allow(dead_code)]
pub fn list_commands(
    binary_path: &Path,
    timeout_ms: u64,
) -> Result<Vec<ExtensionCommandInfo>, String> {
    let result = call_json_rpc(binary_path, "list_commands", serde_json::json!({}), timeout_ms)?;
    serde_json::from_value(result).map_err(|e| format!("Failed to parse list_commands result: {e}"))
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

#[allow(dead_code)]
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

    #[test]
    fn test_check_capabilities_success() {
        let granted = vec!["shell".to_string(), "filesystem".to_string()];
        let required = vec!["shell".to_string()];
        assert!(check_capabilities(&granted, &required).is_ok());

        let required_case = vec!["SHELL".to_string()];
        assert!(check_capabilities(&granted, &required_case).is_ok());

        assert!(check_capabilities(&granted, &[]).is_ok());
    }

    #[test]
    fn test_check_capabilities_missing() {
        let granted = vec!["shell".to_string()];
        let required = vec!["network".to_string()];
        let err = check_capabilities(&granted, &required).unwrap_err();
        assert!(err.contains("missing required capability 'network'"));
    }

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
            &["-c", "read line; sleep 2; echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}'"],
            "slow",
            serde_json::json!({}),
            50,
        );

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.contains("timed out after 50ms"));
    }

    #[test]
    fn test_protocol_types_serde() {
        let init_res: ExtensionInitResult = serde_json::from_str(
            r#"{
            "id": "test.ext",
            "name": "Test Extension",
            "version": "1.0.0",
            "protocol_version": "2.0",
            "capabilities": ["shell"]
        }"#,
        )
        .unwrap();

        assert_eq!(init_res.id, "test.ext");
        assert_eq!(init_res.capabilities, vec!["shell"]);

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
}

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Action, ActionKind, ActionRisk};

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ExtensionCommandInfo {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<String>,
    pub keywords: Option<Vec<String>>,
}

/// Executes a JSON-RPC 2.0 request against an external executable with stdin/stdout isolation.
pub fn call_json_rpc(
    binary_path: &Path,
    method: &str,
    params: Value,
    _timeout_ms: u64,
) -> Result<Value, String> {
    let mut child = Command::new(binary_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", binary_path.display(), e))?;

    let request = JsonRpcRequest::new(1, method, params);
    let request_json = serde_json::to_string(&request)
        .map_err(|e| format!("Failed to serialize request: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        writeln!(stdin, "{}", request_json)
            .map_err(|e| format!("Failed to write to stdin: {e}"))?;
    }

    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();

    // Read the single-line JSON response
    reader
        .read_line(&mut line)
        .map_err(|e| format!("Failed to read response: {e}"))?;

    let response: JsonRpcResponse = serde_json::from_str(line.trim())
        .map_err(|e| format!("Invalid JSON-RPC response: {e} (got: {line})"))?;

    if let Some(err) = response.error {
        return Err(format!("RPC error {}: {}", err.code, err.message));
    }

    response.result.ok_or_else(|| "Empty RPC result".to_string())
}

/// Searches an extension via its `search` method over JSON-RPC 2.0.
pub fn search_extension(binary_path: &Path, extension_name: &str, query: &str) -> Vec<Action> {
    let params = serde_json::json!({
        "query": query,
    });

    let Ok(result) = call_json_rpc(binary_path, "search", params, 400) else {
        return Vec::new();
    };

    let Some(items) = result.as_array() else {
        return Vec::new();
    };

    let mut actions = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let subtitle = item.get("subtitle").and_then(|v| v.as_str()).unwrap_or(extension_name).to_string();
        let icon = item.get("icon").and_then(|v| v.as_str()).unwrap_or("system-run-symbolic").to_string();
        let cmd_id = item.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();

        if !title.is_empty() {
            actions.push(Action {
                category: format!("Extension: {extension_name}"),
                title,
                subtitle,
                icon_name: icon,
                risk: ActionRisk::Normal,
                kind: ActionKind::Launch(cmd_id),
                score: 80 - (i as i32 * 2),
            });
        }
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_rpc_request_serialization() {
        let req = JsonRpcRequest::new(42, "ping", serde_json::json!({"test": true}));
        let serialized = serde_json::to_string(&req).unwrap();
        assert!(serialized.contains(r#""jsonrpc":"2.0""#));
        assert!(serialized.contains(r#""id":42"#));
        assert!(serialized.contains(r#""method":"ping""#));
    }

    #[test]
    fn json_rpc_response_deserialization_success() {
        let raw = r#"{"jsonrpc":"2.0","id":1,"result":[{"id":"cmd1","title":"Deploy app"}]}"#;
        let resp: JsonRpcResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(resp.id, 1);
        assert!(resp.error.is_none());
        let res = resp.result.unwrap();
        assert_eq!(res[0]["title"], "Deploy app");
    }

    #[test]
    fn json_rpc_response_deserialization_error() {
        let raw = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(resp.id, 1);
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
        assert_eq!(err.message, "Method not found");
    }

    #[test]
    fn extension_command_info_serde() {
        let info = ExtensionCommandInfo {
            id: "cmd_run".to_string(),
            title: "Run Task".to_string(),
            subtitle: Some("Subtitle".to_string()),
            icon: Some("icon".to_string()),
            keywords: Some(vec!["run".to_string(), "task".to_string()]),
        };
        let serialized = serde_json::to_string(&info).unwrap();
        let deserialized: ExtensionCommandInfo = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.id, "cmd_run");
        assert_eq!(deserialized.title, "Run Task");
    }
}


use std::path::Path;

use crate::{Action, ActionKind, ActionRisk};

pub use crate::services::extension_protocol::*;

/// Searches an extension via its `search` method over JSON-RPC 2.0.
pub fn search_extension(binary_path: &Path, extension_name: &str, query: &str) -> Vec<Action> {
    let Ok(items) = search(binary_path, query, 400) else {
        return Vec::new();
    };

    let mut actions = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        let title = item.title;
        let subtitle = item.subtitle.unwrap_or_else(|| extension_name.to_string());
        let icon = item.icon.unwrap_or_else(|| "system-run-symbolic".to_string());
        let cmd_id = item.id;

        let kind = if let Some(url) = item.open_url {
            ActionKind::OpenUrl(url)
        } else if let Some(text) = item.copy_text {
            ActionKind::Copy(text)
        } else {
            ActionKind::Launch(cmd_id)
        };

        if !title.is_empty() {
            actions.push(Action {
                category: format!("Extension: {extension_name}"),
                title,
                subtitle,
                icon_name: icon,
                risk: ActionRisk::Normal,
                kind,
                script_mode: None,
                score: item.score.unwrap_or(80 - (i as i32 * 2)),
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
            mode: Some("view".to_string()),
        };
        let serialized = serde_json::to_string(&info).unwrap();
        let deserialized: ExtensionCommandInfo = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.id, "cmd_run");
        assert_eq!(deserialized.title, "Run Task");
        assert_eq!(deserialized.mode.as_deref(), Some("view"));
    }
}


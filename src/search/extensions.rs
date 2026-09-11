use std::path::Path;

use super::commands::{ActionIntent, gate_action_intent};
use crate::{Action, Capability, CapabilitySet};

pub use crate::services::extension_protocol::*;

/// Searches an extension via its `search` method over JSON-RPC 2.0.
///
/// `capabilities` are the permissions the manifest granted. Every item is
/// mapped through [`gate_action_intent`] — the same gate JSON-command output
/// uses — so a search result cannot escalate into an unsanctioned
/// launch/open_url/copy. Code-executing results are always downgraded to a
/// confirmed (Shell-risk) action.
pub fn search_extension(
    binary_path: &Path,
    extension_name: &str,
    query: &str,
    capabilities: &[Capability],
) -> Vec<Action> {
    let Ok(items) = search(binary_path, query, 400) else {
        return Vec::new();
    };

    let mut actions = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        let title = item.title;
        let mut subtitle = item.subtitle.unwrap_or_else(|| extension_name.to_string());
        let icon = item
            .icon
            .unwrap_or_else(|| "system-run-symbolic".to_string());
        let cmd_id = item.id;
        let score = item.score.unwrap_or(80 - (i as i32 * 2));

        let intent = if let Some(url) = item.open_url {
            ActionIntent::OpenUrl(url)
        } else if let Some(text) = item.copy_text {
            ActionIntent::Copy(text)
        } else {
            ActionIntent::Launch(cmd_id)
        };
        let gated = gate_action_intent(intent, capabilities);
        if let Some(denial) = &gated.denial {
            subtitle = if subtitle.is_empty() {
                denial.clone()
            } else {
                format!("{denial} - {subtitle}")
            };
        }

        if !title.is_empty() {
            actions.push(
                Action::new(
                    format!("Extension: {extension_name}"),
                    title,
                    gated.kind,
                    score,
                )
                .with_subtitle(subtitle)
                .with_icon(icon)
                .with_risk(gated.risk)
                .with_capabilities(CapabilitySet::new(capabilities.to_vec())),
            );
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
        let raw =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
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

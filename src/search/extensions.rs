use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::{Action, ActionKind, ShellCommand, fuzzy_score};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionManifest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "default_extension_icon")]
    pub icon: String,
    pub command: String,
    #[serde(default)]
    pub prefix: Option<String>,
}

fn default_extension_icon() -> String {
    "application-x-addon-symbolic".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: T,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse<T> {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<T>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionItem {
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub action_type: Option<String>,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchParams {
    pub query: String,
}

pub fn load_extension_manifests(dir: &Path) -> Vec<ExtensionManifest> {
    let mut manifests = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return manifests;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json")
            || path.file_name().and_then(|n| n.to_str()) == Some("manifest.json")
        {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(manifest) = serde_json::from_str::<ExtensionManifest>(&content) {
                    manifests.push(manifest);
                }
            }
        } else if path.is_dir() {
            let manifest_path = path.join("manifest.json");
            if let Ok(content) = fs::read_to_string(&manifest_path) {
                if let Ok(manifest) = serde_json::from_str::<ExtensionManifest>(&content) {
                    manifests.push(manifest);
                }
            }
        }
    }

    manifests
}

pub(crate) fn query_extension_jsonrpc(
    extension: &ExtensionManifest,
    query: &str,
) -> Option<Vec<ExtensionItem>> {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: 1,
        method: "search".to_string(),
        params: SearchParams {
            query: query.to_string(),
        },
    };

    let payload = serde_json::to_string(&req).ok()?;
    let mut child = Command::new(&extension.command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(payload.as_bytes());
        let _ = stdin.write_all(b"\n");
    }

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }

    let response: JsonRpcResponse<Vec<ExtensionItem>> =
        serde_json::from_slice(&output.stdout).ok()?;
    response.result
}

pub fn search_extensions(
    manifests: &[ExtensionManifest],
    query: &str,
) -> Vec<Action> {
    if manifests.is_empty() || query.trim().is_empty() {
        return Vec::new();
    }

    let query_lower = query.trim().to_lowercase();
    let mut actions = Vec::new();

    for ext in manifests {
        let matches_prefix = if let Some(prefix) = &ext.prefix {
            let p_lower = prefix.to_lowercase();
            if query_lower == p_lower {
                true
            } else if query_lower.starts_with(&format!("{p_lower} ")) {
                true
            } else {
                false
            }
        } else {
            false
        };

        let sub_query = if matches_prefix {
            if let Some(prefix) = &ext.prefix {
                query_lower
                    .strip_prefix(&prefix.to_lowercase())
                    .unwrap_or("")
                    .trim()
            } else {
                query_lower.as_str()
            }
        } else {
            query_lower.as_str()
        };

        // Query the JSON-RPC extension
        if let Some(items) = query_extension_jsonrpc(ext, sub_query) {
            for item in items {
                let score = if matches_prefix {
                    300
                } else {
                    fuzzy_score(&item.title, sub_query).unwrap_or(20)
                };

                let kind = match item.action_type.as_deref() {
                    Some("open_url") | Some("url") => ActionKind::OpenUrl(item.value.clone()),
                    Some("copy") => ActionKind::Copy(item.value.clone()),
                    Some("open_path") | Some("path") => {
                        ActionKind::OpenPath(PathBuf::from(&item.value))
                    }
                    _ => ActionKind::Shell(ShellCommand::new(&item.value)),
                };

                let icon = item.icon.as_deref().unwrap_or(&ext.icon);
                actions.push(
                    Action::new(
                        format!("Extension: {}", ext.name),
                        &item.title,
                        kind,
                        score,
                    )
                    .with_subtitle(item.subtitle)
                    .with_icon(icon),
                );
            }
        }
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_extension_manifest() {
        let json = r#"{
            "name": "GitHub Search",
            "description": "Quick search repositories",
            "command": "zeshicast-gh-extension",
            "prefix": "gh"
        }"#;

        let manifest: ExtensionManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "GitHub Search");
        assert_eq!(manifest.prefix.as_deref(), Some("gh"));
        assert_eq!(manifest.icon, "application-x-addon-symbolic");
    }

    #[test]
    fn serialize_jsonrpc_request() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 42,
            method: "search".to_string(),
            params: SearchParams {
                query: "rust".to_string(),
            },
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"id\":42"));
        assert!(json.contains("\"method\":\"search\""));
        assert!(json.contains("\"query\":\"rust\""));
    }
}

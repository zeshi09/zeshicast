use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::commands::{ActionIntent, gate_action_intent};
use crate::action::{Action, Capability, CapabilitySet};

pub use crate::services::extension_protocol::*;

/// How long an extension's reply to a given query is reused (P1.5b).
const SEARCH_CACHE_TTL: Duration = Duration::from_secs(30);
/// How many (extension, query) replies to keep.
const SEARCH_CACHE_CAP: usize = 64;
/// An extension gets this long to answer `search` before we give up on it.
const SEARCH_TIMEOUT_MS: u64 = 400;

type SearchKey = (PathBuf, String);

struct SearchCache {
    entries: Vec<(SearchKey, Instant, Vec<ExtensionSearchResultItem>)>,
}

fn search_cache() -> &'static Mutex<SearchCache> {
    static CACHE: OnceLock<Mutex<SearchCache>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(SearchCache {
            entries: Vec::new(),
        })
    })
}

/// An extension search as it was answered recently, if still fresh.
fn cached_search(binary_path: &Path, query: &str) -> Option<Vec<ExtensionSearchResultItem>> {
    cached_search_at(binary_path, query, Instant::now())
}

fn cached_search_at(
    binary_path: &Path,
    query: &str,
    now: Instant,
) -> Option<Vec<ExtensionSearchResultItem>> {
    let mut cache = search_cache().lock().ok()?;
    let position = cache
        .entries
        .iter()
        .position(|(key, _, _)| key.0 == binary_path && key.1 == query)?;
    let (_, captured_at, items) = &cache.entries[position];

    if now.duration_since(*captured_at) > SEARCH_CACHE_TTL {
        cache.entries.remove(position);
        return None;
    }
    Some(items.clone())
}

fn store_search(
    binary_path: &Path,
    query: &str,
    items: &[ExtensionSearchResultItem],
    now: Instant,
) {
    let Ok(mut cache) = search_cache().lock() else {
        return;
    };
    let key = (binary_path.to_path_buf(), query.to_string());
    cache.entries.retain(|(existing, _, _)| existing != &key);
    cache.entries.push((key, now, items.to_vec()));
    while cache.entries.len() > SEARCH_CACHE_CAP {
        cache.entries.remove(0);
    }
}

/// Ask an extension to search, reusing a recent identical answer (P1.5b).
///
/// `call_json_rpc` spawns the extension binary per call, and the palette asks
/// every extension again on every keystroke: with the search debounce settling
/// after 80 ms, one short query could spawn the same process dozens of times to
/// get the same answer. Failures are deliberately *not* cached -- the next
/// keystroke may well succeed, and caching an error would hide it for the whole
/// TTL.
fn cached_extension_search(binary_path: &Path, query: &str) -> Vec<ExtensionSearchResultItem> {
    if let Some(items) = cached_search(binary_path, query) {
        return items;
    }

    match search(binary_path, query, SEARCH_TIMEOUT_MS) {
        Ok(items) => {
            store_search(binary_path, query, &items, Instant::now());
            items
        }
        Err(_) => Vec::new(),
    }
}

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
    let items = cached_extension_search(binary_path, query);

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
            // The extension decides what the id means, so the id goes back to
            // the extension over its own JSON-RPC `execute` method instead of to
            // a shell (P1.5c). Still behind the manifest's `shell` capability
            // and a confirmation prompt.
            ActionIntent::ExtensionItem {
                binary: binary_path.to_path_buf(),
                id: cmd_id,
            }
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

    fn item(id: &str, title: &str) -> ExtensionSearchResultItem {
        ExtensionSearchResultItem {
            id: id.to_string(),
            title: title.to_string(),
            subtitle: None,
            icon: None,
            score: None,
            open_url: None,
            copy_text: None,
        }
    }

    #[test]
    fn a_recent_answer_is_reused_and_an_old_one_is_not() {
        let path = Path::new("/nonexistent/cache-ttl");
        let now = Instant::now();
        store_search(path, "deploy", &[item("a", "A")], now);

        assert_eq!(
            cached_search_at(path, "deploy", now + Duration::from_secs(5)),
            Some(vec![item("a", "A")])
        );
        assert_eq!(
            cached_search_at(
                path,
                "deploy",
                now + SEARCH_CACHE_TTL + Duration::from_millis(1)
            ),
            None,
            "an expired answer is not served"
        );
        assert_eq!(
            cached_search_at(path, "deploy", now),
            None,
            "and it is dropped, not merely ignored"
        );
    }

    #[test]
    fn the_cache_is_keyed_by_both_extension_and_query() {
        let path = Path::new("/nonexistent/cache-key");
        let now = Instant::now();
        store_search(path, "deploy", &[item("a", "A")], now);

        assert!(cached_search_at(path, "deploy", now).is_some());
        assert!(
            cached_search_at(path, "rollback", now).is_none(),
            "a different query is a different question"
        );
        assert!(
            cached_search_at(Path::new("/nonexistent/cache-key-2"), "deploy", now).is_none(),
            "so is a different extension"
        );
    }

    #[test]
    fn the_cache_does_not_grow_without_bound() {
        let path = Path::new("/nonexistent/cache-bound");
        let now = Instant::now();
        for index in 0..(SEARCH_CACHE_CAP + 10) {
            store_search(path, &format!("query-{index}"), &[item("a", "A")], now);
        }

        let cache = search_cache().lock().unwrap();
        assert!(
            cache.entries.len() <= SEARCH_CACHE_CAP,
            "{}",
            cache.entries.len()
        );
    }

    #[test]
    fn an_identical_query_does_not_spawn_the_extension_twice() {
        // The directory name must stay shell-safe: a `ThreadId(2)` in the path
        // makes the fake extension's own `>>` redirection a syntax error.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/tmp-ext-cache-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let marker = dir.join("calls");
        let script = dir.join("ext.sh");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nread line\necho x >> '{marker}'\nprintf '%s' '{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":[{{\"id\":\"x\",\"title\":\"X\"}}]}}'\n",
                marker = marker.display()
            ),
        )
        .expect("script");
        let status = std::process::Command::new("chmod")
            .arg("+x")
            .arg(&script)
            .status()
            .expect("chmod");
        assert!(status.success());

        let first = cached_extension_search(&script, "deploy");
        assert_eq!(first, vec![item("x", "X")], "the fake extension answered");
        let second = cached_extension_search(&script, "deploy");
        cached_extension_search(&script, "rollback");

        assert_eq!(second, first, "the same answer comes back");
        let calls = std::fs::read_to_string(&marker)
            .unwrap_or_default()
            .lines()
            .count();
        assert_eq!(
            calls, 2,
            "one spawn for the repeated query, one for the new one"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}

use std::io;
use std::process::Command;

use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotificationSnapshot {
    pub backend: Option<String>,
    pub count: Option<u32>,
    pub dnd: Option<bool>,
    pub history: Vec<NotificationEntrySnapshot>,
}

impl NotificationSnapshot {
    pub fn is_available(&self) -> bool {
        self.backend.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationEntrySnapshot {
    pub app_name: Option<String>,
    pub summary: String,
    pub body: Option<String>,
}

pub fn notification_snapshot() -> NotificationSnapshot {
    swaync_snapshot()
        .or_else(|_| dunst_snapshot())
        .unwrap_or_default()
}

fn swaync_snapshot() -> io::Result<NotificationSnapshot> {
    let count = command_stdout("swaync-client", &["--count"])
        .ok()
        .and_then(|output| parse_first_u32(&output));
    let dnd = command_stdout("swaync-client", &["--get-dnd"])
        .ok()
        .and_then(|output| parse_boolish(&output));

    if count.is_none() && dnd.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "swaync-client unavailable",
        ));
    }

    Ok(NotificationSnapshot {
        backend: Some("swaync".to_string()),
        count,
        dnd,
        history: Vec::new(),
    })
}

fn dunst_snapshot() -> io::Result<NotificationSnapshot> {
    let count = command_stdout("dunstctl", &["count", "history"])
        .ok()
        .and_then(|output| parse_first_u32(&output));
    let dnd = command_stdout("dunstctl", &["is-paused"])
        .ok()
        .and_then(|output| parse_boolish(&output));
    let history = command_stdout("dunstctl", &["history"])
        .ok()
        .and_then(|output| parse_dunst_history(&output).ok())
        .unwrap_or_default();

    if count.is_none() && dnd.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "dunstctl unavailable",
        ));
    }

    Ok(NotificationSnapshot {
        backend: Some("dunst".to_string()),
        count,
        dnd,
        history,
    })
}

fn command_stdout(program: &str, args: &[&str]) -> io::Result<String> {
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        return Err(io::Error::new(io::ErrorKind::Other, "command failed"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn parse_first_u32(output: &str) -> Option<u32> {
    output
        .split(|character: char| !character.is_ascii_digit())
        .find(|part| !part.is_empty())
        .and_then(|part| part.parse().ok())
}

fn parse_boolish(output: &str) -> Option<bool> {
    match output.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

fn parse_dunst_history(output: &str) -> serde_json::Result<Vec<NotificationEntrySnapshot>> {
    let value = serde_json::from_str::<Value>(output)?;
    let mut history = Vec::new();
    collect_dunst_notifications(&value, &mut history);
    Ok(history)
}

fn collect_dunst_notifications(value: &Value, history: &mut Vec<NotificationEntrySnapshot>) {
    match value {
        Value::Object(object) => {
            if let Some(summary) = object.get("summary").and_then(variant_string) {
                history.push(NotificationEntrySnapshot {
                    app_name: object.get("appname").and_then(variant_string),
                    summary,
                    body: object.get("body").and_then(variant_string),
                });
                return;
            }

            for child in object.values() {
                collect_dunst_notifications(child, history);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_dunst_notifications(child, history);
            }
        }
        _ => {}
    }
}

fn variant_string(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return non_empty_string(text);
    }

    let object = value.as_object()?;
    object
        .get("data")
        .and_then(Value::as_str)
        .or_else(|| object.get("value").and_then(Value::as_str))
        .and_then(non_empty_string)
}

fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_parser_extracts_first_number() {
        assert_eq!(parse_first_u32("12"), Some(12));
        assert_eq!(parse_first_u32("count: 7 notifications"), Some(7));
        assert_eq!(parse_first_u32("none"), None);
    }

    #[test]
    fn boolish_parser_handles_common_outputs() {
        assert_eq!(parse_boolish("true"), Some(true));
        assert_eq!(parse_boolish("off"), Some(false));
        assert_eq!(parse_boolish("unknown"), None);
    }

    #[test]
    fn dunst_history_parser_extracts_variant_objects() {
        let history = parse_dunst_history(
            r#"{
              "type": "aa{sv}",
              "data": [[{
                "appname": {"type": "s", "data": "Mail"},
                "summary": {"type": "s", "data": "New message"},
                "body": {"type": "s", "data": "Project update"}
              }]]
            }"#,
        )
        .unwrap();

        assert_eq!(
            history,
            vec![NotificationEntrySnapshot {
                app_name: Some("Mail".to_string()),
                summary: "New message".to_string(),
                body: Some("Project update".to_string()),
            }]
        );
    }
}

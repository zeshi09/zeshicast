use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::Zeshicast;
use gtk::glib;
use gtk::prelude::*;

pub(super) fn ask_ai_from_view(
    launcher: &Rc<RefCell<Zeshicast>>,
    ai_chat_view: &crate::ui::AiChatView,
) {
    let prompt = ai_chat_view.input.text().trim().to_string();
    if prompt.is_empty() {
        return;
    }

    ai_chat_view.output.set_text("Thinking...");
    let config = local_ai_config(launcher);
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = crate::ask_local_ai(&config, &prompt).map_err(|error| error.to_string());
        let _ = sender.send(result);
    });

    let ai_chat_view = ai_chat_view.clone();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        match receiver.try_recv() {
            Ok(Ok(answer)) => {
                ai_chat_view.output.set_text(&answer);
                glib::ControlFlow::Break
            }
            Ok(Err(error)) => {
                ai_chat_view.output.set_text(&error);
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                ai_chat_view.output.set_text("AI request failed");
                glib::ControlFlow::Break
            }
        }
    });
}

pub(super) fn ai_snippet_name(prompt: &str) -> String {
    let mut name = prompt.trim().chars().take(48).collect::<String>();
    if name.is_empty() {
        name = "AI answer".to_string();
    }
    name
}

pub(super) fn preference_enabled(
    launcher: &Rc<RefCell<Zeshicast>>,
    key: &str,
    default_value: bool,
) -> bool {
    launcher
        .borrow()
        .get_preferences()
        .get(key)
        .and_then(|value| parse_bool_preference(value))
        .unwrap_or(default_value)
}

pub(super) fn preference_duration_ms(
    launcher: &Rc<RefCell<Zeshicast>>,
    key: &str,
    default_value: u64,
) -> Duration {
    let milliseconds = launcher
        .borrow()
        .get_preferences()
        .get(key)
        .and_then(|value| parse_duration_ms_preference(value))
        .unwrap_or(default_value);
    Duration::from_millis(milliseconds)
}

pub(super) fn preference_list(
    launcher: &Rc<RefCell<Zeshicast>>,
    key: &str,
    default_value: &[&str],
) -> Vec<String> {
    launcher
        .borrow()
        .get_preferences()
        .get(key)
        .map(|value| parse_list_preference(value))
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| {
            default_value
                .iter()
                .map(|value| value.to_string())
                .collect()
        })
}

fn local_ai_config(launcher: &Rc<RefCell<Zeshicast>>) -> crate::LocalAiConfig {
    let preferences = launcher.borrow().get_preferences().clone();
    let endpoint = preferences
        .get("ollama_endpoint")
        .or_else(|| preferences.get("local_ai_endpoint"))
        .cloned()
        .unwrap_or_else(|| "http://localhost:11434".to_string());
    let model = preferences
        .get("ollama_model")
        .or_else(|| preferences.get("local_ai_model"))
        .or_else(|| preferences.get("ai_model"))
        .cloned()
        .unwrap_or_default();
    crate::LocalAiConfig { endpoint, model }
}

fn parse_bool_preference(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

fn parse_duration_ms_preference(value: &str) -> Option<u64> {
    value
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|value| *value >= 100)
}

fn parse_list_preference(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| !part.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bool_preferences_accept_common_values() {
        assert_eq!(parse_bool_preference("true"), Some(true));
        assert_eq!(parse_bool_preference("off"), Some(false));
        assert_eq!(parse_bool_preference("unknown"), None);
    }

    #[test]
    fn ai_snippet_name_has_fallback() {
        assert_eq!(ai_snippet_name(""), "AI answer");
        assert_eq!(ai_snippet_name("short prompt"), "short prompt");
    }

    #[test]
    fn duration_preferences_have_floor_and_default() {
        assert_eq!(parse_duration_ms_preference("50"), None);
        assert_eq!(parse_duration_ms_preference("750"), Some(750));
        assert_eq!(parse_duration_ms_preference("bad"), None);
    }

    #[test]
    fn list_preferences_parse_comma_separated_values() {
        assert_eq!(
            parse_list_preference("clock, date,network"),
            vec!["clock", "date", "network"]
        );
        assert!(parse_list_preference(" , ").is_empty());
    }
}

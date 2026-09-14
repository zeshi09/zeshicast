use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use crate::Zeshicast;
use crate::services::local_ai::{ChatMessage, StreamChunk, chat_local_ai_streaming};
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
    if !can_start_request(ai_chat_view.streaming.get(), &prompt) {
        return;
    }
    ai_chat_view.streaming.set(true);

    // Clear input entry so the user is ready to type their next query
    ai_chat_view.input.set_text("");

    // Create and append user message bubble
    let user_lbl = gtk::Label::new(Some(&prompt));
    user_lbl.add_css_class("ai-message-user");
    user_lbl.set_wrap(true);
    user_lbl.set_xalign(1.0);
    user_lbl.set_selectable(true);
    ai_chat_view.messages_box.append(&user_lbl);

    // Create and append assistant response bubble
    let assistant_lbl = gtk::Label::new(Some("Thinking…"));
    assistant_lbl.add_css_class("ai-message-assistant");
    assistant_lbl.set_wrap(true);
    assistant_lbl.set_xalign(0.0);
    assistant_lbl.set_selectable(true);
    ai_chat_view.messages_box.append(&assistant_lbl);

    // Keep output pointing to latest assistant label content
    ai_chat_view.output.set_text("");

    // UI: enter "thinking" state
    ai_chat_view.status.set_text("Thinking…");
    ai_chat_view.status.set_visible(true);
    ai_chat_view.ask.set_visible(false);
    ai_chat_view.stop.set_visible(true);
    ai_chat_view.copy.set_sensitive(false);
    ai_chat_view.save.set_sensitive(false);

    // Record user message in history
    ai_chat_view
        .history
        .borrow_mut()
        .push(ChatMessage::user(&prompt));

    let messages = ai_chat_view.history.borrow().clone();
    let config = local_ai_config(launcher);
    let (tx, rx) = mpsc::sync_channel::<StreamChunk>(64);
    let cancel_flag = chat_local_ai_streaming(config, messages, tx);

    // Hand the new flag to the stop button's single, persistent handler.
    *ai_chat_view.cancel.borrow_mut() = Some(cancel_flag.clone());

    let ai_chat_view = ai_chat_view.clone();
    let assistant_lbl = assistant_lbl.clone();
    let accumulated = Rc::new(RefCell::new(String::new()));
    glib::timeout_add_local(Duration::from_millis(30), move || {
        loop {
            match rx.try_recv() {
                Ok(StreamChunk::Token(token)) => {
                    accumulated.borrow_mut().push_str(&token);
                    // Show streaming cursor at end of text
                    assistant_lbl.set_text(&format!("{}▋", *accumulated.borrow()));
                    ai_chat_view.output.set_text(&accumulated.borrow());
                }
                Ok(StreamChunk::Done) => {
                    let text = accumulated.borrow().clone();
                    assistant_lbl.set_markup(&super::markdown::to_pango_markup(&text));
                    ai_chat_view.output.set_text(&text);
                    ai_chat_view
                        .history
                        .borrow_mut()
                        .push(ChatMessage::assistant(text));
                    finish_ai_view(&ai_chat_view);
                    return glib::ControlFlow::Break;
                }
                Ok(StreamChunk::Cancelled) => {
                    let text = accumulated.borrow().clone();
                    assistant_lbl.set_markup(&super::markdown::to_pango_markup(&text));
                    ai_chat_view.output.set_text(&text);
                    if !text.is_empty() {
                        ai_chat_view
                            .history
                            .borrow_mut()
                            .push(ChatMessage::assistant(text));
                    }
                    ai_chat_view.status.set_text("Cancelled");
                    finish_ai_view(&ai_chat_view);
                    return glib::ControlFlow::Break;
                }
                Ok(StreamChunk::Error(e)) => {
                    assistant_lbl.set_text(&e);
                    ai_chat_view.output.set_text(&e);
                    // The error bubble is on screen, so it belongs in the history
                    // too: popping the user's prompt instead (what this used to
                    // do) dropped the question while leaving it visible, and the
                    // next request then went to the model without it.
                    ai_chat_view.history.borrow_mut().push(error_reply(&e));
                    finish_ai_view(&ai_chat_view);
                    return glib::ControlFlow::Break;
                }
                Err(mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    let text = accumulated.borrow().clone();
                    assistant_lbl.set_markup(&super::markdown::to_pango_markup(&text));
                    ai_chat_view.output.set_text(&text);
                    if !text.is_empty() {
                        ai_chat_view
                            .history
                            .borrow_mut()
                            .push(ChatMessage::assistant(text));
                    }
                    finish_ai_view(&ai_chat_view);
                    return glib::ControlFlow::Break;
                }
            }
        }
    });
}

fn finish_ai_view(view: &crate::ui::AiChatView) {
    view.streaming.set(false);
    *view.cancel.borrow_mut() = None;
    view.status.set_visible(false);
    view.ask.set_visible(true);
    view.stop.set_visible(false);
    view.copy.set_sensitive(true);
    view.save.set_sensitive(true);
}

/// Whether a question may be sent now (M-8).
///
/// An empty prompt has nothing to send, and starting a second request while one
/// is streaming pushes a second set of bubbles, interleaves both streams into the
/// same labels, and sends the model a half-answered conversation.
fn can_start_request(streaming: bool, prompt: &str) -> bool {
    !streaming && !prompt.is_empty()
}

/// The assistant turn to record when the request failed (M-8).
fn error_reply(error: &str) -> ChatMessage {
    ChatMessage::assistant(format!("[error] {error}"))
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
    #[test]
    fn a_second_request_is_refused_while_one_is_streaming() {
        assert!(can_start_request(false, "why is the sky blue?"));
        assert!(
            !can_start_request(true, "and why is it blue?"),
            "two streams would write into the same labels"
        );
        assert!(!can_start_request(false, ""), "nothing to ask");
    }

    #[test]
    fn a_failed_request_keeps_the_prompt_it_was_asked() {
        let prompt = "why is the sky blue?";
        let mut history = vec![ChatMessage::user(prompt)];
        history.push(error_reply("connection refused"));

        assert_eq!(
            history.first().map(|m| m.content.as_str()),
            Some(prompt),
            "the question is still on screen, so it stays in the history"
        );
        assert_eq!(history.len(), 2, "the failure is part of the transcript");
        assert!(
            history
                .last()
                .is_some_and(|m| m.content == "[error] connection refused"),
            "the model is told the turn failed instead of seeing an answer"
        );
    }
}

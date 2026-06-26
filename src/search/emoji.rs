use crate::{Action, ActionKind, fuzzy_score};

const EMOJI_DATA: &[(&str, &str, &str)] = &[
    // (emoji, name, category)
    ("😀", "grinning face", "smileys"),
    ("😂", "face with tears of joy", "smileys"),
    ("🤣", "rolling on the floor laughing", "smileys"),
    ("😊", "smiling face with smiling eyes", "smileys"),
    ("😍", "smiling face with heart eyes", "smileys"),
    ("🥰", "smiling face with hearts", "smileys"),
    ("😎", "smiling face with sunglasses", "smileys"),
    ("🤔", "thinking face", "smileys"),
    ("😢", "crying face", "smileys"),
    ("😭", "loudly crying face", "smileys"),
    ("😡", "enraged face", "smileys"),
    ("🥳", "partying face", "smileys"),
    ("😴", "sleeping face", "smileys"),
    ("🫡", "saluting face", "smileys"),
    ("🫠", "melting face", "smileys"),
    ("🙃", "upside down face", "smileys"),
    ("👍", "thumbs up", "gestures"),
    ("👎", "thumbs down", "gestures"),
    ("👋", "waving hand", "gestures"),
    ("🤝", "handshake", "gestures"),
    ("👏", "clapping hands", "gestures"),
    ("🙏", "folded hands", "gestures"),
    ("💪", "flexed biceps", "gestures"),
    ("✌️", "victory hand", "gestures"),
    ("🖕", "middle finger", "gestures"),
    ("👀", "eyes", "body"),
    ("👁️", "eye", "body"),
    ("🧠", "brain", "body"),
    ("❤️", "red heart", "symbols"),
    ("🧡", "orange heart", "symbols"),
    ("💛", "yellow heart", "symbols"),
    ("💚", "green heart", "symbols"),
    ("💙", "blue heart", "symbols"),
    ("💜", "purple heart", "symbols"),
    ("🖤", "black heart", "symbols"),
    ("💔", "broken heart", "symbols"),
    ("⭐", "star", "symbols"),
    ("🔥", "fire", "symbols"),
    ("✅", "check mark", "symbols"),
    ("❌", "cross mark", "symbols"),
    ("⚠️", "warning", "symbols"),
    ("💡", "light bulb", "symbols"),
    ("🔑", "key", "symbols"),
    ("🔒", "locked", "symbols"),
    ("🔓", "unlocked", "symbols"),
    ("🎉", "party popper", "celebration"),
    ("🎊", "confetti ball", "celebration"),
    ("🎁", "wrapped gift", "celebration"),
    ("🏆", "trophy", "celebration"),
    ("🥇", "first place medal", "celebration"),
    ("🎯", "bullseye", "celebration"),
    ("🚀", "rocket", "travel"),
    ("✈️", "airplane", "travel"),
    ("🚗", "automobile", "travel"),
    ("🚌", "bus", "travel"),
    ("🚂", "locomotive", "travel"),
    ("🐶", "dog face", "animals"),
    ("🐱", "cat face", "animals"),
    ("🦊", "fox", "animals"),
    ("🐻", "bear", "animals"),
    ("🐧", "penguin", "animals"),
    ("🦀", "crab", "animals"),
    ("🦄", "unicorn", "animals"),
    ("🌱", "seedling", "nature"),
    ("🌲", "evergreen tree", "nature"),
    ("🌸", "cherry blossom", "nature"),
    ("🍎", "red apple", "food"),
    ("🍕", "pizza", "food"),
    ("☕", "hot beverage coffee", "food"),
    ("🍺", "beer mug", "food"),
    ("🍰", "shortcake", "food"),
    ("💻", "laptop", "technology"),
    ("🖥️", "desktop computer", "technology"),
    ("⌨️", "keyboard", "technology"),
    ("🖱️", "computer mouse", "technology"),
    ("📱", "mobile phone", "technology"),
    ("📡", "satellite antenna", "technology"),
    ("🔧", "wrench", "tools"),
    ("🔨", "hammer", "tools"),
    ("⚙️", "gear", "tools"),
    ("🧰", "toolbox", "tools"),
    ("📋", "clipboard", "office"),
    ("📎", "paperclip", "office"),
    ("🗒️", "spiral notepad", "office"),
    ("📁", "file folder", "office"),
    ("📊", "bar chart", "office"),
    ("🔍", "magnifying glass left", "office"),
    ("✏️", "pencil", "office"),
    ("📌", "pushpin", "office"),
    ("☀️", "sun", "weather"),
    ("🌙", "crescent moon", "weather"),
    ("⛅", "sun behind cloud", "weather"),
    ("🌧️", "cloud with rain", "weather"),
    ("❄️", "snowflake", "weather"),
    ("⚡", "high voltage lightning", "weather"),
    ("🎵", "musical note", "music"),
    ("🎸", "guitar", "music"),
    ("🎹", "musical keyboard", "music"),
    ("🎤", "microphone", "music"),
    ("🎧", "headphone", "music"),
    ("⚽", "soccer ball", "sports"),
    ("🏀", "basketball", "sports"),
    ("🎮", "video game", "sports"),
    ("🏋️", "person lifting weights", "sports"),
    ("💬", "speech balloon", "communication"),
    ("📧", "e-mail", "communication"),
    ("📞", "telephone receiver", "communication"),
    ("🔔", "bell", "communication"),
];

pub(crate) fn search_emoji(query: &str) -> Vec<Action> {
    let lower = query.trim().to_lowercase();
    let explicit = lower.starts_with("emoji ") || lower.starts_with(": ");
    let bare_colon = lower.starts_with(':') && !lower.starts_with(": ");

    let search_term = if explicit {
        query.splitn(2, ' ').nth(1).unwrap_or("").trim()
    } else if bare_colon {
        lower[1..].trim()
    } else {
        return Vec::new();
    };

    if search_term.is_empty() {
        return EMOJI_DATA
            .iter()
            .take(20)
            .map(|(emoji, name, cat)| emoji_action(emoji, name, cat, 100))
            .collect();
    }

    let mut matches: Vec<(i32, Action)> = EMOJI_DATA
        .iter()
        .filter_map(|(emoji, name, cat)| {
            let text = format!("{name} {cat}");
            let score = fuzzy_score(&text, search_term)?;
            Some((score, emoji_action(emoji, name, cat, score)))
        })
        .collect();

    matches.sort_by(|a, b| b.0.cmp(&a.0));
    matches.truncate(30);
    matches.into_iter().map(|(_, a)| a).collect()
}

fn emoji_action(emoji: &str, name: &str, category: &str, score: i32) -> Action {
    crate::Action::new("Emoji", emoji, ActionKind::Copy(emoji.to_string()), score)
        .with_subtitle(format!("{name}  ·  {category}"))
        .with_icon("face-smile-symbolic")
}

#[cfg(feature = "gui")]
pub fn emoji_data() -> &'static [(&'static str, &'static str, &'static str)] {
    EMOJI_DATA
}

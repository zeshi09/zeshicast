use std::process::Command;
use std::thread;
use std::time::Duration;

pub fn is_wtype_available() -> bool {
    Command::new("which")
        .arg("wtype")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

pub fn type_text_via_wtype(text: &str) {
    let text = text.to_string();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        Command::new("wtype").arg(&text).spawn().ok();
    });
}

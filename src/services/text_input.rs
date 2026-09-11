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
        // Fire-and-forget, but the child is remembered so it is reaped instead
        // of lingering as a zombie (M-9).
        crate::process::spawn_detached(Command::new("wtype").arg(&text)).ok();
    });
}

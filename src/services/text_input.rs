use std::thread;
use std::time::Duration;

pub fn is_wtype_available() -> bool {
    crate::process::command_exists("wtype")
}

pub fn type_text_via_wtype(text: &str) {
    let text = text.to_string();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        // Fire-and-forget, but the child is remembered so it is reaped instead
        // of lingering as a zombie (M-9). The command is built in
        // `crate::process` so no execution path rolls its own `Command` (N-15).
        crate::process::spawn_detached_program("wtype", &[&text]).ok();
    });
}

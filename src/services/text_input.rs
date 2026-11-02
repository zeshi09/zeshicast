use std::thread;
use std::time::Duration;

pub fn is_wtype_available() -> bool {
    crate::process::command_exists("wtype")
}

/// Delay before synthetic input, so the palette has hidden and the
/// compositor has handed keyboard focus back to the previous window.
const REFOCUS_DELAY: Duration = Duration::from_millis(200);

/// Send Ctrl+V to the focused window once focus has returned to it (Q1.3).
///
/// The caller puts the value on the clipboard first (synchronously, through
/// the `ExecutionRequest::Copy` path), so the paste reads the new content.
pub fn paste_via_wtype() {
    thread::spawn(move || {
        thread::sleep(REFOCUS_DELAY);
        crate::process::spawn_detached_program("wtype", &["-M", "ctrl", "v", "-m", "ctrl"]).ok();
    });
}

pub fn type_text_via_wtype(text: &str) {
    let text = text.to_string();
    thread::spawn(move || {
        thread::sleep(REFOCUS_DELAY);
        // Fire-and-forget, but the child is remembered so it is reaped instead
        // of lingering as a zombie (M-9). The command is built in
        // `crate::process` so no execution path rolls its own `Command` (N-15).
        crate::process::spawn_detached_program("wtype", &[&text]).ok();
    });
}

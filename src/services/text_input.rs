pub fn is_wtype_available() -> bool {
    crate::process::command_exists("wtype")
}

/// `sh` script that waits for the palette to hide and the compositor to hand
/// keyboard focus back to the previous window, then becomes `wtype` with the
/// positional arguments. The arguments are never spliced into the script.
const WTYPE_AFTER_REFOCUS: &str = "sleep 0.2; exec wtype \"$@\"";

/// `sh` argv that runs `wtype args…` after the refocus delay.
fn wtype_after_refocus_argv<'a>(args: &[&'a str]) -> Vec<&'a str> {
    // `$0` is "sh"; everything after it becomes `"$@"`.
    let mut argv = vec!["-c", WTYPE_AFTER_REFOCUS, "sh"];
    argv.extend_from_slice(args);
    argv
}

/// Run `wtype args…` once focus has returned to the previous window.
///
/// The delay lives in the child process, not in a sleeping thread here:
/// without a daemon the palette's process exits right after hiding, and a
/// thread would die with it before typing anything. The child is still
/// spawned through `crate::process` (N-15) and reaped while we live (M-9).
fn wtype_after_refocus(args: &[&str]) {
    if let Err(error) =
        crate::process::spawn_detached_program("sh", &wtype_after_refocus_argv(args))
    {
        log::warn!("could not start wtype: {error}");
    }
}

/// Send Ctrl+V to the focused window once focus has returned to it (Q1.3).
///
/// The caller puts the value on the clipboard first (synchronously, through
/// the `ExecutionRequest::Copy` path), so the paste reads the new content.
pub fn paste_via_wtype() {
    wtype_after_refocus(&["-M", "ctrl", "v", "-m", "ctrl"]);
}

pub fn type_text_via_wtype(text: &str) {
    wtype_after_refocus(&[text]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_text_stays_a_positional_argument() {
        let hostile = "$(touch /tmp/pwned); `id` \"quoted\"";
        let argv = wtype_after_refocus_argv(&[hostile]);

        assert_eq!(argv, vec!["-c", WTYPE_AFTER_REFOCUS, "sh", hostile]);
        assert!(!WTYPE_AFTER_REFOCUS.contains(hostile));
    }

    #[test]
    fn the_delayed_script_hands_its_arguments_to_wtype_verbatim() {
        // Run the script with `printf` standing in for wtype: every argument
        // must arrive unchanged, metacharacters included, and nothing else.
        let script = WTYPE_AFTER_REFOCUS
            .replace("sleep 0.2; ", "")
            .replace("exec wtype", "printf '%s\\n'");
        let hostile = "$(echo pwned) `id` ; rm -rf ~ \"q\"";
        let output = std::process::Command::new("sh")
            .args(["-c", &script, "sh", "-M", "ctrl", hostile])
            .output()
            .expect("sh runs");

        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("-M\nctrl\n{hostile}\n")
        );
    }
}

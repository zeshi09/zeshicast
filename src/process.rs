//! Process ownership helpers.
//!
//! Two problems are handled here, both from the M-9 review finding:
//!
//! * [`ChildGuard`] — owns a child whose result we consume. `Drop` waits a
//!   bounded time and then kills and reaps it, so an early return (a failed
//!   stdin write, a read error, …) can never leave a live child or a zombie.
//! * [`spawn_detached`] — fire-and-forget spawns (apps, shell commands). The
//!   daemon has no place to `wait()` for them, so without help every launched
//!   process would stay `<defunct>` until the daemon exits. The handles are kept
//!   in a registry that is swept on every new spawn and from the main loop
//!   ([`reap_finished`]).
//!
//! A process-wide `SIGCHLD = SIG_IGN` would also stop zombies, but it is not an
//! option: the kernel reaps the children itself, after which `wait()`/
//! `wait_with_output()` fail with `ECHILD` and the output-capturing paths
//! (script stdout, JSON commands, `wl-copy`) lose their exit status.
//!
//! A third problem is handled here as well, found by a flaky test: Linux
//! refuses to `execve` a file that any process has open for writing
//! (`ETXTBSY`, "Text file busy"). A user script is *normally* in that state --
//! an editor is saving it, a download just finished, or the file was written a
//! moment ago -- and the exec is not going to fail forever, because the writer
//! closes its handle. [`spawn_detached`] therefore retries that one error for a
//! bounded time instead of turning the launch into a silent no-op.

use std::io;
use std::process::{Child, Command};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// How long a child may take to exit on its own once we are done with it.
pub(crate) const CHILD_EXIT_GRACE: Duration = Duration::from_millis(500);

/// `ETXTBSY` — the file to execute is open for writing somewhere. Hand-written
/// instead of pulled from a crate: the project is Linux-only and this is the
/// value on every Linux architecture (the arm64/x86_64 ABI shares it).
const ETXTBSY: i32 = 26;

/// How long a spawn refused with `ETXTBSY` is retried.
const EXEC_BUSY_GRACE: Duration = Duration::from_millis(250);

/// Delay between `ETXTBSY` retries.
const EXEC_BUSY_STEP: Duration = Duration::from_millis(10);

/// Owns a child process and guarantees it is gone when dropped.
pub(crate) struct ChildGuard(Child);

impl ChildGuard {
    pub(crate) fn new(child: Child) -> Self {
        Self(child)
    }

    /// Access the child (e.g. to take its `stdin`/`stdout` or wait for it).
    pub(crate) fn child_mut(&mut self) -> &mut Child {
        &mut self.0
    }

    /// Wait up to `grace` for the child to exit, then kill and reap it.
    pub(crate) fn reap(&mut self, grace: Duration) {
        let deadline = Instant::now() + grace;
        loop {
            match self.0.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }

    /// Wait up to `timeout`, then kill the child and return `None`.
    ///
    /// Unlike [`reap`](Self::reap), this is for a child whose *result* we want:
    /// a tool that hangs must not block the caller forever, but one that
    /// finishes normally still reports its status.
    pub(crate) fn wait_timeout(&mut self, timeout: Duration) -> Option<std::process::ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.0.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(None) => {
                    let _ = self.0.kill();
                    let _ = self.0.wait();
                    return None;
                }
                Err(_) => return None,
            }
        }
    }

    /// Kill the child's whole process group and reap the child.
    ///
    /// The child must have been spawned with `process_group(0)` so its group id
    /// equals its pid — otherwise this would signal the group we are in. A
    /// grandchild that inherited the stdout pipe keeps a reader thread blocked
    /// even after the direct child dies; killing the group closes the pipe and
    /// lets that thread finish (N-5).
    pub(crate) fn kill_group(&mut self) {
        if let Some(pgid) = rustix::process::Pid::from_raw(self.0.id() as i32) {
            let _ = rustix::process::kill_process_group(pgid, rustix::process::Signal::KILL);
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.reap(CHILD_EXIT_GRACE);
    }
}

/// Handle for a process nobody waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DetachedChild {
    pid: u32,
}

impl DetachedChild {
    pub(crate) fn pid(&self) -> u32 {
        self.pid
    }
}

fn registry() -> &'static Mutex<Vec<Child>> {
    static REGISTRY: OnceLock<Mutex<Vec<Child>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

/// Spawn a process the daemon does not wait for and remember it for reaping.
///
/// A lock poisoned by a panic elsewhere must not stop the reaper, hence the
/// `unwrap_or_else(PoisonError::into_inner)`.
///
/// A spawn refused with `ETXTBSY` (the program is open for writing) is retried
/// for [`EXEC_BUSY_GRACE`], because that state is temporary and common for user
/// scripts. Every other error, and a writer that never lets go, is returned to
/// the caller -- fail-closed: no launch, and the caller logs why.
pub(crate) fn spawn_detached(command: &mut Command) -> io::Result<DetachedChild> {
    let child = spawn_retrying_text_file_busy(command)?;
    let handle = DetachedChild { pid: child.id() };
    let mut children = registry().lock().unwrap_or_else(|e| e.into_inner());
    reap_locked(&mut children);
    children.push(child);
    Ok(handle)
}

/// [`spawn_detached`] for a program plus arguments. The `Command` is built here
/// so callers never construct one directly (N-15).
pub(crate) fn spawn_detached_program(program: &str, args: &[&str]) -> io::Result<DetachedChild> {
    let mut command = Command::new(program);
    command.args(args);
    spawn_detached(&mut command)
}

/// Whether `program` is on `PATH`.
pub(crate) fn command_exists(program: &str) -> bool {
    Command::new("which")
        .arg(program)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// `Command::spawn` with a bounded retry on `ETXTBSY`.
///
/// `pub(crate)` so the tests that run a just-written script use the same
/// retrying exec as production instead of a second copy of the loop.
pub(crate) fn spawn_retrying_text_file_busy(command: &mut Command) -> io::Result<Child> {
    let deadline = Instant::now() + EXEC_BUSY_GRACE;
    loop {
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(error) if error.raw_os_error() == Some(ETXTBSY) && Instant::now() < deadline => {
                log::debug!(
                    "spawn refused with ETXTBSY (program open for writing); retrying: {:?}",
                    command.get_program()
                );
                std::thread::sleep(EXEC_BUSY_STEP);
            }
            Err(error) => return Err(error),
        }
    }
}

/// A `sh -c` command carrying the JSON-command environment. Building it here
/// keeps process construction in one module (N-15); the caller still has to run
/// it through the execution gate when the command is a user action.
#[cfg(feature = "gui")]
pub(crate) fn shell_command(
    script: &str,
    env: &std::collections::HashMap<String, String>,
) -> Command {
    let mut command = Command::new("sh");
    command.arg("-c").arg(script);
    command.envs(env);
    command
}

/// A command for `program`, with arguments left to the caller. See
/// [`shell_command`] for why this exists (N-15).
#[cfg(feature = "gui")]
pub(crate) fn program_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    Command::new(program)
}

/// Result of [`run_capped`].
#[cfg(any(feature = "gui", test))]
pub(crate) struct CappedRun {
    pub(crate) status: std::process::ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    /// The deadline fired and the child was killed.
    pub(crate) timed_out: bool,
    /// Output was longer than its cap and the excess was discarded.
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
}

/// Run `command` with a deadline, draining stdout and stderr on their own
/// threads while keeping at most `stdout_cap`/`stderr_cap` bytes each.
///
/// Draining is the point: a child that writes more than the pipe buffer (~64
/// KiB) blocks in `write` forever when nobody reads, so a `wait()`-first helper
/// turns a large-but-legitimate output into a spurious timeout (M-5, N-4).
/// Reading on a thread while the parent waits keeps both the deadline and the
/// output.
#[cfg(any(feature = "gui", test))]
pub(crate) fn run_capped(
    command: &mut Command,
    timeout: Duration,
    stdout_cap: usize,
    stderr_cap: usize,
) -> io::Result<CappedRun> {
    use std::process::Stdio;

    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;

    let stdout_reader = child
        .stdout
        .take()
        .map(|pipe| std::thread::spawn(move || drain_capped(pipe, stdout_cap)));
    let stderr_reader = child
        .stderr
        .take()
        .map(|pipe| std::thread::spawn(move || drain_capped(pipe, stderr_cap)));

    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child.wait()?;
        }
        std::thread::sleep(Duration::from_millis(10));
    };

    let (stdout, stdout_truncated) = stdout_reader
        .map(|handle| handle.join().unwrap_or_default())
        .unwrap_or_default();
    let (stderr, stderr_truncated) = stderr_reader
        .map(|handle| handle.join().unwrap_or_default())
        .unwrap_or_default();

    Ok(CappedRun {
        status,
        stdout,
        stderr,
        timed_out,
        stdout_truncated,
        stderr_truncated,
    })
}

/// Read `reader` to EOF, keeping at most `cap` bytes (the rest is discarded
/// rather than left unread, which would block the writer).
#[cfg(any(feature = "gui", test))]
fn drain_capped<R: io::Read>(mut reader: R, cap: usize) -> (Vec<u8>, bool) {
    let mut kept = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut truncated = false;
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                let take = cap.saturating_sub(kept.len()).min(read);
                kept.extend_from_slice(&buffer[..take]);
                if take < read {
                    truncated = true;
                }
            }
            Err(ref error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    (kept, truncated)
}

/// Reap every remembered child that has already exited.
///
/// Returns how many were reaped. Cheap when nothing is running and called from
/// the main loop, so zombies live at most one tick.
pub(crate) fn reap_finished() -> usize {
    let mut children = registry().lock().unwrap_or_else(|e| e.into_inner());
    reap_locked(&mut children)
}

fn reap_locked(children: &mut Vec<Child>) -> usize {
    let before = children.len();
    children.retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));
    before - children.len()
}

#[cfg(test)]
pub(crate) fn detached_child_pids() -> Vec<u32> {
    registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|child| child.id())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid_is_present(pid: u32) -> bool {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }

    fn sweep_until_gone(pid: u32) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            reap_finished();
            if !pid_is_present(pid) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// The child registry is process-wide, so the tests that inspect it must not
    /// run concurrently with each other.
    fn registry_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn detached_children_are_reaped_without_zombies() {
        let _lock = registry_lock();

        // Five short-lived children: without the registry they would all sit in
        // the process table as <defunct> (M-9).
        let mut pids = Vec::new();
        for _ in 0..5 {
            let child = spawn_detached(&mut Command::new("true")).expect("spawn true");
            pids.push(child.pid());
        }

        for pid in &pids {
            assert!(sweep_until_gone(*pid), "pid {pid} is still in the table");
        }
        // Other tests spawn through the same registry, so only *our* children
        // must be gone.
        let registered = detached_child_pids();
        assert!(
            pids.iter().all(|pid| !registered.contains(pid)),
            "still registered: {registered:?}"
        );
    }

    #[test]
    fn killed_detached_child_is_reaped_by_the_sweep() {
        let _lock = registry_lock();

        // A zombie stays visible in /proc until someone waits for it.
        let child = spawn_detached(Command::new("sleep").arg("60")).expect("spawn sleep");
        let pid = child.pid();
        assert!(pid_is_present(pid), "child should be running");
        reap_finished();
        assert!(
            detached_child_pids().contains(&pid),
            "a running child must stay registered"
        );

        let killed = Command::new("kill")
            .arg(pid.to_string())
            .status()
            .expect("run kill");
        assert!(killed.success(), "kill {pid} failed");

        assert!(sweep_until_gone(pid), "zombie {pid} was never reaped");
        assert!(!detached_child_pids().contains(&pid), "still registered");
    }

    #[test]
    fn guard_reaps_a_child_that_outlives_its_usefulness() {
        let child = Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id();

        {
            let mut guard = ChildGuard::new(child);
            assert!(guard.child_mut().try_wait().expect("try_wait").is_none());
        }

        assert!(!pid_is_present(pid), "guard leaked pid {pid}");
    }

    #[test]
    fn guard_returns_the_exit_status_and_still_reaps() {
        let child = Command::new("true").spawn().expect("spawn true");
        let pid = child.id();
        let mut guard = ChildGuard::new(child);

        let status = guard.child_mut().wait().expect("wait");
        assert!(status.success());
        drop(guard);

        assert!(!pid_is_present(pid));
    }

    /// Write an executable script that touches `marker`, plus a directory to
    /// hold both. The caller decides who keeps the file busy.
    ///
    /// The directory name deliberately avoids parentheses and spaces: the path
    /// is interpolated into a shell script, and a `ThreadId(2)`-style name would
    /// be a syntax error for `/bin/sh`.
    fn script_that_touches(
        dir_name: &str,
    ) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before the epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "zeshicast-{dir_name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let script = dir.join("busy.sh");
        let marker = dir.join("ran");
        std::fs::write(&script, format!("#!/bin/sh\ntouch {}\n", marker.display()))
            .expect("write script");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("chmod script");
        (dir, script, marker)
    }

    /// The kernel refuses to `execve` a file that some process holds open for
    /// writing (`ETXTBSY`). An editor saving a script is exactly that, and it
    /// lasts milliseconds -- so the spawn is retried instead of dropping the
    /// launch on the floor.
    #[test]
    fn a_script_open_for_writing_is_retried_until_it_runs() {
        let (dir, script, marker) = script_that_touches("execbusy-retry");

        // Rust opens files with `O_CLOEXEC`, so the child does not inherit this
        // handle; the parent holds it for a moment, which is what makes the
        // first exec fail.
        let held = std::fs::OpenOptions::new()
            .write(true)
            .open(&script)
            .expect("open for writing");
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            drop(held);
        });

        let child = spawn_detached(&mut Command::new(&script))
            .expect("a temporary writer must not stop the spawn");

        let deadline = Instant::now() + Duration::from_secs(5);
        while !marker.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            marker.exists(),
            "pid {} never ran the script after the retry",
            child.pid()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Fail-closed: a writer that never lets go gives an error after the grace
    /// period, not an unbounded retry loop and not a silent success.
    #[test]
    fn a_writer_that_never_lets_go_fails_within_the_grace() {
        let (dir, script, _marker) = script_that_touches("execbusy-giveup");
        let _held = std::fs::OpenOptions::new()
            .write(true)
            .open(&script)
            .expect("open for writing");

        let started = Instant::now();
        let error = spawn_detached(&mut Command::new(&script))
            .expect_err("a permanently busy program must not spawn");
        assert_eq!(error.raw_os_error(), Some(ETXTBSY), "got {error:?}");
        assert!(
            started.elapsed() >= EXEC_BUSY_STEP,
            "the exec must have been retried at least once"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// N-4/M-5: a child that writes more than the pipe buffer (~64 KiB) must
    /// not deadlock. Draining on a reader thread keeps the output and the
    /// deadline both honest.
    #[test]
    fn run_capped_reads_more_than_the_pipe_buffer() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("seq 1 30000"); // ~170 KiB
        let run = run_capped(&mut command, Duration::from_secs(5), 512 * 1024, 8 * 1024)
            .expect("run seq");

        assert!(!run.timed_out, "large output must not hit the deadline");
        assert!(run.status.success());
        assert!(!run.stdout_truncated);
        assert!(run.stderr.is_empty());
        assert!(!run.stderr_truncated);
        let stdout = String::from_utf8_lossy(&run.stdout);
        assert!(
            stdout.ends_with("30000\n"),
            "got {} bytes",
            run.stdout.len()
        );
    }

    #[test]
    fn run_capped_kills_a_child_that_outlives_the_deadline() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("sleep 5");
        let started = Instant::now();
        let run =
            run_capped(&mut command, Duration::from_millis(200), 1024, 1024).expect("run sleep");

        assert!(run.timed_out);
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn run_capped_discards_output_past_the_cap() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("seq 1 30000");
        let run = run_capped(&mut command, Duration::from_secs(5), 1024, 1024).expect("run seq");

        assert_eq!(run.stdout.len(), 1024);
        assert!(run.stdout_truncated);
    }
}

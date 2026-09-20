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
}

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

use std::io;
use std::process::{Child, Command};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// How long a child may take to exit on its own once we are done with it.
pub(crate) const CHILD_EXIT_GRACE: Duration = Duration::from_millis(500);

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
pub(crate) fn spawn_detached(command: &mut Command) -> io::Result<DetachedChild> {
    let child = command.spawn()?;
    let handle = DetachedChild { pid: child.id() };
    let mut children = registry().lock().unwrap_or_else(|e| e.into_inner());
    reap_locked(&mut children);
    children.push(child);
    Ok(handle)
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
pub(crate) fn detached_child_count() -> usize {
    registry().lock().unwrap_or_else(|e| e.into_inner()).len()
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

        for pid in pids {
            assert!(sweep_until_gone(pid), "pid {pid} is still in the table");
        }
        assert_eq!(detached_child_count(), 0, "registry must be drained");
    }

    #[test]
    fn killed_detached_child_is_reaped_by_the_sweep() {
        let _lock = registry_lock();

        // A zombie stays visible in /proc until someone waits for it.
        let child = spawn_detached(Command::new("sleep").arg("60")).expect("spawn sleep");
        let pid = child.pid();
        assert!(pid_is_present(pid), "child should be running");
        assert_eq!(reap_finished(), 0, "a running child must be kept");

        let killed = Command::new("kill")
            .arg(pid.to_string())
            .status()
            .expect("run kill");
        assert!(killed.success(), "kill {pid} failed");

        assert!(sweep_until_gone(pid), "zombie {pid} was never reaped");
        assert_eq!(detached_child_count(), 0);
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
}

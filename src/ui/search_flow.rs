//! Search flow: debounce keystrokes, drop stale results (M-1).
//!
//! Typing in the palette used to run a full search — including providers that
//! fork processes — synchronously from `connect_changed`, once per keystroke.
//! This module owns the two pieces of state that make that behaviour correct:
//!
//! * a **debounce timer**, so a burst of keystrokes starts one search instead of
//!   ten (re-arming cancels the pending callback);
//! * a **generation counter**, so the result of a slow search that finishes
//!   after a newer one was started cannot overwrite the newer result.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use gtk::glib;

use crate::action::Action;

/// How long the entry has to stay unchanged before the search runs.
pub(crate) const SEARCH_DEBOUNCE: Duration = Duration::from_millis(80);

/// How often the main loop checks whether the worker finished.
pub(crate) const SEARCH_POLL: Duration = Duration::from_millis(25);

/// A finished search may be shown only while its generation is still the
/// current one.
#[must_use]
pub(crate) fn result_is_current(current: u64, finished: u64) -> bool {
    current == finished
}

/// Monotonic generation counter; `Arc` because worker threads read it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Generation(Arc<AtomicU64>);

impl Generation {
    /// Starts a new generation and returns its number.
    pub(crate) fn begin(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Has anything started since `generation`?
    pub(crate) fn is_current(&self, generation: u64) -> bool {
        result_is_current(self.0.load(Ordering::SeqCst), generation)
    }
}

/// Schedules `callback` after `delay` and returns its cancel handle.
///
/// The real implementation is `glib::timeout_add_local_once`; tests substitute a
/// fake, so the coalescing rule can be checked deterministically — without a
/// main loop, and without depending on wall-clock timing.
type ArmFn = Rc<dyn Fn(Duration, Box<dyn FnOnce()>) -> Box<dyn Fn()>>;

type CancelFn = Box<dyn Fn()>;

/// One-shot timer that can be re-armed; arming again cancels the pending
/// callback, which is what coalesces a keystroke burst.
#[derive(Clone)]
struct Debounce {
    arm: ArmFn,
    pending: Rc<RefCell<Option<CancelFn>>>,
}

impl Debounce {
    fn new() -> Self {
        Self {
            arm: Rc::new(|delay, callback| {
                let id = glib::timeout_add_local_once(delay, callback);
                let slot = std::cell::Cell::new(Some(id));
                Box::new(move || {
                    if let Some(id) = slot.take() {
                        id.remove();
                    }
                })
            }),
            pending: Rc::new(RefCell::new(None)),
        }
    }

    #[cfg(test)]
    fn with_arm_fn(arm: ArmFn) -> Self {
        Self {
            arm,
            pending: Rc::new(RefCell::new(None)),
        }
    }

    fn arm(&self, delay: Duration, callback: impl FnOnce() + 'static) {
        self.cancel();
        let pending = Rc::clone(&self.pending);
        let cancel = (self.arm)(
            delay,
            Box::new(move || {
                *pending.borrow_mut() = None;
                callback();
            }),
        );
        *self.pending.borrow_mut() = Some(cancel);
    }

    fn cancel(&self) {
        if let Some(cancel) = self.pending.borrow_mut().take() {
            cancel();
        }
    }
}

/// Runs `search` on a worker thread and returns the channel carrying its result
/// back to the main loop.
///
/// A provider that panics must not take the palette down: the panic is caught,
/// the sender is dropped, and the receiver reports `Disconnected`, which stops
/// the poller and leaves the previous results on screen.
fn spawn_search<Search>(search: Search) -> Receiver<Vec<Action>>
where
    Search: FnOnce() -> Vec<Action> + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        if let Ok(actions) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(search)) {
            let _ = sender.send(actions);
        }
    });
    receiver
}

/// Waits for the worker's result on the main loop and delivers it only while its
/// generation is still current (M-1).
///
/// The channel is polled instead of waking the main context from the worker:
/// that keeps the palette free of an extra dependency, and the timer only exists
/// while a search is in flight, so an idle palette gains no periodic wakeups.
fn deliver_when_current<Deliver>(
    generation: Generation,
    token: u64,
    receiver: Receiver<Vec<Action>>,
    deliver: Deliver,
) where
    Deliver: FnOnce(Vec<Action>) + 'static,
{
    let deliver = RefCell::new(Some(deliver));
    let _source = glib::timeout_add_local(SEARCH_POLL, move || match receiver.try_recv() {
        Ok(actions) => {
            if generation.is_current(token)
                && let Some(deliver) = deliver.borrow_mut().take()
            {
                deliver(actions);
            }
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
    });
}

/// Debounce + generation for one search entry.
#[derive(Clone)]
pub(crate) struct SearchFlow {
    generation: Generation,
    debounce: Debounce,
}

impl SearchFlow {
    pub(crate) fn new() -> Self {
        Self {
            generation: Generation::default(),
            debounce: Debounce::new(),
        }
    }

    /// Test constructor: replaces the glib timer with `arm`.
    #[cfg(test)]
    fn with_arm_fn(arm: ArmFn) -> Self {
        Self {
            generation: Generation::default(),
            debounce: Debounce::with_arm_fn(arm),
        }
    }

    /// The synchronous variant of [`Self::request_off_thread`]: only the last
    /// query of a burst is searched, and `deliver` runs only if no newer query
    /// arrived while `search` ran. Kept for the coalescing tests, which need no
    /// thread and no timing.
    #[cfg(test)]
    pub(crate) fn request(
        &self,
        query: String,
        search: impl FnOnce(&str) -> Vec<Action> + 'static,
        deliver: impl FnOnce(Vec<Action>) + 'static,
    ) {
        self.request_after(SEARCH_DEBOUNCE, query, search, deliver);
    }

    #[cfg(test)]
    fn request_after(
        &self,
        delay: Duration,
        query: String,
        search: impl FnOnce(&str) -> Vec<Action> + 'static,
        deliver: impl FnOnce(Vec<Action>) + 'static,
    ) {
        let generation = self.generation.clone();
        self.debounce.arm(delay, move || {
            let token = generation.begin();
            let actions = search(&query);
            if generation.is_current(token) {
                deliver(actions);
            }
        });
    }

    /// Debounced search on a worker thread (M-1).
    ///
    /// `snapshot` builds the search inputs on the main thread when the delay
    /// elapses, `search` runs off it, and `deliver` runs back on the main thread
    /// -- only if no newer query was started in the meantime. Providers fork
    /// processes, so this is what keeps typing responsive while a search runs.
    pub(crate) fn request_off_thread<Data, Snapshot, Search, Deliver>(
        &self,
        query: String,
        snapshot: Snapshot,
        search: Search,
        deliver: Deliver,
    ) where
        Data: Send + 'static,
        Snapshot: FnOnce() -> Data + 'static,
        Search: FnOnce(&Data, &str) -> Vec<Action> + Send + 'static,
        Deliver: FnOnce(Vec<Action>) + 'static,
    {
        let generation = self.generation.clone();
        self.debounce.arm(SEARCH_DEBOUNCE, move || {
            let token = generation.begin();
            let data = snapshot();
            let receiver = spawn_search(move || search(&data, &query));
            deliver_when_current(generation, token, receiver, deliver);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Callbacks queued by the fake timer; cancelling empties a slot.
    type Queue = Rc<RefCell<Vec<Option<Box<dyn FnOnce()>>>>>;

    /// A stand-in for `glib::timeout_add_local_once`: callbacks wait in `queue`
    /// until the test fires them, and cancelling drops the queued callback.
    fn fake_timer(queue: Queue) -> ArmFn {
        Rc::new(move |_delay, callback| {
            let index = {
                let mut queue = queue.borrow_mut();
                queue.push(Some(callback));
                queue.len() - 1
            };
            let queue = Rc::clone(&queue);
            Box::new(move || {
                if let Some(slot) = queue.borrow_mut().get_mut(index) {
                    *slot = None;
                }
            })
        })
    }

    fn fire_survivors(queue: &Queue) {
        loop {
            let next = queue.borrow_mut().iter_mut().find_map(Option::take);
            match next {
                Some(callback) => callback(),
                None => break,
            }
        }
    }

    #[test]
    fn search_runs_off_the_main_thread() {
        let main_thread = std::thread::current().id();
        let receiver = spawn_search(move || {
            assert_ne!(
                std::thread::current().id(),
                main_thread,
                "providers fork processes, so the search must not run on the main loop"
            );
            Vec::new()
        });

        assert!(
            receiver.recv_timeout(Duration::from_secs(5)).is_ok(),
            "the result comes back over the channel"
        );
    }

    #[test]
    fn a_panicking_search_does_not_wedge_the_flow() {
        let receiver = spawn_search(|| panic!("provider blew up"));

        assert!(
            receiver.recv_timeout(Duration::from_secs(5)).is_err(),
            "a panicking provider closes the channel instead of hanging the palette"
        );
    }

    #[test]
    fn stale_results_are_discarded() {
        let generation = Generation::default();
        let first = generation.begin();
        let second = generation.begin();

        assert!(
            !generation.is_current(first),
            "an older generation is stale"
        );
        assert!(generation.is_current(second));
        assert!(result_is_current(3, 3));
        assert!(!result_is_current(3, 2));
    }

    #[test]
    fn debounce_coalesces_keystrokes() {
        // Ten keystrokes in a row must run one search, not ten.
        let queue = Rc::new(RefCell::new(Vec::new()));
        let flow = SearchFlow::with_arm_fn(fake_timer(Rc::clone(&queue)));
        let searches = Rc::new(Cell::new(0usize));
        let delivered = Rc::new(Cell::new(0usize));

        for _ in 0..10 {
            let searches = Rc::clone(&searches);
            let delivered = Rc::clone(&delivered);
            flow.request(
                "q".to_string(),
                move |_| {
                    searches.set(searches.get() + 1);
                    Vec::new()
                },
                move |_| delivered.set(delivered.get() + 1),
            );
        }

        assert_eq!(searches.get(), 0, "nothing runs before the delay elapses");
        assert_eq!(
            queue.borrow().iter().filter(|slot| slot.is_some()).count(),
            1,
            "only the last keystroke of the burst survives"
        );

        fire_survivors(&queue);
        assert_eq!(searches.get(), 1);
        assert_eq!(delivered.get(), 1);
    }

    #[test]
    fn a_result_that_became_stale_while_searching_is_not_shown() {
        // A newer query starts while the older search is still running: the
        // older result must be dropped instead of replacing the newer one.
        let queue: Queue = Rc::new(RefCell::new(Vec::new()));
        let flow = SearchFlow::with_arm_fn(fake_timer(Rc::clone(&queue)));
        let shown = Rc::new(RefCell::new(Vec::<i32>::new()));

        let flow_for_search = flow.clone();
        let queue_for_search = Rc::clone(&queue);
        let shown_outer = Rc::clone(&shown);
        flow.request(
            "slow".to_string(),
            move |_| {
                // The user keeps typing while this search runs: generation 2 is
                // started and finishes first.
                flow_for_search.request("fast".to_string(), |_| Vec::new(), |_| {});
                fire_survivors(&queue_for_search);
                Vec::new()
            },
            move |_| shown_outer.borrow_mut().push(1),
        );

        fire_survivors(&queue);
        assert!(
            shown.borrow().is_empty(),
            "a result whose generation is no longer current must not be rendered"
        );
    }
}

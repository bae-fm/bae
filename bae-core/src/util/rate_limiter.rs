//! Minimum-interval rate limiter for provider API clients.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::Instant;

/// Which stream a piece of import work belongs to — at bottom, whether a person
/// is waiting on it.
///
/// One fact, two decisions. Provider admission: interactive calls are admitted
/// ahead of background ones, and the interval still bounds the two together.
/// And UI invalidation: a run a person started re-renders their candidate row
/// as it progresses, while a background sweep's does not — the sidebar reads
/// the sweep's own aggregate progress line instead, so per-candidate
/// invalidations from it would be pure re-render cost for a queue nobody is
/// looking at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallPriority {
    /// A person is waiting on this call — a typed search, an opened candidate.
    Interactive,
    /// A sweep the user did not ask for and is not watching.
    Background,
}

/// Enforces a minimum interval between calls. Each provider client holds one
/// `static` instance, shared by every call to that provider.
///
/// Admission order is the limiter's decision, not the caller's: an
/// `Interactive` waiter is handed the next slot ahead of every `Background`
/// waiter already queued, however long that queue is. Background work is
/// admitted only when nothing interactive is waiting, so a sustained
/// interactive stream starves the background one — that is the intent, since
/// the background stream is a resumable sweep nobody is watching.
///
/// One slot per interval covers both classes, so the two streams together stay
/// inside the provider's published rate.
///
/// Nobody drives admission but the waiters themselves: each holds a ticket in
/// its class's queue and wakes to check whether the slot is its own. That is
/// load-bearing, not incidental — the limiter is a process-wide `static` that
/// outlives every runtime in the process (the import worker builds and drops
/// its own), and anything a runtime owns dies with it. A separate admitter task
/// would take the queue's only mover with it and wedge the limiter for the
/// life of the process. Here the state a waiter owns is torn down with the
/// waiter's future.
pub struct RateLimiter {
    interval: Duration,
    inner: Mutex<Inner>,
    /// Woken whenever a ticket leaves a queue — admitted, or given up by a
    /// dropped `wait` future — so the next candidate re-checks its turn instead
    /// of waiting out a timer it can no longer see.
    advanced: Notify,
}

/// The bookkeeping the lock protects. Held only for the bookkeeping itself —
/// never across a sleep, or a waiter blocked on the lock could not be overtaken.
struct Inner {
    /// When the last admitted call was stamped; `None` until the first one.
    last_call: Option<Instant>,
    /// Queued ticket ids, oldest first.
    interactive: VecDeque<u64>,
    background: VecDeque<u64>,
    next_ticket: u64,
}

impl Inner {
    const fn new() -> Self {
        Self {
            last_call: None,
            interactive: VecDeque::new(),
            background: VecDeque::new(),
            next_ticket: 0,
        }
    }

    fn queue(&mut self, priority: CallPriority) -> &mut VecDeque<u64> {
        match priority {
            CallPriority::Interactive => &mut self.interactive,
            CallPriority::Background => &mut self.background,
        }
    }

    fn has_waiters(&self) -> bool {
        !self.interactive.is_empty() || !self.background.is_empty()
    }

    fn push(&mut self, priority: CallPriority) -> u64 {
        let id = self.next_ticket;
        self.next_ticket += 1;
        self.queue(priority).push_back(id);
        id
    }

    fn remove(&mut self, priority: CallPriority, id: u64) {
        let queue = self.queue(priority);
        if let Some(at) = queue.iter().position(|queued| *queued == id) {
            queue.remove(at);
        }
    }

    /// Whether the next slot belongs to `id`: interactive before background,
    /// arrival order within a class.
    fn is_next(&self, priority: CallPriority, id: u64) -> bool {
        match priority {
            CallPriority::Interactive => self.interactive.front() == Some(&id),
            CallPriority::Background => {
                self.interactive.is_empty() && self.background.front() == Some(&id)
            }
        }
    }
}

/// What a queued waiter found when it last looked.
enum Turn {
    /// The next slot is this waiter's, and opens at this instant.
    NotBefore(Instant),
    /// Someone is ahead of it; there is nothing to time, only the queue moving.
    Behind,
}

impl RateLimiter {
    pub const fn new(interval: Duration) -> Self {
        Self {
            interval,
            inner: Mutex::new(Inner::new()),
            advanced: Notify::const_new(),
        }
    }

    /// Wait for this call's admission slot, then stamp it. The first call, and
    /// any call arriving after an idle interval, returns without sleeping.
    ///
    /// Dropping the returned future before it completes gives up the waiter's
    /// place and costs no slot — the interval budget is spent on calls that are
    /// actually made.
    pub async fn wait(&self, priority: CallPriority) {
        {
            let mut inner = self.lock();
            if !inner.has_waiters() && self.hold_until(&inner).is_none() {
                inner.last_call = Some(Instant::now());
                return;
            }
        }

        let mut ticket = Ticket::take(self, priority);
        loop {
            // Register for the wakeup before looking at the queue, so a ticket
            // leaving it between the look and the await cannot be missed.
            let advanced = self.advanced.notified();
            tokio::pin!(advanced);
            advanced.as_mut().enable();

            let turn = {
                let mut inner = self.lock();
                if !inner.is_next(priority, ticket.id) {
                    Turn::Behind
                } else if let Some(deadline) = self.hold_until(&inner) {
                    Turn::NotBefore(deadline)
                } else {
                    inner.last_call = Some(Instant::now());
                    inner.remove(priority, ticket.id);
                    ticket.queued = false;
                    drop(inner);
                    self.advanced.notify_waiters();
                    return;
                }
            };

            match turn {
                Turn::NotBefore(deadline) => {
                    tokio::select! {
                        _ = &mut advanced => {}
                        _ = tokio::time::sleep_until(deadline) => {}
                    }
                }
                Turn::Behind => advanced.await,
            }
        }
    }

    /// Restore the limiter to its freshly-constructed state, so the next `wait`
    /// returns immediately. Tests sharing a static limiter reset it so one
    /// test's requests don't delay the next's.
    ///
    /// Waiters hold ticket ids, so a reset underneath them would either strand
    /// them or admit them all at once in breach of the interval. Neither is a
    /// reset, so this refuses instead.
    #[cfg(test)]
    pub fn reset(&self) {
        // Refuse outside the lock: panicking while holding it would poison a
        // limiter that every later test shares.
        let had_waiters = {
            let mut inner = self.lock();
            let had_waiters = inner.has_waiters();
            if !had_waiters {
                *inner = Inner::new();
            }
            had_waiters
        };
        assert!(
            !had_waiters,
            "rate limiter reset while calls are still queued on it — serialize \
             the tests that share it"
        );
    }

    /// The instant before which nothing may be admitted; `None` once the
    /// interval since the last call has passed.
    fn hold_until(&self, inner: &Inner) -> Option<Instant> {
        let ready = inner.last_call? + self.interval;
        (ready > Instant::now()).then_some(ready)
    }

    /// Poisoning cannot mean the queues are inconsistent — every critical
    /// section here is a few infallible statements — and `Ticket::drop` takes
    /// this lock while unwinding, where a panic would abort the process rather
    /// than report anything. So an unrelated panic elsewhere does not take the
    /// limiter with it.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A waiter's place in its queue. Dropping it — which is what a cancelled
/// `wait` future does, including when the runtime it was spawned on goes away —
/// gives that place up and lets the next waiter through, so a call that is
/// never made costs no slot.
struct Ticket<'a> {
    limiter: &'a RateLimiter,
    priority: CallPriority,
    id: u64,
    queued: bool,
}

impl<'a> Ticket<'a> {
    fn take(limiter: &'a RateLimiter, priority: CallPriority) -> Self {
        let id = limiter.lock().push(priority);
        Self {
            limiter,
            priority,
            id,
            queued: true,
        }
    }
}

impl Drop for Ticket<'_> {
    fn drop(&mut self) {
        if !self.queued {
            return;
        }
        self.limiter.lock().remove(self.priority, self.id);
        self.limiter.advanced.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::task::yield_now;

    const INTERVAL: Duration = Duration::from_secs(1);

    fn queued(limiter: &RateLimiter, priority: CallPriority) -> usize {
        limiter.lock().queue(priority).len()
    }

    /// Spawned waiters reach the queue only when they run, so a test that cares
    /// about arrival order has to let them get there first. Yielding keeps a
    /// task runnable, so the paused clock does not advance while we wait.
    async fn queued_reaches(limiter: &RateLimiter, priority: CallPriority, count: usize) {
        while queued(limiter, priority) < count {
            yield_now().await;
        }
    }

    fn spawn_wait(
        limiter: &Arc<RateLimiter>,
        priority: CallPriority,
    ) -> tokio::task::JoinHandle<()> {
        let limiter = Arc::clone(limiter);
        tokio::spawn(async move { limiter.wait(priority).await })
    }

    #[tokio::test(start_paused = true)]
    async fn wait_spaces_calls_and_reset_clears_the_stamp() {
        let limiter = RateLimiter::new(INTERVAL);

        // First call returns immediately — no previous stamp.
        let start = Instant::now();
        limiter.wait(CallPriority::Interactive).await;
        assert!(start.elapsed() < Duration::from_millis(100));

        // Second call waits out the interval since the first.
        let start = Instant::now();
        limiter.wait(CallPriority::Interactive).await;
        assert!(start.elapsed() >= Duration::from_millis(900));

        // After reset the next call is immediate again.
        limiter.reset();
        let start = Instant::now();
        limiter.wait(CallPriority::Interactive).await;
        assert!(start.elapsed() < Duration::from_millis(100));
    }

    /// The one that fails if the priority is removed: with a single FIFO the
    /// interactive call is admitted 21 intervals in.
    #[tokio::test(start_paused = true)]
    async fn interactive_overtakes_a_queued_background_flood() {
        let limiter = Arc::new(RateLimiter::new(INTERVAL));
        // Spend the free first slot, so everything below has to queue.
        limiter.wait(CallPriority::Background).await;

        for _ in 0..20 {
            spawn_wait(&limiter, CallPriority::Background);
        }
        queued_reaches(&limiter, CallPriority::Background, 20).await;

        let start = Instant::now();
        limiter.wait(CallPriority::Interactive).await;
        assert!(
            start.elapsed() <= INTERVAL,
            "interactive call waited {:?}, behind the background queue",
            start.elapsed()
        );
    }

    /// The guarantee that forbids giving background work its own limiter.
    #[tokio::test(start_paused = true)]
    async fn the_interval_bounds_both_classes_together() {
        let limiter = Arc::new(RateLimiter::new(INTERVAL));
        let admissions: Arc<Mutex<Vec<Instant>>> = Arc::default();

        let mut handles = Vec::new();
        for i in 0..10 {
            let priority = if i % 2 == 0 {
                CallPriority::Interactive
            } else {
                CallPriority::Background
            };
            let limiter = Arc::clone(&limiter);
            let admissions = Arc::clone(&admissions);
            handles.push(tokio::spawn(async move {
                limiter.wait(priority).await;
                admissions.lock().unwrap().push(Instant::now());
            }));
        }
        for handle in handles {
            handle.await.unwrap();
        }

        let mut times = admissions.lock().unwrap().clone();
        times.sort();
        assert_eq!(times.len(), 10);
        for pair in times.windows(2) {
            assert!(
                pair[1] - pair[0] >= INTERVAL,
                "admissions {:?} apart, closer than the interval",
                pair[1] - pair[0]
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn background_drains_in_arrival_order_once_interactive_is_idle() {
        let limiter = Arc::new(RateLimiter::new(INTERVAL));
        // Spend the free first slot, so every waiter below is queued.
        limiter.wait(CallPriority::Interactive).await;

        let order: Arc<Mutex<Vec<usize>>> = Arc::default();
        let mut handles = Vec::new();
        for i in 0..5 {
            let limiter_task = Arc::clone(&limiter);
            let order = Arc::clone(&order);
            handles.push(tokio::spawn(async move {
                limiter_task.wait(CallPriority::Background).await;
                order.lock().unwrap().push(i);
            }));
            queued_reaches(&limiter, CallPriority::Background, i + 1).await;
        }
        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(*order.lock().unwrap(), vec![0, 1, 2, 3, 4]);
    }

    /// The waiter behind a cancelled one keeps the schedule it would have had if
    /// the cancelled call had never been made: it is admitted one interval after
    /// the last *real* call, not one interval after the cancellation, and it is
    /// admitted at all rather than stuck behind an abandoned ticket.
    #[tokio::test(start_paused = true)]
    async fn a_cancelled_waiter_costs_no_slot() {
        let limiter = Arc::new(RateLimiter::new(INTERVAL));
        // Spend the free first slot, so every waiter below is queued.
        limiter.wait(CallPriority::Interactive).await;

        let cancelled = spawn_wait(&limiter, CallPriority::Interactive);
        queued_reaches(&limiter, CallPriority::Interactive, 1).await;
        // Cancel partway into the interval, so a slot wrongly spent here would
        // push the next admission out by the part already served.
        tokio::time::sleep(INTERVAL / 2).await;
        cancelled.abort();
        // Joining an aborted task means its future — and the ticket it holds —
        // is really dropped.
        assert!(cancelled.await.unwrap_err().is_cancelled());

        let start = Instant::now();
        tokio::time::timeout(INTERVAL * 5, limiter.wait(CallPriority::Background))
            .await
            .expect("the cancelled waiter left its ticket in the queue");
        assert!(
            start.elapsed() <= INTERVAL / 2,
            "waited {:?}: the cancelled waiter ate a slot",
            start.elapsed()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn an_uncontended_wait_neither_sleeps_nor_spawns_a_task() {
        let limiter = RateLimiter::new(INTERVAL);
        let alive_before = tokio::runtime::Handle::current()
            .metrics()
            .num_alive_tasks();

        let start = Instant::now();
        limiter.wait(CallPriority::Interactive).await;

        assert_eq!(start.elapsed(), Duration::ZERO);
        assert_eq!(
            tokio::runtime::Handle::current()
                .metrics()
                .num_alive_tasks(),
            alive_before
        );
        assert!(!limiter.lock().has_waiters());
    }

    /// The limiters are `static`s that outlive any one runtime — the import
    /// worker builds and drops its own while the app's keeps going. A waiter
    /// dying with its runtime must leave nothing behind that a later runtime
    /// waits on, which is why admission cannot belong to a spawned task.
    #[test]
    fn a_dropped_runtime_leaves_the_limiter_usable() {
        static LIMITER: RateLimiter = RateLimiter::new(INTERVAL);

        fn runtime() -> tokio::runtime::Runtime {
            tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .start_paused(true)
                .build()
                .unwrap()
        }

        let first = runtime();
        first.block_on(async {
            // Spend the free slot, then leave a waiter queued behind it.
            LIMITER.wait(CallPriority::Interactive).await;
            tokio::spawn(LIMITER.wait(CallPriority::Background));
            queued_reaches(&LIMITER, CallPriority::Background, 1).await;
        });
        drop(first);

        let second = runtime();
        second.block_on(async {
            let start = Instant::now();
            tokio::time::timeout(INTERVAL * 5, LIMITER.wait(CallPriority::Interactive))
                .await
                .expect("the dropped runtime wedged the limiter");
            assert!(
                start.elapsed() <= INTERVAL,
                "waited {:?} behind a waiter that no longer exists",
                start.elapsed()
            );
        });
    }
}

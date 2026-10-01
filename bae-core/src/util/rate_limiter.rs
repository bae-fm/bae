//! Admission control for provider API clients: requests spaced under the
//! provider's published rate, held while the provider's own count of its
//! window says it is nearly spent, and held all together when the provider
//! turns one away for its rate.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::Instant;

mod ledger;
use ledger::Ledger;

/// Which stream a piece of import work belongs to — at bottom, whether a person
/// is waiting on it.
///
/// One fact, two decisions. Provider admission: interactive calls are admitted
/// ahead of background ones, and the interval still bounds the two together.
/// And UI delivery: a run a person started publishes progress for its candidate
/// row, while a background run's does not — the sidebar reads the queue's own
/// aggregate progress line instead, so per-candidate progress from it would
/// only re-render a queue nobody is looking at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallPriority {
    /// A person is waiting on this call — a typed search, an opened candidate.
    Interactive,
    /// A sweep the user did not ask for and is not watching.
    Background,
}

/// What a provider's response said about its rate, read off by the client
/// that knows the provider's headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateAnswer {
    /// The provider turned the request away for its rate. `wait` is how long
    /// it asked to be left alone, when it said.
    Refused { wait: Option<Duration> },
    /// The provider's count of its window as the request found it.
    Counted(WindowCount),
    /// Nothing about the rate.
    Silent,
}

/// How many more requests the provider's window takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowCount {
    /// A window that moves with time: each request leaves the count one window
    /// length after it arrived.
    Rolling { remaining: u32 },
    /// A window that starts over whole, `resets_in` from now.
    Fixed { remaining: u32, resets_in: Duration },
}

/// Admits each call a provider object makes. Each provider object owns one,
/// shared by every call it makes.
///
/// Admission order is the limiter's decision, not the caller's: an
/// `Interactive` waiter is handed the next slot ahead of every `Background`
/// waiter already queued, however long that queue is. Background work is
/// admitted only when nothing interactive is waiting, so a sustained
/// interactive stream starves the background one — that is the intent, since
/// the background stream is a resumable sweep nobody is watching.
///
/// A slot opens once three things allow it, for both classes together:
/// the interval since the last admission has passed; no refusal's hold is in
/// force; and the provider's last count of its window, less what was admitted
/// since, has more than a reserve left.
///
/// Nobody drives admission but the waiters themselves: each holds a ticket in
/// its class's queue and wakes to check whether the slot is its own. That is
/// load-bearing, not incidental — a provider is asked from more than one
/// runtime (the import worker builds and drops its own), and anything a
/// runtime owns dies with it. A separate admitter task would take the queue's
/// only mover with it and wedge the limiter for as long as its provider lives.
/// Here the state a waiter owns is torn down with the waiter's future.
pub struct RateLimiter {
    interval: Duration,
    inner: Mutex<Inner>,
    /// Woken whenever a ticket leaves a queue — admitted, or given up by a
    /// dropped `wait` future — or an answer changes when the next slot opens,
    /// so the next candidate re-checks its turn instead of waiting out a timer
    /// it can no longer see.
    advanced: Notify,
}

/// The bookkeeping the lock protects. Held only for the bookkeeping itself —
/// never across a sleep, or a waiter blocked on the lock could not be overtaken.
struct Inner {
    /// When the last admitted call was stamped; `None` until the first one.
    last_call: Option<Instant>,
    /// No call is admitted before this: the provider refused one for its rate.
    held_until: Option<Instant>,
    /// The provider's count of its window, for a provider that reports one.
    ledger: Option<Ledger>,
    /// Admissions so far; each admitted call is known by its number.
    admissions: u64,
    /// Queued ticket ids, oldest first.
    interactive: VecDeque<u64>,
    background: VecDeque<u64>,
    next_ticket: u64,
    /// Every queued ticket admitted, in admission order, so a test can see
    /// which waiter went ahead of which.
    #[cfg(test)]
    admitted: Vec<(CallPriority, u64)>,
}

impl Inner {
    fn new(ledger: Option<Ledger>) -> Self {
        Self {
            last_call: None,
            held_until: None,
            ledger,
            admissions: 0,
            interactive: VecDeque::new(),
            background: VecDeque::new(),
            next_ticket: 0,
            #[cfg(test)]
            admitted: Vec::new(),
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

    /// When the next slot opens, or `None` when it is open now.
    fn opens_at(&mut self, interval: Duration, now: Instant) -> Option<Instant> {
        let spaced = self.last_call.map(|last| last + interval);
        let counted = self
            .ledger
            .as_mut()
            .and_then(|ledger| ledger.spent_until(now));
        [spaced, self.held_until, counted]
            .into_iter()
            .flatten()
            .filter(|at| *at > now)
            .max()
    }

    /// Stamp an admission and return its number.
    fn admit(&mut self, now: Instant) -> u64 {
        let admission = self.admissions;
        self.admissions += 1;
        self.last_call = Some(now);
        if let Some(ledger) = &mut self.ledger {
            ledger.admit(admission, now);
        }
        admission
    }
}

/// What a queued waiter found when it last looked.
enum Turn {
    Admitted(u64),
    /// The next slot is this waiter's, and opens at this instant.
    NotBefore(Instant),
    /// Someone is ahead of it; there is nothing to time, only the queue moving.
    Behind,
}

impl RateLimiter {
    /// Calls spaced `interval` apart, for a provider that reports nothing
    /// about its rate.
    pub fn new(interval: Duration) -> Self {
        Self::build(interval, None)
    }

    /// Calls spaced `interval` apart, for a provider whose responses report its
    /// count of a `window` — the span it counts requests over.
    pub fn counted(interval: Duration, window: Duration) -> Self {
        Self::build(interval, Some(Ledger::new(window)))
    }

    fn build(interval: Duration, ledger: Option<Ledger>) -> Self {
        Self {
            interval,
            inner: Mutex::new(Inner::new(ledger)),
            advanced: Notify::new(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Wait for this call's admission slot, then stamp it. The first call, and
    /// any call arriving after an idle interval, returns without sleeping.
    ///
    /// Dropping the returned future before it completes gives up the waiter's
    /// place and costs no slot — the interval budget is spent on calls that are
    /// actually made. The call is in flight until the returned [`Admitted`] is
    /// answered or dropped.
    pub async fn wait(&self, priority: CallPriority) -> Admitted<'_> {
        if let Some(admission) = self.admit_when_idle() {
            return Admitted::new(self, admission);
        }

        let mut ticket = Ticket::take(self, priority);
        loop {
            // Register for the wakeup before looking at the queue, so a ticket
            // leaving it between the look and the await cannot be missed.
            let advanced = self.advanced.notified();
            tokio::pin!(advanced);
            advanced.as_mut().enable();

            match self.ticket_turn(priority, ticket.id) {
                Turn::Admitted(admission) => {
                    ticket.queued = false;
                    self.advanced.notify_waiters();
                    return Admitted::new(self, admission);
                }
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

    fn admit_when_idle(&self) -> Option<u64> {
        let mut inner = self.lock();
        let now = Instant::now();
        if inner.has_waiters() || inner.opens_at(self.interval, now).is_some() {
            return None;
        }
        Some(inner.admit(now))
    }

    fn enqueue_ticket(&self, priority: CallPriority) -> u64 {
        self.lock().push(priority)
    }

    fn ticket_turn(&self, priority: CallPriority, id: u64) -> Turn {
        let mut inner = self.lock();
        if !inner.is_next(priority, id) {
            return Turn::Behind;
        }
        let now = Instant::now();
        if let Some(deadline) = inner.opens_at(self.interval, now) {
            return Turn::NotBefore(deadline);
        }
        let admission = inner.admit(now);
        inner.remove(priority, id);
        #[cfg(test)]
        inner.admitted.push((priority, id));
        Turn::Admitted(admission)
    }

    fn remove_ticket(&self, priority: CallPriority, id: u64) {
        self.lock().remove(priority, id);
    }

    /// Admission `admission` came back, carrying `answer` when it reached the
    /// provider and the provider answered.
    fn settle(&self, admission: u64, answer: RateAnswer) {
        let now = Instant::now();
        {
            let mut inner = self.lock();
            if let Some(ledger) = &mut inner.ledger {
                ledger.answer(admission, now);
            }
            match answer {
                RateAnswer::Refused { wait } => {
                    let wait = wait.unwrap_or_else(|| {
                        inner.ledger.as_ref().map_or(self.interval, Ledger::window)
                    });
                    let until = now + wait;
                    inner.held_until = Some(inner.held_until.map_or(until, |held| held.max(until)));
                    if let Some(ledger) = &mut inner.ledger {
                        ledger.forget_count();
                    }
                }
                RateAnswer::Counted(count) => inner
                    .ledger
                    .as_mut()
                    .expect("only a limiter counted over a window reads a provider's count")
                    .counted(admission, now, count),
                RateAnswer::Silent => {}
            }
        }
        self.advanced.notify_waiters();
    }

    #[cfg(test)]
    fn queued_count(&self, priority: CallPriority) -> usize {
        self.queued_tickets(priority).len()
    }

    /// The tickets waiting in `priority`'s queue, oldest first.
    #[cfg(test)]
    pub(crate) fn queued_tickets(&self, priority: CallPriority) -> Vec<u64> {
        self.lock().queue(priority).iter().copied().collect()
    }

    /// Every queued ticket admitted so far, in admission order. A call admitted
    /// without queueing — the limiter was idle — took no ticket and is absent.
    #[cfg(test)]
    pub(crate) fn admitted_tickets(&self) -> Vec<(CallPriority, u64)> {
        self.lock().admitted.clone()
    }

    #[cfg(test)]
    fn has_waiters_for_test(&self) -> bool {
        self.lock().has_waiters()
    }

    /// When the next slot opens, `None` when it is open now.
    #[cfg(test)]
    pub(crate) fn opens_at(&self) -> Option<Instant> {
        self.lock().opens_at(self.interval, Instant::now())
    }

    #[cfg(test)]
    fn sent_in_ledger(&self) -> usize {
        self.lock().ledger.as_ref().map_or(0, Ledger::sent_len)
    }
}

/// An admitted call, in flight until it is answered or dropped. Dropping it
/// unanswered — the request failed before the provider answered, or its future
/// was cancelled — tells the limiter only that it is no longer in flight.
#[must_use = "an admitted call is in flight until its answer is read or it is dropped"]
pub struct Admitted<'a> {
    limiter: &'a RateLimiter,
    admission: u64,
    settled: bool,
}

impl<'a> Admitted<'a> {
    fn new(limiter: &'a RateLimiter, admission: u64) -> Self {
        Self {
            limiter,
            admission,
            settled: false,
        }
    }

    /// What the provider's response said about its rate. A refusal holds every
    /// waiter, not only the caller's next try.
    pub fn answered(mut self, answer: RateAnswer) {
        self.settled = true;
        self.limiter.settle(self.admission, answer);
    }
}

impl Drop for Admitted<'_> {
    fn drop(&mut self) {
        if !self.settled {
            self.limiter.settle(self.admission, RateAnswer::Silent);
        }
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
        let id = limiter.enqueue_ticket(priority);
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
        self.limiter.remove_ticket(self.priority, self.id);
        self.limiter.advanced.notify_waiters();
    }
}

#[cfg(test)]
pub(crate) mod load_model;

#[cfg(test)]
#[path = "rate_limiter_tests.rs"]
mod tests;

//! Admission control for provider API clients: requests spaced under the
//! provider's published rate, held while the provider's own count of its
//! window says it is nearly spent, and held all together when the provider
//! turns one away for its rate.

use std::collections::{BTreeSet, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::Instant;

mod ledger;
use ledger::Ledger;

/// How soon a person needs a call's answer, which orders its admission:
/// awaited calls go ahead of requested ones, and requested ones ahead of
/// background ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallPriority {
    /// A person is looking at the screen the call fills: a pick, a typed
    /// search, the open candidate's own identification.
    Awaited,
    /// A person asked for it and is not watching it.
    Requested,
    /// Work nobody asked for or is watching, such as identifying the releases
    /// a scan found.
    Background,
}

/// One piece of work whose calls a person can come to await, such as a
/// candidate's identification across its runs. The providers hand out each
/// one once ([`crate::providers::Providers::new_work`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallWork(pub(crate) u64);

/// The class a call asks to be admitted in: its priority, or
/// [`CallPriority::Awaited`] while a person awaits the work it is part of.
/// The limiter reads it again whenever the work a person awaits changes, so
/// a call already waiting moves with its work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallClass {
    /// The class the call is admitted in while nobody awaits its work.
    priority: CallPriority,
    /// The work the call is part of.
    work: Option<CallWork>,
}

impl CallClass {
    /// A call a person is looking at.
    pub const AWAITED: Self = Self { priority: CallPriority::Awaited, work: None };
    /// A call a person asked for and is not watching.
    pub const REQUESTED: Self = Self { priority: CallPriority::Requested, work: None };
    /// A call nobody asked for, of no work in particular: a test's. Background
    /// work in the app is always some work a person can come to await.
    #[cfg(any(test, feature = "test-utils"))]
    pub const BACKGROUND: Self = Self { priority: CallPriority::Background, work: None };

    /// A call of `work`, admitted at `priority` while nobody awaits the work.
    pub fn of_work(priority: CallPriority, work: CallWork) -> Self {
        Self { priority, work: Some(work) }
    }
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
/// Admission order is the limiter's decision, not the caller's: a waiter is
/// handed the next slot ahead of every waiter of a lower class already
/// queued, however long that queue is, and in arrival order within its
/// class. A lower class is admitted only when nothing above it is waiting, so
/// a sustained stream of awaited calls starves requested ones, and requested
/// ones starve background ones — that is the intent, since each class below
/// is work a person is waiting on less.
///
/// A call's class is read again whenever the work a person awaits changes
/// ([`crate::providers::Providers::await_work`]): every waiting call of that
/// work moves to the awaited queue, in arrival order among the calls there,
/// and back when the work is no longer awaited.
///
/// A slot opens once three things allow it, for every class together:
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
    /// dropped `wait` future — or moves to another, or an answer changes when
    /// the next slot opens, so the next candidate re-checks its turn instead
    /// of waiting out a timer it can no longer see.
    advanced: Notify,
    /// Woken whenever a ticket joins a queue, for a test to wait on a queue's
    /// length.
    #[cfg(any(test, feature = "test-instrumentation"))]
    joined: Notify,
    /// Woken whenever an answer is settled, for a test to wait on a hold.
    #[cfg(test)]
    settled: Notify,
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
    /// Queued tickets by the class each is admitted in now, oldest first.
    awaited: VecDeque<WaitingCall>,
    requested: VecDeque<WaitingCall>,
    background: VecDeque<WaitingCall>,
    /// The work a person awaits now, whose calls are admitted as awaited.
    awaited_work: BTreeSet<CallWork>,
    next_ticket: u64,
    /// Every queued ticket admitted, in admission order, with the class it
    /// was admitted in, so a test can see which waiter went ahead of which.
    #[cfg(any(test, feature = "test-instrumentation"))]
    admitted: Vec<(CallPriority, u64)>,
}

/// A waiting ticket and the class its call asked for.
struct WaitingCall {
    id: u64,
    class: CallClass,
}

impl Inner {
    fn new(ledger: Option<Ledger>) -> Self {
        Self {
            last_call: None,
            held_until: None,
            ledger,
            admissions: 0,
            awaited: VecDeque::new(),
            requested: VecDeque::new(),
            background: VecDeque::new(),
            awaited_work: BTreeSet::new(),
            next_ticket: 0,
            #[cfg(any(test, feature = "test-instrumentation"))]
            admitted: Vec::new(),
        }
    }

    fn queue(&self, priority: CallPriority) -> &VecDeque<WaitingCall> {
        match priority {
            CallPriority::Awaited => &self.awaited,
            CallPriority::Requested => &self.requested,
            CallPriority::Background => &self.background,
        }
    }

    fn queue_mut(&mut self, priority: CallPriority) -> &mut VecDeque<WaitingCall> {
        match priority {
            CallPriority::Awaited => &mut self.awaited,
            CallPriority::Requested => &mut self.requested,
            CallPriority::Background => &mut self.background,
        }
    }

    /// The class a call asking for `class` is admitted in now.
    fn priority_now(&self, class: CallClass) -> CallPriority {
        match class.work {
            Some(work) if self.awaited_work.contains(&work) => CallPriority::Awaited,
            _ => class.priority,
        }
    }

    fn has_waiters(&self) -> bool {
        !self.awaited.is_empty() || !self.requested.is_empty() || !self.background.is_empty()
    }

    fn push(&mut self, class: CallClass) -> u64 {
        let id = self.next_ticket;
        self.next_ticket += 1;
        let priority = self.priority_now(class);
        self.queue_mut(priority).push_back(WaitingCall { id, class });
        id
    }

    /// Take `id` off whichever queue holds it: a waiter giving up its place
    /// does not know which class its call is in now.
    fn remove(&mut self, id: u64) {
        for queue in [&mut self.awaited, &mut self.requested, &mut self.background] {
            if let Some(at) = queue.iter().position(|queued| queued.id == id) {
                queue.remove(at);
                return;
            }
        }
    }

    /// The ticket the next slot belongs to, and the class it waits in: the
    /// oldest of the highest class waiting.
    fn next(&self) -> Option<(CallPriority, u64)> {
        [CallPriority::Awaited, CallPriority::Requested, CallPriority::Background]
            .into_iter()
            .find_map(|priority| self.queue(priority).front().map(|queued| (priority, queued.id)))
    }

    /// Make `works` the work a person awaits, and move every waiting ticket
    /// to the queue of the class it is admitted in now, keeping arrival order
    /// within each. Returns whether anything changed.
    fn set_awaited_work(&mut self, works: &BTreeSet<CallWork>) -> bool {
        if self.awaited_work == *works {
            return false;
        }
        self.awaited_work = works.clone();
        let mut queued: Vec<WaitingCall> = [
            std::mem::take(&mut self.awaited),
            std::mem::take(&mut self.requested),
            std::mem::take(&mut self.background),
        ]
        .into_iter()
        .flatten()
        .collect();
        // Ticket ids are handed out in arrival order.
        queued.sort_by_key(|queued| queued.id);
        for ticket in queued {
            let priority = self.priority_now(ticket.class);
            self.queue_mut(priority).push_back(ticket);
        }
        true
    }

    /// When the next slot opens, or `None` when it is open now.
    fn opens_at(&mut self, interval: Duration, now: Instant) -> Option<Instant> {
        let spaced = self.last_call.map(|last| last + interval);
        let counted = self.ledger.as_mut().and_then(|ledger| ledger.spent_until(now));
        [spaced, self.held_until, counted].into_iter().flatten().filter(|at| *at > now).max()
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
            #[cfg(any(test, feature = "test-instrumentation"))]
            joined: Notify::new(),
            #[cfg(test)]
            settled: Notify::new(),
        }
    }

    /// The bookkeeping. Nothing done while it is held panics — a broken
    /// guarantee found under it, like an answer the limiter can't read, panics
    /// only once the guard is gone — so a poisoned lock says nothing about it;
    /// and the `Drop`s of [`Ticket`] and [`Admitted`] take it, where a panic
    /// must not start.
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Wait for this call's admission slot, then stamp it. The first call, and
    /// any call arriving after an idle interval, returns without sleeping.
    ///
    /// Dropping the returned future before it completes gives up the waiter's
    /// place and costs no slot — the interval budget is spent on calls that are
    /// actually made. The call is in flight until the returned [`Admitted`] is
    /// answered or dropped.
    pub async fn wait(&self, class: CallClass) -> Admitted<'_> {
        if let Some(admission) = self.admit_when_idle() {
            return Admitted::new(self, admission);
        }

        let mut ticket = Ticket::take(self, class);
        loop {
            // Register for the wakeup before looking at the queue, so a ticket
            // leaving it between the look and the await cannot be missed.
            let advanced = self.advanced.notified();
            tokio::pin!(advanced);
            advanced.as_mut().enable();

            match self.ticket_turn(ticket.id) {
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

    fn enqueue_ticket(&self, class: CallClass) -> u64 {
        let id = self.lock().push(class);
        #[cfg(any(test, feature = "test-instrumentation"))]
        self.joined.notify_waiters();
        id
    }

    /// Whether the next slot is `id`'s, by the queue its class puts it in now.
    fn ticket_turn(&self, id: u64) -> Turn {
        let mut inner = self.lock();
        let priority = match inner.next() {
            Some((priority, next)) if next == id => priority,
            _ => return Turn::Behind,
        };
        let now = Instant::now();
        if let Some(deadline) = inner.opens_at(self.interval, now) {
            return Turn::NotBefore(deadline);
        }
        let admission = inner.admit(now);
        inner.queue_mut(priority).pop_front();
        #[cfg(any(test, feature = "test-instrumentation"))]
        inner.admitted.push((priority, id));
        Turn::Admitted(admission)
    }

    fn remove_ticket(&self, id: u64) {
        self.lock().remove(id);
    }

    /// Make `works` the work a person awaits now, every other work no longer
    /// awaited: the calls of each are admitted ahead of every other class,
    /// those already waiting included.
    pub fn set_awaited_work(&self, works: &BTreeSet<CallWork>) {
        let moved = self.lock().set_awaited_work(works);
        if moved {
            self.advanced.notify_waiters();
        }
    }

    /// Admission `admission` came back, carrying `answer` when it reached the
    /// provider and the provider answered.
    fn settle(&self, admission: u64, answer: RateAnswer) {
        let now = Instant::now();
        let unread = {
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
                    None
                }
                RateAnswer::Counted(count) => match &mut inner.ledger {
                    Some(ledger) => {
                        ledger.counted(admission, now, count);
                        None
                    }
                    None => Some(count),
                },
                RateAnswer::Silent => None,
            }
        };
        self.advanced.notify_waiters();
        #[cfg(test)]
        self.settled.notify_waiters();
        if let Some(count) = unread {
            panic!("only a limiter counted over a window reads a provider's count, got {count:?}");
        }
    }

    /// The tickets waiting in `priority`'s queue, oldest first.
    #[cfg(any(test, feature = "test-instrumentation"))]
    pub(crate) fn queued_tickets(&self, priority: CallPriority) -> Vec<u64> {
        self.lock().queue(priority).iter().map(|queued| queued.id).collect()
    }

    /// The tickets waiting in `priority`'s queue once at least `count` are,
    /// oldest first. Waiters reach the queue only when they run, so a test
    /// that cares about arrival order waits on the queue itself.
    #[cfg(any(test, feature = "test-instrumentation"))]
    pub(crate) async fn queued_tickets_reaching(
        &self,
        priority: CallPriority,
        count: usize,
    ) -> Vec<u64> {
        loop {
            let joined = self.joined.notified();
            tokio::pin!(joined);
            joined.as_mut().enable();
            let queued = self.queued_tickets(priority);
            if queued.len() >= count {
                return queued;
            }
            joined.await;
        }
    }

    /// Every queued ticket admitted so far, in admission order. A call admitted
    /// without queueing — the limiter was idle — took no ticket and is absent.
    #[cfg(any(test, feature = "test-instrumentation"))]
    pub(crate) fn admitted_tickets(&self) -> Vec<(CallPriority, u64)> {
        self.lock().admitted.clone()
    }

    /// The instant a refusal holds every call until, once one has settled.
    #[cfg(test)]
    pub(crate) async fn held_until(&self) -> Instant {
        loop {
            let settled = self.settled.notified();
            tokio::pin!(settled);
            settled.as_mut().enable();
            if let Some(until) = self.lock().held_until {
                return until;
            }
            settled.await;
        }
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
        Self { limiter, admission, settled: false }
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
    id: u64,
    queued: bool,
}

impl<'a> Ticket<'a> {
    fn take(limiter: &'a RateLimiter, class: CallClass) -> Self {
        let id = limiter.enqueue_ticket(class);
        Self { limiter, id, queued: true }
    }
}

impl Drop for Ticket<'_> {
    fn drop(&mut self) {
        if !self.queued {
            return;
        }
        self.limiter.remove_ticket(self.id);
        self.limiter.advanced.notify_waiters();
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub mod load_model;

#[cfg(test)]
#[path = "rate_limiter_tests.rs"]
mod tests;

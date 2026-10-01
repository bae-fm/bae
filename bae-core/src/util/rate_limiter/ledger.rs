//! The provider's own count of its window, kept current between responses
//! with what the limiter knows of its own requests.

use std::collections::VecDeque;
use std::time::Duration;

use tokio::time::Instant;

use super::WindowCount;

/// Requests of the provider's count left unspent. A count is read off a
/// response that left earlier than the requests sent since, and the provider
/// may meet them in another order or count a little differently than it
/// reports, so admission stops while this many remain rather than at none.
const RESERVE: i64 = 2;

/// Every request the limiter admitted that the provider's count may still
/// hold, and the newest count a response reported.
pub(super) struct Ledger {
    /// How long the provider counts a request: a moving count drops a request
    /// this long after it arrived.
    window: Duration,
    /// Oldest admission first.
    sent: VecDeque<Sent>,
    count: Option<Count>,
}

/// One admitted request.
struct Sent {
    admission: u64,
    admitted: Instant,
    /// When its response, or its failure, came back. The provider met it — if
    /// it met it at all — between `admitted` and this.
    answered: Option<Instant>,
}

/// What one response said the provider's window had left.
struct Count {
    /// The admission whose response carried it.
    admission: u64,
    /// When that response came back: the provider counted no later than this.
    answered: Instant,
    remaining: u32,
    /// When the count no longer describes the window: a fixed window's reset,
    /// or a moving window's length after the counted request — by then every
    /// request the count held has left it.
    ends: Instant,
    /// Whether requests leave the count one at a time as they age out.
    rolling: bool,
}

impl Ledger {
    pub(super) fn new(window: Duration) -> Self {
        Self {
            window,
            sent: VecDeque::new(),
            count: None,
        }
    }

    pub(super) fn window(&self) -> Duration {
        self.window
    }

    pub(super) fn admit(&mut self, admission: u64, now: Instant) {
        self.prune(now);
        self.sent.push_back(Sent {
            admission,
            admitted: now,
            answered: None,
        });
    }

    pub(super) fn answer(&mut self, admission: u64, now: Instant) {
        if let Some(sent) = self
            .sent
            .iter_mut()
            .find(|sent| sent.admission == admission)
        {
            sent.answered = Some(now);
        }
    }

    /// Take `count`, read off the response to `admission`, unless a response to
    /// a later admission has already said more.
    pub(super) fn counted(&mut self, admission: u64, now: Instant, count: WindowCount) {
        if self
            .count
            .as_ref()
            .is_some_and(|newest| newest.admission > admission)
        {
            return;
        }
        let (remaining, ends, rolling) = match count {
            WindowCount::Rolling { remaining } => (remaining, now + self.window, true),
            WindowCount::Fixed {
                remaining,
                resets_in,
            } => (remaining, now + resets_in, false),
        };
        self.count = Some(Count {
            admission,
            answered: now,
            remaining,
            ends,
            rolling,
        });
    }

    /// A refusal says the count is spent, and the hold it brings outlasts it.
    pub(super) fn forget_count(&mut self) {
        self.count = None;
    }

    /// When the count is spent down to the reserve: the next instant a request
    /// is certain to have left it. `None` while it has room, or when no count
    /// is current.
    pub(super) fn spent_until(&mut self, now: Instant) -> Option<Instant> {
        self.prune(now);
        let count = self.count.as_ref()?;
        if now >= count.ends {
            self.count = None;
            return None;
        }

        // Every request admitted after the counted one is spent from it.
        let after = self
            .sent
            .iter()
            .filter(|sent| sent.admission > count.admission)
            .count() as i64;
        // In a moving window, each request of ours the count held, or that was
        // spent from it since, gives its place back once it has certainly left
        // — a window after it certainly arrived. The others the count holds
        // are someone else's, whose times nobody here knows, so they are taken
        // to stay until the count ends.
        let mut returned = 0i64;
        let mut next_return: Option<Instant> = None;
        if count.rolling {
            for sent in &self.sent {
                let held = sent.admission > count.admission
                    || sent.admitted + self.window > count.answered;
                let Some(answered) = sent.answered.filter(|_| held) else {
                    continue;
                };
                let leaves = answered + self.window;
                if leaves <= now {
                    returned += 1;
                } else {
                    next_return = Some(next_return.map_or(leaves, |next| next.min(leaves)));
                }
            }
        }

        // The counted request itself is spent too: a count may or may not
        // include the request it answers.
        let budget = i64::from(count.remaining) - 1 - after + returned;
        if budget > RESERVE {
            return None;
        }
        Some(next_return.map_or(count.ends, |next| next.min(count.ends)))
    }

    /// Drop the requests no count can hold any more: answered two windows ago,
    /// so they left any window a count current now describes.
    fn prune(&mut self, now: Instant) {
        let window = self.window;
        self.sent.retain(|sent| {
            sent.answered
                .is_none_or(|answered| answered + window * 2 > now)
        });
    }

    #[cfg(test)]
    pub(super) fn sent_len(&self) -> usize {
        self.sent.len()
    }
}

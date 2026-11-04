//! How bae repeats a provider request that did not land.
//!
//! Each provider's client owns one [`RetryPolicy`], built from what that
//! provider says about being asked too often, and hands it to
//! [`retry_with_backoff_if`] or [`retry_classified`] for every request it
//! makes. A caller never picks attempts or waits: which failures are worth
//! repeating, and how long to leave the provider alone before asking again,
//! are the client's.
//!
//! The wait before a repeat is a plain `tokio::time::sleep` inside the
//! request's own future, taken after the attempt has released its rate-limit
//! slot. So a request waiting out a busy server holds nothing another request
//! needs, and dropping its future — a cancelled identify run — ends the wait
//! with it.

use rand::Rng;
use std::fmt::Display;
use std::time::Duration;
use tracing::warn;

/// How one provider's client repeats a request that did not land: how many
/// times it asks in all, and how long it waits before each repeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Every try, the first included. At least one.
    attempts: u32,
    /// The longest wait before the first repeat.
    first_wait: Duration,
    /// No wait is longer than this, however many repeats came before.
    longest_wait: Duration,
    /// Whether each wait is drawn from the upper half of its span rather than
    /// taken whole. A server shedding load from many clients at once sees
    /// their repeats arrive together if every client waits the same span, and
    /// spread out if each draws its own; the lower half stays fixed, so a
    /// repeat still leaves the server at least half the span.
    jitter: bool,
}

impl RetryPolicy {
    /// Wait `first_wait` before the first repeat and double it before each
    /// one after, up to `longest_wait`, each drawn from the upper half of its
    /// span.
    pub const fn exponential(attempts: u32, first_wait: Duration, longest_wait: Duration) -> Self {
        assert!(attempts > 0, "a retry policy makes at least one attempt");
        Self { attempts, first_wait, longest_wait, jitter: true }
    }

    /// Wait the same `wait` before every repeat.
    pub const fn flat(attempts: u32, wait: Duration) -> Self {
        assert!(attempts > 0, "a retry policy makes at least one attempt");
        Self { attempts, first_wait: wait, longest_wait: wait, jitter: false }
    }

    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// The span of the wait before repeat `retry` (the first repeat is 1):
    /// `first_wait` doubled once per repeat before it, capped at
    /// `longest_wait`.
    fn span(&self, retry: u32) -> Duration {
        let doublings = retry.saturating_sub(1).min(31);
        self.first_wait.saturating_mul(1u32 << doublings).min(self.longest_wait)
    }

    /// The wait before repeat `retry`, with `draw` a number in `[0, 1)` that
    /// places a jittered wait within the upper half of its span.
    fn wait(&self, retry: u32, draw: f64) -> Duration {
        let span = self.span(retry);
        if !self.jitter {
            return span;
        }
        span / 2 + span.mul_f64(draw.clamp(0.0, 1.0) / 2.0)
    }

    /// How long the wait before repeat `retry` sleeps — see [`pause`].
    fn pause(&self, retry: u32) -> Duration {
        pause(self.wait(retry, rand::rng().random::<f64>()))
    }
}

/// How long a wait actually sleeps. Zero in any test build, so a retry-path
/// test spends no real time between attempts — gated on `test` (this crate's
/// own unit tests) and `test-utils` (the tests of every crate built on this
/// one, which compile it as a normal dependency).
/// `test-utils` is dev/test-only, so a production build always waits for real.
fn pause(wait: Duration) -> Duration {
    if cfg!(any(test, feature = "test-utils")) {
        Duration::ZERO
    } else {
        wait
    }
}

/// Whether an HTTP error status is transient and worth retrying: a server error
/// (5xx) or an explicit rate-limit (429). The one definition, shared by every
/// HTTP retry path that classifies by `reqwest::StatusCode`.
pub fn is_transient_status(status: reqwest::StatusCode) -> bool {
    status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS
}

/// Whether a failed try is worth repeating, and when.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    /// The failure is the provider's answer, or a local fault: asking again
    /// gets the same thing.
    Never,
    /// A transient failure the provider said nothing about: ask again after
    /// the policy's own wait.
    AfterBackoff,
    /// The provider said how long to leave it alone — its `Retry-After`. That
    /// wait replaces the policy's, since asking sooner is asking to be turned
    /// away again.
    AfterToldWait(Duration),
}

impl Repeat {
    /// A transient failure, repeated after `told_wait` when the response
    /// stated one and after the policy's own wait otherwise.
    pub fn transient(told_wait: Option<Duration>) -> Self {
        told_wait.map_or(Self::AfterBackoff, Self::AfterToldWait)
    }

    /// The repeat an HTTP error response calls for: a transient status is
    /// repeated after `told_wait` when the response stated one, any other is
    /// the provider's answer.
    pub fn for_status(status: reqwest::StatusCode, told_wait: Option<Duration>) -> Self {
        if is_transient_status(status) {
            Self::transient(told_wait)
        } else {
            Self::Never
        }
    }

    /// [`Self::for_status`] for a status that may not have been read. With no
    /// readable status reqwest classified a send error as carrying one it
    /// couldn't produce, which repeats like any transport failure.
    pub fn for_recorded_status(status: Option<u16>, told_wait: Option<Duration>) -> Self {
        match status {
            None => Self::transient(told_wait),
            Some(status) => reqwest::StatusCode::from_u16(status)
                .map_or(Self::Never, |status| Self::for_status(status, told_wait)),
        }
    }
}

/// The longest a provider may ask bae to wait before a repeat. A failing
/// server asking for more than a minute is saying it is down for now, not
/// busy, and the lookup ends at once rather than asking early only to be
/// turned away again. (Discogs's and MusicBrainz's rate refusals never come
/// through here: their clients hold every request for whatever wait a refusal
/// names, and send the refused one again.)
const LONGEST_TOLD_WAIT: Duration = Duration::from_secs(60);

/// One attempt's outcome for [`retry_classified`], for callers whose retry
/// decision is made inline rather than by inspecting an error type — e.g. a
/// Cover Art Archive 404 is `Done` ("no cover exists"), a valid answer, not an
/// error.
pub enum ClassifiedAttempt<T, E> {
    /// A final answer — return it.
    Done(T),
    /// A failure, and whether and when it is worth repeating.
    Failed(E, Repeat),
}

/// Try `attempt` as `policy` says, until it returns
/// [`ClassifiedAttempt::Done`], a failure not worth repeating, or attempts
/// run out. [`retry_with_backoff_if`] is this for callers that return
/// `Result` and classify the error with a predicate.
pub async fn retry_classified<T, E, F, Fut>(
    policy: RetryPolicy,
    label: &str,
    mut attempt: F,
) -> Result<T, E>
where
    E: Display,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ClassifiedAttempt<T, E>>,
{
    let max_attempts = policy.attempts();
    let mut attempt_index = 1;
    loop {
        let (error, repeat) = match attempt().await {
            ClassifiedAttempt::Done(value) => return Ok(value),
            ClassifiedAttempt::Failed(error, repeat) => (error, repeat),
        };
        let wait = match repeat {
            Repeat::Never => return Err(error),
            _ if attempt_index == max_attempts => {
                warn!("{label} failed after {max_attempts} attempts: {error}");
                return Err(error);
            }
            Repeat::AfterToldWait(told) if told > LONGEST_TOLD_WAIT => {
                warn!(
                    "{label} failed (attempt {attempt_index}/{max_attempts}): {error} — the \
                     provider asks for {told:?} before another try, longer than a lookup waits"
                );
                return Err(error);
            }
            Repeat::AfterToldWait(told) => pause(told),
            Repeat::AfterBackoff => policy.pause(attempt_index),
        };
        warn!("{label} failed (attempt {attempt_index}/{max_attempts}): {error} — retrying in {wait:?}");
        tokio::time::sleep(wait).await;
        attempt_index += 1;
    }
}

/// Try `f` as `policy` says, repeating a failure as `repeat` classifies it.
///
/// The classification is required: most of what an API client returns is an
/// answer, not a fault, and a retry that can't tell the difference asks again
/// only to be told "not found" again.
pub async fn retry_with_backoff_if<F, Fut, T, E, Classify>(
    policy: RetryPolicy,
    label: &str,
    repeat: Classify,
    f: F,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: Display,
    Classify: Fn(&E) -> Repeat,
{
    retry_classified(policy, label, || async {
        match f().await {
            Ok(value) => ClassifiedAttempt::Done(value),
            Err(error) => {
                let repeat = repeat(&error);
                ClassifiedAttempt::Failed(error, repeat)
            }
        }
    })
    .await
}

#[cfg(test)]
#[path = "retry_tests.rs"]
mod tests;

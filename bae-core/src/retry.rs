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
        Self {
            attempts,
            first_wait,
            longest_wait,
            jitter: true,
        }
    }

    /// Wait the same `wait` before every repeat.
    pub const fn flat(attempts: u32, wait: Duration) -> Self {
        assert!(attempts > 0, "a retry policy makes at least one attempt");
        Self {
            attempts,
            first_wait: wait,
            longest_wait: wait,
            jitter: false,
        }
    }

    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// The span of the wait before repeat `retry` (the first repeat is 1):
    /// `first_wait` doubled once per repeat before it, capped at
    /// `longest_wait`.
    fn span(&self, retry: u32) -> Duration {
        let doublings = retry.saturating_sub(1).min(31);
        self.first_wait
            .saturating_mul(1u32 << doublings)
            .min(self.longest_wait)
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
/// test spends no real time between attempts — gated on `test` (crate unit
/// tests) and `test-utils` (integration tests, which compile the crate as a
/// normal dependency), the same seam `install_test_keyring` uses.
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
}

/// The longest a provider may ask bae to wait before a repeat. Discogs counts
/// its limit over a sixty-second window, so no rate limit bae keeps to needs
/// longer to clear. A provider asking for more is saying it is down for now,
/// not busy, and the lookup ends at once rather than asking early only to be
/// turned away again.
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
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::{
        is_transient_status, retry_classified, retry_with_backoff_if, ClassifiedAttempt, Repeat,
        RetryPolicy, LONGEST_TOLD_WAIT,
    };

    const THREE: RetryPolicy = RetryPolicy::flat(3, Duration::from_millis(1));

    /// Each wait doubles from the first until the longest caps it, and a
    /// jittered wait lands in the upper half of its span.
    #[test]
    fn exponential_waits_double_to_their_cap_within_the_upper_half() {
        let policy = RetryPolicy::exponential(6, Duration::from_secs(1), Duration::from_secs(5));
        let spans: Vec<Duration> = (1..=5).map(|retry| policy.wait(retry, 0.999_999)).collect();
        assert_eq!(
            spans
                .iter()
                .map(|wait| wait.as_secs_f64().round() as u64)
                .collect::<Vec<_>>(),
            vec![1, 2, 4, 5, 5]
        );
        assert_eq!(policy.wait(1, 0.0), Duration::from_millis(500));
        assert_eq!(policy.wait(3, 0.0), Duration::from_secs(2));
        assert_eq!(policy.wait(3, 0.5), Duration::from_secs(3));
    }

    /// A flat policy waits the same span every time, drawn or not.
    #[test]
    fn flat_waits_are_whole_and_equal() {
        let policy = RetryPolicy::flat(3, Duration::from_millis(250));
        assert_eq!(policy.wait(1, 0.0), Duration::from_millis(250));
        assert_eq!(policy.wait(2, 0.9), Duration::from_millis(250));
    }

    /// A long run of repeats saturates at the cap rather than overflowing.
    #[test]
    fn a_late_repeat_waits_the_longest_and_no_longer() {
        let policy =
            RetryPolicy::exponential(u32::MAX, Duration::from_secs(1), Duration::from_secs(10));
        assert_eq!(policy.wait(40, 0.0), Duration::from_secs(5));
        assert!(policy.wait(u32::MAX, 0.999) <= Duration::from_secs(10));
    }

    #[test]
    fn is_transient_status_repeats_server_and_rate_limit_only() {
        use reqwest::StatusCode;
        assert!(is_transient_status(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(is_transient_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(is_transient_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(!is_transient_status(StatusCode::BAD_REQUEST));
        assert!(!is_transient_status(StatusCode::FORBIDDEN));
        assert!(!is_transient_status(StatusCode::NOT_FOUND));
    }

    #[tokio::test]
    async fn retry_classified_stops_immediately_on_permanent() {
        let attempts = AtomicUsize::new(0);
        let result: Result<(), &str> = retry_classified(THREE, "test", || async {
            attempts.fetch_add(1, Ordering::SeqCst);
            ClassifiedAttempt::Failed("permanent", Repeat::Never)
        })
        .await;
        assert_eq!(result, Err("permanent"));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_classified_exhausts_attempts_on_retry() {
        let attempts = AtomicUsize::new(0);
        let result: Result<(), &str> = retry_classified(THREE, "test", || async {
            attempts.fetch_add(1, Ordering::SeqCst);
            ClassifiedAttempt::Failed("transient", Repeat::AfterBackoff)
        })
        .await;
        assert_eq!(result, Err("transient"));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_classified_returns_done() {
        let attempts = AtomicUsize::new(0);
        let result: Result<&str, &str> = retry_classified(THREE, "test", || async {
            let n = attempts.fetch_add(1, Ordering::SeqCst);
            if n < 1 {
                ClassifiedAttempt::Failed("transient", Repeat::AfterBackoff)
            } else {
                ClassifiedAttempt::Done("ok")
            }
        })
        .await;
        assert_eq!(result, Ok("ok"));
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn retry_with_backoff_if_stops_on_non_retryable_error() {
        let attempts = AtomicUsize::new(0);

        let result = retry_with_backoff_if(
            THREE,
            "test operation",
            |_| Repeat::Never,
            || async {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>("permanent")
            },
        )
        .await;

        assert_eq!(result, Err("permanent"));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_with_backoff_if_retries_until_success() {
        let attempts = AtomicUsize::new(0);

        let result = retry_with_backoff_if(
            THREE,
            "test operation",
            |_| Repeat::AfterBackoff,
            || async {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                if attempt < 2 {
                    Err("transient")
                } else {
                    Ok("sent")
                }
            },
        )
        .await;

        assert_eq!(result, Ok("sent"));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    /// A provider that says how long to wait is asked again after that wait,
    /// within the longest a lookup waits.
    #[tokio::test]
    async fn a_told_wait_is_repeated_after() {
        let attempts = AtomicUsize::new(0);
        let result: Result<&str, &str> = retry_classified(THREE, "test", || async {
            if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                ClassifiedAttempt::Failed("busy", Repeat::AfterToldWait(LONGEST_TOLD_WAIT))
            } else {
                ClassifiedAttempt::Done("ok")
            }
        })
        .await;
        assert_eq!(result, Ok("ok"));
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    /// A provider asking for longer than a lookup waits is down for now: the
    /// lookup ends rather than asking early to be turned away again.
    #[tokio::test]
    async fn a_told_wait_past_the_longest_ends_the_lookup() {
        let attempts = AtomicUsize::new(0);
        let result: Result<(), &str> = retry_classified(THREE, "test", || async {
            attempts.fetch_add(1, Ordering::SeqCst);
            ClassifiedAttempt::Failed(
                "down",
                Repeat::AfterToldWait(LONGEST_TOLD_WAIT + Duration::from_secs(1)),
            )
        })
        .await;
        assert_eq!(result, Err("down"));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}

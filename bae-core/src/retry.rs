use std::fmt::Display;
use std::time::Duration;
use tracing::warn;

/// Base for the API clients' linear backoff. 500ms in production; zero in any
/// test build so a retry-path test spends no real time between attempts. Gated
/// on `test` (crate unit tests) and `test-utils` (integration tests, which
/// compile the crate as a normal dependency), the same seam
/// `install_test_keyring` uses. `test-utils` is dev/test-only, so a production
/// build always backs off for real.
#[cfg(not(any(test, feature = "test-utils")))]
const LINEAR_BACKOFF_BASE: Duration = Duration::from_millis(500);
#[cfg(any(test, feature = "test-utils"))]
const LINEAR_BACKOFF_BASE: Duration = Duration::ZERO;

/// Linear backoff: `LINEAR_BACKOFF_BASE` × attempt. What the API clients want;
/// diagnostics wants a flat delay instead, which is why the delay stays a
/// parameter to `retry_with_backoff_if`.
pub fn linear_backoff(attempt: u32) -> Duration {
    LINEAR_BACKOFF_BASE * attempt
}

/// Exponential backoff: `base` × 2^(attempt-1) — `base`, 2·base, 4·base before the
/// 2nd, 3rd, 4th attempt. What the Cover Art Archive fetch and image download want;
/// `base` is passed in so tests can shrink it to zero.
pub fn exponential_backoff(base: Duration, attempt: u32) -> Duration {
    base * 2u32.pow(attempt - 1)
}

/// Whether an HTTP error status is transient and worth retrying: a server error
/// (5xx) or an explicit rate-limit (429). The one definition, shared by every
/// HTTP retry path that classifies by `reqwest::StatusCode`.
pub fn is_transient_status(status: reqwest::StatusCode) -> bool {
    status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS
}

/// One attempt's outcome for [`retry_classified`], for callers whose retry/permanent
/// decision is made inline rather than by inspecting an error type — e.g. a Cover
/// Art Archive 404 is `Done` ("no cover exists"), a valid answer, not an error.
pub enum ClassifiedAttempt<T, E> {
    /// A final answer — return it.
    Done(T),
    /// A transient failure — retry until attempts run out, then return this error.
    Retry(E),
    /// A permanent failure — return it immediately.
    Permanent(E),
}

/// Retry `attempt` up to `max_attempts` times, waiting `retry_delay(attempt)`
/// between tries, until it returns [`ClassifiedAttempt::Done`] or
/// [`ClassifiedAttempt::Permanent`], or attempts run out. The sibling of
/// [`retry_with_backoff_if`] for callers that classify each try inline (see
/// [`ClassifiedAttempt`]) instead of returning `Result` + a `should_retry`
/// predicate.
pub async fn retry_classified<T, E, F, Fut, Delay>(
    max_attempts: u32,
    label: &str,
    retry_delay: Delay,
    mut attempt: F,
) -> Result<T, E>
where
    E: Display,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ClassifiedAttempt<T, E>>,
    Delay: Fn(u32) -> Duration,
{
    assert!(max_attempts > 0, "max_attempts must be greater than zero");

    let mut last_error: Option<E> = None;
    for attempt_index in 1..=max_attempts {
        match attempt().await {
            ClassifiedAttempt::Done(value) => return Ok(value),
            ClassifiedAttempt::Permanent(error) => return Err(error),
            ClassifiedAttempt::Retry(error) => {
                if attempt_index == max_attempts {
                    warn!(
                        "{} failed after {} attempts: {}",
                        label, max_attempts, error
                    );
                    return Err(error);
                }
                warn!(
                    "{} failed (attempt {}/{}): {} — retrying",
                    label, attempt_index, max_attempts, error
                );
                last_error = Some(error);
                tokio::time::sleep(retry_delay(attempt_index)).await;
            }
        }
    }

    // The loop returns on the final Retry, so reaching here is impossible.
    Err(last_error.expect("the retry loop ran at least once"))
}

/// Retry `f` up to `max_attempts` times, waiting `retry_delay(attempt)` between
/// tries, for as long as `should_retry` says the failure is worth repeating.
///
/// The predicate is required: most of what an API client returns is an answer,
/// not a fault, and a retry that can't tell the difference asks three times to
/// be told "not found" three times.
pub async fn retry_with_backoff_if<F, Fut, T, E, ShouldRetry, Delay>(
    max_attempts: u32,
    label: &str,
    should_retry: ShouldRetry,
    retry_delay: Delay,
    f: F,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: Display,
    ShouldRetry: Fn(&E) -> bool,
    Delay: Fn(u32) -> Duration,
{
    assert!(max_attempts > 0, "max_attempts must be greater than zero");

    for attempt in 1..=max_attempts {
        match f().await {
            Ok(result) => return Ok(result),
            Err(error) if !should_retry(&error) => return Err(error),
            Err(error) => {
                if attempt == max_attempts {
                    warn!("{} failed after {} attempts", label, max_attempts);
                    return Err(error);
                }
                warn!(
                    "{} failed (attempt {}/{}): {}",
                    label, attempt, max_attempts, error
                );
                tokio::time::sleep(retry_delay(attempt)).await;
            }
        }
    }

    unreachable!("max_attempts is greater than zero")
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::{is_transient_status, retry_classified, retry_with_backoff_if, ClassifiedAttempt};

    fn no_delay(_: u32) -> Duration {
        Duration::ZERO
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
        let result: Result<(), &str> = retry_classified(3, "test", no_delay, || async {
            attempts.fetch_add(1, Ordering::SeqCst);
            ClassifiedAttempt::Permanent("permanent")
        })
        .await;
        assert_eq!(result, Err("permanent"));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_classified_exhausts_attempts_on_retry() {
        let attempts = AtomicUsize::new(0);
        let result: Result<(), &str> = retry_classified(3, "test", no_delay, || async {
            attempts.fetch_add(1, Ordering::SeqCst);
            ClassifiedAttempt::Retry("transient")
        })
        .await;
        assert_eq!(result, Err("transient"));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_classified_returns_done() {
        let attempts = AtomicUsize::new(0);
        let result: Result<&str, &str> = retry_classified(3, "test", no_delay, || async {
            let n = attempts.fetch_add(1, Ordering::SeqCst);
            if n < 1 {
                ClassifiedAttempt::Retry("transient")
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
            3,
            "test operation",
            |_| false,
            |_| Duration::from_millis(1),
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
            3,
            "test operation",
            |_| true,
            |_| Duration::from_millis(1),
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
}

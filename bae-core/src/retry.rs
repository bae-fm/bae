use std::fmt::Display;
use std::time::Duration;
use tracing::warn;

/// Linear backoff: 500ms × attempt. What the API clients want; diagnostics wants
/// a flat delay instead, which is why the delay stays a parameter.
pub fn linear_backoff(attempt: u32) -> Duration {
    Duration::from_millis(500 * u64::from(attempt))
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

    use super::retry_with_backoff_if;

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

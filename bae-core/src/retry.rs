use std::fmt::Display;
use tracing::warn;

/// Retry an async operation, backing off linearly between attempts.
///
/// Calls `f` up to `max_attempts` times. On failure, waits 500ms * attempt
/// before retrying. Returns the first successful result, or the last error.
pub async fn retry_with_backoff<F, Fut, T, E>(max_attempts: u32, label: &str, f: F) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: Display,
{
    let mut last_err = None;
    for attempt in 1..=max_attempts {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if attempt < max_attempts {
                    warn!(
                        "{} failed (attempt {}/{}): {}",
                        label, attempt, max_attempts, e
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64))
                        .await;
                }
                last_err = Some(e);
            }
        }
    }

    warn!("{} failed after {} attempts", label, max_attempts);
    Err(last_err.unwrap())
}

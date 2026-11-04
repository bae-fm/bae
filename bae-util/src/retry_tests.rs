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
        spans.iter().map(|wait| wait.as_secs_f64().round() as u64).collect::<Vec<_>>(),
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

#[test]
fn for_status_repeats_transient_statuses_after_the_told_wait() {
    use reqwest::StatusCode;
    let told = Some(Duration::from_secs(7));
    assert_eq!(
        Repeat::for_status(StatusCode::SERVICE_UNAVAILABLE, told),
        Repeat::AfterToldWait(Duration::from_secs(7)),
    );
    assert_eq!(Repeat::for_status(StatusCode::TOO_MANY_REQUESTS, None), Repeat::AfterBackoff);
    assert_eq!(Repeat::for_status(StatusCode::FORBIDDEN, told), Repeat::Never);
    assert_eq!(Repeat::for_recorded_status(None, None), Repeat::AfterBackoff);
    assert_eq!(Repeat::for_recorded_status(Some(404), None), Repeat::Never);
    assert_eq!(Repeat::for_recorded_status(Some(502), None), Repeat::AfterBackoff);
}

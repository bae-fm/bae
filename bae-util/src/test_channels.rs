//! Waiting on a channel of events for the one a test is after.

/// How long a test waits on an event before failing instead of hanging.
/// Every wait it bounds ends on the code's own event, so only code that never
/// sends it reaches this. It is not a measure of speed, so it sits far above
/// what any step takes on a loaded machine.
pub const HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(300);

/// Read events off `rx` until `matches` returns a value, discarding the ones it
/// declines, and give up after `timeout_duration` (or when the channel closes)
/// with `None`.
///
/// `matches` takes each event by value, so it can move data out of the one it
/// accepts, and it is `FnMut`, so it can also accumulate across the events it
/// declines — which is what a "watch the whole crossing, then report what was
/// seen" wait needs.
pub async fn next_matching<E, T>(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<E>,
    timeout_duration: std::time::Duration,
    mut matches: impl FnMut(E) -> Option<T>,
) -> Option<T> {
    let deadline = std::time::Instant::now() + timeout_duration;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(event)) => {
                if let Some(found) = matches(event) {
                    return Some(found);
                }
            }
            // The channel closed: no later event can arrive, so waiting out the
            // deadline would only spin.
            Ok(None) => return None,
            Err(_) => return None,
        }
    }
}

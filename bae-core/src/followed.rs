//! A test's hold on one live query: the first value it reports that a test
//! is waiting for, failing loudly when none comes.

use std::time::Duration;

/// How long a test waits for the value it expects before failing.
const PATIENCE: Duration = Duration::from_secs(20);

/// The first value `query` reports that `matches` accepts. Panics after
/// twenty seconds without one, on a failed read, and when the query ends.
pub async fn value_matching<T: Clone + PartialEq + std::fmt::Debug + Send + 'static>(
    query: &mut coven::LiveQuery<T>,
    mut matches: impl FnMut(&T) -> bool,
) -> T {
    let mut last = None;
    let found = tokio::time::timeout(PATIENCE, async {
        loop {
            let value = query
                .next()
                .await
                .expect("the live query ended")
                .expect("the live query failed");
            if matches(&value) {
                return value;
            }
            last = Some(value);
        }
    })
    .await;
    match found {
        Ok(value) => value,
        Err(_) => panic!("no matching value within {PATIENCE:?}; the last was {last:?}"),
    }
}

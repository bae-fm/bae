//! A test's hold on one live source of values: the first value it reports
//! that a test is waiting for, failing loudly when none comes.

use bae_util::test_channels::HANG_GUARD;

/// The first value `query` reports that `matches` accepts. Panics when none
/// ever comes, on a failed read, and when the query ends.
pub async fn value_matching<T: Clone + PartialEq + std::fmt::Debug + Send + 'static>(
    query: &mut impl bae_db::live_query::LiveSource<T>,
    mut matches: impl FnMut(&T) -> bool,
) -> T {
    let mut last = None;
    let found = tokio::time::timeout(HANG_GUARD, async {
        loop {
            let value =
                query.next().await.expect("the live query ended").expect("the live query failed");
            if matches(&value) {
                return value;
            }
            last = Some(value);
        }
    })
    .await;
    match found {
        Ok(value) => value,
        Err(_) => panic!("no matching value ever came; the last was {last:?}"),
    }
}

/// [`value_matching`] for the values a library hands out, which the library
/// pulls from one live source: the first `values` gives that `matches`
/// accepts, panicking as `value_matching` does.
pub async fn pulled_matching<T: Clone + PartialEq + std::fmt::Debug + Send + 'static>(
    values: &bae_db::live_query::LiveValues<T>,
    mut matches: impl FnMut(&T) -> bool,
) -> T {
    let mut last = None;
    let found = tokio::time::timeout(HANG_GUARD, async {
        loop {
            let value = values.next().await.expect("the values stop only when cancelled");
            if matches(&value) {
                return value;
            }
            last = Some(value);
        }
    })
    .await;
    match found {
        Ok(value) => value,
        Err(_) => panic!("no matching value ever came; the last was {last:?}"),
    }
}

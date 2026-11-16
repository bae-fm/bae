//! A test's hold on one live source of values: the first value it reports
//! that a test is waiting for, failing loudly when none comes.

use bae_util::test_channels::HANG_GUARD;

/// The first value `query` reports that `matches` accepts. Panics when none
/// ever comes, on a failed read, and when the query ends.
pub async fn value_matching<T: Clone + PartialEq + std::fmt::Debug + Send + 'static>(
    query: &mut impl bae_store::live_query::LiveSource<T>,
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

/// [`value_matching`] for a coven live query, which the test reads on its
/// own thread.
pub async fn query_matching<T, E>(
    query: &mut coven::LiveQuery<T, E>,
    matches: impl FnMut(&T) -> bool,
) -> T
where
    T: Clone + PartialEq + std::fmt::Debug + Send + 'static,
    E: From<coven::DbError> + coven::task::CarriesBug + Into<bae_store::DbError> + Send + 'static,
{
    value_matching(&mut Inline(query), matches).await
}

/// A coven live query that a test reads on its own thread, as a live source.
struct Inline<'a, T, E>(&'a mut coven::LiveQuery<T, E>);

impl<T, E> bae_store::live_query::LiveSource<T> for Inline<'_, T, E>
where
    T: Clone + PartialEq + Send + 'static,
    E: From<coven::DbError> + coven::task::CarriesBug + Into<bae_store::DbError> + Send + 'static,
{
    fn next(&mut self) -> bae_store::live_query::NextValue<'_, T> {
        Box::pin(async move { Some(self.0.next_inline().await?.map_err(Into::into)) })
    }
}

/// [`value_matching`] for the values a library hands out, which the library
/// pulls from one live source: the first `values` gives that `matches`
/// accepts, panicking as `value_matching` does.
pub async fn pulled_matching<T: Clone + PartialEq + std::fmt::Debug + Send + 'static>(
    values: &bae_store::live_query::LiveValues<T>,
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

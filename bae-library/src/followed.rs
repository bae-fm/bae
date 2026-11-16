//! A test's hold on one live source of values: the first value it reports
//! that a test is waiting for, failing loudly when none comes.

use bae_util::test_channels::HANG_GUARD;

/// What a test follows values of: a coven live query, read on the test's
/// own thread, or coven's sync state beside one.
pub trait Followed<T> {
    /// The next value, a failed read, or `None` once the source has ended.
    fn next_value(
        &mut self,
    ) -> impl std::future::Future<Output = Option<Result<T, bae_store::DbError>>> + Send;
}

impl<T, E> Followed<T> for coven::LiveQuery<T, E>
where
    T: Clone + PartialEq + Send + 'static,
    E: From<coven::DbError> + coven::task::CarriesBug + Into<bae_store::DbError> + Send + 'static,
{
    async fn next_value(&mut self) -> Option<Result<T, bae_store::DbError>> {
        Some(self.next_inline().await?.map_err(Into::into))
    }
}

impl<R, T> Followed<T> for bae_store::WithSyncState<R, T>
where
    R: Clone + PartialEq + Send + 'static,
    T: Clone + PartialEq + Send + 'static,
{
    async fn next_value(&mut self) -> Option<Result<T, bae_store::DbError>> {
        self.next().await
    }
}

/// The first value `query` reports that `matches` accepts. Panics when none
/// ever comes, on a failed read, and when the query ends.
pub async fn value_matching<T: Clone + PartialEq + std::fmt::Debug + Send + 'static>(
    query: &mut impl Followed<T>,
    mut matches: impl FnMut(&T) -> bool,
) -> T {
    let mut last = None;
    let found = tokio::time::timeout(HANG_GUARD, async {
        loop {
            let value = query
                .next_value()
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
        Err(_) => panic!("no matching value ever came; the last was {last:?}"),
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

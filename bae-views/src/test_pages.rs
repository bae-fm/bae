//! A page of a list, pulled as a media browser pulls it.

use bae_store::live_query::LiveValues;
use bae_util::test_channels::HANG_GUARD;

/// `page`'s values, one per pull.
pub(crate) fn page_values(
    page: coven::LiveQuery<crate::ListRows, bae_store::DbError>,
) -> LiveValues<crate::ListRows> {
    LiveValues::new(
        page,
        tokio_util::sync::CancellationToken::new(),
        coven::task::HeardBugs::shared(),
    )
}

/// The page's next value.
pub(crate) async fn next_page(values: &LiveValues<crate::ListRows>) -> crate::ListRows {
    tokio::time::timeout(HANG_GUARD, values.next())
        .await
        .expect("a page answers")
        .expect("the page reads")
}

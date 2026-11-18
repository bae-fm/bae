//! A page of a list as a live source: the page's first place is read once,
//! and the page's live read takes the rows from there.

use super::values::{IntoLiveSource, LiveSource, NextValue};
use crate::live_results::LiveResults;
use crate::DbError;
use std::future::Future;
use std::pin::Pin;

/// The future that opens a page's live read: a plain read of the key the
/// page starts at, then the live query of the page's rows from it.
pub type OpenPage<T> =
    Pin<Box<dyn Future<Output = Result<coven::LiveQuery<T, DbError>, DbError>> + Send>>;

/// A page of a list, read live ([`crate::Store::subscribe_list_page`]). Its
/// first pull opens the page's live read; each pull after takes that read's
/// next value. A pull whose opening fails answers with the failure, and the
/// next pull opens the read again.
pub struct LivePage<T> {
    open: Box<dyn Fn() -> OpenPage<T> + Send + Sync>,
}

impl<T> LivePage<T> {
    /// The page `open` opens.
    pub fn new(open: impl Fn() -> OpenPage<T> + Send + Sync + 'static) -> Self {
        Self { open: Box::new(open) }
    }
}

/// A page's source: how it opens, and once open, its live read's results.
struct PageSource<T> {
    open: Box<dyn Fn() -> OpenPage<T> + Send + Sync>,
    query: Option<LiveResults<coven::LiveQuery<T, DbError>>>,
    bugs: coven::task::BugSinkRef,
}

impl<T> PageSource<T> {
    /// A source that opens its page's live query on its first pull, through
    /// `open`, reporting a panic around a coven run to `bugs`.
    fn new(
        open: Box<dyn Fn() -> OpenPage<T> + Send + Sync>,
        bugs: coven::task::BugSinkRef,
    ) -> Self {
        Self { open, query: None, bugs }
    }
}

impl<T: Clone + PartialEq + Send + 'static> LiveSource<T> for PageSource<T> {
    fn next(&mut self) -> NextValue<'_, T> {
        Box::pin(async move {
            let query = match &mut self.query {
                Some(query) => query,
                None => match (self.open)().await {
                    Ok(query) => self.query.insert(LiveResults::new(query, self.bugs.clone())),
                    Err(error) => return Some(Err(error)),
                },
            };
            query.next().await
        })
    }
}

impl<T: Clone + PartialEq + Send + 'static> IntoLiveSource<T> for LivePage<T> {
    fn into_live_source(self, bugs: coven::task::BugSinkRef) -> Box<dyn LiveSource<T>> {
        Box::new(PageSource::new(self.open, bugs))
    }
}

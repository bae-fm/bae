//! A page of a list as a live source: a live read of the key of the row at
//! the page's first place, and a live read of the page's rows from that key,
//! which moves to each new key.

use super::values::{IntoLiveSource, LiveSource, NextValue};
use crate::list_windows::{ListWindow, OrderKey};
use crate::live_results::LiveResults;
use crate::DbError;

/// A page of a list, read live ([`crate::Store::subscribe_list_page`]): the
/// rows from the row at the page's first place, at most the page's limit of
/// them, none past the list's end.
pub struct LivePage<T> {
    /// The key of the row at the page's first place, `None` past the list's
    /// end.
    key: coven::LiveQuery<Option<OrderKey>, DbError>,
    /// The page's rows from the window it is asked for.
    rows: coven::ReconfigurableLiveQuery<ListWindow, T, DbError>,
    limit: u64,
}

impl<T> LivePage<T> {
    /// The page whose first row's key `key` reads, whose `limit` rows from
    /// that key `rows` reads. `rows` answers nothing until the page asks it
    /// for the window from `key`'s first answer.
    pub fn new(
        key: coven::LiveQuery<Option<OrderKey>, DbError>,
        rows: coven::ReconfigurableLiveQuery<ListWindow, T, DbError>,
        limit: u64,
    ) -> Self {
        Self { key, rows, limit }
    }
}

/// A page's source: its two live reads' results, and the window it asked
/// its rows read for.
struct PageSource<T> {
    key: LiveResults<coven::LiveQuery<Option<OrderKey>, DbError>>,
    rows: LiveResults<coven::ReconfigurableLiveQuery<ListWindow, T, DbError>>,
    requests: coven::LiveQueryRequests<ListWindow>,
    limit: u64,
    /// The window from the key's last answer; `None` before its first.
    window: Option<ListWindow>,
}

/// What a page's source heard first: a new key, or its rows' read.
enum Heard<T> {
    Key(Result<Option<OrderKey>, DbError>),
    Rows(coven::ReconfigurableLiveQueryEvent<ListWindow, T, DbError>),
}

impl<T: Clone + PartialEq + Send + 'static> PageSource<T> {
    /// A source over `page`'s reads, reporting a panic around a coven run to
    /// `bugs`.
    fn new(page: LivePage<T>, bugs: coven::task::BugSinkRef) -> Self {
        let requests = page.rows.requests();
        Self {
            key: LiveResults::new(page.key, bugs.clone()),
            rows: LiveResults::new(page.rows, bugs),
            requests,
            limit: page.limit,
            window: None,
        }
    }

    /// Ask the rows read for the window from `key`: the page's rows from
    /// that key, or none past the list's end. `None` once the rows read has
    /// ended.
    fn move_to(&mut self, key: Option<OrderKey>) -> Option<()> {
        let window = match key {
            Some(from) => ListWindow { from: Some(from), limit: self.limit },
            None => ListWindow::top(0),
        };
        self.requests.set(window.clone()).ok()?;
        self.window = Some(window);
        Some(())
    }
}

impl<T: Clone + PartialEq + Send + 'static> LiveSource<T> for PageSource<T> {
    fn next(&mut self) -> NextValue<'_, T> {
        Box::pin(async move {
            if self.window.is_none() {
                match self.key.next().await? {
                    Ok(key) => self.move_to(key)?,
                    Err(error) => return Some(Err(error)),
                }
            }
            loop {
                let heard = tokio::select! {
                    key = self.key.next() => Heard::Key(key?),
                    event = self.rows.next() => Heard::Rows(event?),
                };
                match heard {
                    Heard::Key(Ok(key)) => self.move_to(key)?,
                    Heard::Key(Err(error)) => return Some(Err(error)),
                    // A read of a window the page has moved from answers
                    // nothing.
                    Heard::Rows(event) if Some(&event.request) == self.window.as_ref() => {
                        return Some(event.result)
                    }
                    Heard::Rows(_) => {}
                }
            }
        })
    }
}

impl<T: Clone + PartialEq + Send + 'static> IntoLiveSource<T> for LivePage<T> {
    fn into_live_source(self, bugs: coven::task::BugSinkRef) -> Box<dyn LiveSource<T>> {
        Box::new(PageSource::new(self, bugs))
    }
}

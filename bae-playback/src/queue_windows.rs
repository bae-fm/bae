//! The windows of the context's upcoming tail a queue screen asks for. The
//! tail is read by offset: the queue is a lane of places a position counts
//! through, which a write moves entries within, so no key names a place the
//! way a list's sort values do.

use std::collections::BTreeSet;

/// `limit` entries of the upcoming tail, counted from `offset` after the
/// playing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct QueueWindow {
    pub offset: u64,
    pub limit: u64,
}

impl QueueWindow {
    pub fn at(offset: u64, limit: u64) -> Self {
        Self { offset, limit }
    }
}

/// The windows a screen asks for, each once, in offset order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueueWindows(BTreeSet<QueueWindow>);

impl QueueWindows {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn iter(&self) -> impl Iterator<Item = &QueueWindow> {
        self.0.iter()
    }
}

impl<const N: usize> From<[QueueWindow; N]> for QueueWindows {
    fn from(windows: [QueueWindow; N]) -> Self {
        Self(windows.into())
    }
}

impl FromIterator<QueueWindow> for QueueWindows {
    fn from_iter<I: IntoIterator<Item = QueueWindow>>(windows: I) -> Self {
        Self(windows.into_iter().collect())
    }
}

/// Each window a read names holds that window's entries as of the read,
/// whatever windows are set since.
impl bae_store::live_query::LiveRequest for QueueWindows {
    fn still_answers(&self, _current: &Self) -> bool {
        true
    }
}

/// One window's entries as one read found them.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueWindowRead<Row> {
    pub window: QueueWindow,
    pub rows: Vec<Row>,
}

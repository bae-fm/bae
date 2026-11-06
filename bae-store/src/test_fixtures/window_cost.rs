//! How what one window of a paged list costs to read is measured: at each
//! size, the window at the top, the middle and the end, reached by a jump,
//! which reads it by its offset, and by scrolling, which reads it from the
//! row beside it (`list_windows`), each read the median of several runs.

use crate::list_windows::{read_windows, Keyed, ListOrder, Places, Seek};
use crate::{DbError, SortDirection, Store};
use coven::SqlReadContext;
use std::time::{Duration, Instant};
/// The sizes the lists are measured at, each reached by adding rows to the
/// library the size before it built.
pub const SIZES: [u64; 3] = [20_000, 50_000, 100_000];

/// How many rows one window holds: the macOS list's page.
pub const LIMIT: u64 = 50;

/// How many times each window is read; the median is reported.
pub const RUNS: usize = 15;

/// A recursive `k(i)` counting `start..end`, to put a statement before that
/// inserts one row for each `i`.
pub fn numbered(start: u64, end: u64) -> String {
    format!(
        "WITH RECURSIVE k(i) AS (SELECT {start} UNION ALL SELECT i + 1 FROM k WHERE i + 1 < {end})"
    )
}

/// The median time `read` takes over [`RUNS`] reads, and how many rows its
/// last read returned.
pub async fn median<F>(db: &Store, read: F) -> (Duration, usize)
where
    F: Fn(&SqlReadContext<'_>) -> Result<usize, DbError> + Send + Sync + Clone + 'static,
{
    let mut runs = Vec::with_capacity(RUNS);
    let mut rows = 0;
    for _ in 0..RUNS {
        let read = read.clone();
        let started = Instant::now();
        rows = db.read(move |sql| read(&sql)).await.unwrap();
        runs.push(started.elapsed());
    }
    runs.sort();
    (runs[RUNS / 2], rows)
}

/// The window offsets measured in a list of `total` rows: its top, its middle
/// and its last window.
pub fn offsets(total: u64) -> [(&'static str, u64); 3] {
    [("top", 0), ("middle", total / 2), ("end", total.saturating_sub(LIMIT))]
}

pub fn report(
    list: &str,
    order: &str,
    reach: &str,
    at: &str,
    offset: u64,
    n: u64,
    (time, rows): (Duration, usize),
) {
    eprintln!(
        "WINDOW_COST list={list} order={order} reach={reach} n={n} at={at} offset={offset} \
         rows={rows} median_ms={:.3}",
        time.as_secs_f64() * 1000.0
    );
}

/// Reads a list's rows under a seek, at most a limit from an offset, each
/// with its key: what a list's live query reads its windows with.
pub type Rows<R> =
    fn(&SqlReadContext<'_>, &ListOrder, &Seek, u64, u64) -> Result<Vec<Keyed<R>>, DbError>;

/// Read one window at each measured offset of a list of `n` rows in `order`,
/// once by a jump and once from the row beside it.
pub async fn measure<R: Send + 'static>(
    db: &Store,
    list: &str,
    name: &str,
    n: u64,
    order: ListOrder,
    rows: impl Fn(&SqlReadContext<'_>, &ListOrder, &Seek, u64, u64) -> Result<Vec<Keyed<R>>, DbError>
        + Clone
        + Send
        + Sync
        + 'static,
) {
    let order = std::sync::Arc::new(order);
    for (at, offset) in offsets(n) {
        let window =
            crate::LibraryPageWindows::from([crate::LibraryPageWindow { offset, limit: LIMIT }]);
        // The row beside the window: after it at the top, before it below.
        let beside = if offset == 0 { offset + LIMIT } else { offset - 1 };
        let key = {
            let (order, rows) = (order.clone(), rows.clone());
            db.read(move |sql| {
                Ok(rows(&sql, &order, &order.everything(), 1, beside)?
                    .pop()
                    .expect("the row beside a window is in the list")
                    .1)
            })
            .await
            .unwrap()
        };
        for (reach, known) in
            [("jump", Places::new()), ("scroll", Places::from([(beside, key.clone())]))]
        {
            let (order, window, rows) = (order.clone(), window.clone(), rows.clone());
            let timing = median(db, move |sql| {
                let read = read_windows(&order, &window, known.clone(), |seek, limit, at| {
                    rows(sql, &order, seek, limit, at)
                })?;
                Ok(read.windows[0].1.len())
            })
            .await;
            report(list, name, reach, at, offset, n, timing);
        }
    }
}

pub fn both_ways<F: Copy>(field: F) -> [(F, SortDirection); 2] {
    [(field, SortDirection::Ascending), (field, SortDirection::Descending)]
}

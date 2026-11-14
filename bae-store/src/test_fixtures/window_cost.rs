//! How what one window of a list costs to read is measured: at each size, the
//! window at the top, the middle and the end, reached by scrolling, which
//! reads the range that starts at a row's key (`list_windows`), and by a
//! jump, which reads the key at a position by offset and then that range;
//! and the position of a key, the count of the rows before it, at the middle
//! and the end. Each read is the median of several runs.

use crate::list_windows::{key_at, read_range, rows_before, Keyed, ListOrder, Seek};
use crate::{DbError, ListWindow, SortDirection, Store};
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

/// One measured number that is not a window's: a line `scripts/perf.sh` reads,
/// `metric` naming it without spaces and `unit` saying what `value` counts
/// (`ms`, `ratio`, `count`), lower being better in every unit.
pub fn report_cost(metric: &str, value: f64, unit: &str) {
    eprintln!("COST metric={metric} value={value:.4} unit={unit}");
}

/// Reads a list's rows under a seek, at most a limit from an offset, each
/// with its key: what a list's live query reads its windows with.
pub type Rows<R> =
    fn(&SqlReadContext<'_>, &ListOrder, &Seek, u64, u64) -> Result<Vec<Keyed<R>>, DbError>;

/// Counts the rows a seek holds: what a list's live position adds up.
pub type Count = fn(&SqlReadContext<'_>, &ListOrder, &Seek) -> Result<u64, DbError>;

/// Read one window at each measured position of a list of `n` rows in
/// `order`: by scrolling to it, by jumping to it, and, in the middle and at
/// the end, the position of its first row.
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
    count: impl Fn(&SqlReadContext<'_>, &ListOrder, &Seek) -> Result<u64, DbError>
        + Clone
        + Send
        + Sync
        + 'static,
) {
    let order = std::sync::Arc::new(order);
    for (at, position) in offsets(n) {
        let key = {
            let (order, rows) = (order.clone(), rows.clone());
            db.read(move |sql| {
                Ok(key_at(&order, position, &mut |seek, limit, offset| {
                    rows(&sql, &order, seek, limit, offset)
                })?
                .expect("the list has a row at the position"))
            })
            .await
            .unwrap()
        };
        let window = ListWindow::Forward { from: Some(key.clone()), until: None, limit: LIMIT };
        let scroll = {
            let (order, rows, window) = (order.clone(), rows.clone(), window.clone());
            median(db, move |sql| {
                let read = read_range(&order, &window, &mut |seek, limit, offset| {
                    rows(sql, &order, seek, limit, offset)
                })?;
                Ok(read.rows.len())
            })
            .await
        };
        report(list, name, "scroll", at, position, n, scroll);
        let jump = {
            let (order, rows) = (order.clone(), rows.clone());
            median(db, move |sql| {
                let mut read = |seek: &Seek, limit, offset| rows(sql, &order, seek, limit, offset);
                let key = key_at(&order, position, &mut read)?.expect("a row at the position");
                let window = ListWindow::Forward { from: Some(key), until: None, limit: LIMIT };
                Ok(read_range(&order, &window, &mut read)?.rows.len())
            })
            .await
        };
        report(list, name, "jump", at, position, n, jump);
        if at != "top" {
            let (order, count, key) = (order.clone(), count.clone(), key);
            let place = median(db, move |sql| {
                Ok(rows_before(&order, &key, |seek| count(sql, &order, seek))? as usize)
            })
            .await;
            report(list, name, "position", at, position, n, place);
        }
    }
}

pub fn both_ways<F: Copy>(field: F) -> [(F, SortDirection); 2] {
    [(field, SortDirection::Ascending), (field, SortDirection::Descending)]
}

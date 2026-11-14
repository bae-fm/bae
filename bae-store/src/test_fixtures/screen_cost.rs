//! How what one screen's read costs is measured: a one-off read, a live
//! query's first value, a reconfigurable live query moved to another request,
//! the rerun a write causes and a write itself. Each prints the line
//! `scripts/perf.sh` reads, `COST metric= value= unit=ms bar=`, the bar being
//! what the screen needs of that number.

use crate::live_query::{CancellableLiveQuery, LiveSource};
use crate::test_fixtures::window_cost::RUNS;
use crate::{DbError, Store};
use coven::SqlReadContext;
use std::future::Future;
use std::time::{Duration, Instant};

/// A measured time as a line `scripts/perf.sh` reads, with its bar.
pub fn report_time(metric: &str, time: Duration, bar_ms: f64) {
    eprintln!("{}", cost_line(metric, time, Some(bar_ms)));
}

/// The line a time is reported as: `bar_ms` is left off for a number with
/// none.
fn cost_line(metric: &str, time: Duration, bar_ms: Option<f64>) -> String {
    let bar = bar_ms.map(|bar| format!(" bar={bar}")).unwrap_or_default();
    format!("COST metric={metric} value={:.4} unit=ms{bar}", time.as_secs_f64() * 1000.0)
}

/// A measured time with no bar: a write's, whose gain is the number.
fn report_unbarred(metric: &str, time: Duration) {
    eprintln!("{}", cost_line(metric, time, None));
}

fn median(mut runs: Vec<Duration>) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
}

/// The median time `read` takes over [`RUNS`] reads of `db`, reported as
/// `metric` against `bar_ms`.
pub async fn read_cost<R, F>(db: &Store, metric: &str, bar_ms: f64, read: F)
where
    R: Send + 'static,
    F: Fn(&SqlReadContext<'_>) -> Result<R, DbError> + Send + Sync + Clone + 'static,
{
    let mut runs = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let read = read.clone();
        let started = Instant::now();
        db.read(move |sql| read(&sql)).await.expect("the measured read");
        runs.push(started.elapsed());
    }
    report_time(metric, median(runs), bar_ms);
}

/// The median time of [`RUNS`] calls of `call`, a screen's read as it runs
/// today (its public call), reported as `metric` against `bar_ms`.
pub async fn call_cost<T, E, Fut>(metric: &str, bar_ms: f64, mut call: impl FnMut() -> Fut)
where
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Debug,
{
    let mut runs = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let started = Instant::now();
        call().await.expect("the measured call");
        runs.push(started.elapsed());
    }
    report_time(metric, median(runs), bar_ms);
}

/// The plain query `sql`, bound to `params`, read [`RUNS`] times and the
/// median reported as `metric` against `bar_ms` (none for a number with no
/// bar): the baseline a screen's read is judged against. Returns how many
/// rows the query gives, so a scenario can check it read what it meant to.
pub async fn plain_cost(
    db: &Store,
    metric: &str,
    bar_ms: Option<f64>,
    sql: &str,
    params: Vec<rusqlite::types::Value>,
) -> usize {
    let sql = sql.to_string();
    plans(db, metric, {
        let (sql, params) = (sql.clone(), params.clone());
        move |read| {
            read.query(&sql, rusqlite::params_from_iter(params), |_| Ok(()))?;
            Ok(())
        }
    })
    .await;
    let first = Instant::now();
    let rows = {
        let sql = sql.clone();
        let params = params.clone();
        db.read(move |read| {
            Ok(read.query(&sql, rusqlite::params_from_iter(params), |_| Ok(()))?.len())
        })
        .await
        .expect("the plain query reads")
    };
    // A read over a second is repeated three times, not fifteen: its number
    // is its order of magnitude.
    let repeats = if first.elapsed() > Duration::from_secs(1) { 3 } else { RUNS };
    let mut runs = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let (sql, params) = (sql.clone(), params.clone());
        let started = Instant::now();
        db.read(move |read| {
            read.query(&sql, rusqlite::params_from_iter(params), |_| Ok(()))?;
            Ok(())
        })
        .await
        .expect("the plain query reads");
        runs.push(started.elapsed());
    }
    match bar_ms {
        Some(bar_ms) => report_time(metric, median(runs), bar_ms),
        None => report_unbarred(metric, median(runs)),
    }
    rows
}

/// A list's plain query: its rows, the one key expression its order compares
/// first and the unique expression that breaks ties, written over the
/// domain's own tables.
pub struct ListShape<'a> {
    /// The metric every number of this list is named after.
    pub metric: &'a str,
    /// The columns a window reads.
    pub select: &'a str,
    /// `FROM ... WHERE ...`, always with a WHERE (`WHERE 1` for none), so a
    /// cursor can be added with AND.
    pub source: &'a str,
    /// The order's first term, one expression.
    pub key: &'a str,
    /// The expression that makes the order total.
    pub tie: &'a str,
    pub descending: bool,
}

/// The size of a library seed by its name: S, M or L.
pub fn size_name(albums: u64) -> &'static str {
    match albums {
        2_000 => "S",
        20_000 => "M",
        100_000 => "L",
        _ => "?",
    }
}

/// What a list's plain reads cost at one size, each against its bar: its
/// count (50 ms), its window at the top, middle and end by jump (16 ms at M,
/// 50 ms at L) and its window beside the middle row by cursor (8 ms at M,
/// 16 ms at L). `size` is the seed's album count, which names the metrics
/// `<metric>/<read>@S|M|L`; the bars are M's for S and M.
pub async fn list_costs(db: &Store, size: u64, list: &ListShape<'_>) {
    use crate::test_fixtures::window_cost::{offsets, LIMIT};
    use rusqlite::types::Value;
    let at = size_name(size);
    let large = size >= 100_000;
    let (jump_bar, cursor_bar) = if large { (50.0, 16.0) } else { (16.0, 8.0) };
    let ListShape { metric, select, source, key, tie, descending } = list;
    let (direction, beyond) = if *descending { (" DESC", "<") } else { ("", ">") };
    let total = plain_cost(
        db,
        &format!("{metric}/count@{at}"),
        Some(50.0),
        &format!("SELECT COUNT(*) {source}"),
        vec![],
    )
    .await;
    assert_eq!(total, 1);
    let count: i64 = {
        let sql = format!("SELECT COUNT(*) {source}");
        db.read(move |read| read.query_row(&sql, [], |row| row.get(0)).map_err(Into::into))
            .await
            .expect("the list's count")
    };
    assert!(count > 0, "{metric} has no rows at {at}");
    let window = format!(
        "SELECT {select} {source} ORDER BY {key}{direction}, {tie}{direction} LIMIT {LIMIT} OFFSET ?"
    );
    for (place, offset) in offsets(count as u64) {
        let rows = plain_cost(
            db,
            &format!("{metric}/jump-{place}@{at}"),
            Some(jump_bar),
            &window,
            vec![Value::Integer(offset as i64)],
        )
        .await;
        assert!(rows > 0, "{metric} {place} window is empty at {at}");
    }
    let edge = format!(
        "SELECT {key}, {tie} {source} ORDER BY {key}{direction}, {tie}{direction} LIMIT 1 OFFSET ?"
    );
    let middle = count / 2;
    let (edge_key, edge_tie): (Value, Value) = db
        .read(move |read| {
            read.query_row(&edge, [middle], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(Into::into)
        })
        .await
        .expect("the row at the middle");
    let cursor = format!(
        "SELECT {select} {source} AND ({key}, {tie}) {beyond} (?, ?) \
         ORDER BY {key}{direction}, {tie}{direction} LIMIT {LIMIT}"
    );
    let rows = plain_cost(
        db,
        &format!("{metric}/cursor-middle@{at}"),
        Some(cursor_bar),
        &cursor,
        vec![edge_key, edge_tie],
    )
    .await;
    assert!(rows > 0, "{metric} cursor window is empty at {at}");
}

/// A live query whose request can be moved, and its next value awaited:
/// coven's own, or the cancellable one the surfaces hold.
pub trait Reconfigurable<Request, Projection> {
    /// Move the query to `request`.
    fn move_to(&mut self, request: Request);
    /// The value its latest run delivers.
    fn delivered(&mut self) -> impl Future<Output = Projection>;
}

impl<Request, Projection, E> Reconfigurable<Request, Projection>
    for coven::ReconfigurableLiveQuery<Request, Projection, E>
where
    Request: Clone + PartialEq + Send + Sync + 'static,
    Projection: Clone + PartialEq + Send + 'static,
    E: From<coven::DbError> + coven::task::CarriesBug + std::fmt::Debug + Send + 'static,
{
    fn move_to(&mut self, request: Request) {
        self.requests().set(request).expect("the subscription is open");
    }

    async fn delivered(&mut self) -> Projection {
        self.next().await.expect("the query stays open").result.expect("the query's read")
    }
}

impl<Request, Projection, E> Reconfigurable<Request, Projection>
    for CancellableLiveQuery<Request, Projection, E>
where
    Request: Clone + PartialEq + Send + Sync + 'static,
    Projection: Clone + PartialEq + Send + 'static,
    E: From<coven::DbError> + coven::task::CarriesBug + std::fmt::Debug + Send + 'static,
{
    fn move_to(&mut self, request: Request) {
        self.set(request).expect("the subscription is open");
    }

    async fn delivered(&mut self) -> Projection {
        self.next().await.expect("the query stays open").result.expect("the query's read")
    }
}

/// The median time a reconfigurable live query takes to deliver the value of
/// `request_b` after being moved to it from `request_a`, over [`RUNS`] moves
/// back and forth, reported as `metric` against `bar_ms`. The query is made
/// with `request_a`, its first value not yet taken.
pub async fn live_cost<Request: Clone, Projection>(
    query: &mut impl Reconfigurable<Request, Projection>,
    request_a: Request,
    request_b: Request,
    metric: &str,
    bar_ms: f64,
) {
    query.delivered().await;
    let mut runs = Vec::with_capacity(RUNS);
    for run in 0..2 * RUNS {
        let (target, measured) =
            if run % 2 == 0 { (&request_b, true) } else { (&request_a, false) };
        let started = Instant::now();
        query.move_to(target.clone());
        query.delivered().await;
        if measured {
            runs.push(started.elapsed());
        }
    }
    report_time(metric, median(runs), bar_ms);
}

/// The median time from subscribing to `make`'s live query to its first
/// value, over [`RUNS`] subscriptions, reported as `metric` against `bar_ms`.
pub async fn subscribe_cost<T, S>(mut make: impl FnMut() -> S, metric: &str, bar_ms: f64)
where
    S: LiveSource<T>,
{
    let mut runs = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let started = Instant::now();
        let mut source = make();
        source.next().await.expect("the query has a first value").expect("its first read");
        runs.push(started.elapsed());
        drop(source);
    }
    report_time(metric, median(runs), bar_ms);
}

/// For each of [`RUNS`] distinct writes (`write(run)`), the time the write
/// takes and then the time until `source` delivers a value `reflects` says
/// shows the write (`reflects(value, run)`), reported as `<metric>/write`
/// without a bar and `<metric>/delivered` against `bar_ms`. `source` must have
/// delivered its first value.
pub async fn rerun_cost<T, S, W, Fut>(
    source: &mut S,
    metric: &str,
    bar_ms: f64,
    mut write: W,
    reflects: impl Fn(&T, u64) -> bool,
) where
    S: LiveSource<T>,
    W: FnMut(u64) -> Fut,
    Fut: Future<Output = ()>,
{
    let mut writes = Vec::with_capacity(RUNS);
    let mut delivered = Vec::with_capacity(RUNS);
    for run in 0..RUNS as u64 {
        let started = Instant::now();
        write(run).await;
        writes.push(started.elapsed());
        loop {
            let value = source.next().await.expect("the query stays open").expect("its rerun");
            if reflects(&value, run) {
                break;
            }
        }
        delivered.push(started.elapsed());
    }
    report_unbarred(&format!("{metric}/write"), median(writes));
    report_time(&format!("{metric}/delivered"), median(delivered), bar_ms);
}

/// The median time of [`RUNS`] distinct writes, `write(run)` for each run,
/// reported as `metric` without a bar.
pub async fn write_cost<W, Fut>(metric: &str, mut write: W)
where
    W: FnMut(u64) -> Fut,
    Fut: Future<Output = ()>,
{
    let mut runs = Vec::with_capacity(RUNS);
    for run in 0..RUNS as u64 {
        let started = Instant::now();
        write(run).await;
        runs.push(started.elapsed());
    }
    report_unbarred(metric, median(runs));
}

/// [`write_cost`] where each write needs rows made first: `prepare(run)`
/// makes them, untimed, and only `write` of what it returns is timed, over
/// `count` runs.
pub async fn prepared_write_cost<P, PFut, W, WFut, T>(
    metric: &str,
    count: usize,
    mut prepare: P,
    mut write: W,
) where
    P: FnMut(u64) -> PFut,
    PFut: Future<Output = T>,
    W: FnMut(T) -> WFut,
    WFut: Future<Output = ()>,
{
    let mut runs = Vec::with_capacity(count);
    for run in 0..count as u64 {
        let prepared = prepare(run).await;
        let started = Instant::now();
        write(prepared).await;
        runs.push(started.elapsed());
    }
    report_unbarred(metric, median(runs));
}

/// Run `read` once and print the plan of each statement it ran, returning
/// the lines: a `PLAN` line per step of `EXPLAIN QUERY PLAN`, the statement
/// numbered in the order it ran and cut to its first 200 characters on one
/// line.
pub async fn plans<R, F>(db: &Store, metric: &str, read: F) -> Vec<String>
where
    R: Send + 'static,
    F: FnOnce(&SqlReadContext<'_>) -> Result<R, DbError> + Send + 'static,
{
    let log = db.record_statements_for_test().await.expect("record the read's statements");
    db.read(move |sql| read(&sql)).await.expect("the planned read");
    let mut lines = Vec::new();
    for (n, statement) in log.take().into_iter().enumerate() {
        let explained = format!("EXPLAIN QUERY PLAN {statement}");
        let steps: Vec<String> = db
            .read(move |sql| {
                sql.query(&explained, [], |row| row.get::<_, String>(3)).map_err(Into::into)
            })
            .await
            .expect("explain a recorded statement");
        let shown: String =
            statement.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect();
        for step in steps {
            lines.push(format!("PLAN metric={metric} n={n} sql={shown} step={step}"));
        }
    }
    for line in &lines {
        eprintln!("{line}");
    }
    lines
}

#[cfg(test)]
#[path = "screen_cost_tests.rs"]
mod tests;

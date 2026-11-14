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

#[cfg(test)]
#[path = "screen_cost_tests.rs"]
mod tests;

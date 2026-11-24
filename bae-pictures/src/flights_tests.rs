use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use bae_diagnostics::Diagnostics;
use bae_util::test_channels::HANG_GUARD;
use tokio::sync::Notify;

use crate::{FlightError, Flights, IsCancelled};

/// What a test job returns.
type Job = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, String>> + Send>>;

#[expect(clippy::disallowed_methods, reason = "test code: it takes the runtime the test runs on")]
fn flights() -> Flights<&'static str, Vec<u8>, String> {
    Flights::new(Diagnostics::noop(), tokio::runtime::Handle::current())
}

/// A job that counts its starts and finishes when told.
fn job(starts: Arc<AtomicUsize>, finish: Arc<Notify>) -> impl FnOnce(IsCancelled) -> Job {
    move |_cancel| {
        starts.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            finish.notified().await;
            Ok(vec![1, 2, 3])
        })
    }
}

/// Polls `request` once, so that it has joined its job, and hands it back
/// pending. A request joins its job in its first poll.
async fn join_first<F: std::future::Future + Unpin>(request: &mut F) {
    tokio::select! {
        biased;
        _ = request => panic!("the request finished before the job was released"),
        () = async {} => {}
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn two_requests_for_one_key_run_one_job() {
    let flights = flights();
    let starts = Arc::new(AtomicUsize::new(0));
    let finish = Arc::new(Notify::new());
    let mut first = Box::pin(flights.request("cover", job(starts.clone(), finish.clone())));
    let mut second = Box::pin(flights.request("cover", job(starts.clone(), finish.clone())));
    join_first(&mut first).await;
    join_first(&mut second).await;
    // Both are waiting on the job, which the permit lets through.
    finish.notify_one();
    let (first, second) = tokio::join!(first, second);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    let (first, second) = (first.unwrap(), second.unwrap());
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(*first, vec![1, 2, 3]);
}

/// A job that tells when it has started, asks its flag the way a reader does
/// before each chunk, and tells when it saw the flag set.
fn reading_job(
    started: Arc<Notify>,
    saw_cancel: tokio::sync::oneshot::Sender<()>,
) -> impl FnOnce(IsCancelled) -> Job {
    move |cancel| {
        Box::pin(async move {
            started.notify_one();
            loop {
                if cancel() {
                    saw_cancel.send(()).expect("the test waits for the job");
                    return Err("cancelled".to_string());
                }
                tokio::task::yield_now().await;
            }
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_cancelled_mid_read_stops_the_read() {
    let flights = flights();
    let started = Arc::new(Notify::new());
    let (saw_cancel, cancelled) = tokio::sync::oneshot::channel();
    let request = flights.request("scan", reading_job(started.clone(), saw_cancel));
    // The caller goes away once the job is reading.
    tokio::select! {
        _ = request => panic!("the job never ends before it is cancelled"),
        () = started.notified() => {}
    }
    // The request's drop set the flag, and the job saw it between two reads.
    tokio::time::timeout(HANG_GUARD, cancelled)
        .await
        .expect("the job stopped")
        .expect("the job told that it saw its flag");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_runs_on_while_another_request_waits() {
    let flights = flights();
    let starts = Arc::new(AtomicUsize::new(0));
    let finish = Arc::new(Notify::new());
    let mut leaving = Box::pin(flights.request("cover", job(starts.clone(), finish.clone())));
    let mut staying = Box::pin(flights.request("cover", job(starts.clone(), finish.clone())));
    join_first(&mut leaving).await;
    join_first(&mut staying).await;
    drop(leaving);
    finish.notify_one();
    let copy = staying.await.unwrap();
    assert_eq!(*copy, vec![1, 2, 3]);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_after_the_end_starts_a_new_job() {
    let flights = flights();
    let starts = Arc::new(AtomicUsize::new(0));
    for _ in 0..2 {
        let starts = starts.clone();
        let result = flights
            .request("cover", move |_cancel| {
                starts.fetch_add(1, Ordering::SeqCst);
                async { Ok::<_, String>(vec![9]) }
            })
            .await;
        assert_eq!(*result.unwrap(), vec![9]);
    }
    assert_eq!(starts.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_job_fails_every_request() {
    let flights = flights();
    let result = flights
        .request("cover", |_cancel| async { Err::<Vec<u8>, _>("no such file".to_string()) })
        .await;
    match result {
        Err(FlightError::Failed(error)) => assert_eq!(*error, "no such file"),
        other => panic!("{other:?}"),
    }
}

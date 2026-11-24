use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bae_diagnostics::Diagnostics;
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

#[tokio::test(flavor = "multi_thread")]
async fn two_requests_for_one_key_run_one_job() {
    let flights = flights();
    let starts = Arc::new(AtomicUsize::new(0));
    let finish = Arc::new(Notify::new());
    let first = flights.request("cover", job(starts.clone(), finish.clone()));
    let second = flights.request("cover", job(starts.clone(), finish.clone()));
    let both = async { tokio::join!(first, second) };
    let release = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        finish.notify_waiters();
    };
    let ((first, second), ()) = tokio::join!(both, release);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    let (first, second) = (first.unwrap(), second.unwrap());
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(*first, vec![1, 2, 3]);
}

/// A job that polls its flag the way a reader does between chunks, counts its
/// polls, and ends with an error once the flag is set.
fn reading_job(
    polls: Arc<AtomicUsize>,
    saw_cancel: Arc<AtomicUsize>,
) -> impl FnOnce(IsCancelled) -> Job {
    move |cancel| {
        Box::pin(async move {
            loop {
                if cancel() {
                    saw_cancel.fetch_add(1, Ordering::SeqCst);
                    return Err("cancelled".to_string());
                }
                polls.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_cancelled_mid_read_stops_the_read() {
    let flights = flights();
    let polls = Arc::new(AtomicUsize::new(0));
    let saw_cancel = Arc::new(AtomicUsize::new(0));
    let request = flights.request("scan", reading_job(polls.clone(), saw_cancel.clone()));
    // The request waits for a while, then its caller goes away.
    assert!(tokio::time::timeout(Duration::from_millis(60), request).await.is_err());
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(saw_cancel.load(Ordering::SeqCst), 1, "the job saw its flag set");
    let after = polls.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(polls.load(Ordering::SeqCst), after, "the job read on after it was cancelled");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_runs_on_while_another_request_waits() {
    let flights = flights();
    let starts = Arc::new(AtomicUsize::new(0));
    let finish = Arc::new(Notify::new());
    let leaving = flights.request("cover", job(starts.clone(), finish.clone()));
    let staying = bae_util::test_spawn::task({
        let (flights, starts, finish) = (flights.clone(), starts.clone(), finish.clone());
        async move { flights.request("cover", job(starts, finish)).await }
    });
    assert!(tokio::time::timeout(Duration::from_millis(30), leaving).await.is_err());
    finish.notify_waiters();
    let copy = staying.await.unwrap().unwrap();
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

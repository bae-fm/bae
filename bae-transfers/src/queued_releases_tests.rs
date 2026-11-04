use super::*;
use crate::release_queue::ReleaseQueueState;
use std::collections::BTreeMap;

const RELEASE_A: &str = "0252dedb-ee39-4547-8803-438dbeb57a64";
const RELEASE_B: &str = "64e79a1f-404a-4c34-809a-a3cb44bf1942";

async fn queue() -> (QueuedReleases<(), u8>, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let database = Database::new_test(
        tmp.path().join("test.db").to_str().unwrap(),
        Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    (QueuedReleases::new(database), tmp)
}

fn entry(release_id: &str) -> ReleaseQueueEntry<()> {
    ReleaseQueueEntry { release_id: release_id.to_string(), created_at: 0, payload: () }
}

/// Each entry's phase in the table, in order.
async fn phases(queue: &QueuedReleases<(), u8>) -> Vec<(String, ReleaseQueueState<()>)> {
    queue
        .snapshot()
        .await
        .unwrap()
        .rows
        .into_iter()
        .map(|row| (row.op.release_id, row.op.state))
        .collect()
}

/// Wait until the table holds what `done` looks for.
async fn until(
    queue: &QueuedReleases<(), u8>,
    done: impl Fn(&[(String, ReleaseQueueState<()>)]) -> bool,
) -> Vec<(String, ReleaseQueueState<()>)> {
    let mut values = queue.subscribe();
    loop {
        let rows: Vec<_> = values
            .next()
            .await
            .expect("the query stays open")
            .unwrap()
            .rows
            .into_iter()
            .map(|row| (row.op.release_id, row.op.state))
            .collect();
        if done(&rows) {
            return rows;
        }
    }
}

/// How an entry's work failed: as it says, or by panicking.
#[derive(Debug, thiserror::Error)]
enum WorkError {
    #[error("{0}")]
    Failed(String),
    #[error(transparent)]
    Panicked(#[from] TaskFailed),
}

/// Each outcome the worker reported: the release, and its failure.
type Outcomes = Arc<Mutex<Vec<(String, Result<(), String>)>>>;

/// Run the worker over `queue` with `start`, recording each outcome it
/// reports.
fn worker<Start, StartFut, Fut>(
    queue: &QueuedReleases<(), u8>,
    start: Start,
) -> (tokio::task::JoinHandle<()>, Outcomes)
where
    Start: FnMut(ReleaseQueueEntry<()>) -> StartFut + Send + 'static,
    StartFut: Future<Output = Result<(u8, RunningOp<Fut>), WorkError>> + Send,
    Fut: Future<Output = Result<(), WorkError>> + Send,
{
    let done = Arc::new(Mutex::new(Vec::new()));
    let worker_queue = queue.clone();
    let recorded = done.clone();
    let task = tokio::spawn(async move {
        worker_queue
            .run_serial(
                "Test",
                start,
                move |release_id, result| {
                    recorded.lock().unwrap().push((
                        release_id.to_string(),
                        result.map_err(|error: &WorkError| error.to_string()),
                    ))
                },
                |label, error| panic!("{label}: the queue's table failed: {error}"),
                |failed| panic!("an entry's work panicked: {failed}"),
            )
            .await
    });
    (task, done)
}

/// Wait until the worker has reported `count` outcomes. It reports an
/// entry's outcome after the entry has left the table, so the table emptying
/// does not yet say the outcome is in.
async fn reported(done: &Outcomes, count: usize) -> Vec<(String, Result<(), String>)> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            {
                let done = done.lock().unwrap();
                if done.len() >= count {
                    return done.clone();
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the worker reports the outcomes")
}

/// A spawned task's abort handle and the outcome it yields, mapping a
/// panic to a failure that names it.
fn spawned(
    task: tokio::task::JoinHandle<()>,
) -> RunningOp<impl Future<Output = Result<(), WorkError>>> {
    let abort = task.abort_handle();
    RunningOp::new(abort, async move {
        match task.await {
            Ok(()) => Ok(()),
            Err(join_error) if join_error.is_cancelled() => {
                Err(WorkError::Failed("cancelled".to_string()))
            }
            Err(join_error) => Err(WorkError::Failed(format!("task panicked: {join_error}"))),
        }
    })
}

/// A finished entry leaves the queue and the next one starts; its outcome is
/// reported once.
#[tokio::test]
async fn a_finished_entry_leaves_and_the_next_starts() {
    let (queue, _tmp) = queue().await;
    queue.enqueue_all(vec![entry(RELEASE_A), entry(RELEASE_B)]).await.unwrap();
    let (task, done) = worker(&queue, |_entry| async { Ok((0, spawned(tokio::spawn(async {})))) });

    let outcomes = reported(&done, 2).await;
    task.abort();
    assert_eq!(outcomes, vec![(RELEASE_A.to_string(), Ok(())), (RELEASE_B.to_string(), Ok(()))]);
    assert!(phases(&queue).await.is_empty());
}

/// A queued operation whose task panics leaves the release failed with the
/// panic in its message — a panicked task says so, rather than surfacing as
/// whatever its progress channel looked like on the way down.
#[tokio::test]
async fn a_panicking_operation_marks_the_release_failed_with_the_panic() {
    let (queue, _tmp) = queue().await;
    queue.enqueue_all(vec![entry(RELEASE_A)]).await.unwrap();
    let (task, _done) =
        worker(&queue, |_entry| async { Ok((0, spawned(tokio::spawn(async { panic!("boom") })))) });

    let rows =
        until(&queue, |rows| matches!(rows.first(), Some((_, ReleaseQueueState::Failed { .. }))))
            .await;
    task.abort();
    let ReleaseQueueState::Failed { error } = &rows[0].1 else { unreachable!() };
    assert!(error.contains("panicked"), "the failure names the panic, got: {error}");
}

/// An entry whose work panics in the worker itself fails, the panic is
/// reported once, and the worker goes on to the next entry.
#[tokio::test]
async fn an_entry_that_panics_fails_and_the_next_one_runs() {
    let (queue, _tmp) = queue().await;
    queue.enqueue_all(vec![entry(RELEASE_A), entry(RELEASE_B)]).await.unwrap();
    let bugs: Arc<Mutex<Vec<bae_util::task_failure::Bug>>> = Arc::default();
    let done: Outcomes = Arc::default();
    let worker_queue = queue.clone();
    let (recorded, reports) = (done.clone(), bugs.clone());
    let task = tokio::spawn(async move {
        worker_queue
            .run_serial(
                "Test",
                |entry: ReleaseQueueEntry<()>| async move {
                    let abort = tokio::spawn(async {}).abort_handle();
                    let panics = entry.release_id == RELEASE_A;
                    Ok((
                        0,
                        RunningOp::new(abort, async move {
                            if panics {
                                panic!("the entry's work broke");
                            }
                            Ok(())
                        }),
                    ))
                },
                move |release_id, result| {
                    recorded.lock().unwrap().push((
                        release_id.to_string(),
                        result.map_err(|error: &WorkError| error.to_string()),
                    ))
                },
                |label, error| panic!("{label}: the queue's table failed: {error}"),
                move |failed| {
                    reports.lock().unwrap().push(failed.clone());
                    failed
                },
            )
            .await
    });

    let outcomes = reported(&done, 2).await;
    task.abort();
    assert!(
        matches!(&outcomes[0], (release, Err(error)) if release == RELEASE_A && error.contains("panicked")),
        "{outcomes:?}"
    );
    assert_eq!(outcomes[1], (RELEASE_B.to_string(), Ok(())));
    assert!(matches!(
        phases(&queue).await.as_slice(),
        [(release, ReleaseQueueState::Failed { .. })] if release == RELEASE_A
    ));
    assert_eq!(bugs.lock().unwrap().len(), 1, "{:?}", bugs.lock().unwrap());
}

/// A cancel landing between the worker picking a release and starting it
/// stops the work just spawned, and the entry is neither failed nor
/// re-added.
#[tokio::test]
async fn a_cancel_before_the_start_aborts_the_work() {
    let (queue, _tmp) = queue().await;
    queue.enqueue_all(vec![entry(RELEASE_A)]).await.unwrap();
    let work = Arc::new(Mutex::new(None));
    let canceller = queue.clone();
    let spawned_work = work.clone();
    let (task, done) = worker(&queue, move |entry| {
        let canceller = canceller.clone();
        let spawned_work = spawned_work.clone();
        async move {
            canceller.cancel(&entry.release_id).await.unwrap();
            let pending = tokio::spawn(std::future::pending::<()>());
            let abort = pending.abort_handle();
            *spawned_work.lock().unwrap() = Some(abort.clone());
            Ok((0, RunningOp::new(abort, async { Ok(()) })))
        }
    });

    loop {
        let finished = work.lock().unwrap().as_ref().is_some_and(|abort| abort.is_finished());
        if finished {
            break;
        }
        tokio::task::yield_now().await;
    }
    task.abort();
    assert!(phases(&queue).await.is_empty());
    assert!(done.lock().unwrap().is_empty(), "a cancel is not an outcome");
}

/// A cancel while the work runs aborts it; the entry is gone, its progress
/// with it, and the cancel is no failure.
#[tokio::test]
async fn a_cancel_while_running_aborts_the_work_and_is_no_failure() {
    let (queue, _tmp) = queue().await;
    queue.enqueue_all(vec![entry(RELEASE_A)]).await.unwrap();
    let (task, done) = worker(&queue, |_entry| async {
        Ok((7, spawned(tokio::spawn(std::future::pending::<()>()))))
    });
    let mut transfers = queue.watch_transfers();
    transfers.until(|transfers| transfers.get(RELEASE_A) == Some(&7)).await.unwrap();
    assert_eq!(
        phases(&queue).await,
        vec![(RELEASE_A.to_string(), ReleaseQueueState::Active { progress: () })]
    );

    queue.cancel(RELEASE_A).await.unwrap();
    transfers.until(|transfers| transfers.is_empty()).await.unwrap();
    assert!(phases(&queue).await.is_empty());
    // The worker has seen the abort once it starts the next entry.
    queue.enqueue_all(vec![entry(RELEASE_B)]).await.unwrap();
    transfers.until(|transfers| transfers.contains_key(RELEASE_B)).await.unwrap();
    task.abort();
    assert!(done.lock().unwrap().is_empty(), "a cancel is not an outcome");
}

/// Progress for the running entry is published once it has started; for
/// any other release it changes nothing.
#[tokio::test]
async fn progress_is_the_running_entrys_alone() {
    let (queue, _tmp) = queue().await;
    queue.enqueue_all(vec![entry(RELEASE_A)]).await.unwrap();
    let (task, _done) = worker(&queue, |_entry| async {
        Ok((0, spawned(tokio::spawn(std::future::pending::<()>()))))
    });
    let mut transfers = queue.watch_transfers();
    transfers.until(|transfers| transfers.contains_key(RELEASE_A)).await.unwrap();

    assert!(queue.set_active_progress(RELEASE_A, 50));
    assert!(!queue.set_active_progress(RELEASE_B, 10));
    assert_eq!(
        transfers.until(|transfers| transfers.get(RELEASE_A) == Some(&50)).await.unwrap(),
        BTreeMap::from([(RELEASE_A.to_string(), 50)])
    );
    assert_eq!(queue.active_progress(), BTreeMap::from([(RELEASE_A.to_string(), 50)]));
    task.abort();
}

/// A paused queue starts nothing until it resumes.
#[tokio::test]
async fn a_paused_queue_starts_nothing_until_it_resumes() {
    let (queue, _tmp) = queue().await;
    queue.set_paused(true).await.unwrap();
    queue.enqueue_all(vec![entry(RELEASE_A)]).await.unwrap();
    let (task, done) = worker(&queue, |_entry| async { Ok((0, spawned(tokio::spawn(async {})))) });
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert_eq!(phases(&queue).await, vec![(RELEASE_A.to_string(), ReleaseQueueState::Queued)]);

    queue.set_paused(false).await.unwrap();
    assert_eq!(reported(&done, 1).await.len(), 1);
    task.abort();
    assert!(phases(&queue).await.is_empty());
}

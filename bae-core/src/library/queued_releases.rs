//! A serial release queue's owner: its table, the task running its active
//! entry, and how far that entry has got. The table is the queue: what is
//! queued, active or failed, in order, and whether the queue is paused. The
//! task and its progress live here, since neither outlives the process.

use crate::db::{Database, QueuedPayload};
#[cfg(any(desktop, test, feature = "test-utils"))]
use crate::library::release_queue::ReleaseQueueSnapshot;
use crate::library::release_queue::{ReleaseQueueCounts, ReleaseQueueEntry};
use crate::library::LibraryError;
use std::collections::BTreeMap;
use std::future::Future;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use tokio::sync::{watch, Notify};
use tracing::{debug, error};

/// A queued operation that has been started: the handle a cancel aborts, and the
/// future that yields the operation's outcome.
pub struct RunningOp<Fut> {
    abort: tokio::task::AbortHandle,
    outcome: Fut,
}

impl<Fut> RunningOp<Fut> {
    pub(crate) fn new(abort: tokio::task::AbortHandle, outcome: Fut) -> Self {
        Self { abort, outcome }
    }

    pub async fn finish(self) -> Fut::Output
    where
        Fut: Future,
    {
        self.outcome.await
    }
}

/// The task running the active entry. It is registered before the entry
/// starts in the table and dropped only after the entry has left `active`
/// there, so an entry the table reads as active always has one, with its
/// progress.
struct Running<Progress> {
    release_id: String,
    abort: tokio::task::AbortHandle,
    progress: Progress,
}

struct Shared<Progress> {
    /// Wakes the worker parked on an empty or paused queue.
    wake: Notify,
    running: Mutex<Option<Running<Progress>>>,
    /// The active entry's progress, by release, once it has started.
    transfers: watch::Sender<BTreeMap<String, Progress>>,
}

pub(crate) struct QueuedReleases<Extra, Progress> {
    database: Database,
    shared: Arc<Shared<Progress>>,
    _extra: PhantomData<fn() -> Extra>,
}

impl<Extra, Progress> Clone for QueuedReleases<Extra, Progress> {
    fn clone(&self) -> Self {
        Self {
            database: self.database.clone(),
            shared: self.shared.clone(),
            _extra: PhantomData,
        }
    }
}

impl<Extra, Progress> QueuedReleases<Extra, Progress>
where
    Extra: QueuedPayload,
    Progress: Clone + PartialEq + Send + Sync + 'static,
{
    pub(crate) fn new(database: Database) -> Self {
        let (transfers, _) = watch::channel(BTreeMap::new());
        Self {
            database,
            shared: Arc::new(Shared {
                wake: Notify::new(),
                running: Mutex::new(None),
                transfers,
            }),
            _extra: PhantomData,
        }
    }

    pub(crate) async fn contains(&self, release_id: &str) -> Result<bool, LibraryError> {
        Ok(self.database.release_queued::<Extra>(release_id).await?)
    }

    /// Queue every entry in order, passing over a release already queued,
    /// and wake the worker if any went in. Returns whether any did.
    pub(crate) async fn enqueue_all(
        &self,
        entries: Vec<ReleaseQueueEntry<Extra>>,
    ) -> Result<bool, LibraryError> {
        let added = self.database.enqueue_releases(entries).await?;
        if added {
            self.shared.wake.notify_one();
        }
        Ok(added)
    }

    /// Wake the worker after entries went into its queue elsewhere.
    pub(crate) fn queued(&self) {
        self.shared.wake.notify_one();
    }

    /// Pause or resume. While paused the worker parks instead of starting
    /// the next release; the in-flight one runs to completion. Resuming
    /// wakes the worker.
    pub(crate) async fn set_paused(&self, paused: bool) -> Result<(), LibraryError> {
        let was_paused = self
            .database
            .set_release_queue_paused::<Extra>(paused)
            .await?;
        if was_paused && !paused {
            self.shared.wake.notify_one();
        }
        Ok(())
    }

    /// Take out a queued or failed entry; abort the active one's task.
    pub(crate) async fn cancel(&self, release_id: &str) -> Result<(), LibraryError> {
        self.database
            .cancel_queued_release::<Extra>(release_id)
            .await?;
        self.stop(|running| running == release_id);
        Ok(())
    }

    /// Queue every failed entry again and wake the worker.
    pub(crate) async fn retry_failed(&self) -> Result<(), LibraryError> {
        if self.database.retry_failed_releases::<Extra>().await? {
            self.shared.wake.notify_one();
        }
        Ok(())
    }

    /// Abort the running task when `cancelled` names its release, whose entry
    /// the table no longer holds.
    fn stop(&self, cancelled: impl Fn(&str) -> bool) {
        let mut running = self.shared.running.lock().unwrap();
        let Some(stopped) = running.take_if(|running| cancelled(&running.release_id)) else {
            return;
        };
        stopped.abort.abort();
        self.shared
            .transfers
            .send_if_modified(|transfers| transfers.remove(&stopped.release_id).is_some());
    }

    /// Record the active entry's progress. `false` when no task runs that
    /// release, in which case nothing changes.
    pub(crate) fn set_active_progress(&self, release_id: &str, progress: Progress) -> bool {
        let mut running = self.shared.running.lock().unwrap();
        let Some(running) = running
            .as_mut()
            .filter(|running| running.release_id == release_id)
        else {
            return false;
        };
        running.progress = progress.clone();
        self.shared.transfers.send_if_modified(|transfers| {
            match transfers.get_mut(release_id) {
                Some(shown) if *shown != progress => {
                    *shown = progress;
                    true
                }
                // Not started in the table yet: the start publishes it.
                Some(_) | None => false,
            }
        });
        true
    }

    /// The running entry's progress, by release: an entry for every release
    /// a read of the table made before this one saw as active.
    #[cfg(any(desktop, test))]
    pub(crate) fn active_progress(&self) -> BTreeMap<String, Progress> {
        self.shared
            .running
            .lock()
            .unwrap()
            .iter()
            .map(|running| (running.release_id.clone(), running.progress.clone()))
            .collect()
    }

    /// The active entry's progress, by release, and each change to it.
    pub(crate) fn subscribe_transfers(&self) -> watch::Receiver<BTreeMap<String, Progress>> {
        self.shared.transfers.subscribe()
    }

    /// Every entry of the queue now.
    #[cfg(any(desktop, test, feature = "test-utils"))]
    pub(crate) async fn snapshot(&self) -> Result<ReleaseQueueSnapshot<Extra, ()>, LibraryError> {
        Ok(self.database.release_queue::<Extra>().await?)
    }

    /// Follow every entry of the queue, for a test that waits on the whole
    /// queue.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn subscribe(&self) -> coven::LiveQuery<ReleaseQueueSnapshot<Extra, ()>> {
        self.database.subscribe_release_queue::<Extra>()
    }

    /// Follow how many entries the queue holds and the windows `initial`
    /// names of them, as its pane draws them.
    pub(crate) fn subscribe_windows(
        &self,
        initial: crate::library::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<
        crate::library::LibraryPageWindows,
        crate::library::ReleaseQueueWindows<Extra>,
    > {
        self.database
            .subscribe_release_queue_windows::<Extra>(initial)
    }

    /// How many entries are in each phase now, and whether it is paused.
    pub(crate) async fn counts(&self) -> Result<ReleaseQueueCounts, LibraryError> {
        Ok(self.database.release_queue_counts::<Extra>().await?)
    }

    /// Follow how many entries are in each phase, and whether it is paused.
    pub(crate) fn subscribe_counts(&self) -> coven::LiveQuery<ReleaseQueueCounts> {
        self.database.subscribe_release_queue_counts::<Extra>()
    }

    fn register(&self, release_id: &str, abort: tokio::task::AbortHandle, progress: Progress) {
        *self.shared.running.lock().unwrap() = Some(Running {
            release_id: release_id.to_string(),
            abort,
            progress,
        });
    }

    /// Publish the running entry's progress once the table has it active,
    /// unless a cancel stopped it meanwhile.
    fn publish_started(&self, release_id: &str) {
        let running = self.shared.running.lock().unwrap();
        let Some(running) = running
            .as_ref()
            .filter(|running| running.release_id == release_id)
        else {
            return;
        };
        let progress = running.progress.clone();
        self.shared.transfers.send_modify(|transfers| {
            transfers.insert(release_id.to_string(), progress);
        });
    }

    /// Start `release_id`'s entry with `abort` as its task, as the worker
    /// does — for a test that drives progress without a worker.
    #[cfg(test)]
    pub(crate) async fn activate_for_test(
        &self,
        release_id: &str,
        abort: tokio::task::AbortHandle,
        progress: Progress,
    ) -> Result<bool, LibraryError> {
        self.register(release_id, abort, progress);
        let started = self
            .database
            .activate_queued_release::<Extra>(release_id)
            .await?;
        if started {
            self.publish_started(release_id);
        } else {
            self.unregister(release_id);
        }
        Ok(started)
    }

    /// Drop the running entry's task once the table no longer has it active.
    fn unregister(&self, release_id: &str) {
        let mut running = self.shared.running.lock().unwrap();
        if running
            .as_ref()
            .is_some_and(|running| running.release_id == release_id)
        {
            *running = None;
        }
        self.shared
            .transfers
            .send_if_modified(|transfers| transfers.remove(release_id).is_some());
    }

    /// Drain the queue serially, one release at a time, forever. The caller
    /// supplies only `start` — resolve the release's initial progress, spawn its
    /// work, and hand back the abort handle plus the future that yields its
    /// outcome — the hook that reports the outcome to diagnostics (`on_done`),
    /// and the one that reports a failed write of the queue's own table
    /// (`store_failed`), after which the worker waits to be woken again.
    ///
    /// The protocol:
    ///
    /// - A cancel can land in the gap between picking a release and starting it.
    ///   The task is registered before the table marks the entry active, and a
    ///   cancel takes the entry out of the table before it aborts the registered
    ///   task: either the table refuses the start, and the work just started is
    ///   aborted and dropped, or the cancel finds the task and aborts it.
    /// - A cancel during the work takes the entry out and aborts the task, so
    ///   the table refusing the finish or the failure is what tells a cancel
    ///   from either. A cancelled release is neither re-added nor marked failed.
    /// - Success takes the entry out; failure marks it failed and leaves it for
    ///   retry. A `start` that fails marks it failed the same way — nothing was
    ///   spawned.
    pub(crate) async fn run_serial<Start, StartFut, Fut, Done, StoreFailed>(
        &self,
        label: &'static str,
        mut start: Start,
        mut on_done: Done,
        store_failed: StoreFailed,
    ) where
        Start: FnMut(ReleaseQueueEntry<Extra>) -> StartFut,
        StartFut: Future<Output = Result<(Progress, RunningOp<Fut>), LibraryError>>,
        Fut: Future<Output = Result<(), LibraryError>>,
        Done: FnMut(&str, Result<(), &LibraryError>),
        StoreFailed: Fn(&str, &LibraryError),
    {
        let database = &self.database;
        loop {
            let entry = match database.next_queued_release::<Extra>().await {
                Ok(Some(entry)) => entry,
                Ok(None) => {
                    self.shared.wake.notified().await;
                    continue;
                }
                Err(error) => {
                    store_failed(label, &error.into());
                    self.shared.wake.notified().await;
                    continue;
                }
            };
            let release_id = entry.release_id.clone();

            let (progress, running) = match start(entry).await {
                Ok(started) => started,
                Err(error) => {
                    error!("{label} failed for release {release_id}: {error}");
                    match database
                        .fail_queued_release::<Extra>(&release_id, error.to_string())
                        .await
                    {
                        Ok(true) => on_done(&release_id, Err(&error)),
                        Ok(false) => debug!("{label} for {release_id} cancelled before it started"),
                        Err(store) => {
                            store_failed(label, &store.into());
                            self.shared.wake.notified().await;
                        }
                    }
                    continue;
                }
            };
            let RunningOp { abort, outcome } = running;

            self.register(&release_id, abort.clone(), progress);
            match database.activate_queued_release::<Extra>(&release_id).await {
                Ok(true) => self.publish_started(&release_id),
                Ok(false) => {
                    self.unregister(&release_id);
                    abort.abort();
                    debug!("{label} for {release_id} cancelled before it started; aborting");
                    continue;
                }
                Err(store) => {
                    self.unregister(&release_id);
                    abort.abort();
                    store_failed(label, &store.into());
                    self.shared.wake.notified().await;
                    continue;
                }
            }

            let result = outcome.await;
            let left = match &result {
                Ok(()) => database.finish_queued_release::<Extra>(&release_id).await,
                Err(error) => {
                    database
                        .fail_queued_release::<Extra>(&release_id, error.to_string())
                        .await
                }
            };
            self.unregister(&release_id);
            match left {
                Ok(true) => {
                    if let Err(error) = &result {
                        error!("{label} failed for release {release_id}: {error}");
                    }
                    on_done(&release_id, result.as_ref().map(|_| ()));
                }
                Ok(false) => {
                    debug!("{label} for {release_id} ended after cancel; leaving queue as-is")
                }
                Err(store) => {
                    store_failed(label, &store.into());
                    self.shared.wake.notified().await;
                }
            }
        }
    }
}

#[cfg(desktop)]
impl<Extra: QueuedPayload> QueuedReleases<Extra, u8> {
    /// Record the active entry's percent.
    pub(crate) fn set_active_percent(&self, release_id: &str, percent: u8) {
        if !self.set_active_progress(release_id, percent) {
            tracing::warn!(
                release_id,
                percent,
                "ignored active queue percent for a release no task runs"
            );
        }
    }
}

#[cfg(test)]
#[path = "queued_releases_tests.rs"]
mod tests;

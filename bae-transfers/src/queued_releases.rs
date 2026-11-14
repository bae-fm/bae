//! A serial release queue's owner: its table, the task running its active
//! entry, and how far that entry has got. The table is the queue: what is
//! queued, active or failed, in order, and whether the queue is paused. The
//! task and its progress live here, since neither outlives the process.

pub mod tables;

use crate::download_snapshot::DownloadRequest;
#[cfg(any(desktop, test, feature = "test-utils"))]
use crate::release_queue::ReleaseQueueSnapshot;
use crate::release_queue::{ReleaseQueueCounts, ReleaseQueueEntry};
use bae_store::DbError;
use bae_store::Store;
use bae_util::task_failure::{caught, caught_sync, Bug, TaskFailed};
use coven::CovenWrite;
use std::future::Future;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
pub use tables::QueuedPayload;
use tokio::sync::Notify;
use tracing::{debug, error};

/// A queued operation that has been started: the handle a cancel aborts, and the
/// future that yields the operation's outcome.
pub struct RunningOp<Fut> {
    abort: tokio::task::AbortHandle,
    outcome: Fut,
}

impl<Fut> RunningOp<Fut> {
    pub fn new(abort: tokio::task::AbortHandle, outcome: Fut) -> Self {
        Self { abort, outcome }
    }

    /// The same operation with its outcome boxed, for a worker whose entries
    /// run different futures.
    pub fn boxed(self) -> RunningOp<std::pin::Pin<Box<dyn Future<Output = Fut::Output> + Send>>>
    where
        Fut: Future + Send + 'static,
    {
        RunningOp { abort: self.abort, outcome: Box::pin(self.outcome) }
    }

    /// The same operation with its outcome handed to `then`, whose future
    /// awaits it as part of the outcome it makes.
    pub fn then<Next>(self, then: impl FnOnce(Fut) -> Next) -> RunningOp<Next> {
        RunningOp { abort: self.abort, outcome: then(self.outcome) }
    }

    pub async fn finish(self) -> Fut::Output
    where
        Fut: Future,
    {
        self.outcome.await
    }
}

/// The entry a worker's operation is running, which only the operation's own
/// write can end. A successful operation hands back the [`Finished`] that
/// [`ActiveEntry::finish_on`] returns, so its work and its entry's end commit
/// in one write or not at all.
pub struct ActiveEntry<Extra> {
    release_id: String,
    _queue: PhantomData<fn() -> Extra>,
}

impl<Extra: QueuedPayload> ActiveEntry<Extra> {
    pub(crate) fn new(release_id: &str) -> Self {
        Self { release_id: release_id.to_string(), _queue: PhantomData }
    }

    /// End the entry in `sql`, the write that also commits the operation's
    /// work: it leaves the queue. Fails the write, rolling the work back
    /// with it, with [`DbError::EntryCancelled`] when the entry had already
    /// left, cancelled while it ran.
    pub fn finish_on(&self, sql: &CovenWrite<'_>) -> Result<Finished, DbError> {
        if tables::finish_queued_release_on::<Extra>(sql, &self.release_id)? {
            Ok(Finished(()))
        } else {
            Err(DbError::EntryCancelled { release_id: self.release_id.clone() })
        }
    }
}

/// That an operation ended its entry: only [`ActiveEntry::finish_on`] makes
/// one.
#[derive(Debug)]
pub struct Finished(());

/// Resolves once coven's word on whether the device has a usable network
/// differs from `seen`, the value the worker's last read of the queue held it
/// back by. A fresh subscription's first result is the value now, so a change
/// that happened since that read counts, which one subscription kept across
/// reads would miss: its results skip a change that has changed back, while
/// the read saw the network in between. A failed read answers too: the
/// worker's own read reports it. Never once the store has closed.
async fn network_changed_from(store: &Store, seen: Option<bool>) {
    let mut usable = store.subscribe(|sql| tables::network_usable_on(&sql));
    match usable.next().await {
        Some(Ok(now)) if now == seen => {}
        Some(_) => return,
        None => return std::future::pending().await,
    }
    if usable.next().await.is_none() {
        std::future::pending::<()>().await;
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
    transfers: crate::ReleaseTransfers<Progress>,
    /// What a test waits on: how often the worker found nothing to start and
    /// parked, and how many wakes it had been sent when it last read the
    /// queue.
    #[cfg(any(test, feature = "test-utils"))]
    rest: tokio::sync::watch::Sender<Rest>,
    /// How many wakes have been sent.
    #[cfg(any(test, feature = "test-utils"))]
    wakes_sent: std::sync::atomic::AtomicU64,
}

/// How the worker last came to rest, for a test.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Default)]
struct Rest {
    parks: u64,
    /// The wakes sent before the read that found nothing to start.
    wakes_heard: u64,
}

pub struct QueuedReleases<Extra, Progress> {
    store: Store,
    shared: Arc<Shared<Progress>>,
    _extra: PhantomData<fn() -> Extra>,
}

impl<Extra, Progress> Clone for QueuedReleases<Extra, Progress> {
    fn clone(&self) -> Self {
        Self { store: self.store.clone(), shared: self.shared.clone(), _extra: PhantomData }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl<Extra, Progress> QueuedReleases<Extra, Progress> {
    /// Wait until the worker has found nothing to start and parked, `count`
    /// times in all.
    #[cfg(test)]
    pub(crate) async fn parked(&self, count: u64) {
        self.shared
            .rest
            .subscribe()
            .wait_for(|rest| rest.parks >= count)
            .await
            .expect("the queue's sender lives as long as the queue");
    }

    /// Wait until the worker has read the queue as the last wake sent left
    /// it and found nothing to start. A test that moves entries without the
    /// worker waits for this first: a wake the worker has yet to hear would
    /// have it read the entries as the test left them, and take them.
    pub async fn settled(&self) {
        self.shared
            .rest
            .subscribe()
            .wait_for(|rest| {
                rest.wakes_heard == self.shared.wakes_sent.load(std::sync::atomic::Ordering::SeqCst)
            })
            .await
            .expect("the queue's sender lives as long as the queue");
    }
}

impl<Extra, Progress> QueuedReleases<Extra, Progress>
where
    Extra: QueuedPayload,
    Progress: Clone + PartialEq + Send + Sync + 'static,
{
    pub fn new(store: Store) -> Self {
        Self {
            store,
            shared: Arc::new(Shared {
                wake: Notify::new(),
                running: Mutex::new(None),
                transfers: crate::ReleaseTransfers::new(),
                #[cfg(any(test, feature = "test-utils"))]
                rest: tokio::sync::watch::channel(Rest::default()).0,
                #[cfg(any(test, feature = "test-utils"))]
                wakes_sent: std::sync::atomic::AtomicU64::new(0),
            }),
            _extra: PhantomData,
        }
    }

    pub async fn contains(&self, release_id: &str) -> Result<bool, DbError> {
        let release_id = release_id.to_string();
        self.store.read(move |sql| tables::release_queued_on::<Extra>(&sql, &release_id)).await
    }

    /// Queue every entry in order, passing over a release already queued,
    /// and wake the worker if any went in. Returns whether any did.
    #[cfg(desktop)]
    pub async fn enqueue_all(
        &self,
        entries: Vec<ReleaseQueueEntry<Extra>>,
    ) -> Result<bool, DbError> {
        let added = self.store.call(move |sql| tables::enqueue_releases_on(sql, &entries)).await?;
        if added {
            self.wake();
        }
        Ok(added)
    }

    /// Wake the worker parked on an empty or paused queue.
    fn wake(&self) {
        #[cfg(any(test, feature = "test-utils"))]
        self.shared.wakes_sent.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.shared.wake.notify_one();
    }

    /// Wake the worker after entries went into its queue elsewhere.
    pub fn queued(&self) {
        self.wake();
    }

    /// Pause or resume. Pausing stops the running task at once, as a cancel
    /// does, but leaves its entry queued; while paused the worker parks
    /// instead of starting the next release. Resuming wakes the worker, which
    /// starts the stopped entry again from the beginning.
    pub async fn set_paused(&self, paused: bool) -> Result<(), DbError> {
        let was_paused = self
            .store
            .call(move |sql| tables::set_release_queue_paused_on::<Extra>(sql, paused))
            .await?;
        if paused {
            self.stop(|_| true);
        } else if was_paused {
            self.wake();
        }
        Ok(())
    }

    /// Take out a queued or failed entry; abort the active one's task.
    pub async fn cancel(&self, release_id: &str) -> Result<(), DbError> {
        let cancelled = release_id.to_string();
        self.store
            .call(move |sql| tables::cancel_queued_release_on::<Extra>(sql, &cancelled))
            .await?;
        self.stop(|running| running == release_id);
        Ok(())
    }

    /// Queue every failed entry again and wake the worker.
    pub async fn retry_failed(&self) -> Result<(), DbError> {
        if self.store.call(|sql| tables::retry_failed_releases_on::<Extra>(sql)).await? {
            self.wake();
        }
        Ok(())
    }

    /// Queue `release_id`'s failed entry again and wake the worker; any other
    /// entry is left as it is.
    pub async fn retry_failed_release(&self, release_id: &str) -> Result<(), DbError> {
        let retried = release_id.to_string();
        if self
            .store
            .call(move |sql| tables::retry_failed_release_on::<Extra>(sql, &retried))
            .await?
        {
            self.wake();
        }
        Ok(())
    }

    /// Abort the running task when `cancelled` names its release, whose entry
    /// the table no longer holds as active.
    fn stop(&self, cancelled: impl Fn(&str) -> bool) {
        let mut running = self.shared.running.lock().unwrap();
        let Some(stopped) = running.take_if(|running| cancelled(&running.release_id)) else {
            return;
        };
        stopped.abort.abort();
        self.shared.transfers.remove(&stopped.release_id);
    }

    /// Record the active entry's progress. `false` when no task runs that
    /// release, in which case nothing changes.
    pub fn set_active_progress(&self, release_id: &str, progress: Progress) -> bool {
        let mut running = self.shared.running.lock().unwrap();
        let Some(running) = running.as_mut().filter(|running| running.release_id == release_id)
        else {
            return false;
        };
        running.progress = progress.clone();
        // Not started in the table yet, the release is unlisted: the start
        // publishes it.
        self.shared.transfers.advance(release_id, progress);
        true
    }

    /// The running entry's progress, by release: an entry for every release
    /// a read of the table made before this one saw as active.
    #[cfg(any(desktop, test))]
    pub fn active_progress(&self) -> std::collections::BTreeMap<String, Progress> {
        self.shared
            .running
            .lock()
            .unwrap()
            .iter()
            .map(|running| (running.release_id.clone(), running.progress.clone()))
            .collect()
    }

    /// Hear the active entry's progress, by release, and each change to it.
    pub fn watch_transfers(&self) -> crate::ReleaseTransfersWatch<Progress> {
        self.shared.transfers.watch()
    }

    /// Every entry of the queue now.
    #[cfg(any(desktop, test, feature = "test-utils"))]
    pub async fn snapshot(&self) -> Result<ReleaseQueueSnapshot<Extra, ()>, DbError> {
        self.store.read(|sql| tables::release_queue_on::<Extra>(&sql)).await
    }

    /// Follow every entry of the queue, for a test that waits on the whole
    /// queue.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe(
        &self,
    ) -> coven::LiveQuery<ReleaseQueueSnapshot<Extra, ()>, bae_store::DbError> {
        self.store.subscribe(|sql| tables::release_queue_on::<Extra>(&sql))
    }

    /// Follow how many entries the queue holds and the windows `initial`
    /// names of them, as its pane draws them.
    pub fn subscribe_windows(
        &self,
        initial: bae_store::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<
        bae_store::LibraryPageWindows,
        crate::release_queue::ReleaseQueueWindows<Extra>,
        bae_store::DbError,
    > {
        let places = bae_store::list_windows::KnownPlaces::new();
        self.store
            .subscribe_reconfigurable(initial, move |windows, sql| {
                tables::release_queue_windows_on::<Extra>(&sql, windows, &places)
            })
            .process(Ok)
    }

    /// How many entries are in each phase now, and whether it is paused.
    pub async fn counts(&self) -> Result<ReleaseQueueCounts, DbError> {
        self.store.read(|sql| tables::release_queue_counts_on::<Extra>(&sql)).await
    }

    /// Follow how many entries are in each phase, and whether it is paused.
    pub fn subscribe_counts(&self) -> coven::LiveQuery<ReleaseQueueCounts, bae_store::DbError> {
        self.store.subscribe(|sql| tables::release_queue_counts_on::<Extra>(&sql))
    }

    fn register(&self, release_id: &str, abort: tokio::task::AbortHandle, progress: Progress) {
        *self.shared.running.lock().unwrap() =
            Some(Running { release_id: release_id.to_string(), abort, progress });
    }

    /// Publish the running entry's progress once the table has it active,
    /// unless a cancel stopped it meanwhile.
    fn publish_started(&self, release_id: &str) {
        let running = self.shared.running.lock().unwrap();
        let Some(running) = running.as_ref().filter(|running| running.release_id == release_id)
        else {
            return;
        };
        self.shared.transfers.set(release_id, running.progress.clone());
    }

    /// Start `release_id`'s entry with `abort` as its task, as the worker
    /// does — for a test that drives progress without a worker. An entry
    /// starts only on a running queue, so the queue is resumed in the same
    /// write, and no worker is woken: one parked on the paused queue stays
    /// parked, leaving the entry to the test.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn activate_for_test(
        &self,
        release_id: &str,
        abort: tokio::task::AbortHandle,
        progress: Progress,
    ) -> Result<bool, DbError> {
        self.register(release_id, abort, progress);
        let activated = release_id.to_string();
        let started = self
            .store
            .call(move |sql| {
                tables::set_release_queue_paused_on::<Extra>(sql, false)?;
                tables::activate_queued_release_on::<Extra>(sql, &activated)
            })
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
        if running.as_ref().is_some_and(|running| running.release_id == release_id) {
            *running = None;
        }
        self.shared.transfers.remove(release_id);
    }

    /// Drain the queue serially, one release at a time, forever. The caller
    /// supplies only `start` — resolve the release's initial progress, spawn its
    /// work, and hand back the abort handle plus the future that yields its
    /// outcome — the hook that reports the outcome to diagnostics (`on_done`),
    /// the one that reports a failed write of the queue's own table
    /// (`store_failed`), after which the worker waits to be woken again, and
    /// the one that hands on a panic's bug (`bug`).
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
    /// - A pause during the work puts the entry back to queued and aborts the
    ///   task, so the table refusing the failure tells a pause from a failure;
    ///   the entry stays queued for the resume. A pause landing between the
    ///   pick and the start makes the table refuse the start, and the work
    ///   just started is aborted the same way.
    /// - Success takes the entry out, in the operation's own write: it ends
    ///   its entry with [`ActiveEntry::finish_on`] beside its work, so both
    ///   commit together, and a cancel that took the entry out meanwhile
    ///   fails that write ([`DbError::EntryCancelled`]) and rolls the work
    ///   back. Failure marks the entry failed and leaves it for retry. A `start` that fails marks it failed the same way — nothing was
    ///   spawned.
    /// - An entry whose `start`, work or `on_done` panics hands its bug to
    ///   `bug` and fails as any failure does; the worker goes on to the next
    ///   entry.
    pub async fn run_serial<Error, Start, StartFut, Fut, Done, StoreFailed, ReportBug>(
        &self,
        label: &'static str,
        mut start: Start,
        mut on_done: Done,
        store_failed: StoreFailed,
        bug: ReportBug,
    ) where
        Error: From<TaskFailed> + std::fmt::Display,
        Start: FnMut(ReleaseQueueEntry<Extra>, ActiveEntry<Extra>) -> StartFut,
        StartFut: Future<Output = Result<(Progress, RunningOp<Fut>), Error>>,
        Fut: Future<Output = Result<Finished, Error>>,
        Done: FnMut(&str, Result<(), &Error>),
        StoreFailed: Fn(&str, &DbError),
        ReportBug: Fn(Bug) -> Bug,
    {
        let failed = |panic: Bug| -> Error { TaskFailed::Panicked(bug(panic)).into() };
        let mut done = |release_id: &str, result: Result<(), &Error>| {
            if let Err(panic) = caught_sync(label, || on_done(release_id, result)) {
                bug(panic);
            }
        };
        let store = &self.store;
        loop {
            #[cfg(any(test, feature = "test-utils"))]
            let wakes_heard = self.shared.wakes_sent.load(std::sync::atomic::Ordering::SeqCst);
            let read = store
                .read(|sql| {
                    Ok((
                        tables::next_queued_release_on::<Extra>(&sql)?,
                        tables::network_usable_on(&sql)?,
                    ))
                })
                .await;
            let entry = match read {
                Ok((Some(entry), _)) => entry,
                Ok((None, usable)) => {
                    #[cfg(any(test, feature = "test-utils"))]
                    self.shared.rest.send_modify(|rest| {
                        rest.parks += 1;
                        rest.wakes_heard = wakes_heard;
                    });
                    // Woken by a change to the queue, or, for a queue that
                    // needs the network, by the network's change: the entry
                    // it left waiting may be takeable now.
                    tokio::select! {
                        () = self.shared.wake.notified() => {}
                        () = network_changed_from(store, usable), if Extra::NEEDS_NETWORK => {}
                    }
                    continue;
                }
                Err(error) => {
                    store_failed(label, &error);
                    self.shared.wake.notified().await;
                    continue;
                }
            };
            let release_id = entry.release_id.clone();
            let active = ActiveEntry::new(&release_id);

            let started = caught(label, async { start(entry, active).await })
                .await
                .unwrap_or_else(|panic| Err(failed(panic)));
            let (progress, running) = match started {
                Ok(started) => started,
                Err(error) => {
                    error!("{label} failed for release {release_id}: {error}");
                    let (failed_id, message) = (release_id.clone(), error.to_string());
                    match store
                        .call(move |sql| {
                            tables::fail_queued_release_on::<Extra>(sql, &failed_id, &message)
                        })
                        .await
                    {
                        Ok(true) => done(&release_id, Err(&error)),
                        Ok(false) => debug!("{label} for {release_id} cancelled before it started"),
                        Err(store) => {
                            store_failed(label, &store);
                            self.shared.wake.notified().await;
                        }
                    }
                    continue;
                }
            };
            let RunningOp { abort, outcome } = running;

            self.register(&release_id, abort.clone(), progress);
            let activated = release_id.clone();
            match store
                .call(move |sql| tables::activate_queued_release_on::<Extra>(sql, &activated))
                .await
            {
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
                    store_failed(label, &store);
                    self.shared.wake.notified().await;
                    continue;
                }
            }

            let result = caught(label, outcome).await.unwrap_or_else(|panic| Err(failed(panic)));
            let left_id = release_id.clone();
            let left = match &result {
                // The operation's own write ended the entry.
                Ok(Finished(())) => Ok(true),
                Err(error) => {
                    let message = error.to_string();
                    store
                        .call(move |sql| {
                            tables::fail_active_release_on::<Extra>(sql, &left_id, &message)
                        })
                        .await
                }
            };
            self.unregister(&release_id);
            match left {
                Ok(true) => {
                    if let Err(error) = &result {
                        error!("{label} failed for release {release_id}: {error}");
                    }
                    done(&release_id, result.as_ref().map(|_| ()));
                }
                Ok(false) => {
                    debug!("{label} for {release_id} ended after cancel; leaving queue as-is")
                }
                Err(store) => {
                    store_failed(label, &store);
                    self.shared.wake.notified().await;
                }
            }
        }
    }
}

impl<Progress> QueuedReleases<DownloadRequest, Progress>
where
    Progress: Clone + PartialEq + Send + Sync + 'static,
{
    /// Queue to pin, in the order given, each of `release_ids` that is
    /// uploaded and not wholly pinned on this device, in one write that
    /// decides which: one waiting or downloading, one the library no longer
    /// holds, and one local are passed over, and one whose download failed is
    /// queued again in its place. `true` when any went in or was queued
    /// again.
    pub async fn enqueue_pinnable(
        &self,
        release_ids: Vec<String>,
        created_at: i64,
    ) -> Result<bool, DbError> {
        self.store
            .call(move |sql| tables::enqueue_pinnable_releases_on(sql, &release_ids, created_at))
            .await
    }

    /// Queue to move to this device into `target_dir`, in the order given, each of
    /// `release_ids` that is stored in the cloud, in one write that decides
    /// which: one the library no longer holds, one local, and one waiting or
    /// downloading are passed over, and one whose download failed becomes this
    /// move-to-this-device in its place. `true` when any went in or was queued again.
    #[cfg(desktop)]
    pub async fn enqueue_moves_to_this_device(
        &self,
        release_ids: Vec<String>,
        target_dir: std::path::PathBuf,
        created_at: i64,
    ) -> Result<bool, DbError> {
        let added = self
            .store
            .call(move |sql| {
                tables::enqueue_moves_to_this_device_on(sql, &release_ids, &target_dir, created_at)
            })
            .await?;
        if added {
            self.wake();
        }
        Ok(added)
    }
}

#[cfg(desktop)]
impl<Extra: QueuedPayload> QueuedReleases<Extra, u8> {
    /// Record the active entry's percent.
    pub fn set_active_percent(&self, release_id: &str, percent: u8) {
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

//! The import event channel: the events the import service emits, each
//! recorded in the candidate runtime, then handed to every reader, and the
//! readers the handle gives out.

use super::*;

/// Every event the import service emits, on one channel.
#[derive(Debug, Clone)]
pub enum ImportEvent {
    Scan(ScanEvent),
    ImportProgress {
        candidate_key: crate::CandidateKey,
        progress: ImportProgress,
    },
    /// An identify run's new state, in full; the signals toolbar shows a
    /// projection of it.
    IdentifyStateChanged {
        candidate_key: crate::CandidateKey,
        /// The run this state belongs to. A settled run keeps reporting, so
        /// a consumer waiting on a later run of the candidate matches on this,
        /// not the key.
        run: crate::IdentifyRunId,
        state: bae_identify::IdentifyState,
        /// The run's priority, which tells a candidate a person opened from
        /// one the automatic admission picked up.
        priority: bae_catalogs::rate_limiter::CallPriority,
    },
    /// Full snapshot of a candidate's extracted signals (disc ID, barcodes,
    /// classified text), sent on every change of the extraction, including an
    /// abort, which fails every signal.
    SignalsUpdated {
        candidate_key: crate::CandidateKey,
        /// The identify run this snapshot was extracted for. Runs of one
        /// candidate share its key, so a run's verdict is stored beside only
        /// the snapshots that name that run.
        run: crate::IdentifyRunId,
        signals: bae_identify::signals::Signals,
        /// Where the artwork pass feeding the snapshot has got to, shown as the
        /// run's step. Kept outside the snapshot because a stored snapshot has
        /// no pass.
        artwork: bae_identify::signals::ArtworkScan,
        /// The extraction's priority, as in
        /// [`ImportEvent::IdentifyStateChanged`].
        priority: bae_catalogs::rate_limiter::CallPriority,
    },
}

#[derive(Debug, Clone)]
pub enum ScanEvent {
    /// The full ordered watched-folder list, sent at load and after each
    /// change.
    WatchedFoldersChanged {
        folders: Vec<WatchedFolder>,
    },
    FolderCandidate {
        candidate: FolderCandidate,
        /// The write this announces listed the release for the first time —
        /// new to the library, or listed anew by a person's decision about
        /// how its folder reads — while identification runs on its own.
        found_while_automatic: bool,
    },
    CandidateDiscovered {
        candidate: FolderCandidate,
    },
    /// A folder that looks like a release but failed validation (corrupt or
    /// empty audio, corrupt image, a CUE sheet naming missing audio), keyed by
    /// its folder path.
    InvalidCandidate(InvalidCandidate),
    /// A candidate is gone: a rescan no longer finds it, or its watched folder
    /// was removed (one event per candidate).
    CandidateRemoved {
        candidate_key: crate::CandidateKey,
    },
    /// A person skipped or unskipped a candidate.
    CandidateSkipChanged {
        candidate_key: crate::CandidateKey,
        skipped: bool,
    },
    /// A person bound one of a candidate's track sheets to an audio file, or
    /// cleared the binding. Carries the re-derived candidate, since a bound
    /// sheet is a different disc, and says its stored identify verdict was
    /// cleared.
    CandidateBindingChanged {
        candidate: FolderCandidate,
    },
    /// The candidate's metadata draft, its provenance, or its release link
    /// changed.
    CandidateMetadataChanged {
        candidate_key: crate::CandidateKey,
    },
    FolderScanStatusChanged {
        status: WatchedFolderScanStatus,
    },
    Finished,
}

/// Import events, none dropped, in the order they were sent, from the
/// import event bus that made it.
pub struct ImportEvents(mpsc::UnboundedReceiver<ImportEvent>);

impl ImportEvents {
    /// The next event, or `None` once the bus is gone.
    pub async fn recv(&mut self) -> Option<ImportEvent> {
        self.0.recv().await
    }

    /// The next event already sent, without waiting.
    pub fn try_recv(&mut self) -> Result<ImportEvent, mpsc::error::TryRecvError> {
        self.0.try_recv()
    }

    /// Events over `events`, for a test that sends a reader its events
    /// itself.
    #[cfg(test)]
    pub(crate) fn of_test_channel(events: mpsc::UnboundedReceiver<ImportEvent>) -> Self {
        Self(events)
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl bae_util::test_channels::EventReader<ImportEvent> for ImportEvents {
    fn recv(&mut self) -> impl std::future::Future<Output = Option<ImportEvent>> {
        self.0.recv()
    }
}

/// One import's progress, none dropped, from the import service handle
/// that made it.
#[cfg(any(test, feature = "test-utils"))]
pub struct ImportProgressFeed(mpsc::UnboundedReceiver<ImportProgress>);

#[cfg(any(test, feature = "test-utils"))]
impl ImportProgressFeed {
    /// The next import's progress, or `None` once the import service is gone.
    pub async fn recv(&mut self) -> Option<ImportProgress> {
        self.0.recv().await
    }

    /// The next import's progress already sent, without waiting.
    pub fn try_recv(&mut self) -> Result<ImportProgress, mpsc::error::TryRecvError> {
        self.0.try_recv()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl bae_util::test_channels::EventReader<ImportProgress> for ImportProgressFeed {
    fn recv(&mut self) -> impl std::future::Future<Output = Option<ImportProgress>> {
        self.0.recv()
    }
}

/// Scan events, none dropped, from the import service handle that made
/// it.
#[cfg(any(test, feature = "test-utils"))]
pub struct ScanEvents(mpsc::UnboundedReceiver<ScanEvent>);

#[cfg(any(test, feature = "test-utils"))]
impl ScanEvents {
    /// The next scan event, or `None` once the import service is gone.
    pub async fn recv(&mut self) -> Option<ScanEvent> {
        self.0.recv().await
    }

    /// The next scan event already sent, without waiting.
    pub fn try_recv(&mut self) -> Result<ScanEvent, mpsc::error::TryRecvError> {
        self.0.try_recv()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl bae_util::test_channels::EventReader<ScanEvent> for ScanEvents {
    fn recv(&mut self) -> impl std::future::Future<Output = Option<ScanEvent>> {
        self.0.recv()
    }
}

/// The import event channel and the candidate runtime it records into.
///
/// Every event is recorded in the runtime before any reader hears it, so a
/// reader that hears an event and then asks the runtime finds its effect
/// already there.
#[derive(Clone)]
pub struct ImportEventBus {
    delivery: EventDelivery,
    runtime: CandidateRuntime,
    /// Stops the thread sending one chosen event until a test lets it
    /// through, so the test can act partway through the work that sends it.
    #[cfg(any(test, feature = "test-utils"))]
    send_hold: Arc<SendHold>,
}

/// Where [`ImportEventBus::hold_send_where`] stops a sender.
#[cfg(any(test, feature = "test-utils"))]
struct SendHold {
    state: std::sync::Mutex<HoldState>,
    changed: std::sync::Condvar,
    /// Set once a sender is stopped, for a test to await.
    held: tokio::sync::watch::Sender<bool>,
}

#[cfg(any(test, feature = "test-utils"))]
impl Default for SendHold {
    fn default() -> Self {
        Self {
            state: Default::default(),
            changed: Default::default(),
            held: tokio::sync::watch::Sender::new(false),
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Default)]
enum HoldState {
    #[default]
    Off,
    /// The next event this matches stops its sender.
    Armed(Box<dyn Fn(&ImportEvent) -> bool + Send>),
    /// A sender is stopped, its event neither recorded nor delivered.
    Held,
    /// The stopped sender was let go.
    Released,
}

/// The one way an event reaches the bus's readers.
#[derive(Clone)]
struct EventDelivery {
    every_event: Arc<std::sync::Mutex<EveryEvent>>,
    /// The feed's end, holding every event since the bus started, until the
    /// one reader in the app that acts on every event takes it.
    feed: Arc<std::sync::Mutex<Option<ImportEvents>>>,
}

/// The readers that hear every event, and — for a test reader from the start
/// — every event delivered so far, under one lock so a reader misses nothing
/// and hears nothing twice.
struct EveryEvent {
    readers: Vec<mpsc::UnboundedSender<ImportEvent>>,
    #[cfg(any(test, feature = "test-utils"))]
    delivered: Vec<ImportEvent>,
}

impl EventDelivery {
    fn new() -> Self {
        let (feed_reader, feed) = mpsc::unbounded_channel();
        Self {
            every_event: Arc::new(std::sync::Mutex::new(EveryEvent {
                readers: vec![feed_reader],
                #[cfg(any(test, feature = "test-utils"))]
                delivered: Vec::new(),
            })),
            feed: Arc::new(std::sync::Mutex::new(Some(ImportEvents(feed)))),
        }
    }

    /// Hand `event` to every reader.
    fn deliver(&self, event: ImportEvent) {
        let mut every_event = self.every_event.lock().unwrap();
        #[cfg(any(test, feature = "test-utils"))]
        every_event.delivered.push(event.clone());
        every_event.readers.retain(|reader| reader.send(event.clone()).is_ok());
    }
}

impl ImportEventBus {
    /// A bus recording into `runtime`.
    pub fn new(runtime: CandidateRuntime) -> Self {
        Self {
            delivery: EventDelivery::new(),
            runtime,
            #[cfg(any(test, feature = "test-utils"))]
            send_hold: Arc::default(),
        }
    }

    /// Record `event` in the runtime, then hand it to every reader.
    pub fn send(&self, event: ImportEvent) {
        #[cfg(any(test, feature = "test-utils"))]
        self.wait_if_held(&event);
        self.runtime.record_event(&event);
        self.delivery.deliver(event);
    }

    /// Record `event` in the runtime, await `written` — the standing it
    /// announces — and hand the event to every reader only once that write
    /// landed: a reader that hears it finds the standing in the table, and
    /// what the event says beside it already recorded. A write that fails is
    /// returned, and the event is not sent.
    pub(crate) async fn send_once_written<E>(
        &self,
        event: ImportEvent,
        written: impl std::future::Future<Output = Result<(), E>>,
    ) -> Result<(), E> {
        #[cfg(any(test, feature = "test-utils"))]
        self.wait_if_held(&event);
        self.runtime.record_event(&event);
        written.await?;
        self.delivery.deliver(event);
        Ok(())
    }

    /// Every event since the bus started, none dropped, each on the reader
    /// before its send returns, for the one reader in the app that acts on
    /// every event, whenever it starts. A reader that also takes commands
    /// hears every event sent before a command before the command itself.
    /// `None` once taken.
    pub(crate) fn take_feed(&self) -> Option<ImportEvents> {
        self.delivery.feed.lock().unwrap().take()
    }

    /// Every event sent from now on, none dropped, for a test to wait on.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_event(&self) -> ImportEvents {
        let (reader, events) = mpsc::unbounded_channel();
        self.delivery.every_event.lock().unwrap().readers.push(reader);
        ImportEvents(events)
    }

    /// Every event this bus has delivered and will deliver, none dropped —
    /// for a test that starts reading after the work it waits on began.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_event_from_start(&self) -> ImportEvents {
        let (reader, events) = mpsc::unbounded_channel();
        let mut every_event = self.delivery.every_event.lock().unwrap();
        for event in &every_event.delivered {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the receiver is returned below, so the send cannot fail"
            )]
            let _ = reader.send(event.clone());
        }
        every_event.readers.push(reader);
        ImportEvents(events)
    }

    /// Stop the first thread that sends an event `matches` takes, before the
    /// event is recorded or delivered, until [`Self::release_send`].
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn hold_send_where(&self, matches: impl Fn(&ImportEvent) -> bool + Send + 'static) {
        *self.send_hold.state.lock().unwrap() = HoldState::Armed(Box::new(matches));
    }

    /// [`Self::hold_send_where`] a measured percent of `phase` above zero:
    /// those come from the threads doing the work, while a zero comes from
    /// the worker, which must stay free to hear a cancel.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn hold_progress_at(&self, phase: crate::ImportPhase) {
        self.hold_send_where(move |event| {
            matches!(
                event,
                ImportEvent::ImportProgress {
                    progress: ImportProgress::Progress {
                        phase: sent,
                        percent: Some(percent),
                        ..
                    },
                    ..
                } if *sent == phase && *percent > 0
            )
        });
    }

    /// Wait until a sender is stopped where [`Self::hold_send_where`] said.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn send_held(&self) {
        self.send_hold
            .held
            .subscribe()
            .wait_for(|held| *held)
            .await
            .expect("the hold outlives the wait for it");
    }

    /// Let the stopped sender go on.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn release_send(&self) {
        *self.send_hold.state.lock().unwrap() = HoldState::Released;
        self.send_hold.changed.notify_all();
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn wait_if_held(&self, event: &ImportEvent) {
        let mut state = self.send_hold.state.lock().unwrap();
        if !matches!(&*state, HoldState::Armed(matches) if matches(event)) {
            return;
        }
        *state = HoldState::Held;
        self.send_hold.held.send_replace(true);
        let _released = self
            .send_hold
            .changed
            .wait_while(state, |state| matches!(state, HoldState::Held))
            .unwrap();
    }
}

impl ImportServiceHandle {
    /// Every `ImportProgress` of `import_id` from its start, none dropped.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_import(&self, import_id: String) -> ImportProgressFeed {
        let mut events = self.event_tx.every_event_from_start();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.runtime_handle.spawn(async move {
            while let Some(event) = events.recv().await {
                if tx.is_closed() {
                    break;
                }
                if let ImportEvent::ImportProgress { progress, .. } = event {
                    if progress.import_id() == import_id && tx.send(progress).is_err() {
                        break;
                    }
                }
            }
        });
        ImportProgressFeed(rx)
    }

    /// Every import event since the service started, for the one
    /// identification queue that takes it (see
    /// [`super::ImportEventBus::take_feed`]).
    pub(crate) fn take_event_feed(&self) -> Option<ImportEvents> {
        self.event_tx.take_feed()
    }

    /// Every import event from now on, none dropped, for a test to wait on.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_event(&self) -> ImportEvents {
        self.event_tx.every_event()
    }

    /// Every scan event from now on, none dropped, for a test to wait on.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_scan_event_for_test(&self) -> ScanEvents {
        let mut events = self.event_tx.every_event();
        let (reader, scan_events) = mpsc::unbounded_channel();
        self.runtime_handle.spawn(async move {
            while let Some(event) = events.recv().await {
                if let ImportEvent::Scan(event) = event {
                    if reader.send(event).is_err() {
                        break;
                    }
                }
            }
        });
        ScanEvents(scan_events)
    }

    /// Stop the first thread that reports a measured percent of `phase` of
    /// an import before the report is recorded or delivered, until
    /// [`Self::release_progress`], for a test that acts while an import is
    /// partway through.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn hold_progress_at(&self, phase: crate::ImportPhase) {
        self.event_tx.hold_progress_at(phase);
    }

    /// Wait until a thread is stopped where [`Self::hold_progress_at`] said.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn progress_held(&self) {
        self.event_tx.send_held().await;
    }

    /// Let the stopped thread go on.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn release_progress(&self) {
        self.event_tx.release_send();
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn emit_event_for_test(&self, event: ImportEvent) {
        self.event_tx.send(event);
    }
}

#[cfg(test)]
#[path = "event_bus_tests.rs"]
mod tests;

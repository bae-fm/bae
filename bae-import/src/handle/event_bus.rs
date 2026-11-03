//! The import event channel: every event recorded in the candidate runtime,
//! then handed to every reader.

use super::*;

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
    feed: Arc<std::sync::Mutex<Option<mpsc::UnboundedReceiver<ImportEvent>>>>,
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
            feed: Arc::new(std::sync::Mutex::new(Some(feed))),
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
    pub(crate) fn take_feed(&self) -> Option<mpsc::UnboundedReceiver<ImportEvent>> {
        self.delivery.feed.lock().unwrap().take()
    }

    /// Every event sent from now on, none dropped, for a test to wait on.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_event(&self) -> mpsc::UnboundedReceiver<ImportEvent> {
        let (reader, events) = mpsc::unbounded_channel();
        self.delivery.every_event.lock().unwrap().readers.push(reader);
        events
    }

    /// Every event this bus has delivered and will deliver, none dropped —
    /// for a test that starts reading after the work it waits on began.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_event_from_start(&self) -> mpsc::UnboundedReceiver<ImportEvent> {
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
        events
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

#[cfg(test)]
#[path = "event_bus_tests.rs"]
mod tests;

use super::timeline::TimelineFold;
use super::{PlaybackPositions, PlaybackProgress, PlaybackValues};
use bae_util::task_failure::caught_sync;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::mpsc as tokio_mpsc;
use tokio::sync::watch;
use tracing::{error, info};
/// Fans one progress stream out to every subscriber, writes what playback is
/// doing into the device's `playback_now` row, and folds the stream into the
/// positions and the timeline they make, which it writes into the device's
/// `playback_timeline` row as it changes.
#[derive(Clone)]
pub struct PlaybackProgressHandle {
    subscriptions: Arc<Mutex<Vec<tokio_mpsc::UnboundedSender<PlaybackProgress>>>>,
    positions: watch::Receiver<PlaybackPositions>,
}

/// Every progress event playback reports from the moment it was asked for,
/// none dropped, from the [`PlaybackProgressHandle`] that made it. Dropping
/// it unsubscribes.
pub struct PlaybackProgressFeed(tokio_mpsc::UnboundedReceiver<PlaybackProgress>);

impl PlaybackProgressFeed {
    /// The next event, or `None` once playback has stopped running.
    pub async fn recv(&mut self) -> Option<PlaybackProgress> {
        self.0.recv().await
    }

    /// The next event already reported, without waiting.
    pub fn try_recv(&mut self) -> Result<PlaybackProgress, tokio_mpsc::error::TryRecvError> {
        self.0.try_recv()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl bae_util::test_channels::EventReader<PlaybackProgress> for PlaybackProgressFeed {
    fn recv(&mut self) -> impl std::future::Future<Output = Option<PlaybackProgress>> {
        self.0.recv()
    }
}

/// Where playback is, now and as audio plays, from the
/// [`PlaybackProgressHandle`] that made it.
pub struct PlaybackPositionsWatch(watch::Receiver<PlaybackPositions>);

impl PlaybackPositionsWatch {
    /// The positions now; the value seen from here on.
    pub fn positions(&mut self) -> PlaybackPositions {
        self.0.borrow_and_update().clone()
    }

    /// Wait until the positions differ from the ones last seen; an error
    /// once playback has stopped running.
    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        self.0.changed().await
    }

    /// Whether the positions changed since they were last seen; an error
    /// once playback has stopped running.
    #[cfg(test)]
    pub(crate) fn has_changed(&self) -> Result<bool, watch::error::RecvError> {
        self.0.has_changed()
    }

    /// Wait until the positions satisfy `done`, and return them; an error
    /// once playback has stopped running.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn wait_for(
        &mut self,
        done: impl FnMut(&PlaybackPositions) -> bool,
    ) -> Result<PlaybackPositions, watch::error::RecvError> {
        Ok(self.0.wait_for(done).await?.clone())
    }
}

impl PlaybackProgressHandle {
    /// Spawn the fan-out task that takes each event from `progress_rx` in
    /// order: it writes the change the event makes to what playback is doing
    /// into `row`, folds the event into the positions and, read at `clock`'s
    /// now, the timeline, writing a timeline that changed into
    /// `timeline_row`, then forwards it to every live subscriber. A panic
    /// folding one event, or a failed write of the timeline, is reported
    /// through `diagnostics`, and the fan-out forwards that event and serves
    /// the next. A failed write of `row` is a playback failure: it is reported
    /// through `diagnostics`, written to the row as the failure when that
    /// works, and forwarded as a `PlaybackError` after the event it could not
    /// write, which halts the service as any playback error does.
    pub fn new(
        mut progress_rx: tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
        runtime_handle: tokio::runtime::Handle,
        clock: crate::PlaybackClockRef,
        row: crate::tables::PlaybackNowRow,
        timeline_row: crate::tables::PlaybackTimelineRow,
        diagnostics: bae_diagnostics::Diagnostics,
    ) -> Self {
        let subscriptions: Arc<Mutex<Vec<tokio_mpsc::UnboundedSender<PlaybackProgress>>>> =
            Arc::new(Mutex::new(Vec::new()));
        let subscriptions_clone = subscriptions.clone();
        let (positions_tx, positions) = watch::channel(PlaybackPositions::initial());
        let fan_out_diagnostics = diagnostics.clone();
        let fan_out = async move {
            // What the row holds, as starting playback left it and this
            // fan-out wrote it since: what each event is compared against.
            let mut values = PlaybackValues::initial();
            let mut fold = TimelineFold::new();
            loop {
                match progress_rx.recv().await {
                    Some(progress) => {
                        let mut write_failure = None;
                        let folded = caught_sync("fold a playback event", || {
                            let change = values.change_for(&progress);
                            publish_change(&positions_tx, |positions| {
                                positions.applying(&progress)
                            });
                            change
                        });
                        match folded {
                            Ok(change) => {
                                if let Some(change) = change {
                                    match row.apply(&change).await {
                                        Ok(()) => values.took(&change),
                                        Err(e) => {
                                            error!(
                                                "couldn't write what playback is doing ({change:?}): {e}"
                                            );
                                            diagnostics
                                                .report_failure("write what playback is doing", &e);
                                            diagnostics.event(
                                                bae_diagnostics::TelemetryEvent::PlaybackFailed {
                                                    operation:
                                                        bae_diagnostics::PlaybackOperation::Write,
                                                },
                                            );
                                            // The change is told below, then this failure: the
                                            // row holds it for the screens when it can. Playback
                                            // halts on hearing it, so the row, the audio and the
                                            // screens agree. The values stay what the row holds.
                                            let reason = crate::PlaybackErrorReason::internal(
                                                format!("write what playback is doing: {e}"),
                                            );
                                            if let Err(e) = row
                                                .apply(&super::PlaybackChange::Failed(
                                                    reason.clone(),
                                                ))
                                                .await
                                            {
                                                error!("couldn't write why playback failed: {e}");
                                                diagnostics.report_failure(
                                                    "write why playback failed",
                                                    &e,
                                                );
                                            }
                                            write_failure =
                                                Some(PlaybackProgress::PlaybackError { reason });
                                        }
                                    }
                                }
                                match caught_sync("fold a playback event", || {
                                    fold.next(&positions_tx.borrow(), &values, clock.now())
                                }) {
                                    Ok(Some(timeline)) => {
                                        if let Err(e) = timeline_row.write(&timeline).await {
                                            error!(
                                                "couldn't write where playback is ({timeline:?}): {e}"
                                            );
                                            diagnostics
                                                .report_failure("write where playback is", &e);
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(bug) => {
                                        diagnostics.panicked(bug);
                                    }
                                }
                            }
                            Err(bug) => {
                                diagnostics.panicked(bug);
                            }
                        }
                        let mut subs = subscriptions_clone.lock().unwrap();
                        subs.retain(|tx| tx.send(progress.clone()).is_ok());
                        if let Some(failure) = write_failure {
                            subs.retain(|tx| tx.send(failure.clone()).is_ok());
                        }
                    }
                    None => {
                        info!("Playback progress channel closed, exiting");
                        break;
                    }
                }
            }
        };
        // Each event's fold catches its own panic. A panic outside them ends
        // the fan-out, and its subscribers' feeds end with it.
        let ended_subscriptions = subscriptions.clone();
        drop(fan_out_diagnostics.spawn_reported(
            &runtime_handle,
            "fan out playback progress",
            fan_out,
            move |_| ended_subscriptions.lock().unwrap_or_else(PoisonError::into_inner).clear(),
        ));
        Self { subscriptions, positions }
    }
    /// Every progress update from now on. Dropping the feed unsubscribes
    /// (the fan-out prunes closed senders).
    pub fn subscribe_all(&self) -> PlaybackProgressFeed {
        let (tx, rx) = tokio_mpsc::unbounded_channel();
        self.subscriptions.lock().unwrap().push(tx);
        PlaybackProgressFeed(rx)
    }

    /// The playback positions, which change as audio plays.
    pub fn subscribe_positions(&self) -> PlaybackPositionsWatch {
        PlaybackPositionsWatch(self.positions.clone())
    }
}

/// Store `next`'s result for `tx`'s value and wake its receivers, only when
/// that result differs from the value they hold.
fn publish_change<T: PartialEq>(tx: &watch::Sender<T>, next: impl FnOnce(&T) -> Option<T>) {
    tx.send_if_modified(|current| match next(current) {
        Some(next) if next != *current => {
            *current = next;
            true
        }
        Some(_) | None => false,
    });
}

#[cfg(test)]
#[path = "handle_tests.rs"]
mod tests;

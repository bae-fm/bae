use super::timeline::TimelineFold;
use super::{PlaybackPositions, PlaybackProgress, PlaybackValues};
use bae_util::task_failure::caught_sync;
use std::sync::{Arc, Mutex};
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
impl PlaybackProgressHandle {
    /// Spawn the fan-out task that takes each event from `progress_rx` in
    /// order: it writes the change the event makes to what playback is doing
    /// into `row`, folds the event into the positions and, read at `clock`'s
    /// now, the timeline, writing a timeline that changed into
    /// `timeline_row`, then forwards it to every live subscriber. A panic
    /// folding one event, or a failed write, is reported through
    /// `diagnostics`, and the fan-out forwards that event and serves the next.
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
        runtime_handle.spawn(async move {
            // What the row holds, as starting playback left it and this
            // fan-out wrote it since: what each event is compared against.
            let mut values = PlaybackValues::initial();
            let mut fold = TimelineFold::new();
            loop {
                match progress_rx.recv().await {
                    Some(progress) => {
                        let folded = caught_sync("fold a playback event", || {
                            let change = values.apply(&progress);
                            publish_change(&positions_tx, |positions| {
                                positions.applying(&progress)
                            });
                            let timeline =
                                fold.next(&positions_tx.borrow(), &values, clock.now());
                            (change, timeline)
                        });
                        match folded {
                            Ok((change, timeline)) => {
                                if let Some(change) = change {
                                    if let Err(e) = row.apply(&change).await {
                                        error!(
                                            "couldn't write what playback is doing ({change:?}): {e}"
                                        );
                                        diagnostics.report_bug("write what playback is doing", &e);
                                    }
                                }
                                if let Some(timeline) = timeline {
                                    if let Err(e) = timeline_row.write(&timeline).await {
                                        error!("couldn't write where playback is ({timeline:?}): {e}");
                                        diagnostics.report_bug("write where playback is", &e);
                                    }
                                }
                            }
                            Err(failed) => {
                                diagnostics.bug(failed);
                            }
                        }
                        let mut subs = subscriptions_clone.lock().unwrap();
                        subs.retain(|tx| tx.send(progress.clone()).is_ok());
                    }
                    None => {
                        info!("Playback progress channel closed, exiting");
                        break;
                    }
                }
            }
        });
        Self { subscriptions, positions }
    }
    /// A receiver yielding every progress update. Dropping it unsubscribes (the
    /// fan-out prunes closed senders).
    pub fn subscribe_all(&self) -> tokio_mpsc::UnboundedReceiver<PlaybackProgress> {
        let (tx, rx) = tokio_mpsc::unbounded_channel();
        self.subscriptions.lock().unwrap().push(tx);
        rx
    }

    /// The playback positions, which change as audio plays.
    pub fn subscribe_positions(&self) -> watch::Receiver<PlaybackPositions> {
        self.positions.clone()
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

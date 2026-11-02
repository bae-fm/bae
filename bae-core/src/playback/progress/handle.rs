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
        clock: crate::playback::PlaybackClockRef,
        row: crate::playback::tables::PlaybackNowRow,
        timeline_row: crate::playback::tables::PlaybackTimelineRow,
        diagnostics: crate::diagnostics::Diagnostics,
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
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    /// The fan-out over a fresh library's `playback_now` row, with the
    /// sender that feeds it. The `TempDir` holds the library.
    async fn started() -> (
        PlaybackProgressHandle,
        tokio_mpsc::UnboundedSender<PlaybackProgress>,
        crate::db::Database,
        tempfile::TempDir,
    ) {
        let tmp = tempfile::TempDir::new().unwrap();
        let db = crate::db::Database::new_test(
            tmp.path().join("test.db").to_str().unwrap(),
            Arc::new(coven::SystemClock),
        )
        .await
        .unwrap();
        let (progress_tx, progress_rx) = tokio_mpsc::unbounded_channel();
        let handle = PlaybackProgressHandle::new(
            progress_rx,
            tokio::runtime::Handle::current(),
            Arc::new(crate::playback::ManualPlaybackClock::new(chrono::Utc::now())),
            crate::playback::tables::PlaybackNowRow::new(db.clone()),
            crate::playback::tables::PlaybackTimelineRow::new(db.clone()),
            crate::diagnostics::Diagnostics::noop(),
        );
        (handle, progress_tx, db, tmp)
    }

    #[tokio::test]
    async fn forwards_to_live_subscribers_and_prunes_dropped_ones() {
        let (handle, progress_tx, _db, _tmp) = started().await;
        let mut live = handle.subscribe_all();
        let dropped = handle.subscribe_all();
        drop(dropped);

        progress_tx.send(PlaybackProgress::VolumeChanged { volume: 0.5 }).unwrap();

        match timeout(Duration::from_secs(1), live.recv())
            .await
            .expect("live subscriber receives event")
            .expect("live subscriber channel stays open")
        {
            PlaybackProgress::VolumeChanged { volume } => assert_eq!(volume, 0.5),
            other => panic!("expected volume event, got {other:?}"),
        }

        timeout(Duration::from_secs(1), async {
            loop {
                if handle.subscriptions.lock().unwrap().len() == 1 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("dropped subscriber is pruned");
    }

    /// The row holds an event's change by the time the event is forwarded.
    #[tokio::test]
    async fn a_change_is_in_the_row_before_its_event_is_forwarded() {
        let (handle, progress_tx, db, _tmp) = started().await;
        let mut events = handle.subscribe_all();

        progress_tx.send(PlaybackProgress::VolumeChanged { volume: 0.25 }).unwrap();
        progress_tx.send(PlaybackProgress::MuteChanged { is_muted: true }).unwrap();
        next_event(&mut events).await;
        next_event(&mut events).await;

        assert_eq!(crate::playback::tables::playback_level(&db).await.unwrap(), (0.25, true));
    }

    #[tokio::test]
    async fn seek_revision_is_retained_across_later_position_ticks() {
        let (handle, progress_tx, _db, _tmp) = started().await;
        let mut events = handle.subscribe_all();

        progress_tx
            .send(PlaybackProgress::Seeked {
                track_id: "track-1".into(),
                position_ms: 70,
                duration_ms: Some(100),
                progress: 0.7,
            })
            .unwrap();
        progress_tx
            .send(PlaybackProgress::PositionUpdate {
                track_id: "track-1".into(),
                position_ms: 71,
                duration_ms: Some(100),
                progress: 0.71,
            })
            .unwrap();
        next_event(&mut events).await;
        next_event(&mut events).await;

        let positions = handle.subscribe_positions();
        let positions = positions.borrow();
        assert_eq!(positions.library.as_ref().map(|position| position.position_ms), Some(71));
        assert_eq!(positions.seek_revision, 1);
    }

    /// A new line is in the timeline row by the time its event is
    /// forwarded; a tick along the line it holds writes nothing.
    #[tokio::test]
    async fn a_new_line_is_in_the_timeline_row_before_its_event_is_forwarded() {
        let (handle, progress_tx, db, _tmp) = started().await;
        let mut events = handle.subscribe_all();
        let tick = |position_ms| PlaybackProgress::PositionUpdate {
            track_id: "track-1".into(),
            position_ms,
            duration_ms: Some(100_000),
            progress: position_ms as f64 / 100_000.0,
        };
        progress_tx
            .send(PlaybackProgress::StateChanged {
                state: crate::playback::PlaybackState::Playing {
                    track: crate::playback::PlayingTrack {
                        track_id: "track-1".into(),
                        duration_ms: Some(100_000),
                    },
                },
            })
            .unwrap();
        progress_tx.send(tick(10_000)).unwrap();
        progress_tx.send(tick(10_200)).unwrap();
        for _ in 0..3 {
            next_event(&mut events).await;
        }

        let timeline = crate::playback::tables::playback_timeline(&db).await.unwrap();
        assert_eq!(
            timeline.library,
            Some(crate::playback::TimelinePoint {
                track_id: "track-1".into(),
                position_ms: 10_000,
                duration_ms: Some(100_000),
                moving: true,
            }),
            "the tick along the line left the line as told"
        );
    }

    /// The next event the fan-out forwards, which it writes and folds into
    /// the positions first.
    async fn next_event(
        events: &mut tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
    ) -> PlaybackProgress {
        timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("the fan-out forwards the event")
            .expect("the fan-out stays open")
    }

    /// A state change reaches the row and leaves the positions; a position
    /// tick moves the positions and leaves the row.
    #[tokio::test]
    async fn a_position_tick_moves_only_the_positions_and_a_state_change_only_the_row() {
        let (handle, progress_tx, db, _tmp) = started().await;
        let mut events = handle.subscribe_all();
        let mut positions = handle.subscribe_positions();
        positions.mark_unchanged();
        assert_eq!(
            crate::playback::tables::now_playing(&db).await.unwrap().map(|now| now.playback.state),
            Some(crate::playback::PlaybackState::Stopped)
        );

        progress_tx
            .send(PlaybackProgress::StateChanged {
                state: crate::playback::PlaybackState::Playing {
                    track: crate::playback::PlayingTrack {
                        track_id: "track-1".into(),
                        duration_ms: Some(100),
                    },
                },
            })
            .unwrap();
        next_event(&mut events).await;
        assert_eq!(
            crate::playback::tables::now_playing(&db).await.unwrap(),
            None,
            "the row names track-1, which the library does not hold"
        );
        assert!(!positions.has_changed().unwrap(), "a state change moves no position");

        progress_tx
            .send(PlaybackProgress::PositionUpdate {
                track_id: "track-1".into(),
                position_ms: 10,
                duration_ms: Some(100),
                progress: 0.1,
            })
            .unwrap();
        next_event(&mut events).await;
        assert!(positions.has_changed().unwrap(), "a position tick sends a position");
        assert_eq!(crate::playback::tables::playback_level(&db).await.unwrap(), (1.0, false));
    }
}

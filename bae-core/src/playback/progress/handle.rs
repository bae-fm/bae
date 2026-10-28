use super::timeline::{PlaybackTimeline, TimelineFold};
use super::{PlaybackPositions, PlaybackProgress, PlaybackValues};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc as tokio_mpsc;
use tokio::sync::watch;
use tracing::info;
/// Fans one progress stream out to every subscriber, and folds it into the
/// playback values, the positions, and the timeline they make.
#[derive(Clone)]
pub struct PlaybackProgressHandle {
    subscriptions: Arc<Mutex<Vec<tokio_mpsc::UnboundedSender<PlaybackProgress>>>>,
    values: watch::Receiver<PlaybackValues>,
    positions: watch::Receiver<PlaybackPositions>,
    timeline: watch::Receiver<PlaybackTimeline>,
}
impl PlaybackProgressHandle {
    /// Spawn the fan-out task that forwards each event from `progress_rx` to
    /// every live subscriber, after folding it into the values, the positions
    /// and, read at `clock`'s now, the timeline.
    pub fn new(
        mut progress_rx: tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
        runtime_handle: tokio::runtime::Handle,
        clock: crate::playback::PlaybackClockRef,
    ) -> Self {
        let subscriptions: Arc<Mutex<Vec<tokio_mpsc::UnboundedSender<PlaybackProgress>>>> =
            Arc::new(Mutex::new(Vec::new()));
        let subscriptions_clone = subscriptions.clone();
        let (values_tx, values) = watch::channel(PlaybackValues::initial());
        let (positions_tx, positions) = watch::channel(PlaybackPositions::initial());
        let (timeline_tx, timeline) = watch::channel(PlaybackTimeline::initial());
        runtime_handle.spawn(async move {
            let mut fold = TimelineFold::new();
            loop {
                match progress_rx.recv().await {
                    Some(progress) => {
                        publish_change(&values_tx, |values| values.applying(&progress));
                        publish_change(&positions_tx, |positions| positions.applying(&progress));
                        if let Some(next) =
                            fold.next(&positions_tx.borrow(), &values_tx.borrow(), clock.now())
                        {
                            timeline_tx.send_replace(next);
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
        Self {
            subscriptions,
            values,
            positions,
            timeline,
        }
    }
    /// A receiver yielding every progress update. Dropping it unsubscribes (the
    /// fan-out prunes closed senders).
    pub fn subscribe_all(&self) -> tokio_mpsc::UnboundedReceiver<PlaybackProgress> {
        let (tx, rx) = tokio_mpsc::unbounded_channel();
        self.subscriptions.lock().unwrap().push(tx);
        rx
    }

    /// The playback values, which a receiver sees change only when one of
    /// them does: a position tick leaves them alone.
    pub fn subscribe_values(&self) -> watch::Receiver<PlaybackValues> {
        self.values.clone()
    }

    /// The playback positions, which change as audio plays.
    pub fn subscribe_positions(&self) -> watch::Receiver<PlaybackPositions> {
        self.positions.clone()
    }

    /// The timeline, which changes only as playback leaves the line it
    /// holds.
    pub fn subscribe_timeline(&self) -> watch::Receiver<PlaybackTimeline> {
        self.timeline.clone()
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

    fn test_clock() -> crate::playback::PlaybackClockRef {
        std::sync::Arc::new(crate::playback::ManualPlaybackClock::new(chrono::Utc::now()))
    }

    #[tokio::test]
    async fn forwards_to_live_subscribers_and_prunes_dropped_ones() {
        let (progress_tx, progress_rx) = tokio_mpsc::unbounded_channel();
        let handle = PlaybackProgressHandle::new(
            progress_rx,
            tokio::runtime::Handle::current(),
            test_clock(),
        );
        let mut live = handle.subscribe_all();
        let dropped = handle.subscribe_all();
        drop(dropped);

        progress_tx
            .send(PlaybackProgress::VolumeChanged { volume: 0.5 })
            .unwrap();

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

    #[tokio::test]
    async fn persistent_values_are_replayed_to_late_subscribers() {
        let (progress_tx, progress_rx) = tokio_mpsc::unbounded_channel();
        let handle = PlaybackProgressHandle::new(
            progress_rx,
            tokio::runtime::Handle::current(),
            test_clock(),
        );

        progress_tx
            .send(PlaybackProgress::VolumeChanged { volume: 0.25 })
            .unwrap();
        tokio::task::yield_now().await;

        let late = handle.subscribe_values();
        assert_eq!(late.borrow().volume, 0.25);
    }

    #[tokio::test]
    async fn seek_revision_is_retained_across_later_position_ticks() {
        let (progress_tx, progress_rx) = tokio_mpsc::unbounded_channel();
        let handle = PlaybackProgressHandle::new(
            progress_rx,
            tokio::runtime::Handle::current(),
            test_clock(),
        );
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
        assert_eq!(
            positions
                .library
                .as_ref()
                .map(|position| position.position_ms),
            Some(71)
        );
        assert_eq!(positions.seek_revision, 1);
    }

    /// The next event the fan-out forwards, which it folds into the values
    /// and positions first.
    async fn next_event(
        events: &mut tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
    ) -> PlaybackProgress {
        timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("the fan-out forwards the event")
            .expect("the fan-out stays open")
    }

    #[tokio::test]
    async fn a_position_tick_sends_no_value_and_a_state_change_sends_one() {
        let (progress_tx, progress_rx) = tokio_mpsc::unbounded_channel();
        let handle = PlaybackProgressHandle::new(
            progress_rx,
            tokio::runtime::Handle::current(),
            test_clock(),
        );
        let mut events = handle.subscribe_all();
        let mut values = handle.subscribe_values();
        let mut positions = handle.subscribe_positions();
        values.mark_unchanged();
        positions.mark_unchanged();
        let playing = PlaybackProgress::StateChanged {
            state: crate::playback::PlaybackState::Playing {
                track: crate::playback::PlayingTrack {
                    track_id: "track-1".into(),
                    duration_ms: Some(100),
                },
            },
        };

        progress_tx.send(playing.clone()).unwrap();
        next_event(&mut events).await;
        assert!(values.has_changed().unwrap(), "the state change is sent");
        values.mark_unchanged();

        progress_tx
            .send(PlaybackProgress::PositionUpdate {
                track_id: "track-1".into(),
                position_ms: 10,
                duration_ms: Some(100),
                progress: 0.1,
            })
            .unwrap();
        next_event(&mut events).await;
        assert!(
            !values.has_changed().unwrap(),
            "a position tick sends no value"
        );
        assert!(
            positions.has_changed().unwrap(),
            "a position tick sends a position"
        );
        positions.mark_unchanged();

        progress_tx.send(playing).unwrap();
        next_event(&mut events).await;
        assert!(
            !values.has_changed().unwrap(),
            "the same state again sends nothing"
        );
        assert!(
            !positions.has_changed().unwrap(),
            "the same track's state leaves its position"
        );

        progress_tx
            .send(PlaybackProgress::VolumeChanged { volume: 0.5 })
            .unwrap();
        next_event(&mut events).await;
        assert!(values.has_changed().unwrap(), "a volume change is sent");
        assert!(
            !positions.has_changed().unwrap(),
            "a volume change moves no position"
        );
    }
}

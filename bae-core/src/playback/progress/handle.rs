use super::{PlaybackProgress, PlaybackValues};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc as tokio_mpsc;
use tracing::info;
/// Fans one progress stream out to every subscriber.
#[derive(Clone)]
pub struct PlaybackProgressHandle {
    subscriptions: Arc<Mutex<Vec<tokio_mpsc::UnboundedSender<PlaybackProgress>>>>,
    values: tokio::sync::watch::Receiver<PlaybackValues>,
}
impl PlaybackProgressHandle {
    /// Spawn the fan-out task that forwards each event from `progress_rx` to
    /// every live subscriber.
    pub fn new(
        mut progress_rx: tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        let subscriptions: Arc<Mutex<Vec<tokio_mpsc::UnboundedSender<PlaybackProgress>>>> =
            Arc::new(Mutex::new(Vec::new()));
        let subscriptions_clone = subscriptions.clone();
        let (values_tx, values) = tokio::sync::watch::channel(PlaybackValues::initial());
        runtime_handle.spawn(async move {
            loop {
                match progress_rx.recv().await {
                    Some(progress) => {
                        let next = { values_tx.borrow().applying(&progress) };
                        if let Some(next) = next {
                            values_tx.send_replace(next);
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
        }
    }
    /// A receiver yielding every progress update. Dropping it unsubscribes (the
    /// fan-out prunes closed senders).
    pub fn subscribe_all(&self) -> tokio_mpsc::UnboundedReceiver<PlaybackProgress> {
        let (tx, rx) = tokio_mpsc::unbounded_channel();
        self.subscriptions.lock().unwrap().push(tx);
        rx
    }

    pub fn subscribe_values(&self) -> tokio::sync::watch::Receiver<PlaybackValues> {
        self.values.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn forwards_to_live_subscribers_and_prunes_dropped_ones() {
        let (progress_tx, progress_rx) = tokio_mpsc::unbounded_channel();
        let handle = PlaybackProgressHandle::new(progress_rx, tokio::runtime::Handle::current());
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
        let handle = PlaybackProgressHandle::new(progress_rx, tokio::runtime::Handle::current());

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
        let handle = PlaybackProgressHandle::new(progress_rx, tokio::runtime::Handle::current());

        progress_tx
            .send(PlaybackProgress::Seeked {
                track_id: "track-1".into(),
                position_ms: 70,
                duration_ms: 100,
                progress: 0.7,
            })
            .unwrap();
        progress_tx
            .send(PlaybackProgress::PositionUpdate {
                track_id: "track-1".into(),
                position_ms: 71,
                duration_ms: 100,
                progress: 0.71,
            })
            .unwrap();

        timeout(Duration::from_secs(1), async {
            loop {
                if handle
                    .values
                    .borrow()
                    .position
                    .as_ref()
                    .is_some_and(|position| position.position_ms == 71)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("position tick is retained");
        let values = handle.subscribe_values();
        assert_eq!(values.borrow().seek_revision, 1);
    }
}

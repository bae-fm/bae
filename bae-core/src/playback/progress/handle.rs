use super::PlaybackProgress;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc as tokio_mpsc;
use tracing::info;
/// Fans one progress stream out to every subscriber.
#[derive(Clone)]
pub struct PlaybackProgressHandle {
    subscriptions: Arc<Mutex<Vec<tokio_mpsc::UnboundedSender<PlaybackProgress>>>>,
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
        runtime_handle.spawn(async move {
            loop {
                match progress_rx.recv().await {
                    Some(progress) => {
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
        Self { subscriptions }
    }
    /// A receiver yielding every progress update. Dropping it unsubscribes (the
    /// fan-out prunes closed senders).
    pub fn subscribe_all(&self) -> tokio_mpsc::UnboundedReceiver<PlaybackProgress> {
        let (tx, rx) = tokio_mpsc::unbounded_channel();
        self.subscriptions.lock().unwrap().push(tx);
        rx
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
}

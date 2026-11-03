use super::*;

fn progress(bytes_done: u64) -> DownloadTransferProgress {
    DownloadTransferProgress::new("release", bytes_done, 100).unwrap()
}

/// The first read is every downloading release; a step of bytes names
/// only the release it moved; finishing removes it.
#[tokio::test]
async fn a_reader_hears_each_release_whose_progress_moved() {
    let (tx, rx) = watch::channel(BTreeMap::from([("release-1".to_string(), progress(10))]));
    let mut watch = DownloadTransfersWatch::of(rx);
    assert_eq!(
        watch.next().await,
        Some(vec![ReleaseTransferChange::Reset(BTreeMap::from([(
            "release-1".to_string(),
            progress(10)
        )]))])
    );

    tx.send_replace(BTreeMap::from([("release-1".to_string(), progress(40))]));
    assert_eq!(
        watch.next().await,
        Some(vec![ReleaseTransferChange::Updated {
            release_id: "release-1".to_string(),
            progress: progress(40),
        }])
    );

    tx.send_replace(BTreeMap::new());
    assert_eq!(
        watch.next().await,
        Some(vec![ReleaseTransferChange::Removed { release_id: "release-1".to_string() }])
    );
}

/// An export's percent is its progress: a send that moves no percent says
/// nothing, and the next step is the next change heard.
#[tokio::test]
async fn a_reader_hears_each_export_whose_percent_moved() {
    let (tx, rx) = watch::channel(BTreeMap::from([("release-1".to_string(), 10u8)]));
    let mut watch = OutputTransfersWatch::of(rx);
    assert_eq!(
        watch.next().await,
        Some(vec![ReleaseTransferChange::Reset(BTreeMap::from([("release-1".to_string(), 10)]))])
    );

    tx.send_replace(BTreeMap::from([("release-1".to_string(), 10u8)]));
    let mut next = std::pin::pin!(watch.next());
    // Asked once with the send already there to read: it reads it, finds
    // no percent moved, and waits on.
    let polled = tokio::task::unconstrained(std::future::poll_fn(|cx| {
        std::task::Poll::Ready(std::future::Future::poll(next.as_mut(), cx))
    }))
    .await;
    assert!(polled.is_pending(), "an unmoved percent says nothing");
    tx.send_replace(BTreeMap::from([("release-1".to_string(), 62u8)]));
    assert_eq!(
        next.await,
        Some(vec![ReleaseTransferChange::Updated {
            release_id: "release-1".to_string(),
            progress: 62,
        }])
    );
}

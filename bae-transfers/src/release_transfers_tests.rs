use super::*;
use crate::DownloadTransferProgress;

fn progress(bytes_done: u64) -> DownloadTransferProgress {
    DownloadTransferProgress::new("release", bytes_done, 100).unwrap()
}

/// Each read is every downloading release's progress: at once, then after a
/// step of bytes, then with the release gone once it finishes.
#[tokio::test]
async fn a_reader_hears_every_release_each_time_progress_moves() {
    let transfers = ReleaseTransfers::new();
    transfers.set("release-1", progress(10));
    transfers.set("release-2", progress(20));
    let mut watch = transfers.watch();
    assert_eq!(
        watch.next().await,
        Some(BTreeMap::from([
            ("release-1".to_string(), progress(10)),
            ("release-2".to_string(), progress(20)),
        ]))
    );

    transfers.advance("release-1", progress(40));
    assert_eq!(
        watch.next().await,
        Some(BTreeMap::from([
            ("release-1".to_string(), progress(40)),
            ("release-2".to_string(), progress(20)),
        ]))
    );

    transfers.remove("release-1");
    assert_eq!(watch.next().await, Some(BTreeMap::from([("release-2".to_string(), progress(20))])));
}

/// An export's percent is its progress: a step that moves no percent says
/// nothing, and the next step is the next read.
#[tokio::test]
async fn a_reader_hears_nothing_until_a_percent_moves() {
    let transfers = ReleaseTransfers::new();
    transfers.set("release-1", 10u8);
    let mut watch = transfers.watch();
    assert_eq!(watch.next().await, Some(BTreeMap::from([("release-1".to_string(), 10)])));

    transfers.advance("release-1", 10);
    let mut next = std::pin::pin!(watch.next());
    // Asked once: nothing moved, so it waits on.
    let polled = tokio::task::unconstrained(std::future::poll_fn(|cx| {
        std::task::Poll::Ready(std::future::Future::poll(next.as_mut(), cx))
    }))
    .await;
    assert!(polled.is_pending(), "an unmoved percent says nothing");
    transfers.advance("release-1", 62);
    assert_eq!(next.await, Some(BTreeMap::from([("release-1".to_string(), 62)])));
}

/// A report lists its release while it lives and removes it once it drops.
#[tokio::test]
async fn a_report_lists_its_release_until_it_drops() {
    let transfers = ReleaseTransfers::new();
    let mut watch = transfers.watch();
    assert_eq!(watch.next().await, Some(BTreeMap::new()));
    let report = transfers.report("release-1");
    report.progress(7u8);
    assert_eq!(watch.next().await, Some(BTreeMap::from([("release-1".to_string(), 7)])));
    drop(report);
    assert_eq!(watch.next().await, Some(BTreeMap::new()));
}

/// The first read comes at once even after the transfers are gone; the next
/// says they are gone.
#[tokio::test]
async fn the_first_read_holds_the_progress_the_transfers_left() {
    let transfers = ReleaseTransfers::new();
    transfers.set("release-1", 3u8);
    let mut watch = transfers.watch();
    drop(transfers);
    assert_eq!(watch.next().await, Some(BTreeMap::from([("release-1".to_string(), 3)])));
    assert_eq!(watch.next().await, None);
}

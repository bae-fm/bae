use super::*;
use crate::DownloadTransferProgress;

fn progress(bytes_done: u64) -> DownloadTransferProgress {
    DownloadTransferProgress::new("release", bytes_done, 100).unwrap()
}

/// The first read is every downloading release; a step of bytes names
/// only the release it moved; finishing removes it.
#[tokio::test]
async fn a_reader_hears_each_release_whose_progress_moved() {
    let transfers = ReleaseTransfers::new();
    transfers.set("release-1", progress(10));
    let mut watch = transfers.watch();
    assert_eq!(
        watch.next().await,
        Some(vec![ReleaseTransferChange::Reset(BTreeMap::from([(
            "release-1".to_string(),
            progress(10)
        )]))])
    );

    transfers.advance("release-1", progress(40));
    assert_eq!(
        watch.next().await,
        Some(vec![ReleaseTransferChange::Updated {
            release_id: "release-1".to_string(),
            progress: progress(40),
        }])
    );

    transfers.remove("release-1");
    assert_eq!(
        watch.next().await,
        Some(vec![ReleaseTransferChange::Removed { release_id: "release-1".to_string() }])
    );
}

/// An export's percent is its progress: a step that moves no percent says
/// nothing, and the next step is the next change heard.
#[tokio::test]
async fn a_reader_hears_each_export_whose_percent_moved() {
    let transfers = ReleaseTransfers::new();
    transfers.set("release-1", 10u8);
    let mut watch = transfers.watch();
    assert_eq!(
        watch.next().await,
        Some(vec![ReleaseTransferChange::Reset(BTreeMap::from([("release-1".to_string(), 10)]))])
    );

    transfers.advance("release-1", 10);
    let mut next = std::pin::pin!(watch.next());
    // Asked once: nothing moved, so it waits on.
    let polled = tokio::task::unconstrained(std::future::poll_fn(|cx| {
        std::task::Poll::Ready(std::future::Future::poll(next.as_mut(), cx))
    }))
    .await;
    assert!(polled.is_pending(), "an unmoved percent says nothing");
    transfers.advance("release-1", 62);
    assert_eq!(
        next.await,
        Some(vec![ReleaseTransferChange::Updated {
            release_id: "release-1".to_string(),
            progress: 62,
        }])
    );
}

/// A report lists its release while it lives and removes it once it drops.
#[tokio::test]
async fn a_report_lists_its_release_until_it_drops() {
    let transfers = ReleaseTransfers::new();
    let mut watch = transfers.watch();
    watch.next().await;
    let report = transfers.report("release-1");
    report.progress(7u8);
    assert_eq!(
        watch.next().await,
        Some(vec![ReleaseTransferChange::Updated {
            release_id: "release-1".to_string(),
            progress: 7,
        }])
    );
    drop(report);
    assert_eq!(
        watch.next().await,
        Some(vec![ReleaseTransferChange::Removed { release_id: "release-1".to_string() }])
    );
}

/// Applying what a reader hears holds the progress it describes: the
/// changes between two reads take the first to the second.
#[test]
fn applying_the_changes_heard_holds_the_progress_they_describe() {
    let told = BTreeMap::from([
        ("release-1".to_string(), progress(10)),
        ("release-2".to_string(), progress(20)),
    ]);
    let next = BTreeMap::from([
        ("release-1".to_string(), progress(40)),
        ("release-3".to_string(), progress(5)),
    ]);
    let mut held = BTreeMap::new();
    apply_release_changes(&mut held, vec![ReleaseTransferChange::Reset(told.clone())]);
    assert_eq!(held, told);
    apply_release_changes(&mut held, release_changes(&told, &next));
    assert_eq!(held, next);
}

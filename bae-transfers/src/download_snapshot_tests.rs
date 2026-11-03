use super::*;
use std::time::{Duration, SystemTime};

fn progress(bytes_done: u64) -> DownloadTransferProgress {
    DownloadTransferProgress::new("release", bytes_done, 1_000).unwrap()
}

/// A download's rate is what it moved since it began over the time since,
/// none before it moves, and a drop in what the cache holds begins it
/// again.
#[test]
fn a_download_rate_counts_from_where_it_began() {
    let began = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    let mut rate = DownloadRate::begin(began, 100);
    assert_eq!(rate.measure(progress(100), began).throughput_bps, None);
    let later = began + Duration::from_secs(2);
    assert_eq!(rate.measure(progress(500), later).throughput_bps, Some(200));
    let dropped = later + Duration::from_secs(1);
    assert_eq!(rate.measure(progress(300), dropped).throughput_bps, None);
    let after = dropped + Duration::from_secs(1);
    assert_eq!(rate.measure(progress(400), after).throughput_bps, Some(100));
}

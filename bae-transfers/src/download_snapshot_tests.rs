use super::*;
use std::time::Duration;

fn progress(bytes_done: u64) -> DownloadTransferProgress {
    DownloadTransferProgress::new("release", bytes_done, 1_000).unwrap()
}

/// A download's rate is what it moved since it began over the time since,
/// none before it moves, and a drop in what the cache holds begins it
/// again.
#[test]
fn a_download_rate_counts_from_where_it_began() {
    let began = Duration::from_secs(100);
    let mut rate = DownloadRate::begin(began, 100);
    assert_eq!(rate.measure(progress(100), began).throughput_bps, None);
    let later = began + Duration::from_secs(2);
    assert_eq!(rate.measure(progress(500), later).throughput_bps, Some(200));
    let dropped = later + Duration::from_secs(1);
    assert_eq!(rate.measure(progress(300), dropped).throughput_bps, None);
    let after = dropped + Duration::from_secs(1);
    assert_eq!(rate.measure(progress(400), after).throughput_bps, Some(100));
}

fn step(before: u64, all: u64, done: u64, total: u64) -> coven::Progress<coven::DeviceOnlyStep> {
    coven::Progress {
        step: coven::DeviceOnlyStep::Downloading {
            file: coven::FileId(uuid::Uuid::from_u128(1)),
            before,
            all,
        },
        done,
        total,
    }
}

/// A release's progress is the file's bytes and, of the whole release,
/// the files before it and the file's own.
#[test]
fn a_release_has_come_as_far_as_its_files_before_and_its_file() {
    assert_eq!(
        MakeLocalTransferProgress::of(step(300, 1_000, 50, 200)),
        MakeLocalTransferProgress {
            file_bytes_done: 50,
            file_bytes_total: 200,
            bytes_done: 350,
            bytes_total: 1_000,
        }
    );
}

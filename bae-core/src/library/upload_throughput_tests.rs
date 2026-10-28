use super::*;

fn file(n: u128) -> FileId {
    FileId(uuid::Uuid::from_u128(n))
}

fn secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

#[test]
fn an_empty_tracker_has_no_rates() {
    assert!(UploadThroughput::default().rates(secs(5)).is_empty());
}

#[test]
fn restarting_or_retrying_does_not_count_old_bytes_as_new() {
    let id = file(1);
    let mut tracker = UploadThroughput::default();
    tracker.record(id, 400, 1000, Duration::ZERO);
    tracker.record(id, 600, 1000, secs(2));
    assert_eq!(tracker.rates(secs(2))[&id], 100);
    tracker.record(id, 100, 1000, secs(3));
    assert_eq!(tracker.rates(secs(3))[&id], 0);
    tracker.record(id, 300, 1000, secs(5));
    assert_eq!(tracker.rates(secs(5))[&id], 100);
    assert_eq!(tracker.rates(secs(16))[&id], 0);
}

#[test]
fn bytes_in_the_window_divide_by_the_window_once_it_has_filled() {
    let id = file(1);
    let mut tracker = UploadThroughput::default();
    tracker.record(id, 0, 20_000_000, Duration::ZERO);
    tracker.record(id, 5_000_000, 20_000_000, secs(5));
    tracker.record(id, 10_000_000, 20_000_000, secs(10));
    // 10 MB over the 10 s window is 1 MB/s.
    assert_eq!(tracker.rates(secs(10))[&id], 1_000_000);
}

#[test]
fn a_new_transfer_divides_by_its_elapsed_time_until_the_window_fills() {
    let id = file(1);
    let mut tracker = UploadThroughput::default();
    tracker.record(id, 0, 1_000_000, Duration::ZERO);
    let first_tick = Duration::from_millis(500);
    tracker.record(id, 500_000, 1_000_000, first_tick);
    assert_eq!(tracker.rates(first_tick)[&id], 1_000_000);
}

#[test]
fn a_transfer_measured_at_its_first_instant_has_no_rate_yet() {
    let id = file(1);
    let mut tracker = UploadThroughput::default();
    tracker.record(id, 100, 1000, secs(3));
    assert_eq!(tracker.rates(secs(3))[&id], 0);
}

#[test]
fn bytes_older_than_the_window_stop_counting() {
    let id = file(1);
    let mut tracker = UploadThroughput::default();
    tracker.record(id, 0, 20_000_000, Duration::ZERO);
    tracker.record(id, 10_000_000, 20_000_000, secs(1));
    assert_eq!(tracker.rates(secs(10))[&id], 1_000_000);
    assert_eq!(tracker.rates(secs(12))[&id], 0, "the sample aged out");
}

#[test]
fn concurrent_files_keep_their_own_rates() {
    let (first, second) = (file(1), file(2));
    let mut tracker = UploadThroughput::default();
    tracker.record(first, 0, 100_000_000, Duration::ZERO);
    tracker.record(second, 0, 100_000_000, Duration::ZERO);
    tracker.record(first, 10_000_000, 100_000_000, secs(10));
    tracker.record(second, 20_000_000, 100_000_000, secs(10));
    let rates = tracker.rates(secs(10));
    assert_eq!(rates[&first], 1_000_000);
    assert_eq!(rates[&second], 2_000_000);
}

#[test]
fn a_new_total_starts_a_new_measurement() {
    let id = file(1);
    let mut tracker = UploadThroughput::default();
    tracker.record(id, 0, 1000, Duration::ZERO);
    tracker.record(id, 500, 1000, secs(1));
    tracker.record(id, 600, 2000, secs(2));
    assert_eq!(
        tracker.rates(secs(2))[&id],
        0,
        "bytes against another total are another attempt"
    );
}

#[test]
#[should_panic(expected = "upload clock regressed")]
fn the_measurement_clock_cannot_regress() {
    let id = file(1);
    let mut tracker = UploadThroughput::default();
    tracker.record(id, 0, 1000, secs(5));
    tracker.rates(secs(4));
}

#[test]
#[should_panic(expected = "upload clock regressed")]
fn a_sample_clock_cannot_regress() {
    let id = file(1);
    let mut tracker = UploadThroughput::default();
    tracker.record(id, 0, 1000, Duration::ZERO);
    tracker.record(id, 10, 1000, secs(5));
    tracker.rates(secs(4));
}

fn sending(files: &[FileId], paused: bool) -> DbSendingUploads {
    DbSendingUploads {
        files: files
            .iter()
            .map(|file| crate::db::DbSendingFile {
                file: *file,
                release_id: None,
                file_size: None,
            })
            .collect(),
        releases: Vec::new(),
        paused,
    }
}

/// How far each file has got, as the upload watch tells it.
fn moved(files: &[(FileId, u64, u64)]) -> BTreeMap<FileId, coven::TransferProgress> {
    files
        .iter()
        .map(|(file, bytes_transferred, bytes_total)| {
            (
                *file,
                coven::TransferProgress {
                    bytes_transferred: *bytes_transferred,
                    bytes_total: *bytes_total,
                },
            )
        })
        .collect()
}

/// Each file being sent is measured from the bytes the watch tells it has
/// handed over, whether it goes in parts or in one request; a file the watch
/// doesn't name yet has no rate, a file that stops being sent loses its
/// rate, and a pause ends every measurement.
#[test]
fn the_files_being_sent_are_measured_from_the_watch_until_they_stop_or_pause() {
    let (first, second, unheard) = (file(1), file(2), file(3));
    let mut tracker = UploadThroughput::default();
    let all = [first, second, unheard];
    tracker.observe(
        &sending(&all, false),
        &moved(&[(first, 0, 1000), (second, 0, 2000)]),
        Duration::ZERO,
    );
    tracker.observe(
        &sending(&all, false),
        &moved(&[(first, 500, 1000), (second, 2000, 2000)]),
        secs(1),
    );
    let rates = tracker.rates(secs(1));
    assert_eq!(rates.get(&first), Some(&500));
    assert_eq!(rates.get(&second), Some(&2000));
    assert_eq!(rates.get(&unheard), None);

    tracker.observe(
        &sending(&[first], true),
        &moved(&[(first, 600, 1000)]),
        secs(2),
    );
    assert!(
        tracker.rates(secs(2)).is_empty(),
        "a pause ends the measurement"
    );

    tracker.observe(
        &sending(&[first], false),
        &moved(&[(first, 600, 1000)]),
        secs(3),
    );
    tracker.observe(&sending(&[], false), &BTreeMap::new(), secs(4));
    assert!(tracker.rates(secs(4)).is_empty(), "a file sent has no rate");
}

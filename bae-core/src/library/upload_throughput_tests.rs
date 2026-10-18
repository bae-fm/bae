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
fn restarting_or_retrying_does_not_count_old_confirmed_bytes_as_new() {
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

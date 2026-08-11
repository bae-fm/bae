use super::*;
use std::thread;
use std::time::Duration;

#[test]
fn test_append_and_read_single_range() {
    let buffer = create_sparse_buffer(11);
    buffer.append_at(0, b"hello world");

    let mut reader = buffer.new_reader();
    let mut buf = [0u8; 5];
    reader.seek(0);
    assert_eq!(reader.read(&mut buf), Some(5));
    assert_eq!(&buf, b"hello");
}

#[test]
fn test_is_buffered_single_range() {
    let buffer = SparseStreamingBuffer::new(10);
    buffer.append_at(0, b"0123456789");

    assert!(buffer.is_buffered(0));
    assert!(buffer.is_buffered(5));
    assert!(buffer.is_buffered(9));
    assert!(!buffer.is_buffered(10));
}

#[test]
fn test_multiple_non_contiguous_ranges() {
    let buffer = SparseStreamingBuffer::new(104);
    buffer.append_at(0, b"aaaa"); // 0-3
    buffer.append_at(100, b"bbbb"); // 100-103

    assert!(buffer.is_buffered(2));
    assert!(!buffer.is_buffered(50));
    assert!(buffer.is_buffered(101));
    assert_eq!(buffer.ranges_count(), 2);
}

#[test]
fn test_adjacent_ranges_merge() {
    let buffer = create_sparse_buffer(8);
    buffer.append_at(0, b"aaaa"); // 0-3
    buffer.append_at(4, b"bbbb"); // 4-7, should merge with above

    assert_eq!(buffer.ranges_count(), 1);
    assert!(buffer.is_buffered(5));

    // Verify data is correct after merge
    let mut reader = buffer.new_reader();
    reader.seek(0);
    let mut buf = [0u8; 8];
    assert_eq!(reader.read(&mut buf), Some(8));
    assert_eq!(&buf, b"aaaabbbb");
}

#[test]
fn test_overlapping_ranges_merge() {
    let buffer = create_sparse_buffer(10);
    buffer.append_at(0, b"aaaaaa"); // 0-5
    buffer.append_at(4, b"bbbbbb"); // 4-9, overlaps

    assert_eq!(buffer.ranges_count(), 1);

    let mut reader = buffer.new_reader();
    reader.seek(0);
    let mut buf = [0u8; 10];
    assert_eq!(reader.read(&mut buf), Some(10));
    // First 4 bytes are 'a', bytes 4-9 are 'b'
    assert_eq!(&buf, b"aaaabbbbbb");
}

#[test]
fn test_read_blocks_until_data_available() {
    let buffer = create_sparse_buffer(5);
    let buf_clone = buffer.clone();

    let handle = thread::spawn(move || {
        let mut reader = buf_clone.new_reader();
        let mut data = [0u8; 5];
        reader.seek(0);
        reader.read(&mut data)
    });

    thread::sleep(Duration::from_millis(10));
    buffer.append_at(0, b"hello");

    assert_eq!(handle.join().unwrap(), Some(5));
}

#[test]
fn test_contiguous_bytes_from_position() {
    let buffer = SparseStreamingBuffer::new(24);
    buffer.append_at(0, b"0123456789"); // 0-9
    buffer.append_at(20, b"abcd"); // 20-23

    assert_eq!(buffer.contiguous_from(5), 5); // 5 bytes until end of first range
    assert_eq!(buffer.contiguous_from(20), 4); // 4 bytes in second range
    assert_eq!(buffer.contiguous_from(15), 0); // Not buffered
}

#[test]
fn test_seek_and_read_from_different_ranges() {
    let buffer = create_sparse_buffer(106);
    buffer.append_at(0, b"first");
    buffer.append_at(100, b"second");

    let mut reader = buffer.new_reader();

    // Read from first range
    reader.seek(0);
    let mut buf = [0u8; 5];
    assert_eq!(reader.read(&mut buf), Some(5));
    assert_eq!(&buf, b"first");

    // Seek to second range and read
    reader.seek(100);
    let mut buf = [0u8; 6];
    assert_eq!(reader.read(&mut buf), Some(6));
    assert_eq!(&buf, b"second");
}

#[test]
fn test_cancel_unblocks_reader() {
    // Size exceeds the seek target so the reader blocks (not EOF) until cancel.
    let buffer = create_sparse_buffer(100);
    let buf_clone = buffer.clone();

    let handle = thread::spawn(move || {
        let mut reader = buf_clone.new_reader();
        let mut data = [0u8; 5];
        reader.seek(50); // Position not buffered
        reader.read(&mut data)
    });

    thread::sleep(Duration::from_millis(10));
    buffer.cancel();

    assert_eq!(handle.join().unwrap(), None);
}

#[test]
fn test_eof_with_total_size() {
    let buffer = create_sparse_buffer(8);
    buffer.append_at(0, b"all data");

    let mut reader = buffer.new_reader();

    // Read all data
    reader.seek(0);
    let mut buf = [0u8; 20];
    assert_eq!(reader.read(&mut buf), Some(8));

    // Reaching the total size is end-of-file.
    assert_eq!(reader.read(&mut buf), Some(0));
}

#[test]
fn test_merge_three_ranges() {
    let buffer = SparseStreamingBuffer::new(12);
    buffer.append_at(0, b"aa"); // 0-1
    buffer.append_at(10, b"cc"); // 10-11
    assert_eq!(buffer.ranges_count(), 2);

    // Add range that bridges them
    buffer.append_at(2, b"bbbbbbbb"); // 2-9, should merge all into one

    assert_eq!(buffer.ranges_count(), 1);
    assert!(buffer.is_buffered(0));
    assert!(buffer.is_buffered(5));
    assert!(buffer.is_buffered(11));
}

#[test]
fn test_get_ranges() {
    let buffer = SparseStreamingBuffer::new(104);
    buffer.append_at(0, b"aaaa");
    buffer.append_at(100, b"bbbb");

    let ranges = buffer.get_ranges();
    assert_eq!(ranges, vec![(0, 4), (100, 104)]);
}

#[test]
fn test_total_buffered() {
    let buffer = SparseStreamingBuffer::new(106);
    buffer.append_at(0, b"aaaa"); // 4 bytes
    buffer.append_at(100, b"bbbbbb"); // 6 bytes

    assert_eq!(buffer.total_buffered(), 10);
}

#[test]
fn test_multiple_readers_independent_positions() {
    let buffer = create_sparse_buffer(10);
    buffer.append_at(0, b"abcdefghij");

    let mut reader1 = buffer.new_reader();
    let mut reader2 = buffer.new_reader();

    // Reader 1 reads from start
    let mut buf1 = [0u8; 3];
    assert_eq!(reader1.read(&mut buf1), Some(3));
    assert_eq!(&buf1, b"abc");

    // Reader 2 seeks to middle and reads
    reader2.seek(5);
    let mut buf2 = [0u8; 3];
    assert_eq!(reader2.read(&mut buf2), Some(3));
    assert_eq!(&buf2, b"fgh");

    // Reader 1 continues from where it left off
    assert_eq!(reader1.read(&mut buf1), Some(3));
    assert_eq!(&buf1, b"def");
}

#[test]
fn token_cancel_only_affects_one_reader() {
    let buffer = create_sparse_buffer(100);
    let cancel1 = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel2 = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut reader2 = buffer.new_reader_with_cancel(cancel2);

    let reader_buffer = buffer.clone();
    let reader_cancel = cancel1.clone();
    let handle = thread::spawn(move || {
        let mut reader1 = reader_buffer.new_reader_with_cancel(reader_cancel);
        let mut buf = [0u8; 5];
        reader1.read(&mut buf)
    });

    thread::sleep(Duration::from_millis(10));
    cancel1.store(true, std::sync::atomic::Ordering::Release);
    buffer.wake_readers();

    assert_eq!(join_within(handle, Duration::from_millis(500)), Some(None));

    buffer.append_at(0, b"hello");
    let mut buf = [0u8; 5];
    assert_eq!(reader2.read(&mut buf), Some(5));
    assert_eq!(&buf, b"hello");
}

#[test]
fn test_seek_returns_false_after_cancel() {
    let buffer = create_sparse_buffer(11);
    buffer.append_at(0, b"hello world");
    let mut reader = buffer.new_reader();

    // A normal seek succeeds and moves the cursor.
    assert!(reader.seek(6));

    // Once the buffer is cancelled the reader is shutting down, so seek is
    // refused rather than silently repositioning a dead cursor.
    buffer.cancel();
    assert!(
        !reader.seek(0),
        "seek must return false once the buffer is cancelled"
    );
}

/// Join `handle`, returning its value, or `None` if it hasn't finished within
/// `timeout` (i.e. it's stuck) — lets a test fail instead of hanging.
fn join_within<T: Send + 'static>(
    handle: std::thread::JoinHandle<T>,
    timeout: Duration,
) -> Option<T> {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if handle.is_finished() {
            return handle.join().ok();
        }
        thread::sleep(Duration::from_millis(10));
    }
    None
}

#[test]
fn next_gap_reports_boundaries_within_the_window() {
    let buffer = SparseStreamingBuffer::new(20);
    buffer.append_at(0, b"aaaa"); // [0, 4)
    buffer.append_at(10, b"bbbb"); // [10, 14)

    // Front gap between the cursor and the first range.
    assert_eq!(buffer.next_gap(0, 20), Some((4, 10)));
    // Starting inside the first range skips to the gap after it.
    assert_eq!(buffer.next_gap(2, 20), Some((4, 10)));
    // The window clamps the gap end to `limit`.
    assert_eq!(buffer.next_gap(4, 8), Some((4, 8)));
    // Starting inside the second range, past all data, is the trailing gap.
    assert_eq!(buffer.next_gap(12, 20), Some((14, 20)));
    // A fully buffered window has no gap.
    assert_eq!(buffer.next_gap(0, 4), None);
    assert_eq!(buffer.next_gap(10, 14), None);
    // An empty/inverted window has no gap.
    assert_eq!(buffer.next_gap(5, 5), None);
    assert_eq!(buffer.next_gap(8, 4), None);
}

#[test]
fn next_gap_is_none_when_window_fully_buffered_across_merged_ranges() {
    let buffer = SparseStreamingBuffer::new(12);
    buffer.append_at(0, b"aaaa");
    buffer.append_at(4, b"bbbb"); // merges into [0, 8)
    assert_eq!(buffer.ranges_count(), 1);
    assert_eq!(buffer.next_gap(0, 8), None);
    assert_eq!(buffer.next_gap(2, 6), None);
    assert_eq!(buffer.next_gap(0, 12), Some((8, 12)));
}

#[test]
fn read_publishes_demand_at_the_advanced_position() {
    let buffer = create_sparse_buffer(10);
    buffer.append_at(0, b"0123456789");

    let mut reader = buffer.new_reader();
    // A reader that hasn't read or sought yet has no registered demand, so
    // it doesn't pull a fetch toward byte 0 or pin the eviction floor there.
    assert!(buffer.demands_sorted().is_empty());

    let mut buf = [0u8; 4];
    assert_eq!(reader.read(&mut buf), Some(4));
    // After reading 4 bytes the demand has advanced to where the next read
    // will land, so the fill loop reads ahead of the playhead.
    assert_eq!(buffer.demands_sorted(), vec![4]);
}

#[test]
fn seek_publishes_demand_at_the_target() {
    let buffer = create_sparse_buffer(10);
    buffer.append_at(0, b"0123456789");

    let mut reader = buffer.new_reader();
    assert!(reader.seek(7));
    assert_eq!(
        buffer.demands_sorted(),
        vec![7],
        "seek must publish the target as the reader's demand"
    );
}

#[test]
fn two_readers_keep_independent_demands() {
    // Per-reader demand is the real shape: a CUE+APE album reads the playing
    // track's decoder and the gapless-preload decoder on ONE shared buffer at
    // DIFFERENT offsets. A single scalar (last-writer-wins) could not hold both
    // at once — this test asserts both demands coexist, which is exactly what a
    // scalar cannot express.
    let buffer = create_sparse_buffer(200);
    buffer.append_at(0, &[0u8; 200]);

    let mut playing = buffer.new_reader();
    let mut preload = buffer.new_reader();

    assert!(playing.seek(10));
    assert!(preload.seek(150));

    assert_eq!(
        buffer.demands_sorted(),
        vec![10, 150],
        "both readers' positions must be visible to the fill loop at once"
    );
    assert_eq!(buffer.demand_count(), 2);

    // Dropping the preload reader removes only its demand; the playing
    // track's stays so the fill loop keeps feeding it.
    drop(preload);
    assert_eq!(buffer.demands_sorted(), vec![10]);
    assert_eq!(buffer.demand_count(), 1);

    drop(playing);
    assert!(buffer.demands_sorted().is_empty());
    assert_eq!(buffer.demand_count(), 0);
}

#[test]
fn demand_windows_carry_the_per_reader_ceiling() {
    let buffer = create_sparse_buffer(1000);
    buffer.append_at(0, &[0u8; 500]);

    let mut a = buffer.new_reader();
    let mut b = buffer.new_reader();
    assert!(a.seek(100));
    assert!(b.seek(300));
    a.set_readahead_ceiling(250);
    // `b` has no ceiling set -> None, so the fill falls back to its cushion.

    assert_eq!(
        buffer.demand_windows(),
        vec![
            ReaderDemand {
                pos: 100,
                ceiling: Some(250)
            },
            ReaderDemand {
                pos: 300,
                ceiling: None
            },
        ],
        "each reader carries its own position and ceiling, ascending by position"
    );

    // Dropping a reader removes both its demand and its ceiling.
    drop(a);
    assert_eq!(
        buffer.demand_windows(),
        vec![ReaderDemand {
            pos: 300,
            ceiling: None
        }]
    );
}

#[test]
fn blocked_reader_publishes_demand_at_its_blocked_position() {
    // A reader that seeks past buffered data and blocks must leave its demand
    // at the blocked position so the fill loop knows where to fetch.
    // Size exceeds the seek target so the reader blocks (not EOF) until data lands.
    let buffer = create_sparse_buffer(1000);
    buffer.append_at(0, b"front");

    let buf = buffer.clone();
    let handle = thread::spawn(move || {
        let mut reader = buf.new_reader();
        assert!(reader.seek(500));
        let mut out = [0u8; 4];
        reader.read(&mut out)
    });

    // Let the reader reach its blocked wait, then inspect its published demand.
    thread::sleep(Duration::from_millis(50));
    assert_eq!(
        buffer.demands_sorted(),
        vec![500],
        "a blocked reader's demand must point at the position it's waiting for"
    );

    // Satisfy the demand and let the reader finish.
    buffer.append_at(500, b"late");
    assert_eq!(join_within(handle, Duration::from_secs(5)), Some(Some(4)));
}

#[test]
fn evict_before_drops_passed_ranges_and_trims_a_straddling_one() {
    let buffer = create_sparse_buffer(24);
    buffer.append_at(0, b"0123456789"); // [0, 10)
    buffer.append_at(20, b"abcd"); // [20, 24)

    // Evicting mid-range trims its front; the later range is untouched.
    buffer.evict_before(5);
    assert_eq!(buffer.get_ranges(), vec![(5, 10), (20, 24)]);

    // The retained bytes are still correct after the front trim.
    let mut reader = buffer.new_reader();
    assert!(reader.seek(5));
    let mut buf = [0u8; 5];
    assert_eq!(reader.read(&mut buf), Some(5));
    assert_eq!(&buf, b"56789");
    drop(reader);

    // Evicting past a whole range drops it; a range starting exactly at the
    // floor is kept.
    buffer.evict_before(20);
    assert_eq!(buffer.get_ranges(), vec![(20, 24)]);
    buffer.evict_before(20);
    assert_eq!(buffer.get_ranges(), vec![(20, 24)]);
}

/// Appending a range that sits entirely before an existing one (a backward
/// seek fetched an earlier window) inserts it ahead in the sorted range list
/// rather than at the end. Both ranges stay independently readable.
#[test]
fn append_before_an_existing_range_inserts_in_sorted_order() {
    let buffer = create_sparse_buffer(104);
    buffer.append_at(100, b"bbbb"); // [100, 104)
    buffer.append_at(0, b"aaaa"); // [0, 4) — disjoint, belongs before the first

    assert_eq!(buffer.get_ranges(), vec![(0, 4), (100, 104)]);
    assert_eq!(buffer.ranges_count(), 2);

    let mut reader = buffer.new_reader();
    let mut buf = [0u8; 4];
    reader.seek(0);
    assert_eq!(reader.read(&mut buf), Some(4));
    assert_eq!(&buf, b"aaaa");
    reader.seek(100);
    assert_eq!(reader.read(&mut buf), Some(4));
    assert_eq!(&buf, b"bbbb");
}

/// An empty append is a no-op: no range is created and no bytes are counted
/// as fetched, so it neither perturbs the range list nor the fetch tally.
#[test]
fn empty_append_is_a_noop() {
    let buffer = create_sparse_buffer(10);
    buffer.append_at(0, b"");
    assert_eq!(buffer.ranges_count(), 0);
    assert_eq!(buffer.bytes_fetched(), 0);

    // A subsequent real append still lands and is counted.
    buffer.append_at(0, b"data");
    assert_eq!(buffer.get_ranges(), vec![(0, 4)]);
    assert_eq!(buffer.bytes_fetched(), 4);
}

use super::*;

#[test]
fn pregap_seek_position_cases() {
    use std::time::Duration;
    // (pregap_ms, is_natural_transition) -> seek position.
    // A natural transition always plays from the start (None, so the pregap
    // is heard); a direct selection skips a positive pregap and otherwise
    // needs no seek.
    let cases = [
        (Some(3000i64), false, Some(Duration::from_millis(3000))),
        (Some(3000i64), true, None),
        (None, false, None),
        (None, true, None),
    ];
    for (pregap_ms, is_natural_transition, expected) in cases {
        assert_eq!(
            pregap_seek_position(pregap_ms, is_natural_transition),
            expected,
            "pregap_ms={pregap_ms:?} natural={is_natural_transition}"
        );
    }
}

// Seek tests for SparseStreamingBuffer integration
use crate::playback::sparse_buffer::SparseStreamingBuffer;

#[test]
fn test_seek_within_buffer() {
    let buffer = SparseStreamingBuffer::new(10000);
    // Buffer has first 10000 bytes
    buffer.append_at(0, &vec![0u8; 10000]);

    // Seek to byte 5000 - should be buffered
    assert!(
        buffer.is_buffered(5000),
        "Position 5000 should be within buffered range"
    );
}

#[test]
fn test_seek_past_buffer() {
    let buffer = SparseStreamingBuffer::new(60000);
    // Buffer has first 10000 bytes
    buffer.append_at(0, &vec![0u8; 10000]);

    // Seek to byte 50000 - should NOT be buffered
    assert!(
        !buffer.is_buffered(50000),
        "Position 50000 should be past buffered range"
    );
}

#[test]
fn test_seek_multiple_ranges() {
    let buffer = SparseStreamingBuffer::new(60000);
    // Buffer has 0-10000 and 50000-60000
    buffer.append_at(0, &vec![0u8; 10000]);
    buffer.append_at(50000, &vec![0u8; 10000]);

    // Currently at 55000, seek back to 5000 should reuse first range
    assert!(buffer.is_buffered(5000), "Position 5000 should be buffered");
    assert!(
        buffer.is_buffered(55000),
        "Position 55000 should be buffered"
    );
    assert!(
        !buffer.is_buffered(30000),
        "Position 30000 should NOT be buffered (gap)"
    );
}

#[test]
fn test_seek_back_after_forward_seek() {
    use crate::playback::sparse_buffer::create_sparse_buffer;

    let buffer = create_sparse_buffer(90000);

    // Initial download: 0-30000
    buffer.append_at(0, &vec![0u8; 30000]);

    // User seeks forward to byte 70000 - new download starts there
    // Simulating: 70000-90000
    buffer.append_at(70000, &vec![0u8; 20000]);

    // Now we have two ranges: 0-30000 and 70000-90000
    assert_eq!(
        buffer.get_ranges(),
        vec![(0, 30000), (70000, 90000)],
        "Should have two non-contiguous ranges"
    );

    // User seeks back to byte 15000 - should be buffered (first range)
    assert!(buffer.is_buffered(15000), "15000 should be in first range");

    // User seeks to byte 75000 - should be buffered (second range)
    assert!(buffer.is_buffered(75000), "75000 should be in second range");

    // User seeks to byte 50000 - gap between ranges, not buffered
    assert!(!buffer.is_buffered(50000), "50000 should be in the gap");
}

#[test]
fn test_ranges_merge_when_gap_filled() {
    use crate::playback::sparse_buffer::create_sparse_buffer;

    let buffer = create_sparse_buffer(30000);

    // Initial download: 0-10000
    buffer.append_at(0, &vec![0u8; 10000]);

    // Seek forward creates second range: 20000-30000
    buffer.append_at(20000, &vec![0u8; 10000]);

    assert_eq!(buffer.get_ranges().len(), 2, "Should have two ranges");

    // Original download continues and fills gap: 10000-20000
    buffer.append_at(10000, &vec![0u8; 10000]);

    // Ranges should now be merged
    assert_eq!(buffer.get_ranges().len(), 1, "Ranges should be merged");
    assert_eq!(
        buffer.get_ranges(),
        vec![(0, 30000)],
        "Should be single contiguous range"
    );
}

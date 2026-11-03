use super::*;

fn span(start_byte: Option<u64>, end_byte: Option<u64>) -> SegmentSpan {
    SegmentSpan { start_sample: 0, end_sample: None, start_byte, end_byte }
}

/// A file's held byte ranges, each from its first byte to one past its last.
fn held(ranges: &[(u64, u64)]) -> Vec<Range<u64>> {
    ranges.iter().map(|&(start, end)| start..end).collect()
}

fn stretch(start: f64, end: f64) -> TrackSpan {
    TrackSpan { start, end }
}

fn point(sample: u64, byte: u64) -> ScrubPoint {
    ScrubPoint { sample, byte, exact_from: sample }
}

/// A track that is its whole file holds as much of its length as its file's
/// buffer holds of its bytes, and all of it once every byte is here.
#[test]
fn a_whole_files_track_holds_its_files_held_bytes() {
    let bytes = TrackBytes::new(span(None, None), 1_000, &[], 44_100);

    assert_eq!(bytes.spans(&[]), vec![]);
    assert_eq!(bytes.spans(&held(&[(0, 250)])), vec![stretch(0.0, 0.25)]);
    assert_eq!(
        bytes.spans(&held(&[(0, 250), (500, 750)])),
        vec![stretch(0.0, 0.25), stretch(0.5, 0.75)]
    );
    assert_eq!(bytes.spans(&held(&[(0, 1_000)])), vec![stretch(0.0, 1.0)]);
}

/// A track inside a larger file holds only its own bytes: the file's other
/// tracks' bytes, and the file's start the buffer keeps, are no part of it.
#[test]
fn a_track_in_a_larger_file_holds_only_its_own_bytes() {
    let bytes = TrackBytes::new(
        SegmentSpan {
            start_sample: 441_000,
            end_sample: Some(882_000),
            start_byte: Some(4_000),
            end_byte: Some(8_000),
        },
        20_000,
        &[],
        441_000,
    );

    assert_eq!(bytes.spans(&held(&[(0, 1_000), (9_000, 12_000)])), vec![]);
    assert_eq!(bytes.spans(&held(&[(0, 5_000)])), vec![stretch(0.0, 0.25)]);
    assert_eq!(bytes.spans(&held(&[(0, 20_000)])), vec![stretch(0.0, 1.0)]);
}

/// Scrub points place the bytes between them: a stretch of the file packed
/// with more sound per byte plays longer than its share of the bytes.
#[test]
fn scrub_points_place_the_bytes_between_them() {
    // Half the samples sit in the first fifth of the bytes.
    let bytes = TrackBytes::new(span(None, Some(1_000)), 1_000, &[point(500, 200)], 1_000);

    assert_eq!(bytes.spans(&held(&[(0, 200)])), vec![stretch(0.0, 0.5)]);
    assert_eq!(bytes.spans(&held(&[(100, 200)])), vec![stretch(0.25, 0.5)]);
    assert_eq!(bytes.spans(&held(&[(600, 1_000)])), vec![stretch(0.75, 1.0)]);
}

/// Bytes before an MP3's first frame, its tag, carry no sound: they hold
/// none of the track, and the first frame's bytes play from its start.
#[test]
fn bytes_before_the_first_frame_hold_none_of_the_track() {
    let bytes = TrackBytes::new(span(None, None), 1_100, &[point(0, 100)], 1_000);

    assert_eq!(bytes.spans(&held(&[(0, 100)])), vec![]);
    assert_eq!(bytes.spans(&held(&[(0, 600)])), vec![stretch(0.0, 0.5)]);
}

/// Held ranges that meet once placed in the track, across bytes that carry
/// no sound, are one stretch.
#[test]
fn stretches_that_meet_are_one() {
    // Bytes 400 to 600 carry no sound: both points play from the middle.
    let bytes =
        TrackBytes::new(span(None, None), 1_000, &[point(500, 400), point(500, 600)], 1_000);

    assert_eq!(bytes.spans(&held(&[(0, 450), (550, 1_000)])), vec![stretch(0.0, 1.0)]);
}

/// The playing track's held stretches are told as its buffer's ranges
/// change, under its id, and as another track plays; nothing while none
/// does.
#[tokio::test]
async fn the_playing_tracks_stretches_follow_its_buffer() {
    let (playing_tx, playing_rx) = watch::channel(None);
    let mut follow = TrackBufferedFollow::new(playing_rx);
    assert_eq!(follow.next().await, Some(None));

    let buffer = watch::Sender::new(held(&[(0, 250)]));
    playing_tx.send_replace(Some(PlayingBytes {
        track_id: "track-1".into(),
        bytes: TrackBytes::new(span(None, None), 1_000, &[], 1_000),
        held: buffer.subscribe(),
    }));
    assert_eq!(
        follow.next().await,
        Some(Some(TrackBuffered { track_id: "track-1".into(), ranges: vec![stretch(0.0, 0.25)] }))
    );

    buffer.send_replace(held(&[(0, 1_000)]));
    assert_eq!(
        follow.next().await,
        Some(Some(TrackBuffered { track_id: "track-1".into(), ranges: vec![stretch(0.0, 1.0)] }))
    );

    drop(buffer);
    assert_eq!(
        follow.next().await,
        Some(Some(TrackBuffered { track_id: "track-1".into(), ranges: vec![] })),
        "a buffer gone holds nothing"
    );

    playing_tx.send_replace(None);
    assert_eq!(follow.next().await, Some(None));
    drop(playing_tx);
    assert_eq!(follow.next().await, None);
}

use super::*;

#[test]
fn a_frames_start_gives_that_frame_back() {
    for rate in [44_100, 48_000, 88_200, 96_000, 192_000] {
        for frame in [0, 1, 88_199, 88_201, 44_100 * 60 * 74 + 17] {
            assert_eq!(frame_at(frame_start(frame, rate), rate), frame, "frame {frame} at {rate}");
        }
    }
}

#[test]
fn a_frame_on_a_whole_nanosecond_starts_there() {
    assert_eq!(frame_start(88_200, 44_100), Duration::from_secs(2));
    assert_eq!(frame_start(24, 48_000), Duration::from_micros(500));
}

#[test]
fn a_time_inside_a_frame_falls_on_it() {
    assert_eq!(frame_at(Duration::from_nanos(22_675), 44_100), 0);
    assert_eq!(frame_at(Duration::from_nanos(22_676), 44_100), 1);
    assert_eq!(frame_at(Duration::from_secs(2), 44_100), 88_200);
}

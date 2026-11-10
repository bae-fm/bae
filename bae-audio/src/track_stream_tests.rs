#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;

#[test]
fn test_push_pull_samples() {
    let (mut sink, mut source, _ready) = create_track_stream_pair(44100, 2);

    let samples = vec![0.1, 0.2, 0.3, 0.4];
    let pushed = sink.push_samples(&samples);
    assert_eq!(pushed, 4);

    let mut output = vec![0.0; 4];
    let pulled = source.pull_samples(&mut output);
    assert_eq!(pulled, 4);
    assert_eq!(output, samples);
}

#[test]
fn test_finished_flag() {
    let (mut sink, source, _ready) = create_track_stream_pair(44100, 2);

    assert!(!source.is_finished());
    assert!(!source.producer_finished());

    sink.mark_finished();

    assert!(source.producer_finished());
    assert!(source.is_finished()); // Empty buffer + finished = done
}

#[test]
fn test_position_tracking() {
    let (mut sink, mut source, _ready) = create_track_stream_pair_with_capacity(44100, 2, 10000);

    // 1000 stereo samples = 500 frames.
    let samples: Vec<f32> = (0..1000).map(|i| i as f32 * 0.001).collect();
    sink.push_samples(&samples);

    let mut output = vec![0.0; 1000];
    source.pull_samples(&mut output);

    // Position counts frames, not samples.
    assert_eq!(source.state.position_samples(), 500);

    // 500 / 44100 ≈ 11.3ms
    let pos = source.position();
    assert!(pos.as_millis() >= 11 && pos.as_millis() <= 12);
}

#[test]
fn test_cancel() {
    let (sink, source, _ready) = create_track_stream_pair(44100, 2);

    assert!(!sink.is_cancelled());
    assert!(!source.is_cancelled());

    source.cancel();

    assert!(sink.is_cancelled());
    assert!(source.is_cancelled());
}

#[test]
fn test_buffer_full() {
    let (mut sink, _source, _ready) = create_track_stream_pair_with_capacity(44100, 2, 10);

    // Pushing past capacity pushes only what fits.
    let samples = vec![0.5; 20];
    let pushed = sink.push_samples(&samples);

    assert!(pushed <= 10);
}

#[test]
fn test_buffer_empty() {
    let (_sink, mut source, _ready) = create_track_stream_pair(44100, 2);

    let mut output = vec![0.0; 10];
    let pulled = source.pull_samples(&mut output);

    assert_eq!(pulled, 0);
}

/// Ready fires when the ring is half full, not before: 49 samples of a
/// 100-sample ring leave it unfired, the 50th fires it.
#[test]
fn test_ready_signal_fires_at_threshold() {
    let (mut sink, _source, mut ready_rx) = create_track_stream_pair_with_capacity(44100, 2, 100);

    let samples: Vec<f32> = (0..50).map(|i| i as f32 * 0.01).collect();
    sink.push_samples_blocking(&samples[..49]);
    assert!(
        matches!(ready_rx.try_recv(), Err(oneshot::error::TryRecvError::Empty)),
        "Ready signal should not fire below 50%"
    );
    sink.push_samples_blocking(&samples[49..]);
    assert!(ready_rx.try_recv().is_ok(), "Ready signal should fire at 50%");
}

/// A track shorter than the threshold is ready once it finishes.
#[test]
fn test_ready_signal_on_finish() {
    // Capacity 1000, so the 50% threshold is 500.
    let (mut sink, _source, mut ready_rx) = create_track_stream_pair_with_capacity(44100, 2, 1000);

    let samples: Vec<f32> = (0..100).map(|i| i as f32 * 0.001).collect();
    sink.push_samples_blocking(&samples);
    assert!(
        matches!(ready_rx.try_recv(), Err(oneshot::error::TryRecvError::Empty)),
        "100 samples never reach the threshold"
    );
    sink.mark_finished();
    assert!(ready_rx.try_recv().is_ok(), "Ready signal should fire on finish");
}

/// The production producer pushes more samples than the ring holds, so it
/// parks when the ring fills and resumes as a draining consumer frees space.
/// Every sample is delivered in order across the park boundary.
#[test]
fn push_samples_blocking_parks_until_consumer_drains() {
    use std::thread;

    // Ring capacity (8) is far below the total pushed (40), so the producer
    // must park on full at least once.
    let (mut sink, mut source, _ready) = create_track_stream_pair_with_capacity(44100, 1, 8);
    let total = 40usize;
    let samples: Vec<f32> = (0..total).map(|i| i as f32).collect();
    let expected = samples.clone();

    let consumer = thread::spawn(move || {
        let mut got: Vec<f32> = Vec::new();
        while got.len() < total {
            let mut buf = [0.0f32; 4];
            let seen = source.pushes();
            let n = source.pull_samples(&mut buf);
            if n == 0 {
                source.wait_for_push_after(seen);
                continue;
            }
            got.extend_from_slice(&buf[..n]);
        }
        got
    });

    let pushed = sink.push_samples_blocking(&samples);
    assert_eq!(pushed, total, "every sample is eventually pushed");
    assert_eq!(
        consumer.join().unwrap(),
        expected,
        "samples arrive in order across the park boundary"
    );
}

/// A blocked producer (ring full, no consumer draining) returns from
/// `push_samples_blocking` when the stream is cancelled mid-push, having
/// pushed fewer than all its samples.
#[test]
fn push_samples_blocking_returns_on_cancel_mid_push() {
    use std::thread;

    let (mut sink, source, _ready) = create_track_stream_pair_with_capacity(44100, 1, 8);
    let samples = vec![1.0f32; 100];

    // Nobody drains, so the producer fills the ring and parks; cancel wakes it.
    let canceller = thread::spawn(move || {
        source.wait_until_producer_parked();
        source.cancel();
    });

    let pushed = sink.push_samples_blocking(&samples);
    canceller.join().unwrap();

    assert!(sink.is_cancelled(), "the stream is cancelled");
    assert!(pushed < samples.len(), "cancel interrupts the blocked push before all samples land");
}

/// Silence frames are pushed as zeroed samples, `channels` per frame.
#[test]
fn push_silence_frames_blocking_pushes_zeroed_frames() {
    let (mut sink, mut source, _ready) = create_track_stream_pair_with_capacity(44100, 2, 10000);

    let pushed = sink.push_silence_frames_blocking(100);
    assert_eq!(pushed, 100, "100 frames pushed");

    let mut out = vec![0.5f32; 200]; // 100 frames * 2 channels
    let n = source.pull_samples(&mut out);
    assert_eq!(n, 200);
    assert!(out[..200].iter().all(|&s| s == 0.0), "silence frames are zeroed");
}

/// A stream with zero channels can't form frames, so silence-frame pushing is
/// a no-op rather than dividing by zero.
#[test]
fn push_silence_frames_blocking_zero_channels_is_noop() {
    let (mut sink, _source, _ready) = create_track_stream_pair_with_capacity(44100, 0, 100);
    assert_eq!(sink.push_silence_frames_blocking(50), 0);
}

/// A producer parked on a full ring returns once the consumer is dropped:
/// nothing can drain the ring any more, and the drop is what wakes it.
#[test]
fn push_samples_blocking_returns_when_the_stream_is_dropped() {
    use std::sync::mpsc;
    use std::thread;

    let (mut sink, source, _ready) = create_track_stream_pair_with_capacity(44100, 1, 8);
    let (pushed_tx, pushed_rx) = mpsc::channel();
    let producer = thread::spawn(move || {
        pushed_tx.send(sink.push_samples_blocking(&[1.0f32; 100])).unwrap();
    });

    source.wait_until_producer_parked();
    drop(source);

    let pushed = pushed_rx
        .recv_timeout(bae_util::test_channels::HANG_GUARD)
        .expect("the parked producer never woke after the stream was dropped");
    producer.join().unwrap();
    assert!(pushed < 100, "the push stops with samples left over");
}

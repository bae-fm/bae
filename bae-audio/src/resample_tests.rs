use super::*;

/// Resampling 48 kHz → 44.1 kHz preserves duration: a second of 48 kHz stereo
/// comes out as ~44 100 frames (within the converter's small latency).
#[test]
fn downsamples_48k_to_44k_preserving_duration() {
    let mut r = Resampler::new(48_000, 2, 44_100, 2).unwrap();
    // One second of stereo silence: 48 000 frames = 96 000 interleaved samples.
    let input = vec![0.0f32; 48_000 * 2];
    let mut out = Vec::new();
    r.convert(&input, &mut out).unwrap();
    r.flush(&mut out).unwrap();
    let out_frames = out.len() / 2;
    // Allow a few frames of resampler latency either way.
    assert!((out_frames as i64 - 44_100).abs() < 100, "expected ~44100 frames, got {out_frames}");
}

/// Mono upmixes to stereo: same rate preserves the frame count, and each
/// frame's two channels carry the (matrixed) mono sample equally.
#[test]
fn upmixes_mono_to_stereo() {
    let mut r = Resampler::new(44_100, 1, 44_100, 2).unwrap();
    let input = vec![0.5f32; 100];
    let mut out = Vec::new();
    r.convert(&input, &mut out).unwrap();
    r.flush(&mut out).unwrap();
    assert_eq!(out.len() / 2, 100, "frame count preserved at the same rate");
    // The upmix may apply a downmix gain, but the two channels are equal and
    // non-zero.
    for frame in out.chunks_exact(2) {
        assert!((frame[0] - frame[1]).abs() < 1e-4, "channels are equal");
        assert!(frame[0] > 0.0, "the mono signal reaches both channels");
    }
}

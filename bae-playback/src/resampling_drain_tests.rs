use super::*;
use crate::audio_output::audio_event_channel;
use crate::source::{PlaybackSource, TrackFmt};
use bae_audio::track_stream::create_track_stream_pair_with_capacity;
use std::sync::{Arc, Mutex};

/// A playing drain over a finished source holding `samples` at `rate`
/// stereo.
fn drain_over(rate: u32, samples: &[f32]) -> (AudioDrain, AudioOutputControls) {
    let (mut sink, stream, _ready) = create_track_stream_pair_with_capacity(rate, 2, samples.len());
    sink.push_samples(samples);
    sink.mark_finished();
    let source = Arc::new(Mutex::new(PlaybackSource::new(stream, TrackFmt::for_test("t"))));
    let controls = AudioOutputControls::new(crate::Volume::FULL);
    controls.set_state(AudioState::Playing);
    let (tx, _rx) = audio_event_channel();
    (AudioDrain::new(controls.clone(), source, tx, 100), controls)
}

/// Read the whole source in device buffers of `buffer_frames`.
fn play_out(drain: &mut ResamplingDrain, buffer_frames: usize) -> Vec<f32> {
    let mut heard = Vec::new();
    let mut buffer = vec![0.0; buffer_frames * 2];
    for _ in 0..10_000 {
        let written = drain.read(&mut buffer);
        if written == 0 {
            return heard;
        }
        heard.extend_from_slice(&buffer[..written]);
    }
    panic!("the source didn't drain within 10 000 buffers");
}

/// At the device's own rate the samples pass through untouched, whatever
/// the device's buffer size.
#[test]
fn a_source_at_the_output_rate_passes_through() {
    let samples: Vec<f32> = (0..4_000).map(|i| i as f32 / 4_000.0).collect();
    let (drain, controls) = drain_over(44_100, &samples);
    let mut drain =
        ResamplingDrain::new(drain, controls, (44_100, 2), (44_100, 2), None, "the test").unwrap();

    assert_eq!(play_out(&mut drain, 333), samples);
}

/// A 192 kHz track on a device at 96 kHz comes out at 96 kHz: half the
/// frames, and a tone keeps its pitch, so it neither plays fast nor
/// loses its end. Sabotage: pass the samples through, and twice the frames
/// come out.
#[test]
fn a_source_at_another_rate_is_converted_to_the_output_rate() {
    let frames = 19_200;
    let tone = |rate: f32, frame: usize| {
        (2.0 * std::f32::consts::PI * 1_000.0 * frame as f32 / rate).sin() * 0.5
    };
    let samples: Vec<f32> = (0..frames)
        .flat_map(|frame| {
            let sample = tone(192_000.0, frame);
            [sample, sample]
        })
        .collect();
    let (drain, controls) = drain_over(192_000, &samples);
    let mut drain =
        ResamplingDrain::new(drain, controls, (192_000, 2), (96_000, 2), None, "the test").unwrap();

    let heard = play_out(&mut drain, 512);

    let heard_frames = heard.len() / 2;
    assert!(
        (heard_frames as i64 - 9_600).abs() < 64,
        "19 200 frames at 192 kHz are about 9 600 at 96 kHz, not {heard_frames}"
    );
    // Past the converter's start-up, the output is the same 1 kHz tone
    // sampled at 96 kHz.
    for frame in 1_000..8_000 {
        let expected = tone(96_000.0, frame);
        let actual = heard[frame * 2];
        assert!((actual - expected).abs() < 0.05, "frame {frame}: {actual} vs {expected}");
    }
}

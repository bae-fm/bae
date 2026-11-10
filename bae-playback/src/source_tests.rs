use super::*;
use crate::audio_output::audio_event_channel;
use bae_audio::track_stream::create_track_stream_pair;

fn fmt_gain(track_id: &str, replay_gain_linear: f32) -> TrackFmt {
    TrackFmt { replay_gain_linear, ..TrackFmt::for_test(track_id) }
}

/// A staged next track plays immediately after the current one: a single
/// source advances across the boundary in place (no rebuild), emits the
/// finishing track's identity + decode stats, and reports finished only
/// once both tracks are drained.
#[test]
fn advances_into_staged_next_track() {
    let (mut sink1, src1, _r1) = create_track_stream_pair(44100, 1);
    let (mut sink2, src2, _r2) = create_track_stream_pair(44100, 1);

    // Track 1: three samples + decode stats, then EOF.
    assert_eq!(sink1.push_samples(&[1.0, 2.0, 3.0]), 3);
    sink1.set_decode_error_count(7);
    sink1.mark_finished();
    // Track 2: two samples, then EOF.
    assert_eq!(sink2.push_samples(&[10.0, 11.0]), 2);
    sink2.mark_finished();

    let (mut audio_tx, mut audio_rx) =
        audio_event_channel(&bae_diagnostics::Diagnostics::noop()).unwrap();
    let mut source =
        PlaybackSource::new(src1, TrackFmt::for_test("08c7ff07-b56a-4e16-8df6-ae2967fa0806"));
    source.stage_next(src2, TrackFmt::for_test("08c7fe07-b56a-4c63-8df6-ad2967fa0653"));
    assert!(source.has_next());
    assert!(!source.is_finished());
    assert_eq!(source.position_event().0.track_id, "08c7ff07-b56a-4e16-8df6-ae2967fa0806");

    // One pull spans the boundary: track 1 then track 2, contiguous.
    let mut out = [0.0f32; 8];
    let n = source.pull_samples(&mut out, &mut audio_tx);
    assert_eq!(n, 5, "should yield track 1 (3) + track 2 (2)");
    assert_eq!(&out[..5], &[1.0, 2.0, 3.0, 10.0, 11.0]);

    // The boundary fired exactly once, carrying t1's finishing stats and
    // both tracks' identities.
    let crossing = match audio_rx.pop().unwrap() {
        AudioEvent::TrackCrossing(crossing) => crossing,
        event => panic!("expected crossing event, got {event:?}"),
    };
    assert_eq!(crossing.finished_fmt.track_id, "08c7ff07-b56a-4e16-8df6-ae2967fa0806");
    assert_eq!(crossing.incoming_fmt.track_id, "08c7fe07-b56a-4c63-8df6-ad2967fa0653");
    assert_eq!(crossing.decode_error_count, 7);
    assert_eq!(crossing.samples_decoded, 3);
    assert!(audio_rx.pop().is_none());

    // After the crossing the fmt advanced to the new track.
    assert_eq!(source.position_event().0.track_id, "08c7fe07-b56a-4c63-8df6-ad2967fa0653");

    // Successor consumed; with both tracks drained the source is finished.
    assert!(!source.has_next());
    assert!(source.is_finished());
}

/// With nothing staged, the source finishes at the current track's end and
/// never advances or signals a boundary.
#[test]
fn finishes_without_a_staged_next() {
    let (mut sink, src, _r) = create_track_stream_pair(44100, 1);
    assert_eq!(sink.push_samples(&[1.0, 2.0]), 2);
    sink.mark_finished();

    let (mut audio_tx, mut audio_rx) =
        audio_event_channel(&bae_diagnostics::Diagnostics::noop()).unwrap();
    let mut source =
        PlaybackSource::new(src, TrackFmt::for_test("08c7ff07-b56a-4e16-8df6-ae2967fa0806"));

    let mut out = [0.0f32; 8];
    assert_eq!(source.pull_samples(&mut out, &mut audio_tx), 2);
    assert_eq!(&out[..2], &[1.0, 2.0]);
    assert!(source.is_finished());
    assert!(audio_rx.pop().is_none(), "no boundary without a staged next");
}

/// Cancelling the staged successor invalidates the chain on
/// user actions); the source then finishes at the current track's end with
/// no boundary.
#[test]
fn cancel_staged_next_unstages_the_successor() {
    let (mut sink1, src1, _r1) = create_track_stream_pair(44100, 1);
    let (mut sink2, src2, _r2) = create_track_stream_pair(44100, 1);
    assert_eq!(sink1.push_samples(&[1.0]), 1);
    sink1.mark_finished();
    assert_eq!(sink2.push_samples(&[10.0]), 1);
    sink2.mark_finished();

    let (mut audio_tx, mut audio_rx) =
        audio_event_channel(&bae_diagnostics::Diagnostics::noop()).unwrap();
    let mut source =
        PlaybackSource::new(src1, TrackFmt::for_test("08c7ff07-b56a-4e16-8df6-ae2967fa0806"));
    source.stage_next(src2, TrackFmt::for_test("08c7fe07-b56a-4c63-8df6-ad2967fa0653"));
    source.cancel_staged_next();
    assert!(sink2.is_cancelled());
    assert!(!source.has_next());

    let mut out = [0.0f32; 8];
    assert_eq!(source.pull_samples(&mut out, &mut audio_tx), 1);
    assert!(source.is_finished());
    assert!(audio_rx.pop().is_none());
}

#[test]
fn promote_staged_next_swaps_without_returning_the_stream() {
    let (current_sink, current, _current_ready) = create_track_stream_pair(44100, 1);
    let (mut next_sink, next, _next_ready) = create_track_stream_pair(44100, 1);
    assert_eq!(next_sink.push_samples(&[4.0, 5.0]), 2);
    next_sink.mark_finished();

    let (mut audio_tx, _audio_rx) =
        audio_event_channel(&bae_diagnostics::Diagnostics::noop()).unwrap();
    let mut source = PlaybackSource::new(current, TrackFmt::for_test("current"));
    source.stage_next(next, TrackFmt::for_test("next"));

    assert!(source.promote_staged_next());
    assert!(current_sink.is_cancelled());
    assert!(!next_sink.is_cancelled());
    assert!(!source.has_next());
    assert_eq!(source.position_event().0.track_id, "next");

    let mut out = [0.0; 4];
    assert_eq!(source.pull_samples(&mut out, &mut audio_tx), 2);
    assert_eq!(&out[..2], &[4.0, 5.0]);
    assert!(!source.promote_staged_next());
}

/// Replay gain is per-track, so crossing into the staged next track swaps in
/// its gain — the loudness normalization the audio callback folds into its
/// volume multiply changes discontinuously at the boundary, which is the
/// whole reason the field lives on the fmt rather than the source.
#[test]
fn replay_gain_flips_at_the_gapless_boundary() {
    let (mut sink1, src1, _r1) = create_track_stream_pair(44100, 1);
    let (mut sink2, src2, _r2) = create_track_stream_pair(44100, 1);
    assert_eq!(sink1.push_samples(&[1.0, 2.0]), 2);
    sink1.mark_finished();
    assert_eq!(sink2.push_samples(&[3.0]), 1);
    sink2.mark_finished();

    let (mut audio_tx, _audio_rx) =
        audio_event_channel(&bae_diagnostics::Diagnostics::noop()).unwrap();
    let mut source =
        PlaybackSource::new(src1, fmt_gain("08c7ff07-b56a-4e16-8df6-ae2967fa0806", 0.5));
    source.stage_next(src2, fmt_gain("08c7fe07-b56a-4c63-8df6-ad2967fa0653", 2.0));

    // Before the crossing the callback reads the current track's gain.
    assert_eq!(source.current_replay_gain_linear(), 0.5);

    // One pull spans the boundary into t2.
    let mut out = [0.0f32; 8];
    assert_eq!(source.pull_samples(&mut out, &mut audio_tx), 3);

    // After the crossing the incoming track's gain takes effect.
    assert_eq!(source.current_replay_gain_linear(), 2.0);
}

/// `cancel` stops both the current decoder and the staged-next decoder, so a
/// user action that invalidates the chain doesn't leave the preloaded track's
/// decoder running. Observed through the retained sinks, which share the
/// cancel flag with their streams.
#[test]
fn cancel_cancels_current_and_staged_next() {
    let (sink1, src1, _r1) = create_track_stream_pair(44100, 1);
    let (sink2, src2, _r2) = create_track_stream_pair(44100, 1);

    let mut source =
        PlaybackSource::new(src1, TrackFmt::for_test("08c7ff07-b56a-4e16-8df6-ae2967fa0806"));
    source.stage_next(src2, TrackFmt::for_test("08c7fe07-b56a-4c63-8df6-ad2967fa0653"));

    assert!(!sink1.is_cancelled());
    assert!(!sink2.is_cancelled());

    source.cancel();

    assert!(sink1.is_cancelled(), "the current track is cancelled");
    assert!(sink2.is_cancelled(), "the staged next track is cancelled");
}

/// `replace` swaps in a new current track without rebuilding: the audio
/// callback keeps pulling from the same source and now sees the new track's
/// samples and fmt. The outgoing track and any staged next are cancelled;
/// the replacement stays live.
#[test]
fn replace_swaps_current_and_cancels_outgoing_and_staged() {
    let (sink1, src1, _r1) = create_track_stream_pair(44100, 1);
    let (staged_sink, staged_src, _rs) = create_track_stream_pair(44100, 1);
    let (mut sink2, src2, _r2) = create_track_stream_pair(44100, 1);

    // The replacement has its own samples, then EOF.
    assert_eq!(sink2.push_samples(&[9.0, 8.0]), 2);
    sink2.mark_finished();

    let (mut audio_tx, _audio_rx) =
        audio_event_channel(&bae_diagnostics::Diagnostics::noop()).unwrap();
    let mut source =
        PlaybackSource::new(src1, TrackFmt::for_test("08c7ff07-b56a-4e16-8df6-ae2967fa0806"));
    source.stage_next(staged_src, TrackFmt::for_test("staged"));

    source.replace(src2, TrackFmt::for_test("08c7fe07-b56a-4c63-8df6-ad2967fa0653"));

    // The outgoing and staged decoders are cancelled; the replacement is not.
    assert!(sink1.is_cancelled(), "the outgoing track is cancelled");
    assert!(staged_sink.is_cancelled(), "the staged next is cancelled");
    assert!(!sink2.is_cancelled(), "the replacement track stays live");
    assert!(!source.has_next(), "the staged next is cleared");

    // The callback now reads the replacement track's samples and fmt.
    assert_eq!(source.position_event().0.track_id, "08c7fe07-b56a-4c63-8df6-ad2967fa0653");
    let mut out = [0.0f32; 8];
    assert_eq!(source.pull_samples(&mut out, &mut audio_tx), 2);
    assert_eq!(&out[..2], &[9.0, 8.0]);
}

/// The completion latch fires once per drained track and resets on
/// `replace`, so a single source playing several tracks in turn (a
/// persistent output stream) reports each track's end exactly once — the
/// property the drain relies on to auto-advance every track, not just the
/// first.
#[test]
fn take_completion_event_fires_once_per_track_across_replace() {
    let (mut sink1, src1, _r1) = create_track_stream_pair(44100, 1);
    assert_eq!(sink1.push_samples(&[1.0]), 1);
    sink1.mark_finished();

    let (mut audio_tx, _audio_rx) =
        audio_event_channel(&bae_diagnostics::Diagnostics::noop()).unwrap();
    let mut source =
        PlaybackSource::new(src1, TrackFmt::for_test("08c7ff07-b56a-4e16-8df6-ae2967fa0806"));

    // Drain track 1.
    let mut out = [0.0f32; 8];
    assert_eq!(source.pull_samples(&mut out, &mut audio_tx), 1);
    assert!(source.is_finished());

    // First observation reports; the second doesn't (already reported).
    assert!(source.take_completion_event().is_some(), "track 1's completion is reported once");
    assert!(source.take_completion_event().is_none(), "track 1's completion is not reported twice");

    // Replace with track 2 over the SAME source; its completion reports fresh.
    // That `replace` resets the completion latch is proven by track 2's
    // take succeeding below — an unreset latch would swallow it.
    let (mut sink2, src2, _r2) = create_track_stream_pair(44100, 1);
    assert_eq!(sink2.push_samples(&[2.0]), 1);
    sink2.mark_finished();
    source.replace(src2, TrackFmt::for_test("08c7fe07-b56a-4c63-8df6-ad2967fa0653"));

    let mut out = [0.0f32; 8];
    assert_eq!(source.pull_samples(&mut out, &mut audio_tx), 1);
    assert!(source.is_finished());
    let event =
        source.take_completion_event().expect("track 2's completion is reported after replace");
    assert_eq!(event.0.track_id, "08c7fe07-b56a-4c63-8df6-ad2967fa0653");
    assert!(
        source.take_completion_event().is_none(),
        "track 2's completion is not reported twice either"
    );
}

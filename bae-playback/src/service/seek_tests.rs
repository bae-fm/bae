use super::*;
use crate::audio_output::{audio_event_channel, AudioEvent};
use crate::service::tests::*;
use bae_audio::track_stream::create_track_stream_pair;

#[tokio::test]
async fn seek_drains_pending_gapless_crossing_before_reading_current_track() {
    let (_home, mut service, mut progress_rx) =
        seeded_playback_service(&[(LOADING_RELEASE, &[FINISHED_TRACK, INCOMING_TRACK])]).await;
    fill_test_release(&mut service, LOADING_RELEASE).await;

    let finished_buffer = create_sparse_buffer(1_024);
    let incoming_buffer = create_sparse_buffer(1_024);
    let (mut audio_tx, audio_rx) =
        audio_event_channel(&bae_diagnostics::Diagnostics::noop()).unwrap();
    audio_tx.push_required(AudioEvent::TrackCrossing(TrackCrossing {
        finished_fmt: Arc::new(TrackFmt::for_test(FINISHED_TRACK)),
        decode_error_count: 0,
        samples_decoded: 44_100,
        incoming_fmt: Arc::new(TrackFmt::for_test(INCOMING_TRACK)),
    }));
    let (_sink, source, _ready) = create_track_stream_pair(44_100, 2);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(
        source,
        TrackFmt::for_test(FINISHED_TRACK),
    )));
    // The crossing event lives in the persistent output's audio-events receiver;
    // the source and receiver survive the track transition.
    service.output = Some(test_output(source, audio_rx));
    service.slot = PlaybackSlot::Active(CurrentTrack {
        prepared: test_prepared_track(FINISHED_TRACK, finished_buffer.clone()),
        decoder: test_decoder(),
        phase: TrackPhase::Playing,
        position: StreamPosition::START,
    });
    service.preloaded_next = Some(test_preloaded_next(
        test_prepared_track(INCOMING_TRACK, incoming_buffer),
        PreloadedNextSource::Staged,
    ));

    service.seek(StreamPosition::from_duration(std::time::Duration::ZERO)).await;

    assert_eq!(service.slot.current_track_id().unwrap(), INCOMING_TRACK);
    let mut saw_incoming_seek = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if let PlaybackProgress::Seeked { track_id, .. } = progress {
            saw_incoming_seek = track_id == INCOMING_TRACK;
        }
    }
    assert!(saw_incoming_seek, "seek should emit for the crossed-into track");
    assert!(finished_buffer.is_cancelled());
}

/// After a track drains naturally, the decoder-completion callback flips the
/// shared audio-state atomic to `Stopped` while the track's bookkeeping is
/// retained (so AutoAdvance / the side-pause decision can still read it). A seek
/// arriving in that window must resume audible playback at the seek target — not
/// rebuild a stream that stays silent because the atomic is still `Stopped`.
#[tokio::test]
async fn seek_after_natural_completion_resumes_audibly() {
    let (_home, mut service, _progress_rx) =
        seeded_playback_service(&[(LOADING_RELEASE, &[FINISHED_TRACK])]).await;
    fill_test_release(&mut service, LOADING_RELEASE).await;

    let buffer = create_sparse_buffer(1_024);
    // The track drained: phase is Completed with its bookkeeping retained, and
    // the audio callback already flipped the atomic to Stopped.
    service.slot =
        active_slot(test_prepared_track(FINISHED_TRACK, buffer.clone()), TrackPhase::Completed);
    service.audio_output.set_state(crate::audio_output::AudioState::Stopped);

    service.seek(StreamPosition::from_duration(std::time::Duration::from_millis(500))).await;

    assert_eq!(
        service.audio_output.get_state(),
        crate::audio_output::AudioState::Playing,
        "seeking after a track finished naturally should resume audible playback"
    );
}

/// A seek bar's fill is a fraction of the track's length. A track with no
/// length has no position at any fill, so a seek by ratio leaves it playing
/// where it is rather than jumping to a fraction of an assumed length.
#[tokio::test]
async fn a_seek_by_ratio_leaves_a_track_with_no_length_where_it_is() {
    let (_home, mut service, _progress) = test_playback_service().await;
    let mut prepared = test_prepared_track("track", create_sparse_buffer(1_024));
    prepared.timeline = TrackTimeline::new(None, std::time::Duration::ZERO);
    service.slot = active_slot(prepared, TrackPhase::Playing);
    if let PlaybackSlot::Active(cur) = &mut service.slot {
        cur.position = StreamPosition::from_millis(10_000);
    }

    service.seek_by_ratio(0.5).await;

    let PlaybackSlot::Active(cur) = &service.slot else {
        panic!("the track is still current");
    };
    assert!(matches!(cur.phase, TrackPhase::Playing));
    assert_eq!(cur.position, StreamPosition::from_millis(10_000));
}

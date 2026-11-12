use super::*;
use crate::service::tests::*;
use bae_audio::track_stream::create_track_stream_pair;

/// Table-drive `playback_state()` over each slot/phase.
#[tokio::test]
async fn playback_state_mapping() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);

    service.slot = PlaybackSlot::Stopped;
    assert!(matches!(service.playback_state(), PlaybackState::Stopped));

    service.slot = PlaybackSlot::Loading(LoadingTrack::Unprepared { track_id: "t".to_string() });
    assert!(matches!(
        service.playback_state(),
        PlaybackState::Loading { track: LoadingTrack::Unprepared { .. } }
    ));

    let generation = service.next_load_generation();
    service.slot = active_slot(
        test_prepared_track("t", buffer.clone()),
        TrackPhase::Loading { generation, target: PlayTarget::Playing },
    );
    assert!(matches!(
        service.playback_state(),
        PlaybackState::Loading { track: LoadingTrack::Prepared(_) }
    ));

    service.slot = active_slot(test_prepared_track("t", buffer.clone()), TrackPhase::Playing);
    assert!(matches!(service.playback_state(), PlaybackState::Playing { .. }));

    service.slot = active_slot(
        test_prepared_track("t", buffer.clone()),
        TrackPhase::Paused(PausePhase::Manual),
    );
    assert!(matches!(
        service.playback_state(),
        PlaybackState::Paused { reason: PlaybackPauseReason::Manual, .. }
    ));

    service.slot = active_slot(
        test_prepared_track("t", buffer),
        TrackPhase::Paused(PausePhase::SideEnded(SidePauseDecision {
            track_id: "next".to_string(),
            boundary: SideBoundary {
                id: "id".to_string(),
                kind: PlaybackPauseBoundary::Side,
                side_label: "B".to_string(),
            },
            resumes_at: None,
        })),
    );
    assert!(matches!(
        service.playback_state(),
        PlaybackState::Paused { reason: PlaybackPauseReason::SideEnded(_), .. }
    ));
    // `playback_state` treats Completed as unreachable, so it has no case here.
}

/// Skipping to the next track after the current one drained naturally must
/// resume audible playback, not carry the completion's Stopped atomic forward as
/// a silent/paused next track. Drives the real `handle_next` over a Completed
/// slot (the phase a track sits in briefly after natural completion, before
/// AutoAdvance runs) with a preloaded next so no DB lookup is needed. This locks
/// the `TrackPhase::Completed` arm of `current_play_target`; reverting that arm
/// to a paused/stopped target turns the atomic assertion red.
#[tokio::test]
async fn next_after_natural_completion_resumes_audibly() {
    let (_home, mut service, _progress_rx) =
        seeded_playback_service(&[(LOADING_RELEASE, &[FINISHED_TRACK, NEXT_TRACK])]).await;
    fill_test_release(&mut service, LOADING_RELEASE).await;

    // The current track drained naturally: phase Completed, and the audio
    // callback already flipped the atomic to Stopped.
    let finished_buffer = create_sparse_buffer(1_024);
    service.slot =
        active_slot(test_prepared_track(FINISHED_TRACK, finished_buffer), TrackPhase::Completed);
    service.audio_output.set_state(crate::audio_output::AudioState::Stopped);

    // A next track is preloaded and ready to play without a fresh decode.
    let (_next_sink, next_source, _next_ready) = create_track_stream_pair(44_100, 2);
    let next_buffer = create_sparse_buffer(1_024);
    service.preloaded_next = Some(test_preloaded_next(
        test_prepared_track(NEXT_TRACK, next_buffer),
        PreloadedNextSource::Held(next_source),
    ));

    service.handle_next().await;

    assert_eq!(
        service.audio_output.get_state(),
        crate::audio_output::AudioState::Playing,
        "Next after natural completion should resume audible playback on the new track"
    );
    assert!(
        matches!(
            &service.slot,
            PlaybackSlot::Active(cur)
                if cur.prepared.track_id == NEXT_TRACK
                    && matches!(cur.phase, TrackPhase::Playing)
        ),
        "the next track should be current and Playing"
    );
}

/// A `TrackReady` from a superseded load (same track id, replayed through a
/// fresh load) carries the old generation and must be dropped; only the live
/// load's generation resolves the phase and emits. A same-id replay is exactly
/// the case load identity must reject.
#[tokio::test]
async fn track_ready_with_stale_generation_is_ignored() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let stale = service.next_load_generation();
    let live = service.next_load_generation();
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("t", buffer),
        TrackPhase::Loading { generation: live, target: PlayTarget::Playing },
    );

    service.resolve_track_ready("t".to_string(), stale).await;
    assert!(progress_rx.try_recv().is_err(), "a stale-generation TrackReady must not emit");
    assert!(
        matches!(
            &service.slot,
            PlaybackSlot::Active(cur) if matches!(cur.phase, TrackPhase::Loading { .. })
        ),
        "the phase must stay Loading after a stale signal"
    );

    service.resolve_track_ready("t".to_string(), live).await;
    assert!(
        matches!(
            progress_rx.try_recv(),
            Ok(PlaybackProgress::StateChanged { state: PlaybackState::Playing { .. } })
        ),
        "the live load resolves to Playing and emits"
    );
}

/// Pausing during a load collapses the Loading phase to Paused (emitting Paused
/// once); the pending `TrackReady` then no longer matches the phase and is
/// dropped rather than re-emitting a second Paused.
#[tokio::test]
async fn pause_during_load_emits_paused_and_supersedes_track_ready() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let generation = service.next_load_generation();
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("t", buffer),
        TrackPhase::Loading { generation, target: PlayTarget::Playing },
    );

    service.pause();
    assert!(
        matches!(
            progress_rx.try_recv(),
            Ok(PlaybackProgress::StateChanged {
                state: PlaybackState::Paused { reason: PlaybackPauseReason::Manual, .. }
            })
        ),
        "pause during a load emits Paused(Manual)"
    );

    service.resolve_track_ready("t".to_string(), generation).await;
    assert!(
        progress_rx.try_recv().is_err(),
        "the collapsed load's TrackReady must not emit a second state"
    );
}

/// The resume point follows what plays: a Playing position saves it once the
/// track has played a second past the position saved last, so reports at
/// 0.4 s and 0.8 s leave the track change's 0 s, 1.2 s saves, 2.1 s does not, and
/// 2.2 s does. Sabotage: save on every report, and 0.4 s is saved.
#[tokio::test]
async fn the_resume_point_is_saved_after_each_second_of_play() {
    use bae_audio::track_stream::create_track_stream_pair_with_capacity;
    use std::time::Duration;
    const RELEASE: &str = "c61a9e19-f3ba-4728-842c-c59dbc82e238";
    let (_home, mut service, _progress) = seeded_playback_service(&[(RELEASE, &[TRACK_T])]).await;
    let mut prepared = test_prepared_track(TRACK_T, create_sparse_buffer(1024));
    prepared.timeline = TrackTimeline::new(Some(Duration::from_secs(40)), Duration::ZERO);
    // A sample is 10 ms.
    prepared.sample_rate = 100;
    prepared.channels = 1;
    let fmt = prepared.track_fmt(StreamPosition::START);
    let (mut sink, stream, _) = create_track_stream_pair_with_capacity(100, 1, 4000);
    sink.push_samples(&vec![0.25; 4000]);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(stream, fmt)));
    let (mut audio_tx, audio_rx) =
        crate::audio_output::audio_event_channel(&bae_diagnostics::Diagnostics::noop()).unwrap();
    service.output = Some(test_output(source.clone(), audio_rx));
    let generation = service.next_load_generation();
    service.slot =
        active_slot(prepared, TrackPhase::Loading { generation, target: PlayTarget::Playing });
    service.resolve_track_ready(TRACK_T.to_string(), generation).await;
    // What a track change saves before its first report.
    service.persist_playback_state().await;
    let store = service.store.clone();
    let saved = async || {
        crate::tables::load_playback_state(&store).await.unwrap().and_then(|row| row.position_ms)
    };
    assert_eq!(saved().await, Some(0));

    let mut reported = Vec::new();
    for samples in [40, 40, 40, 90, 10] {
        source.lock().unwrap().pull_samples(&mut vec![0.0; samples], &mut audio_tx);
        let (fmt, position) = source.lock().unwrap().position_event();
        service.handle_position_event(fmt, position).await;
        reported.push((position.as_millis(), saved().await));
    }

    assert_eq!(
        reported,
        [
            (400, Some(0)),
            (800, Some(0)),
            (1_200, Some(1_200)),
            (2_100, Some(1_200)),
            (2_200, Some(2_200)),
        ]
    );
}

const RESUME_RELEASE: &str = "0e3d7a52-6c0f-4e5b-9a8e-3d1f4b7c2a90";

/// A resume whose volume can't be read resumes nothing, tells the person, and
/// leaves the saved resume point as it was, rather than resuming at full
/// volume or writing over the point.
#[tokio::test]
async fn a_failed_read_of_the_volume_fails_the_resume_and_keeps_the_resume_row() {
    let (home, mut service, mut progress_rx) =
        seeded_playback_service(&[(RESUME_RELEASE, &[TRACK_T])]).await;
    fill_test_release(&mut service, RESUME_RELEASE).await;
    service.playback_queue.save_playback_state(Some(5_000)).await.expect("the row saves");
    let saved = crate::tables::load_playback_state(home.store())
        .await
        .expect("the row reads")
        .expect("the row was saved");
    crate::tables::lose_playback_level_for_test(home.directory());

    service.restore(saved.clone()).await;
    service.persist_playback_state().await;

    assert!(matches!(service.slot, PlaybackSlot::Stopped), "nothing resumes");
    assert!(drained_a_playback_error(&mut progress_rx), "the failure reaches the person");
    assert_eq!(
        crate::tables::load_playback_state(home.store()).await.expect("the row reads"),
        Some(saved),
        "the resume point is as it was"
    );
}

/// With the volume unread, the output sits at the full volume it opened with,
/// so nothing plays: a play fails with an error rather than starting loud.
#[tokio::test]
async fn nothing_plays_while_the_volume_is_unread() {
    let (home, mut service, mut progress_rx) =
        seeded_playback_service(&[(RESUME_RELEASE, &[TRACK_T])]).await;
    fill_test_release(&mut service, RESUME_RELEASE).await;
    let resume = crate::tables::PlaybackResumeRow { current: None, position_ms: None };
    crate::tables::lose_playback_level_for_test(home.directory());
    service.restore(resume).await;
    assert!(drained_a_playback_error(&mut progress_rx));

    service
        .play_track(
            TRACK_T,
            TrackStart::Direct,
            PlayTarget::Playing,
            bae_diagnostics::TrackTransition::Manual,
        )
        .await;

    assert!(matches!(service.slot, PlaybackSlot::Stopped), "nothing plays");
    assert!(drained_a_playback_error(&mut progress_rx), "the failure reaches the person");
}

/// A resume row that can't be read resumes nothing, tells the person, and is
/// not cleared by the stop that follows.
#[tokio::test]
async fn a_failed_read_of_the_resume_row_keeps_it() {
    let (home, mut service, mut progress_rx) =
        seeded_playback_service(&[(RESUME_RELEASE, &[TRACK_T])]).await;
    crate::tables::plant_unreadable_resume_row_for_test(home.directory());

    service.restore_from_cache().await;
    service.persist_playback_state().await;

    assert!(matches!(service.slot, PlaybackSlot::Stopped));
    assert!(drained_a_playback_error(&mut progress_rx), "the failure reaches the person");
    assert!(
        crate::tables::load_playback_state(home.store()).await.is_err(),
        "the row is still there, unread"
    );
}

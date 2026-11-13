use super::*;
use crate::audio_output::{audio_event_channel, AudioEvent};
use crate::renderer::{ReceiverStatus, RendererPlayerState, RendererSessionEvent};
use crate::service::tests::*;

/// Exercise listening through the real source's consumed-frame counter, replacing
/// its decoder at a seek and retaining the same play across a pause.
#[tokio::test]
async fn listening_counts_consumed_audio_across_decoder_replacement_and_pause() {
    use std::time::Duration;
    const RELEASE: &str = "c61a9e19-f3ba-4728-842c-c59dbc82e238";
    let (_home, mut service, mut progress) =
        seeded_playback_service(&[(RELEASE, &[TRACK_T])]).await;
    let mut prepared = test_prepared_track(TRACK_T, create_sparse_buffer(1024));
    prepared.timeline =
        TrackTimeline::new(Some(Duration::from_secs(40)), std::time::Duration::ZERO);
    prepared.sample_rate = 100;
    prepared.channels = 1;
    let fmt = prepared.track_fmt(StreamPosition::START);
    let (mut sink, stream, _) =
        bae_audio::track_stream::create_track_stream_pair_with_capacity(100, 1, 4000);
    sink.push_samples(&vec![0.25; 4000]);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(stream, fmt)));
    let (mut audio_tx, audio_rx) =
        audio_event_channel(&bae_diagnostics::Diagnostics::noop()).unwrap();
    service.output = Some(test_output(source.clone(), audio_rx));
    let generation = service.next_load_generation();
    service.slot =
        active_slot(prepared, TrackPhase::Loading { generation, target: PlayTarget::Playing });
    service.resolve_track_ready(TRACK_T.to_string(), generation).await;
    source.lock().unwrap().pull_samples(&mut vec![0.0; 1900], &mut audio_tx);
    let (fmt, position) = source.lock().unwrap().position_event();
    service.handle_position_event(fmt, position).await;
    assert!(service.people.history().await.unwrap().is_empty());
    service.pause();
    service.capture_listening_position();
    service.persist_listening().await;
    service.resume().await;
    assert!(service.people.history().await.unwrap().is_empty());

    // The decoder starts at thirty seconds after a seek. Only its hundred
    // consumed samples add to the nineteen seconds already heard.
    let (mut sink, stream, _) =
        bae_audio::track_stream::create_track_stream_pair_with_capacity(100, 1, 1000);
    sink.push_samples(&vec![0.25; 1000]);
    let fmt = match &service.slot {
        PlaybackSlot::Active(cur) => cur.prepared.track_fmt(StreamPosition::from_millis(30_000)),
        _ => panic!("active"),
    };
    source.lock().unwrap().replace(stream, fmt);
    source.lock().unwrap().pull_samples(&mut vec![0.0; 100], &mut audio_tx);
    let (fmt, position) = source.lock().unwrap().position_event();
    service.handle_position_event(fmt, position).await;
    let rows = service.people.history().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].played, Duration::from_secs(20));
    assert_eq!(rows[0].started_at, test_clock_start());
    let id = rows[0].id;
    // No position tick covers these final samples. Stop captures them itself.
    source.lock().unwrap().pull_samples(&mut [0.0; 50], &mut audio_tx);
    service.stop().await;
    let rows = service.people.history().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, id);
    assert_eq!(rows[0].played, Duration::from_millis(20_500));
    assert!(!drained_a_playback_error(&mut progress));
}

/// Each save is a synced library write. Once a play counts as played, audio
/// progress alone must not save it again; the next boundary saves it.
#[tokio::test]
async fn a_counted_play_is_saved_again_only_at_its_next_boundary() {
    use std::time::Duration;
    const RELEASE: &str = "c61a9e19-f3ba-4728-842c-c59dbc82e238";
    let (_home, mut service, mut progress) =
        seeded_playback_service(&[(RELEASE, &[TRACK_T])]).await;
    let mut prepared = test_prepared_track(TRACK_T, create_sparse_buffer(1024));
    prepared.timeline =
        TrackTimeline::new(Some(Duration::from_secs(40)), std::time::Duration::ZERO);
    prepared.sample_rate = 100;
    prepared.channels = 1;
    let fmt = prepared.track_fmt(StreamPosition::START);
    let (mut sink, stream, _) =
        bae_audio::track_stream::create_track_stream_pair_with_capacity(100, 1, 4000);
    sink.push_samples(&vec![0.25; 4000]);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(stream, fmt)));
    let (mut audio_tx, audio_rx) =
        audio_event_channel(&bae_diagnostics::Diagnostics::noop()).unwrap();
    service.output = Some(test_output(source.clone(), audio_rx));
    let generation = service.next_load_generation();
    service.slot =
        active_slot(prepared, TrackPhase::Loading { generation, target: PlayTarget::Playing });
    service.resolve_track_ready(TRACK_T.to_string(), generation).await;
    let mut hear = |samples: usize| {
        source.lock().unwrap().pull_samples(&mut vec![0.0; samples], &mut audio_tx);
        source.lock().unwrap().position_event()
    };
    let (fmt, position) = hear(2100);
    service.handle_position_event(fmt, position).await;
    let saved = |rows: Vec<bae_people::history::HistoryEntry>| {
        assert_eq!(rows.len(), 1);
        rows[0].played
    };
    assert_eq!(saved(service.people.history().await.unwrap()), Duration::from_secs(21));
    for _ in 0..5 {
        let (fmt, position) = hear(100);
        service.handle_position_event(fmt, position).await;
        service.checkpoint_listening().await;
    }
    assert_eq!(
        saved(service.people.history().await.unwrap()),
        Duration::from_secs(21),
        "progress after the play counted must not write it again"
    );
    service.pause();
    service.capture_listening_position();
    service.persist_listening().await;
    assert_eq!(saved(service.people.history().await.unwrap()), Duration::from_secs(26));
    assert!(!drained_a_playback_error(&mut progress));
}

#[tokio::test]
async fn gapless_crossing_records_the_finishing_play_and_starts_a_distinct_one() {
    use std::time::Duration;
    const RELEASE: &str = "c61a9e19-f3ba-4728-842c-c59dbc82e238";
    let (_home, mut service, mut progress) =
        seeded_playback_service(&[(RELEASE, &[TRACK_A, TRACK_B])]).await;
    fill_test_release(&mut service, RELEASE).await;
    let prepared = |id: &str| {
        let mut track = test_prepared_track(id, create_sparse_buffer(1024));
        track.sample_rate = 100;
        track.channels = 1;
        track
    };
    let first = prepared(TRACK_A);
    let second = prepared(TRACK_B);
    let (mut a, source_a, _) =
        bae_audio::track_stream::create_track_stream_pair_with_capacity(100, 1, 100);
    let (mut b, source_b, _) =
        bae_audio::track_stream::create_track_stream_pair_with_capacity(100, 1, 100);
    a.push_samples(&vec![0.25; 100]);
    a.mark_finished();
    b.push_samples(&vec![0.25; 100]);
    b.mark_finished();
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(
        source_a,
        first.track_fmt(StreamPosition::START),
    )));
    source.lock().unwrap().stage_next(source_b, second.track_fmt(StreamPosition::START));
    let (mut audio_tx, audio_rx) =
        audio_event_channel(&bae_diagnostics::Diagnostics::noop()).unwrap();
    service.output = Some(test_output(source.clone(), audio_rx));
    service.slot = active_slot(first, TrackPhase::Playing);
    service.preloaded_next = Some(test_preloaded_next(second, PreloadedNextSource::Staged));
    assert!(service.begin_listening().await);
    source.lock().unwrap().pull_samples(&mut vec![0.0; 150], &mut audio_tx);
    service.drain_current_audio_events().await;
    let (fmt, position) = source.lock().unwrap().position_event();
    service.handle_position_event(fmt, position).await;
    let rows = service.people.history().await.unwrap();
    assert_eq!(rows.len(), 2);
    let a = rows.iter().find(|row| row.track_id == TRACK_A).unwrap();
    let b = rows.iter().find(|row| row.track_id == TRACK_B).unwrap();
    assert_ne!(a.id, b.id);
    assert_eq!(a.played, Duration::from_secs(1));
    assert_eq!(b.played, Duration::from_millis(500));
    service.stop().await;
    assert!(!drained_a_playback_error(&mut progress));
}

#[tokio::test]
async fn airplay_completion_waits_for_the_audio_still_in_the_receiver() {
    use std::time::Duration;
    for pause_tail in [false, true] {
        const RELEASE: &str = "c61a9e19-f3ba-4728-842c-c59dbc82e238";
        let (_home, mut service, mut progress) =
            seeded_playback_service(&[(RELEASE, &[TRACK_T])]).await;
        let clock = Arc::new(crate::ManualPlaybackClock::new(test_clock_start()));
        service.clock = clock.clone();
        let mut prepared = test_prepared_track(TRACK_T, create_sparse_buffer(1024));
        prepared.timeline =
            TrackTimeline::new(Some(Duration::from_secs(40)), std::time::Duration::ZERO);
        prepared.sample_rate = 100;
        prepared.channels = 1;
        let (mut sink, stream, _) =
            bae_audio::track_stream::create_track_stream_pair_with_capacity(100, 1, 4000);
        sink.push_samples(&vec![0.25; 4000]);
        sink.mark_finished();
        let mut source =
            source::PlaybackSource::new(stream, prepared.track_fmt(StreamPosition::START));
        service.slot = active_slot(prepared, TrackPhase::Playing);
        let _airplay = install_airplay(&mut service, Duration::from_secs(2));
        assert!(service.begin_listening().await);
        let (mut tx, _) = audio_event_channel(&bae_diagnostics::Diagnostics::noop()).unwrap();
        source.pull_samples(&mut vec![0.0; 4000], &mut tx);
        service
            .handle_audio_event(AudioEvent::Completion(source.take_completion_event().unwrap()))
            .await;
        assert_eq!(service.people.history().await.unwrap()[0].played, Duration::from_secs(38));
        assert!(
            drained_track_completed_ids(&mut progress).is_empty(),
            "the receiver still has two seconds to play"
        );
        clock.advance(Duration::from_secs(1));
        service.persist_listening().await;
        assert_eq!(service.people.history().await.unwrap()[0].played, Duration::from_secs(39));
        if pause_tail {
            service.pause();
        }
        clock.advance(Duration::from_secs(1));
        service.persist_listening().await;
        assert_eq!(
            service.people.history().await.unwrap()[0].played,
            Duration::from_secs(if pause_tail { 39 } else { 40 })
        );
        service.publish_audible_completion();
        if pause_tail {
            assert!(drained_track_completed_ids(&mut progress).is_empty());
        } else {
            assert_eq!(drained_track_completed_ids(&mut progress), vec![TRACK_T.to_string()]);
        }
    }
}

/// A play that comes to count as played while its last audio is still in the
/// AirPlay receiver is due a save at that moment, and the loop's audible
/// deadline is it rather than the tail's end. Sabotage: leave the moment out
/// of the deadline, and the save waits for the tail to finish.
#[tokio::test]
async fn a_play_that_counts_partway_through_the_airplay_tail_is_saved_then() {
    use std::time::Duration;
    const RELEASE: &str = "c61a9e19-f3ba-4728-842c-c59dbc82e238";
    let (_home, mut service, _progress) = seeded_playback_service(&[(RELEASE, &[TRACK_T])]).await;
    let clock = Arc::new(crate::ManualPlaybackClock::new(test_clock_start()));
    service.clock = clock.clone();
    let mut prepared = test_prepared_track(TRACK_T, create_sparse_buffer(1024));
    // Four seconds: it counts once two are heard.
    prepared.timeline = TrackTimeline::new(Some(Duration::from_secs(4)), Duration::ZERO);
    prepared.sample_rate = 100;
    prepared.channels = 1;
    let (mut sink, stream, _) =
        bae_audio::track_stream::create_track_stream_pair_with_capacity(100, 1, 400);
    sink.push_samples(&vec![0.25; 400]);
    sink.mark_finished();
    let mut source = source::PlaybackSource::new(stream, prepared.track_fmt(StreamPosition::START));
    service.slot = active_slot(prepared, TrackPhase::Playing);
    let _airplay = install_airplay(&mut service, Duration::from_secs(3));
    assert!(service.begin_listening().await);
    let (mut tx, _) = audio_event_channel(&bae_diagnostics::Diagnostics::noop()).unwrap();
    source.pull_samples(&mut vec![0.0; 400], &mut tx);
    service
        .handle_audio_event(AudioEvent::Completion(source.take_completion_event().unwrap()))
        .await;

    // One second heard, three still in the receiver.
    assert!(service.people.history().await.unwrap().is_empty());
    let second = |seconds| test_clock_start() + chrono::Duration::seconds(seconds);
    assert_eq!(service.audible_deadline(), Some(second(1)));

    clock.advance(Duration::from_secs(1));
    service.checkpoint_listening().await;

    assert_eq!(service.people.history().await.unwrap()[0].played, Duration::from_secs(2));
    assert_eq!(service.audible_deadline(), Some(second(3)), "next, the tail's end");
}

#[tokio::test]
async fn remote_listening_keeps_one_play_across_seek_and_pause() {
    use std::time::Duration;
    let (_home, mut service, _device, mut progress) = remote_over_fake().await;
    let clock = Arc::new(crate::ManualPlaybackClock::new(test_clock_start()));
    service.clock = clock.clone();
    let status = |state, seconds| ReceiverStatus {
        player_state: state,
        position: Some(Duration::from_secs(seconds)),
        duration: None,
        volume: Some(1.0),
        rate: None,
    };
    clock.advance(Duration::from_secs(30));
    service
        .handle_remote_event(status_event(&service, status(RendererPlayerState::Playing, 30)))
        .await;
    let first = service.people.history().await.unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].played, Duration::from_secs(30));
    service.seek(StreamPosition::from_millis(90_000)).await;
    clock.advance(Duration::from_secs(2));
    service
        .handle_remote_event(status_event(&service, status(RendererPlayerState::Playing, 92)))
        .await;
    clock.advance(Duration::from_secs(1));
    service
        .handle_remote_event(status_event(&service, status(RendererPlayerState::Paused, 93)))
        .await;
    clock.advance(Duration::from_secs(100));
    service
        .handle_remote_event(status_event(&service, status(RendererPlayerState::Paused, 93)))
        .await;
    service.stop().await;
    let rows = service.people.history().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, first[0].id);
    assert_eq!(rows[0].played, Duration::from_secs(33));
    assert_eq!(rows[0].started_at, test_clock_start());
    assert!(!drained_a_playback_error(&mut progress));
}

/// A track with no stored length learns it when its audio ends, so a short
/// track heard to its end counts as played even though an unknown length starts
/// out needing thirty seconds heard.
#[tokio::test]
async fn a_short_track_of_unknown_length_heard_to_its_end_counts_as_played() {
    use std::time::Duration;
    const RELEASE: &str = "c61a9e19-f3ba-4728-842c-c59dbc82e238";
    let (_home, mut service, mut progress) =
        seeded_playback_service(&[(RELEASE, &[TRACK_T])]).await;
    let mut prepared = test_prepared_track(TRACK_T, create_sparse_buffer(1024));
    prepared.timeline = TrackTimeline::new(None, std::time::Duration::ZERO);
    prepared.sample_rate = 100;
    prepared.channels = 1;
    let (mut sink, stream, _) =
        bae_audio::track_stream::create_track_stream_pair_with_capacity(100, 1, 2000);
    sink.push_samples(&vec![0.25; 2000]);
    sink.mark_finished();
    let mut source = source::PlaybackSource::new(stream, prepared.track_fmt(StreamPosition::START));
    service.slot = active_slot(prepared, TrackPhase::Playing);
    assert!(service.begin_listening().await);
    let (mut tx, _) = audio_event_channel(&bae_diagnostics::Diagnostics::noop()).unwrap();
    source.pull_samples(&mut vec![0.0; 2000], &mut tx);

    service
        .handle_audio_event(AudioEvent::Completion(source.take_completion_event().unwrap()))
        .await;
    service.persist_listening().await;

    let rows = service.people.history().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].played, Duration::from_secs(20));
    assert!(!drained_a_playback_error(&mut progress));
}

/// A listening history that can't be recorded ships the history as the
/// operation that failed.
#[tokio::test]
async fn a_listening_that_cannot_be_recorded_ships_a_history_failure() {
    let (diagnostics, transport) = recording_diagnostics();
    let (_home, mut service, _progress) = test_playback_service().await;
    service.diagnostics = diagnostics.clone();

    service.listening_failed(PlaybackError::internal("the play could not be saved"));

    diagnostics.flush().await.expect("flush succeeds");
    assert_eq!(playback_failed_operations(&transport), ["history"]);
}

/// A status event for `service` as if it arrived now, on its clock.
fn status_event(service: &PlaybackService, status: ReceiverStatus) -> RendererSessionEvent {
    RendererSessionEvent::Status { status, observed_at: service.clock.now() }
}

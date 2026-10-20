/// Exercise listening through the real source's consumed-frame counter, replacing
/// its decoder at a seek and retaining the same play across a pause.
#[tokio::test]
async fn listening_counts_consumed_audio_across_decoder_replacement_and_pause() {
    use std::time::Duration;
    const RELEASE: &str = "c61a9e19-f3ba-4728-842c-c59dbc82e238";
    let (_home, mut service, mut progress) =
        seeded_playback_service(&[(RELEASE, &[TRACK_T])]).await;
    let mut prepared = test_prepared_track(TRACK_T, create_sparse_buffer(1024));
    prepared.release_id = RELEASE.to_string();
    prepared.duration = Some(Duration::from_secs(40));
    prepared.timeline = TrackTimeline::new(Duration::from_secs(40), None);
    prepared.sample_rate = 100;
    prepared.channels = 1;
    let fmt = prepared.track_fmt(StreamPosition::START);
    let (mut sink, stream, _) =
        crate::playback::create_track_stream_pair_with_capacity(100, 1, 4000);
    sink.push_samples(&vec![0.25; 4000]);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(stream, fmt)));
    let (mut audio_tx, audio_rx) = audio_event_channel();
    service.output = Some(test_output(source.clone(), audio_rx));
    let generation = service.next_load_generation();
    service.slot = active_slot(
        prepared,
        TrackPhase::Loading {
            generation,
            target: PlayTarget::Playing,
        },
    );
    service
        .resolve_track_ready(TRACK_T.to_string(), generation)
        .await;
    source
        .lock()
        .unwrap()
        .pull_samples(&mut vec![0.0; 1900], &mut audio_tx);
    let (fmt, position) = source.lock().unwrap().position_event();
    service.handle_position_event(fmt, position).await;
    assert!(service.library_manager.history().await.unwrap().is_empty());
    service.pause();
    service.capture_listening_position();
    service.persist_listening().await;
    service.resume().await;
    assert!(service.library_manager.history().await.unwrap().is_empty());

    // The decoder starts at thirty seconds after a seek. Only its hundred
    // consumed samples add to the nineteen seconds already heard.
    let (mut sink, stream, _) =
        crate::playback::create_track_stream_pair_with_capacity(100, 1, 1000);
    sink.push_samples(&vec![0.25; 1000]);
    let fmt = match &service.slot {
        PlaybackSlot::Active(cur) => cur.prepared.track_fmt(StreamPosition::from_millis(30_000)),
        _ => panic!("active"),
    };
    source.lock().unwrap().replace(stream, fmt);
    source
        .lock()
        .unwrap()
        .pull_samples(&mut vec![0.0; 100], &mut audio_tx);
    let (fmt, position) = source.lock().unwrap().position_event();
    service.handle_position_event(fmt, position).await;
    let rows = service.library_manager.history().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].played, Duration::from_secs(20));
    assert_eq!(rows[0].started_at, test_clock_start());
    let id = rows[0].id;
    // No position tick covers these final samples. Stop captures them itself.
    source
        .lock()
        .unwrap()
        .pull_samples(&mut [0.0; 50], &mut audio_tx);
    service.stop().await;
    let rows = service.library_manager.history().await.unwrap();
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
    prepared.release_id = RELEASE.to_string();
    prepared.duration = Some(Duration::from_secs(40));
    prepared.timeline = TrackTimeline::new(Duration::from_secs(40), None);
    prepared.sample_rate = 100;
    prepared.channels = 1;
    let fmt = prepared.track_fmt(StreamPosition::START);
    let (mut sink, stream, _) =
        crate::playback::create_track_stream_pair_with_capacity(100, 1, 4000);
    sink.push_samples(&vec![0.25; 4000]);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(stream, fmt)));
    let (mut audio_tx, audio_rx) = audio_event_channel();
    service.output = Some(test_output(source.clone(), audio_rx));
    let generation = service.next_load_generation();
    service.slot = active_slot(
        prepared,
        TrackPhase::Loading {
            generation,
            target: PlayTarget::Playing,
        },
    );
    service
        .resolve_track_ready(TRACK_T.to_string(), generation)
        .await;
    let mut hear = |samples: usize| {
        source
            .lock()
            .unwrap()
            .pull_samples(&mut vec![0.0; samples], &mut audio_tx);
        source.lock().unwrap().position_event()
    };
    let (fmt, position) = hear(2100);
    service.handle_position_event(fmt, position).await;
    let saved = |rows: Vec<crate::history::HistoryEntry>| {
        assert_eq!(rows.len(), 1);
        rows[0].played
    };
    assert_eq!(
        saved(service.library_manager.history().await.unwrap()),
        Duration::from_secs(21)
    );
    for _ in 0..5 {
        let (fmt, position) = hear(100);
        service.handle_position_event(fmt, position).await;
        service.checkpoint_listening().await;
    }
    assert_eq!(
        saved(service.library_manager.history().await.unwrap()),
        Duration::from_secs(21),
        "progress after the play counted must not write it again"
    );
    service.pause();
    service.capture_listening_position();
    service.persist_listening().await;
    assert_eq!(
        saved(service.library_manager.history().await.unwrap()),
        Duration::from_secs(26)
    );
    assert!(!drained_a_playback_error(&mut progress));
}

#[tokio::test]
async fn gapless_crossing_records_the_finishing_play_and_starts_a_distinct_one() {
    use std::time::Duration;
    const RELEASE: &str = "c61a9e19-f3ba-4728-842c-c59dbc82e238";
    let (_home, mut service, mut progress) =
        seeded_playback_service(&[(RELEASE, &[TRACK_A, TRACK_B])]).await;
    service.playback_queue.apply(|queue| {
        queue.play_release(
            ContextSource::Release(RELEASE.into()),
            vec![TRACK_A.into(), TRACK_B.into()],
            ContextStart::Index(0),
        )
    });
    let prepared = |id: &str| {
        let mut track = test_prepared_track(id, create_sparse_buffer(1024));
        track.release_id = RELEASE.into();
        track.sample_rate = 100;
        track.channels = 1;
        track
    };
    let first = prepared(TRACK_A);
    let second = prepared(TRACK_B);
    let (mut a, source_a, _) = crate::playback::create_track_stream_pair_with_capacity(100, 1, 100);
    let (mut b, source_b, _) = crate::playback::create_track_stream_pair_with_capacity(100, 1, 100);
    a.push_samples(&vec![0.25; 100]);
    a.mark_finished();
    b.push_samples(&vec![0.25; 100]);
    b.mark_finished();
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(
        source_a,
        first.track_fmt(StreamPosition::START),
    )));
    source
        .lock()
        .unwrap()
        .stage_next(source_b, second.track_fmt(StreamPosition::START));
    let (mut audio_tx, audio_rx) = audio_event_channel();
    service.output = Some(test_output(source.clone(), audio_rx));
    service.slot = active_slot(first, TrackPhase::Playing);
    service.preloaded_next = Some(test_preloaded_next(second, PreloadedNextSource::Staged));
    assert!(service.begin_listening().await);
    source
        .lock()
        .unwrap()
        .pull_samples(&mut vec![0.0; 150], &mut audio_tx);
    service.drain_current_audio_events().await;
    let (fmt, position) = source.lock().unwrap().position_event();
    service.handle_position_event(fmt, position).await;
    let rows = service.library_manager.history().await.unwrap();
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
        let clock = Arc::new(crate::playback::ManualPlaybackClock::new(test_clock_start()));
        service.clock = clock.clone();
        let mut prepared = test_prepared_track(TRACK_T, create_sparse_buffer(1024));
        prepared.release_id = RELEASE.into();
        prepared.duration = Some(Duration::from_secs(40));
        prepared.timeline = TrackTimeline::new(Duration::from_secs(40), None);
        prepared.sample_rate = 100;
        prepared.channels = 1;
        let (mut sink, stream, _) =
            crate::playback::create_track_stream_pair_with_capacity(100, 1, 4000);
        sink.push_samples(&vec![0.25; 4000]);
        sink.mark_finished();
        let mut source =
            source::PlaybackSource::new(stream, prepared.track_fmt(StreamPosition::START));
        service.slot = active_slot(prepared, TrackPhase::Playing);
        install_airplay(&mut service, 88_200, 0.5);
        assert!(service.begin_listening().await);
        let (mut tx, _) = audio_event_channel();
        source.pull_samples(&mut vec![0.0; 4000], &mut tx);
        service
            .handle_audio_event(AudioEvent::Completion(
                source.take_completion_event().unwrap(),
            ))
            .await;
        assert_eq!(
            service.library_manager.history().await.unwrap()[0].played,
            Duration::from_secs(38)
        );
        assert!(
            drained_track_completed_ids(&mut progress).is_empty(),
            "the receiver still has two seconds to play"
        );
        clock.advance(Duration::from_secs(1));
        service.persist_listening().await;
        assert_eq!(
            service.library_manager.history().await.unwrap()[0].played,
            Duration::from_secs(39)
        );
        if pause_tail {
            service.pause();
        }
        clock.advance(Duration::from_secs(1));
        service.persist_listening().await;
        assert_eq!(
            service.library_manager.history().await.unwrap()[0].played,
            Duration::from_secs(if pause_tail { 39 } else { 40 })
        );
        service.publish_audible_completion();
        if pause_tail {
            assert!(drained_track_completed_ids(&mut progress).is_empty());
        } else {
            assert_eq!(
                drained_track_completed_ids(&mut progress),
                vec![TRACK_T.to_string()]
            );
        }
    }
}

#[tokio::test]
async fn remote_listening_keeps_one_play_across_seek_and_pause() {
    use std::time::Duration;
    let (_home, mut service, _device, mut progress) = remote_over_fake().await;
    let clock = Arc::new(crate::playback::ManualPlaybackClock::new(test_clock_start()));
    service.clock = clock.clone();
    let status = |state, seconds| RendererSessionStatus {
        player_state: state,
        position: Some(Duration::from_secs(seconds)),
        duration: None,
        volume: Some(1.0),
        ended: false,
    };
    clock.advance(Duration::from_secs(30));
    service
        .handle_remote_status(status(RendererPlayerState::Playing, 30))
        .await;
    let first = service.library_manager.history().await.unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].played, Duration::from_secs(30));
    service.seek(StreamPosition::from_millis(90_000)).await;
    clock.advance(Duration::from_secs(2));
    service
        .handle_remote_status(status(RendererPlayerState::Playing, 92))
        .await;
    clock.advance(Duration::from_secs(1));
    service
        .handle_remote_status(status(RendererPlayerState::Paused, 93))
        .await;
    clock.advance(Duration::from_secs(100));
    service
        .handle_remote_status(status(RendererPlayerState::Paused, 93))
        .await;
    service.stop().await;
    let rows = service.library_manager.history().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, first[0].id);
    assert_eq!(rows[0].played, Duration::from_secs(33));
    assert_eq!(rows[0].started_at, test_clock_start());
    assert!(!drained_a_playback_error(&mut progress));
}

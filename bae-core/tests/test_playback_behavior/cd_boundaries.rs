#[tokio::test]
async fn cd_auto_advance_respects_disc_boundaries_and_setting() {
    for (format, positions, enabled) in [
        ("CD", ["1-1", "1-2", "2-1"], true),
        ("CD", ["1-1", "2-1", "2-2"], false),
        ("Digital Media", ["1-1", "2-1", "2-2"], true),
    ] {
        let mut fixture = SidePauseTestFixture::new(format, positions, enabled)
            .await
            .expect("disc-pause fixture");
        let first = fixture.track_ids[0].clone();
        let second = fixture.track_ids[1].clone();
        fixture.play_track_and_wait(0, &first).await;
        fixture.seek_to_auto_advance();
        fixture
            .wait_for_playing_track(
                &second,
                Duration::from_secs(10),
                "same disc, disabled setting, and digital releases should continue",
            )
            .await;
        fixture.playback_handle.shutdown().await;
    }
}

#[tokio::test]
async fn cd_disc_pause_resumes_at_next_disc() {
    let mut fixture = SidePauseTestFixture::new("2xCD", ["1-1", "2-1", "2-2"], true)
        .await
        .expect("disc-pause fixture");
    let first = fixture.track_ids[0].clone();
    let second = fixture.track_ids[1].clone();
    let paused = fixture
        .play_to_side_pause(
            0,
            &first,
            "1",
            bae_core::playback::DISC_PAUSE_CD_MESSAGE_KEY,
        )
        .await;
    match paused {
        PlaybackState::Paused {
            track_info,
            reason: PlaybackPauseReason::SideEnded(prompt),
            ..
        } => {
            assert_eq!(track_info.track_id, first);
            assert_eq!(prompt.title_key, bae_core::playback::DISC_PAUSE_TITLE_KEY);
        }
        other => panic!("expected disc-ended pause, got {other:?}"),
    }
    fixture.playback_handle.resume();
    fixture
        .wait_for_playing_track(
            &second,
            Duration::from_secs(5),
            "Play should start the next disc",
        )
        .await;
    fixture.playback_handle.shutdown().await;
}

#[tokio::test]
async fn cd_setting_changes_mid_track_apply_at_disc_boundary() {
    for enabled in [true, false] {
        let mut fixture = SidePauseTestFixture::new("CD", ["1-1", "2-1", "2-2"], !enabled)
            .await
            .expect("disc-pause fixture");
        let first = fixture.track_ids[0].clone();
        let second = fixture.track_ids[1].clone();
        fixture.play_track_and_wait(0, &first).await;
        fixture.set_pause_between_sides_mid_track(enabled);
        fixture.seek_to_auto_advance();
        if enabled {
            fixture
                .wait_for_side_pause("1", bae_core::playback::DISC_PAUSE_CD_MESSAGE_KEY)
                .await;
        } else {
            fixture
                .wait_for_playing_track(
                    &second,
                    Duration::from_secs(10),
                    "disabling the setting should continue into the next disc",
                )
                .await;
        }
        fixture.playback_handle.shutdown().await;
    }
}

#[tokio::test]
async fn cd_manual_next_and_repeat_track_do_not_pause_at_disc_boundary() {
    for repeat_track in [false, true] {
        let mut fixture = SidePauseTestFixture::new("CD", ["1-1", "2-1", "2-2"], true)
            .await
            .expect("disc-pause fixture");
        let first = fixture.track_ids[0].clone();
        let second = fixture.track_ids[1].clone();
        fixture.play_track_and_wait(0, &first).await;
        let expected = if repeat_track {
            fixture.playback_handle.set_repeat_mode(RepeatMode::Track);
            fixture.seek_to_auto_advance();
            &first
        } else {
            fixture.playback_handle.next();
            &second
        };
        fixture
            .wait_for_playing_track(
                expected,
                Duration::from_secs(10),
                "manual navigation and repeat-track should keep playing",
            )
            .await;
        fixture.playback_handle.shutdown().await;
    }
}

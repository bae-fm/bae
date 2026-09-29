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
        fixture.play_track_and_wait(&first).await;
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
            &first,
            "1",
            PlaybackPauseBoundary::Disc,
        )
        .await;
    match paused {
        PlaybackState::Paused {
            track,
            reason: PlaybackPauseReason::SideEnded(prompt),
            ..
        } => {
            assert_eq!(track.track_id, first);
            assert_eq!(prompt.boundary, PlaybackPauseBoundary::Disc);
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
        fixture.play_track_and_wait(&first).await;
        fixture.set_pause_between_sides_mid_track(enabled).await;
        fixture.seek_to_auto_advance();
        if enabled {
            fixture
                .wait_for_side_pause("1", PlaybackPauseBoundary::Disc)
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
        fixture.play_track_and_wait(&first).await;
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

/// Moving the staged next track onto the next disc while the track before it
/// plays pauses at the boundary the edit made: the gapless handoff staged when
/// both were on one disc is taken back.
#[tokio::test(flavor = "multi_thread")]
async fn a_disc_edit_mid_track_applies_at_the_boundary_it_makes() {
    let mut fixture = SidePauseTestFixture::new("2xCD", ["1-1", "1-2", "2-1"], true)
        .await
        .expect("disc-pause fixture");
    let first = fixture.track_ids[0].clone();
    fixture.play_track_and_wait(&first).await;

    let mut edit = fixture
        .library_manager
        .release_edit_seed(&fixture.release_id)
        .await
        .expect("read the release's edit form")
        .edit;
    assert_eq!(edit.tracks[1].side, Some(1), "the second track starts on disc 1");
    edit.tracks[1].side = Some(2);
    fixture
        .library_manager
        .apply_release_metadata_user_edit(&fixture.release_id, &edit.shape().expect("the edit shapes"))
        .await
        .expect("apply the edit");
    // A seek would prepare the next track again and read its disc afresh, so
    // the track plays out to its end.
    fixture
        .wait_for_side_pause("1", PlaybackPauseBoundary::Disc)
        .await;
    fixture.playback_handle.shutdown().await;
}

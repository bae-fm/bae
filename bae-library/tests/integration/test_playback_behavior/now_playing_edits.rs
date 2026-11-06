use crate::test_playback_behavior::support::*;

/// `fixture`'s playback playing its first track with the queue's context
/// holding the rest of the release, and that release's id.
async fn playing_first_track(fixture: &PlaybackTestFixture) -> String {
    let release_id = fixture
        .library
        .get_track_display(&fixture.track_ids[0])
        .await
        .expect("read the first track's release")
        .release_id;
    let mut progress = fixture.playback_handle.subscribe_progress();
    let played = fixture
        .playback_handle
        .play_release(release_id.clone(), Some(fixture.track_ids[0].clone()), false)
        .await
        .expect("the play is answered");
    assert_eq!(played, bae_playback::PlayOutcome::Playing);
    bae_playback::test_capture::wait_until_playing(&mut progress, &fixture.track_ids[0]).await;
    release_id
}

/// `fixture`'s playback paused on its first track with the queue's context
/// holding the rest of the release, and that release's id.
async fn paused_on_first_track(fixture: &PlaybackTestFixture) -> String {
    let release_id = playing_first_track(fixture).await;
    fixture.playback_handle.pause();
    release_id
}

/// What the now-playing value shows for `track_id`, once a value shows it
/// paused and `accept` takes what it shows. The system media controls are
/// handed the same value, so they must show it too.
async fn now_playing_until(
    values: &bae_store::live_query::LiveValues<Option<bae_playback::NowPlayingValues>>,
    track_id: &str,
    what: &str,
    accept: impl Fn(&bae_playback::TrackDisplay) -> bool,
) -> bae_playback::TrackDisplay {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let Some(value) = values.next().await.expect("the playback values resolve") else {
                continue;
            };
            let media_control = value.playback.media_control();
            let PlaybackState::Paused { track, .. } = value.playback.state else {
                continue;
            };
            if track.track.track_id != track_id || !accept(&track.display) {
                continue;
            }
            assert_eq!(
                media_control,
                bae_playback::MediaControlPlayback::Library,
                "the media controls show the library's paused track"
            );
            return track.display;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the now-playing value never showed {what}"))
}

/// The queue's first upcoming item once `accept` takes it.
async fn first_upcoming_until(
    queue: &bae_library::LiveQueueUpcoming,
    accept: impl Fn(&bae_playback::QueueItem) -> bool,
) -> bae_playback::QueueItem {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let value = queue.next().await.expect("the queue's upcoming read resolves");
            if let Some(item) = value
                .windows
                .into_iter()
                .flat_map(|window| window.rows)
                .next()
                .filter(|item| accept(item))
            {
                return item;
            }
        }
    })
    .await
    .expect("the queue shows the edit")
}

/// The queue's upcoming read over its first track.
fn first_upcoming(library: &TestLibrary) -> bae_library::LiveQueueUpcoming {
    let queue = bae_library::subscribe_queue_upcoming(library);
    queue
        .set(bae_store::LibraryPageWindows::from([bae_store::LibraryPageWindow {
            offset: 0,
            limit: 1,
        }]))
        .expect("the read takes its window");
    queue
}

/// Changing the cover of the release the paused track is on shows the new
/// cover for that track in the now-playing value and on the queue's rows,
/// with no track change.
#[tokio::test(flavor = "multi_thread")]
async fn a_changed_cover_reaches_the_paused_track_and_the_queue() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        let release_id = paused_on_first_track(fixture).await;
        let track_id = fixture.track_ids[0].clone();
        let values = playback_values(fixture);
        let queue = first_upcoming(&fixture.library);
        let before =
            now_playing_until(&values, &track_id, "the paused track", |_| true).await.cover_image;
        first_upcoming_until(&queue, |item| item.display.cover_image == before).await;

        let image_dir = TempDir::new().unwrap();
        let image_path = image_dir.path().join("front.gif");
        let image_bytes = bae_util::test_cover_art::SOLID_GIF.to_vec();
        std::fs::write(&image_path, &image_bytes).unwrap();
        let file = bae_records::NewReleaseFile::new(
            &release_id,
            "front.gif",
            bae_util::content_type::ContentType::Gif,
            uuid::Uuid::new_v4().to_string(),
            chrono::Utc::now(),
        );
        fixture.library.add_external_file_for_test(&file, &image_path).await.unwrap();
        fixture
            .library
            .change_cover(
                &release_id,
                bae_library::CoverSelection::ReleaseImage { file_id: file.id },
            )
            .await
            .unwrap();

        let changed = |cover: &Option<bae_records::ImageRef>| {
            cover.as_ref().is_some_and(|cover| {
                cover.id == release_id
                    && before.as_ref().is_none_or(|before| before.version != cover.version)
            })
        };
        let after = now_playing_until(&values, &track_id, "the new cover", |display| {
            changed(&display.cover_image)
        })
        .await
        .cover_image;
        let upcoming =
            first_upcoming_until(&queue, |item| changed(&item.display.cover_image)).await;
        assert_eq!(upcoming.display.cover_image, after, "the queue shows the same cover");
    })
    .await;
}

/// Renaming the paused track and its album shows the new names in the
/// now-playing value and on the queue's rows, with no track change.
#[tokio::test(flavor = "multi_thread")]
async fn renamed_titles_reach_the_paused_track_and_the_queue() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        let release_id = paused_on_first_track(fixture).await;
        let track_id = fixture.track_ids[0].clone();
        let values = playback_values(fixture);
        let queue = first_upcoming(&fixture.library);
        now_playing_until(&values, &track_id, "the paused track", |_| true).await;
        first_upcoming_until(&queue, |_| true).await;

        let mut edit = fixture
            .library
            .open_release_edit(&release_id)
            .await
            .expect("read the release's edit form")
            .opened()
            .clone()
            .edit;
        edit.album_title = "Renamed Album".to_string();
        edit.tracks[0].title = "Renamed Opening Track".to_string();
        edit.tracks[1].title = "Renamed Second Track".to_string();
        fixture
            .library
            .apply_release_metadata_user_edit(&release_id, &edit.shape().expect("the edit shapes"))
            .await
            .expect("apply the edit");

        now_playing_until(&values, &track_id, "the new names", |display| {
            display.title == "Renamed Opening Track" && display.album_title == "Renamed Album"
        })
        .await;
        first_upcoming_until(&queue, |item| {
            item.display.title == "Renamed Second Track"
                && item.display.album_title == "Renamed Album"
        })
        .await;
    })
    .await;
}

/// While a track plays its positions move, and no playback value follows
/// them: a value is sent only when one changes.
#[tokio::test(flavor = "multi_thread")]
async fn a_playing_track_moves_its_positions_without_sending_a_value() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        playing_first_track(fixture).await;
        let track_id = fixture.track_ids[0].clone();
        let values = playback_values(fixture);
        let mut positions = fixture.playback_handle.subscribe_positions();
        let value = tokio::time::timeout(HANG_GUARD, values.next())
            .await
            .expect("the playing value arrives")
            .expect("the playback values resolve")
            .expect("the library holds the playing track");
        let PlaybackState::Playing { track } = &value.playback.state else {
            panic!("the first value shows the track playing, not {:?}", value.playback.state);
        };
        assert_eq!(track.track.track_id, track_id);

        let mut seen = Vec::new();
        while seen.len() < 3 {
            // The positions take each report before the progress feed shows it.
            play_report(&fixture.playback_handle, &fixture.audio_clock, &mut fixture.progress_rx)
                .await;
            let position = positions.positions().library;
            if let Some(position) = position.filter(|position| position.track_id == track_id) {
                seen.push(position.position_ms);
            }
        }
        assert!(
            seen.windows(2).all(|pair| pair[0] < pair[1]),
            "the position moves forward: {seen:?}"
        );
        // A volume change commits after those positions, so its value comes
        // after any value they would have sent: the next value must be it,
        // the first value with only the volume changed.
        fixture.playback_handle.set_volume(SENTINEL_VOLUME);
        let next = tokio::time::timeout(HANG_GUARD, values.next())
            .await
            .expect("the volume's value arrives")
            .expect("the playback values resolve")
            .expect("the library holds the playing track");
        let mut expected = value.clone();
        expected.playback.volume = SENTINEL_VOLUME;
        assert_eq!(next, expected, "no playback value follows the positions");
    })
    .await;
}

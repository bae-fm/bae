/// Services over `fixture`'s library and playback, paused on its first track
/// with the queue's context holding the rest of the release.
async fn paused_on_first_track(
    fixture: &PlaybackTestFixture,
) -> (bae_core::library::AppServices, String) {
    let import = fixture
        .library_manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .expect("start the import service");
    let services = bae_core::library::AppServices::new(
        fixture.library_manager.clone(),
        fixture.playback_handle.clone(),
        import,
    );
    let release_id = fixture
        .library_manager
        .get_play_context(&fixture.track_ids[0])
        .await
        .expect("read the first track's release")
        .release_id;
    let mut progress = services.subscribe_playback_progress();
    services.playback_play_release(
        release_id.clone(),
        Some(fixture.track_ids[0].clone()),
        false,
    );
    assert!(
        support::wait_until_playing(&mut progress, &fixture.track_ids[0], PLAY_START_BACKSTOP)
            .await,
        "the first track plays"
    );
    services.playback_pause();
    (services, release_id)
}

/// What the now-playing value shows for `track_id`, once a value shows it
/// paused and `accept` takes what it shows. The system media controls are
/// handed the same value, so they must show it too.
async fn now_playing_until(
    values: &mut tokio::sync::mpsc::UnboundedReceiver<
        Result<bae_core::playback::NowPlayingValues, bae_core::library::LibraryError>,
    >,
    track_id: &str,
    what: &str,
    accept: impl Fn(&bae_core::playback::TrackDisplay) -> bool,
) -> bae_core::playback::TrackDisplay {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let value = values
                .recv()
                .await
                .expect("the playback values stay open")
                .expect("the playback values resolve");
            let media_control = value.media_control_values().playback;
            let PlaybackState::Paused { track, .. } = value.state else {
                continue;
            };
            if track.track.track_id != track_id || !accept(&track.display) {
                continue;
            }
            match media_control {
                bae_core::playback::MediaControlPlayback::Library {
                    state: PlaybackState::Paused { track: shown, .. },
                    ..
                } => assert_eq!(shown, track, "the media controls show the same track"),
                other => panic!("the media controls show the library's paused track, not {other:?}"),
            }
            return track.display;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the now-playing value never showed {what}"))
}

/// The queue's first upcoming item once `accept` takes it.
async fn first_upcoming_until(
    queue: &mut tokio::sync::mpsc::UnboundedReceiver<
        Result<bae_core::queue::ResolvedQueueSnapshot, bae_core::library::LibraryError>,
    >,
    accept: impl Fn(&bae_core::queue::QueueItem) -> bool,
) -> bae_core::queue::QueueItem {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let value = queue
                .recv()
                .await
                .expect("the queue subscription stays open")
                .expect("the queue resolves");
            if let Some(item) = value
                .context
                .and_then(|context| context.upcoming.into_iter().next())
                .filter(|item| accept(item))
            {
                return item;
            }
        }
    })
    .await
    .expect("the queue shows the edit")
}

/// Changing the cover of the release the paused track is on shows the new
/// cover for that track in the now-playing value and on the queue's rows,
/// with no track change.
#[tokio::test(flavor = "multi_thread")]
async fn a_changed_cover_reaches_the_paused_track_and_the_queue() {
    let fixture = PlaybackTestFixture::new().await;
    let (services, release_id) = paused_on_first_track(&fixture).await;
    let track_id = fixture.track_ids[0].clone();
    let mut values = services.subscribe_playback_values(&tokio::runtime::Handle::current());
    let mut queue = services.subscribe_queue_values(&tokio::runtime::Handle::current());
    let before = now_playing_until(&mut values, &track_id, "the paused track", |_| true)
        .await
        .cover_image;
    first_upcoming_until(&mut queue, |item| item.display.cover_image == before).await;

    let image_dir = TempDir::new().unwrap();
    let image_path = image_dir.path().join("front.gif");
    let image_bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/test-fixtures/cover-art/solid.gif"
    ))
    .unwrap();
    std::fs::write(&image_path, &image_bytes).unwrap();
    let file = bae_core::db::DbFile::new(
        &release_id,
        "front.gif",
        image_bytes.len() as i64,
        bae_core::util::content_type::ContentType::Gif,
        uuid::Uuid::new_v4().to_string(),
        chrono::Utc::now(),
    );
    fixture
        .library_manager
        .add_external_file_for_test(&file, &image_path)
        .await
        .unwrap();
    services
        .change_cover(
            &release_id,
            bae_core::library::CoverSelection::ReleaseImage { file_id: file.id },
        )
        .await
        .unwrap();

    let changed = |cover: &Option<bae_core::album_detail::ImageRef>| {
        cover.as_ref().is_some_and(|cover| {
            cover.id == release_id
                && before
                    .as_ref()
                    .is_none_or(|before| before.version != cover.version)
        })
    };
    let after = now_playing_until(&mut values, &track_id, "the new cover", |display| changed(&display.cover_image))
        .await
        .cover_image;
    let upcoming = first_upcoming_until(&mut queue, |item| changed(&item.display.cover_image)).await;
    assert_eq!(upcoming.display.cover_image, after, "the queue shows the same cover");
}

/// Renaming the paused track and its album shows the new names in the
/// now-playing value and on the queue's rows, with no track change.
#[tokio::test(flavor = "multi_thread")]
async fn renamed_titles_reach_the_paused_track_and_the_queue() {
    let fixture = PlaybackTestFixture::new().await;
    let (services, release_id) = paused_on_first_track(&fixture).await;
    let track_id = fixture.track_ids[0].clone();
    let mut values = services.subscribe_playback_values(&tokio::runtime::Handle::current());
    let mut queue = services.subscribe_queue_values(&tokio::runtime::Handle::current());
    now_playing_until(&mut values, &track_id, "the paused track", |_| true).await;
    first_upcoming_until(&mut queue, |_| true).await;

    let mut edit = services
        .release_edit_seed(&release_id)
        .await
        .expect("read the release's edit form")
        .edit;
    edit.album_title = "Renamed Album".to_string();
    edit.tracks[0].title = "Renamed Opening Track".to_string();
    edit.tracks[1].title = "Renamed Second Track".to_string();
    services
        .apply_release_metadata_user_edit(&release_id, &edit.shape().expect("the edit shapes"))
        .await
        .expect("apply the edit");

    now_playing_until(&mut values, &track_id, "the new names", |display| {
        display.title == "Renamed Opening Track" && display.album_title == "Renamed Album"
    })
    .await;
    first_upcoming_until(&mut queue, |item| {
        item.display.title == "Renamed Second Track" && item.display.album_title == "Renamed Album"
    })
    .await;
}

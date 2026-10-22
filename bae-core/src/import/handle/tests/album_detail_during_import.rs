//! Opening an album while another release imports. An import reads its files
//! from a watched folder, which is often a network share that also holds the
//! library's other originals; while it reads, that volume answers late. What the
//! database holds for an album never waits on the volume: the album shows at
//! once, and whether this device still holds its originals follows when the
//! volume answers.

use super::*;

/// A candidate ready to import into an album of its own.
async fn importable(handle: &ImportServiceHandle, key: &str) {
    handle
        .select_candidate_file_tags(key.to_string())
        .await
        .unwrap();
    handle
        .set_candidate_album_artists(key, vec![crate::import::Credited::plain(crate::import::ArtistAssignment::named("Artist"))])
        .await
        .unwrap();
}

/// The import's measuring thread, held by [`ImportEventBus::hold_progress_at`]
/// and let go on drop, so a test that fails first does not leave the import
/// worker waiting while the services it runs in are torn down.
struct HeldImport<'a>(&'a ImportEventBus);

impl<'a> HeldImport<'a> {
    fn at(bus: &'a ImportEventBus, phase: crate::import::ImportPhase) -> Self {
        bus.hold_progress_at(phase);
        Self(bus)
    }
}

impl Drop for HeldImport<'_> {
    fn drop(&mut self) {
        self.0.release_send();
    }
}

/// The next value the detail read delivers for `album_id`.
async fn next_for(
    values: &crate::library::DetailSubscription<crate::album_detail::AlbumDetail>,
    album_id: &str,
    waiting_for: &str,
) -> crate::album_detail::AlbumDetail {
    loop {
        let snapshot = tokio::time::timeout(HANG_GUARD, values.next())
            .await
            .unwrap_or_else(|_| panic!("{waiting_for}"))
            .expect("the album detail resolves");
        if snapshot.id.as_deref() == Some(album_id) {
            return snapshot.value.expect("the album is present");
        }
    }
}

/// The import is held partway through measuring its tracks, and the volume
/// the shown album's originals sit on is held with it. The album's detail
/// arrives anyway, its originals not yet answered for; once the volume
/// answers, it says the release plays here.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_album_opened_while_another_release_imports_shows_at_once() {
    let (manager, tmp) = setup_test_manager().await;
    let other = TempDir::new().unwrap();
    let (_, shown_key, _) = picked_candidate(&manager, &tmp, "Shown Album").await;
    let (_, importing_key, _) = picked_candidate(&manager, &other, "Importing Album").await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    let playback = manager.start_playback_service_with_audio_device(
        tokio::runtime::Handle::current(),
        50,
        false,
        // No hardware behind it: this test never plays anything.
        Box::new(crate::playback::audio_output::FailingAudioDevice),
    );
    let services = crate::library::AppServices::new(manager.clone(), playback, handle.clone());
    importable(&handle, &shown_key).await;
    importable(&handle, &importing_key).await;
    let mut events = handle.every_event();
    let shown_import = handle.start_import(&shown_key).await.unwrap();
    let (_, shown_album) = await_import_outcome(&mut events, &shown_import)
        .await
        .expect("the shown album imports");

    let import = HeldImport::at(
        &handle.event_tx,
        crate::import::ImportPhase::MeasuringLoudness,
    );
    let volume = manager.hold_disk();
    let importing = handle.start_import(&importing_key).await.unwrap();
    tokio::time::timeout(HANG_GUARD, handle.event_tx.send_held())
        .await
        .expect("the import reaches its measurement");

    let values = services.subscribe_album_detail(&tokio::runtime::Handle::current());
    values.set(Some(shown_album.clone())).unwrap();
    let shown = next_for(
        &values,
        &shown_album,
        "the album detail arrives while the import holds the volume",
    )
    .await;
    assert_eq!(shown.album.id, shown_album);
    assert_eq!(
        shown.releases[0].playability,
        crate::album_detail::ReleasePlayability::CheckingOriginals,
        "nothing has looked at the originals while the volume is held"
    );

    drop(volume);
    let looked = next_for(
        &values,
        &shown_album,
        "the album detail follows the volume's answer",
    )
    .await;
    assert_eq!(
        looked.releases[0].playability,
        crate::album_detail::ReleasePlayability::Playable,
        "the originals are where the import recorded them"
    );
    drop(import);
    await_import_outcome(&mut events, &importing)
        .await
        .expect("the held import finishes");

    // Dropping `services` joins the playback and import threads, so it happens
    // on a blocking thread.
    drop(values);
    tokio::task::spawn_blocking(move || drop(services))
        .await
        .unwrap();
    shut_down(handle).await;
}

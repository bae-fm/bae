//! Starting an import: the claim one import holds on a candidate, the checks
//! the start makes of its files and tags, what the import commits from the
//! draft, the artist pictures the import that creates those artists fetches
//! before it writes anything, and an automatic import that cannot start
//! recorded as its failed import.

use super::*;
use crate::handle::tests::*;
use crate::identification::tests::*;
use bae_catalogs::fake_catalogs::FakeCatalogs;
use bae_catalogs::test_releases::seed_mb_release_with_works;
use bae_catalogs::test_releases::seed_two_track_discogs_release;
use bae_catalogs::test_releases::seed_two_track_mb_release;
use bae_catalogs::test_releases::two_track_mb_release_json;
use bae_util::test_channels::HANG_GUARD;
use tempfile::TempDir;

/// A candidate picked from a Discogs release crediting an album artist and a
/// track artist, whose artist documents Discogs has not been asked for: no
/// answer is seeded for them, so a lookup of either would fail on the test
/// transport.
struct DiscogsPick {
    handle: ImportServiceHandle,
    /// The catalog fakes the library asks, which the test seeds.
    fakes: FakeCatalogs,
    tmp: TempDir,
    key: crate::CandidateKey,
    images: bae_catalogs::test_image_host::RemoteImageHost,
    album_artist_id: String,
    track_artist_id: String,
    /// Where the track artist's picture is served on the image host.
    picture_path: String,
}

impl DiscogsPick {
    /// Discogs' artist documents: none for the album artist, and for the
    /// track artist one whose primary image is `picture_url`.
    fn seed_artists(&self, picture_url: String) {
        self.fakes.seed_discogs_artist_image(&self.album_artist_id, None);
        self.fakes.seed_discogs_artist_image(&self.track_artist_id, Some(picture_url));
    }

    async fn import(&self) -> Result<(String, String), String> {
        let mut events = self.handle.every_event();
        let import_id = self.handle.start_import(&self.key).await.unwrap().expect_queued();
        await_import_outcome(&mut events, &import_id).await
    }

    /// The library artist the release's first track credits as the track
    /// artist.
    async fn track_artist(&self, release_id: &str) -> bae_db::DbArtist {
        let first_track = self
            .handle
            .database
            .get_tracks_for_release(release_id)
            .await
            .unwrap()
            .into_iter()
            .next()
            .expect("the imported release has its first track");
        self.handle
            .database
            .get_artists_for_track(&first_track.id)
            .await
            .unwrap()
            .into_iter()
            .find(|artist| {
                artist.artist.discogs_artist_id.as_deref() == Some(&self.track_artist_id)
            })
            .expect("the imported track credits the Discogs track artist")
            .artist
    }

    /// The bytes of the picture the library holds for `artist_id`, if any.
    async fn picture_of(&self, artist_id: &str) -> Option<Vec<u8>> {
        let database = &self.handle.database;
        let file = database
            .library_image_file(artist_id, &bae_db::LibraryImageType::Artist)
            .await
            .unwrap()?;
        Some(database.read_file(&file).await.unwrap())
    }
}

/// A picture larger than bae stores, as a PNG: 1200 by 800 pixels.
fn large_picture() -> Vec<u8> {
    let image = image::RgbImage::from_pixel(1200, 800, image::Rgb([90, 30, 160]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("the picture encodes");
    bytes.into_inner()
}

/// That `stored` is [`large_picture`] as bae stores it: a JPEG downscaled to
/// 600 pixels on its longer side.
fn assert_stored_large_picture(stored: Option<Vec<u8>>) {
    let stored = stored.expect("the imported artist has a picture");
    assert_eq!(
        image::guess_format(&stored).expect("the stored picture has a format"),
        image::ImageFormat::Jpeg,
        "the stored picture is a JPEG"
    );
    let decoded = image::load_from_memory(&stored).expect("the stored picture decodes");
    assert_eq!(
        (decoded.width(), decoded.height()),
        (600, 400),
        "the stored picture is downscaled to 600 pixels on its longer side"
    );
}

async fn picked_discogs_release() -> DiscogsPick {
    let images = bae_catalogs::test_image_host::RemoteImageHost::start().await;
    let fakes = FakeCatalogs::over(
        bae_util::http::Http::for_test()
            .serve(bae_catalogs::test_image_host::TEST_IMAGE_HOST, images.origin()),
    );
    let StoredCandidate { handle, key, tmp, .. } =
        picked_stored_candidate_with(fakes.providers()).await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();

    let numeric_id = || (uuid::Uuid::new_v4().as_u128() % u128::from(u64::MAX)) as u64;
    let album_artist_id = numeric_id();
    let track_artist_id = numeric_id();
    let source_release_id = numeric_id().to_string();
    let raw_release = serde_json::json!({
        "id": source_release_id.parse::<u64>().unwrap(),
        "title": "Album Title",
        "year": 2020,
        "formats": [{ "name": "CD" }],
        "artists": [{ "id": album_artist_id, "name": "Album Artist" }],
        "tracklist": [
            {
                "position": "1",
                "title": "Track One",
                "duration": "0:01",
                "artists": [{ "id": track_artist_id, "name": "Track Artist" }],
            },
            { "position": "2", "title": "Track Two", "duration": "0:01" },
        ],
    })
    .to_string();
    bae_catalogs::discogs::documents::parse_discogs_release_json(&raw_release)
        .expect("the rendered Discogs release parses");
    fakes.seed_discogs_release(&source_release_id, raw_release);
    fakes.seed_discogs_page_lookup(&[(
        bae_catalogs::musicbrainz::DiscogsPage::Release(source_release_id.clone()),
        None,
    )]);

    handle
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    source_release_id,
                ),
                partners: vec![],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .expect("the pick asks Discogs for no artist");

    DiscogsPick {
        handle,
        fakes,
        tmp,
        key,
        images,
        album_artist_id: album_artist_id.to_string(),
        track_artist_id: track_artist_id.to_string(),
        picture_path: format!("/discogs-artist-{}.png", uuid::Uuid::new_v4()),
    }
}

/// The pick asks for no picture; the import fetches the new track artist's
/// once and stores it as bae stores every image, a JPEG at most 600 pixels
/// on its longer side, whatever Discogs served.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_fetches_no_artist_picture_and_the_import_stores_each_as_a_600_pixel_jpeg() {
    let pick = picked_discogs_release().await;
    let picture_url = pick.images.serve_image(&pick.picture_path, large_picture());
    assert_eq!(pick.images.requests(&pick.picture_path), 0, "the pick downloads no artist picture");

    pick.seed_artists(picture_url);
    let (release_id, _album_id) =
        pick.import().await.unwrap_or_else(|error| panic!("import failed: {error}"));

    assert_eq!(
        pick.images.requests(&pick.picture_path),
        1,
        "the import downloads the track artist's picture once"
    );
    let artist = pick.track_artist(&release_id).await;
    assert_stored_large_picture(pick.picture_of(&artist.id).await);
    shut_down(pick.handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_artist_the_library_already_has_is_not_fetched_again() {
    let pick = picked_discogs_release().await;
    let existing = make_artist("Track Artist", Some(&pick.track_artist_id), None);
    pick.handle.database.insert_artist(&existing).await.unwrap();
    let picture_url =
        pick.images.serve_image(&pick.picture_path, bae_catalogs::test_image_host::cover_png());

    pick.seed_artists(picture_url);
    let (release_id, _album_id) =
        pick.import().await.unwrap_or_else(|error| panic!("import failed: {error}"));

    assert_eq!(pick.track_artist(&release_id).await.id, existing.id);
    assert_eq!(
        pick.images.requests(&pick.picture_path),
        0,
        "an artist the import links to keeps its own picture, so none is fetched"
    );
    shut_down(pick.handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_picture_that_fails_to_download_fails_the_import_before_any_row_and_a_retry_imports() {
    let pick = picked_discogs_release().await;
    let picture_url = pick.images.fail_image(&pick.picture_path, 500);
    pick.seed_artists(picture_url);

    pick.import().await.expect_err("a picture that fails to download fails the import");
    let failed_requests = pick.images.requests(&pick.picture_path);
    assert!(failed_requests > 0, "the failed import asked for the picture");
    let rows = pick.handle.database.every_row_for_test().await.unwrap();
    for table in ["releases", "albums", "artists", "artist_images", "release_files"] {
        assert!(
            rows.get(table).is_none_or(Vec::is_empty),
            "the failed import wrote nothing to {table}"
        );
    }

    pick.images.serve_image(&pick.picture_path, large_picture());
    let (release_id, _album_id) =
        pick.import().await.unwrap_or_else(|error| panic!("the retry failed: {error}"));

    assert_eq!(
        pick.images.requests(&pick.picture_path),
        failed_requests + 1,
        "the retry fetches the picture again"
    );
    let artist = pick.track_artist(&release_id).await;
    assert_stored_large_picture(pick.picture_of(&artist.id).await);
    shut_down(pick.handle).await;
}

/// The step `event` reports for `import_id`, if it reports one.
fn step_of(event: ImportEvent, import_id: &str) -> Option<crate::ImportStep> {
    let ImportEvent::ImportProgress { progress, .. } = event else {
        return None;
    };
    if progress.import_id() != import_id {
        return None;
    }
    match progress {
        crate::ImportProgress::Preparing { step, .. } => Some(crate::ImportStep::Preparing(step)),
        crate::ImportProgress::Progress { phase, .. } => Some(crate::ImportStep::Running(phase)),
        crate::ImportProgress::Complete { .. }
        | crate::ImportProgress::RemoteUploadQueued { .. }
        | crate::ImportProgress::Failed { .. }
        | crate::ImportProgress::Cancelled { .. } => None,
    }
}

/// While the import waits on a new artist's picture, the candidate says it
/// is fetching artist pictures: the step is reported before the picture is
/// asked for, and no file is read until it answers.
#[tokio::test(flavor = "multi_thread")]
async fn the_import_says_it_is_fetching_artist_pictures_while_it_fetches_them() {
    use crate::{ImportStep, PrepareStep};
    let pick = picked_discogs_release().await;
    let picture_url = pick.images.serve_image(&pick.picture_path, large_picture());
    let hold = pick.images.hold_image(&pick.picture_path);
    pick.seed_artists(picture_url);

    let mut events = pick.handle.every_event();
    let import_id = pick.handle.start_import(&pick.key).await.unwrap().expect_queued();
    tokio::time::timeout(HANG_GUARD, hold.arrived())
        .await
        .expect("the import asks for the track artist's picture");

    let fetching = ImportStep::Preparing(PrepareStep::FetchingArtistPictures);
    let steps: Vec<_> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| step_of(event, &import_id))
        .collect();
    assert_eq!(
        steps,
        vec![ImportStep::Preparing(PrepareStep::ValidatingSourceFiles), fetching,],
        "the fetch is reported before the picture is asked for, and nothing is read yet"
    );
    assert_eq!(
        pick.handle
            .candidate_runtime(&pick.key)
            .await
            .unwrap()
            .and_then(|runtime| runtime.import)
            .map(|import| import.step),
        Some(fetching),
        "the candidate shows the fetch while it waits on the picture"
    );

    hold.release();
    let (release_id, _album_id) = await_import_outcome(&mut events, &import_id)
        .await
        .unwrap_or_else(|error| panic!("import failed: {error}"));
    let artist = pick.track_artist(&release_id).await;
    assert_stored_large_picture(pick.picture_of(&artist.id).await);
    shut_down(pick.handle).await;
}

/// A file the scan read that changes before the import stages it fails the
/// import before any row: the release's rows describe the file the scan
/// read, and coven would record the one it finds. The files are checked
/// against the scan before the artist's picture is asked for, and staged
/// after it answers, so a change while the picture is held reaches only the
/// staging.
#[tokio::test(flavor = "multi_thread")]
async fn a_file_changed_after_its_scan_fails_the_import_before_any_row() {
    let pick = picked_discogs_release().await;
    let picture_url = pick.images.serve_image(&pick.picture_path, large_picture());
    let hold = pick.images.hold_image(&pick.picture_path);
    pick.seed_artists(picture_url);

    let mut events = pick.handle.every_event();
    let import_id = pick.handle.start_import(&pick.key).await.unwrap().expect_queued();
    tokio::time::timeout(HANG_GUARD, hold.arrived())
        .await
        .expect("the import asks for the track artist's picture");
    let changed = pick.tmp.path().join("watched/Album/02 Track.flac");
    std::fs::File::options()
        .write(true)
        .open(&changed)
        .and_then(|file| file.set_modified(std::time::SystemTime::now()))
        .expect("set the file's modification time");
    hold.release();

    let error = await_import_outcome(&mut events, &import_id)
        .await
        .expect_err("a file changed since its scan fails the import");
    assert!(error.contains("changed after its import candidate was scanned"), "{error}");
    let rows = pick.handle.database.every_row_for_test().await.unwrap();
    for table in ["releases", "release_files"] {
        assert!(
            rows.get(table).is_none_or(Vec::is_empty),
            "the failed import wrote nothing to {table}"
        );
    }
    shut_down(pick.handle).await;
}

/// What the import writes is the draft: storing the release again after it
/// was applied, its tracklist and credits changed by its editors, changes
/// nothing the candidate imports (B2).
#[tokio::test(flavor = "multi_thread")]
async fn a_release_stored_again_after_apply_changes_nothing_the_draft_imports() {
    let fakes = FakeCatalogs::offline();
    let StoredCandidate { handle, candidate, key, tmp: _tmp, .. } =
        stored_candidate_with(fakes.providers()).await;
    let tables = handle.tables.clone();
    seed_mb_release_with_works(
        &fakes,
        "edited-release",
        "edited-release",
        &["First Track", "Second Track"],
        &[1_000, 1_000],
    );
    pick_mb(&handle, &key, "edited-release").await;
    let hash = candidate.files.content_hash();
    let applied = stored_draft(&handle, &hash).await;

    // The catalog's editors retitle the tracks and credit other composers;
    // the release is fetched and stored again over the copy applied.
    seed_mb_release_with_works(
        &fakes,
        "edited-release",
        "changed",
        &["Changed One", "Changed Two"],
        &[1_000, 1_000],
    );
    let edited = bae_catalogs::records::LookupRef::new(
        bae_catalogs::catalog::LookupCatalog::MusicBrainz,
        "edited-release",
    );
    let payloads = handle
        .catalogs
        .release_read_session(&edited)
        .await
        .unwrap()
        .fetch_payloads(
            &edited,
            bae_catalogs::payloads::ReadDepth::Complete,
            bae_catalogs::rate_limiter::CallPriority::Interactive,
        )
        .await
        .unwrap();
    tables.save_source_release(&payloads.extract().unwrap()).await.unwrap();
    assert_eq!(stored_draft(&handle, &hash).await, applied);

    let mut events = handle.every_event();
    let import_id = handle.start_import(&key).await.unwrap().expect_queued();
    let (release_id, _album_id) =
        await_import_outcome(&mut events, &import_id).await.expect("the import completes");
    let written = handle.database.track_sources_for_test(&release_id).await.unwrap();
    shut_down(handle).await;

    assert_eq!(
        written,
        [
            (
                "First Track".to_string(),
                vec!["edited-release-composer-1".to_string()],
                vec!["edited-release-work-1".to_string()],
            ),
            (
                "Second Track".to_string(),
                vec!["edited-release-composer-2".to_string()],
                vec!["edited-release-work-2".to_string()],
            ),
        ]
    );
}

/// A draft read from the files' tags, and a draft typed over a blank one,
/// import exactly as the draft stands: nothing is read from the tags again,
/// and nothing a blank release would say fills a field (B14).
#[tokio::test(flavor = "multi_thread")]
async fn a_tags_draft_and_a_blank_draft_import_exactly_the_draft() {
    for from_tags in [true, false] {
        let StoredCandidate { handle, key, tmp: _tmp, .. } = stored_candidate().await;
        if from_tags {
            handle.select_candidate_file_tags(key.clone()).await.unwrap();
        } else {
            handle.clear_candidate_metadata(key.clone()).await.unwrap();
        }
        handle
            .set_candidate_album_artists(
                &key,
                vec![crate::Credited::plain(crate::ArtistAssignment::named("Typed Artist"))],
            )
            .await
            .unwrap();
        handle
            .set_candidate_edit_field(
                &key,
                crate::DraftFieldEdit::Text {
                    field: crate::CandidateEditField::AlbumTitle,
                    value: "Typed Album".to_string(),
                },
            )
            .await
            .unwrap();
        let draft = pane(&handle, &key).await.draft().unwrap().metadata_draft.clone();
        let mut track = draft.tracks[1].clone();
        track.title = "Typed Track".to_string();
        handle.set_candidate_track_edit(&key, track).await.unwrap();
        let draft = pane(&handle, &key).await.draft().unwrap().metadata_draft.clone();

        let mut events = handle.every_event();
        let import_id = handle.start_import(&key).await.unwrap().expect_queued();
        let (release_id, album_id) =
            await_import_outcome(&mut events, &import_id).await.expect("the import completes");
        let tracks = handle.database.get_tracks_for_release(&release_id).await.unwrap();
        let album = handle
            .database
            .find_album_by_id(&album_id)
            .await
            .unwrap()
            .expect("the album is written");
        shut_down(handle).await;

        assert_eq!(album.title, "Typed Album", "from tags: {from_tags}");
        // A track left untitled is titled after its file when its audio is
        // bound; every other value is the draft's.
        assert_eq!(
            tracks
                .iter()
                .map(|track| (track.title.clone(), track.track_number.get()))
                .collect::<Vec<_>>(),
            draft
                .tracks
                .iter()
                .zip(["01 Track", "02 Track"])
                .zip(1..)
                .map(|((track, file), number)| {
                    let title = if track.title.is_empty() { file } else { &track.title };
                    (title.to_string(), number)
                })
                .collect::<Vec<_>>(),
            "from tags: {from_tags}"
        );
        assert_eq!(tracks[1].title, "Typed Track");
    }
}

/// An Import whose claim cannot be written is refused with the failure: no
/// import owns the candidate, none is queued, and the candidate reads as it
/// did before it was asked for.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_whose_claim_cannot_be_written_fails_and_claims_nothing() {
    let (handle, tmp, key, _hash) = pane_fixture().await;
    let before = handle.candidate_runtime(&key).await.unwrap();
    let mut events = handle.every_event();
    crate::candidate_runtime::tests::fail_standing_writes(
        tmp.path(),
        &[("INSERT", "NEW.import_step = 'queued'"), ("UPDATE", "NEW.import_step = 'queued'")],
    )
    .await;

    let error = handle.start_import(&key).await.unwrap_err();

    assert!(error.to_string().contains("the disk is full"), "{error}");
    assert_eq!(handle.candidate_runtime(&key).await.unwrap(), before);
    assert_eq!(handle.standing_facts_for_test(&key).await, Default::default());
    assert!(
        !std::iter::from_fn(|| events.try_recv().ok())
            .any(|event| matches!(event, ImportEvent::ImportProgress { .. })),
        "no import was queued"
    );
    shut_down(handle).await;
}

/// Every release in the library.
async fn release_count(handle: &ImportServiceHandle) -> usize {
    bae_db::test_fixtures::row_count(&handle.database, "releases").await
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_imports_keep_one_owner_and_later_imports_of_its_files_are_refused() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    handle
        .set_candidate_album_artists(
            &key,
            vec![crate::Credited::plain(crate::ArtistAssignment::named("Artist Name"))],
        )
        .await
        .unwrap();
    let mut events = handle.every_event();

    let presses = (0..5).map(|_| {
        let handle = handle.clone();
        let key = key.clone();
        tokio::spawn(async move { handle.start_import(&key).await })
    });
    let mut started = Vec::new();
    for press in presses.collect::<Vec<_>>() {
        match press.await.unwrap() {
            Ok(start) => started.push(start.expect_queued()),
            Err(crate::ImportError::CandidateImportInProgress) => {}
            Err(error) => panic!("a repeated press is refused as in progress: {error}"),
        }
    }
    assert_eq!(started.len(), 1, "exactly one press starts an import");
    let (release_id, _) = await_import_outcome(&mut events, &started[0])
        .await
        .unwrap_or_else(|error| panic!("import failed: {error}"));

    for selected in [false, true] {
        let refused = if selected {
            handle.import_selected(&key).await
        } else {
            handle.start_import(&key).await
        };
        assert!(
            matches!(
                &refused,
                Err(crate::ImportError::FilesAlreadyImported { release_id: imported })
                    if *imported == release_id
            ),
            "a later import of the same files is refused before it runs: {refused:?}"
        );
    }
    assert_eq!(release_count(&handle).await, 1);
    shut_down(handle).await;
}

/// Two folders whose files hash alike share one preparation. Both asked to
/// import before either runs, both run: the one that writes second finds the
/// hash taken and fails with the typed error, and the files make one release.
#[tokio::test(flavor = "multi_thread")]
async fn two_imports_of_the_same_files_asked_before_either_runs_make_one_release() {
    let (parts, database, tmp) = setup_test_import().await;
    let tables = crate::tables::ImportTables::new(database);
    let (original, original_key, _) = picked_candidate(&tables, &tmp, "Album").await;
    let copy = twin_of(&original, "Copy");
    let copy_key = crate::CandidateKey::folder(&copy.path);
    rescan_all_into(&tables, &[original, copy]).await;
    let handle = parts.start().await.unwrap();
    // Each folder's own tags are read; the draft they share is then given its
    // artist. Nothing changes it once an import is asked for.
    for key in [&original_key, &copy_key] {
        handle.select_candidate_file_tags(key.clone()).await.unwrap();
    }
    handle
        .set_candidate_album_artists(
            &original_key,
            vec![crate::Credited::plain(crate::ArtistAssignment::named("Artist Name"))],
        )
        .await
        .unwrap();
    let mut events = handle.every_event();

    handle.import_cancels.hold_runs();
    let first = handle.start_import(&original_key).await.unwrap().expect_queued();
    let second = handle.start_import(&copy_key).await.unwrap().expect_queued();
    handle.import_cancels.release_runs();

    let (release_id, _) = await_import_outcome(&mut events, &first)
        .await
        .unwrap_or_else(|error| panic!("the first import lands: {error}"));
    assert_eq!(
        await_import_outcome(&mut events, &second).await,
        Err(crate::ImportError::FilesAlreadyImported { release_id }.to_string()),
        "the second finds the files imported here"
    );
    assert_eq!(release_count(&handle).await, 1);
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn import_worker_refuses_a_prepared_but_invalid_metadata_draft() {
    // The scan could not read the folder's tags, so its draft is blank.
    let StoredCandidate { handle, key, tmp: _tmp, .. } = unread_stored_candidate().await;

    let mut events = handle.every_event();
    let import_id = handle
        .start_import(&key)
        .await
        .expect("the complete candidate enters source validation")
        .expect_queued();
    let error = await_import_outcome(&mut events, &import_id)
        .await
        .expect_err("the invalid draft cannot import");

    assert!(error.contains("Album title is required"), "unexpected worker error: {error}");
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn import_refuses_audio_changed_after_the_file_tags_pane_was_read() {
    let (handle, tmp, key, _hash) = pane_fixture().await;
    let tables = handle.tables.clone();
    let root = tmp.path().join("watched");
    let before = tables
        .load_candidate_file_tag_snapshot(&root, &key)
        .await
        .unwrap()
        .expect("the candidate remains stored")
        .snapshot
        .expect("the pane read stored a complete snapshot");

    let changed_audio = tmp.path().join("watched/Album/01 Track.flac");
    let audio = std::fs::OpenOptions::new().write(true).open(&changed_audio).unwrap();
    audio
        .set_times(
            std::fs::FileTimes::new()
                .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5)),
        )
        .unwrap();

    let mut events = handle.every_event();
    let import_id = handle
        .start_import(&key)
        .await
        .expect("the prepared candidate enters source validation")
        .expect_queued();
    let error = await_import_outcome(&mut events, &import_id)
        .await
        .expect_err("the changed audio cannot import");
    let after = tables
        .load_candidate_file_tag_snapshot(&root, &key)
        .await
        .unwrap()
        .expect("the candidate remains stored")
        .snapshot
        .expect("a refused import preserves the pane snapshot");
    shut_down(handle).await;

    assert_eq!(after, before, "refusing import must not replace the snapshot the pane displayed");
    assert!(
        error.contains("changed after its import candidate was scanned"),
        "source validation names the stale scan: {error}"
    );
}

/// A bulk import of a selection reaching a row identification is still
/// answering skips that row and says why, leaving it unclaimed for the person
/// to import once the run settles; a row an import already owns is refused the
/// same way rather than claimed twice.
#[tokio::test(flavor = "multi_thread")]
async fn a_bulk_import_skips_a_row_being_identified_and_says_so() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    admit(&handle, &key).await;

    let refused = handle.import_selected(&key).await;

    assert!(matches!(refused, Err(crate::ImportError::CandidateBeingIdentified)));
    let runtime =
        handle.candidate_runtime(&key).await.unwrap().expect("the queued run is still the key's");
    assert!(runtime.import.is_none(), "the skipped row is not claimed");
    assert!(
        matches!(runtime.identification, Some(crate::IdentificationRuntime::Queued(_))),
        "and its run is not cancelled"
    );

    handle.withdraw_identification(&key).await.unwrap();
    handle.claim_candidate_for_import(&key, "import-1").await;
    assert!(matches!(
        handle.import_selected(&key).await,
        Err(crate::ImportError::CandidateImportInProgress)
    ));
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn numeric_vinyl_import_preserves_unknown_sides_and_track_order() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, hash) = pane_fixture_with(fakes.providers()).await;
    let release_id = "numeric-vinyl-release";
    fakes
        .seed_musicbrainz_release(release_id, serde_json::json!({
        "id": release_id, "title": "Numbered Record",
        "artist-credit": [{ "name": "Record Artist", "artist": { "id": "record-artist", "name": "Record Artist" } }],
        "media": [{ "format": "12\" Vinyl", "tracks": [
            { "position": 1, "number": "9", "recording": { "title": "First Listed" } },
            { "position": 2, "number": "3", "recording": { "title": "Second Listed" } }
        ] }],
        "cover-art-archive": { "front": false, "darkened": false }
    }).to_string());
    handle
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    release_id,
                ),
                partners: vec![],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    let draft =
        crate::preparation::import_preparation(&handle.tables, &hash).await.unwrap().unwrap().draft;
    assert_eq!(
        draft.pressing.facts.media,
        bae_types::pressing::made_of(bae_types::pressing::Medium::Vinyl, 1).media
    );
    assert_eq!(
        bae_catalogs::release_values::place_labels(
            &draft.mediums,
            draft.tracks.iter().map(|track| track.place)
        ),
        ["A9", "A3"],
        "one record whose positions name no letter is one side, numbered as printed"
    );
    let mut events = handle.every_event();
    let import_id = handle.start_import(&key).await.unwrap().expect_queued();
    let (release_id, _) = await_import_outcome(&mut events, &import_id).await.unwrap();
    let tracks = handle.database.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(
        tracks
            .iter()
            .map(|track| (track.title.as_str(), track.track_number.get(), track.side.get()))
            .collect::<Vec<_>>(),
        [("First Listed", 9, 1), ("Second Listed", 3, 1)]
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_import_commits_what_its_picked_releases_store_now() {
    // The pick offers the partner's album address, which the archive holds
    // no image at.
    let archive = bae_util::http::serve_not_found().await;
    let fakes =
        FakeCatalogs::over(bae_util::http::Http::for_test().serve("coverartarchive.org", &archive));
    let (handle, _tmp, key, _hash) = pane_fixture_with(fakes.providers()).await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    let primary = "70000004";
    let partner = "refetched-partner-release";
    let group = "refetched-partner-group";
    seed_two_track_discogs_release(&fakes, primary);
    seed_two_track_mb_release(&fakes, partner, group);
    let group_json = |allmusic: &str| {
        serde_json::json!({"id":group,"relations":[{"url":{"resource":format!("https://www.allmusic.com/album/{allmusic}")}}]}).to_string()
    };
    fakes.seed_musicbrainz_release_group(group, group_json("mw111"));
    handle
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    primary,
                ),
                partners: vec![bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    partner,
                )],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    // The partner is fetched again after the pick, and its album now names
    // another album on AllMusic: a pick references its releases rather than
    // copying them, so the import commits what the partner says now.
    handle
        .tables
        .save_source_release(
            &bae_catalogs::payloads::ReleasePayloads::for_test(
                bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    partner,
                ),
                two_track_mb_release_json(partner, group),
                vec![bae_catalogs::records::SourcePayload::new(
                    bae_catalogs::records::PayloadSource::MusicBrainzReleaseGroup,
                    group,
                    group_json("mw222"),
                )],
            )
            .extract()
            .unwrap(),
        )
        .await
        .unwrap();
    let mut events = handle.every_event();
    let import_id = handle.start_import(&key).await.unwrap().expect_queued();
    let (release_id, _) = await_import_outcome(&mut events, &import_id).await.unwrap();
    let records = handle.database.get_release_records(&release_id).await.unwrap();
    shut_down(handle).await;
    assert_eq!(
        records
            .iter()
            .find(|record| record.catalog() == bae_catalogs::catalog::Catalog::AllMusic)
            .map(|record| record.key()),
        Some("mw222")
    );
}

/// An automatic start the import path refuses is the candidate's failed
/// import, shown where one that failed while running is, with nothing claimed.
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_import_that_cannot_start_is_recorded_as_its_failed_import() {
    let fixture = Fixture::new("auto-import-refused").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.identify_ready(&dir, "mb-refused", "rg-refused").await;
    // A remote cover whose stored bytes are gone is a broken candidate no
    // import starts from.
    let hash = fixture.content_hash(&dir);
    fixture
        .preparations
        .set_cover(
            &hash,
            &crate::CoverSelection::Remote(
                bae_catalogs::remote_image::RemoteImageSet::original(
                    "https://example.invalid/cover".to_string(),
                ),
                bae_catalogs::catalog::Catalog::MusicBrainz,
            ),
            Some(&bae_catalogs::remote_image::RemoteImage {
                bytes: vec![1, 2, 3],
                content_type: bae_util::content_type::ContentType::Jpeg,
            }),
        )
        .await
        .unwrap();
    fixture
        .database
        .execute_for_test(
            "DELETE FROM import_candidate_remote_cover_asset WHERE content_hash = ?",
            vec![hash],
        )
        .await
        .unwrap();

    let error = fixture
        .import
        .import_identified(&key)
        .await
        .expect_err("a remote cover without its bytes does not start an import");

    let failure = fixture
        .import_failure(&dir)
        .await
        .expect("the refused start is the candidate's failed import");
    assert_eq!(failure.reason, crate::ImportFailureReason::error(error.to_string()));
    assert!(
        fixture
            .import
            .candidate_runtime(&key)
            .await
            .unwrap()
            .is_none_or(|runtime| runtime.import.is_none()),
        "nothing claims the candidate"
    );
}

/// An import that already owns the candidate is not a failure of the
/// automatic one: it says how it goes itself.
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_import_behind_another_import_records_no_failure() {
    let fixture = Fixture::new("auto-import-behind").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.identify_ready(&dir, "mb-behind", "rg-behind").await;
    fixture.import.claim_candidate_for_import(&key, "import-1").await;

    let error = fixture
        .import
        .import_identified(&key)
        .await
        .expect_err("a candidate an import owns is not imported again");

    assert!(matches!(error, crate::ImportError::CandidateImportInProgress), "{error}");
    assert_eq!(fixture.import_failure(&dir).await, None);
}

impl Fixture {
    /// The failed import `dir`'s pane shows, if any.
    async fn import_failure(&self, dir: &Path) -> Option<crate::ImportFailure> {
        self.tables()
            .load_import_candidate_pane_rows(&self.content_hash(dir))
            .await
            .unwrap()
            .failure
    }
}

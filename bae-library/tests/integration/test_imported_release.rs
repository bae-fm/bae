//! What the library shows of a release an import writes: the album detail
//! while another import reads its files, the release editor's copy of an
//! imported release, and the artists the import created. Each test opens a library, starts its import service,
//! and imports folders through the service's handle the way the import pane
//! does: the folder's watched root is read, its draft is read from its own
//! tags, and the import is asked for.

use bae_import::{CandidateKey, ImportEvent, ImportServiceHandle};
use bae_library::test_libraries::TestLibrary;
use bae_util::test_channels::HANG_GUARD;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;

/// A library and the import service started over it.
struct ImportingLibrary {
    library: TestLibrary,
    import: ImportServiceHandle,
    temp: TempDir,
}

impl ImportingLibrary {
    async fn open() -> Self {
        Self::asking(
            bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
            bae_util::http::Http::for_test(),
        )
        .await
    }

    /// A library asking `providers`, which were built on `http`.
    async fn asking(
        providers: bae_catalogs::providers::Providers,
        http: bae_util::http::Http,
    ) -> Self {
        let temp = TempDir::new().unwrap();
        let database = bae_store::Store::new_test(
            bae_library::SCHEMA,
            temp.path().join("test.db").to_str().unwrap(),
            Arc::new(coven::SystemClock),
        )
        .await
        .unwrap();
        let library = TestLibrary::open(
            database,
            bae_app_dir::AppDir::under_home(temp.path()).unwrap(),
            providers,
            http,
        )
        .await;
        let import = library.start_import().await.unwrap();
        Self { library, import, temp }
    }

    /// A folder named `name` under the watched root `root`, holding two
    /// FLACs and a cover, read by the import service. The folder's files
    /// carry a modification time no other folder's has, so two folders of
    /// the same fixtures are not one release's files.
    async fn scanned_folder(&self, root: &str, name: &str) -> CandidateKey {
        static FOLDERS_BUILT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let modified = std::time::UNIX_EPOCH
            + std::time::Duration::from_secs(
                1_700_000_000 + FOLDERS_BUILT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            );
        let root = self.temp.path().join(root);
        let folder = root.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        let fixtures = bae_import::test_fixtures::flac_dir();
        for (index, fixture) in ["01 Test Track 1.flac", "02 Test Track 2.flac"].iter().enumerate()
        {
            let path = folder.join(format!("{:02} Track.flac", index + 1));
            std::fs::copy(fixtures.join(fixture), &path).unwrap();
            set_modified(&path, modified);
        }
        let cover = folder.join("cover.jpg");
        std::fs::write(&cover, cover_jpeg()).unwrap();
        set_modified(&cover, modified);
        self.import.add_watched_folder(root.clone()).await.unwrap();
        self.import.refresh_watched_folder(root).await.unwrap();
        CandidateKey::folder(&folder)
    }

    /// [`Self::scanned_folder`] with its draft read from its own tags and
    /// `artist` as its album artist: ready to import.
    async fn importable_folder(&self, root: &str, name: &str, artist: &str) -> CandidateKey {
        let key = self.scanned_folder(root, name).await;
        self.import.select_candidate_file_tags(key.clone()).await.unwrap();
        self.import
            .set_candidate_album_artists(
                &key,
                vec![bae_import::Credited::plain(bae_import::ArtistAssignment::named(artist))],
            )
            .await
            .unwrap();
        key
    }

    /// Import `key` and wait for its outcome.
    async fn import(&self, key: &CandidateKey) -> Result<(String, String), String> {
        let mut events = self.import.every_event();
        let import_id = self.import.start_import(key).await.unwrap().expect_queued();
        import_outcome(&mut events, &import_id).await
    }

    /// One candidate as the pane reads it.
    async fn pane(&self, key: &CandidateKey) -> bae_import::ImportCandidateDetail {
        self.import.load_candidate(key).await.unwrap().expect("the candidate reads back")
    }

    /// The release editor's copy of `release_id`, as it opens.
    async fn release_edit(&self, release_id: &str) -> bae_import::ReleaseEditSeed {
        self.library.manager().open_release_edit(release_id).await.unwrap().opened().clone()
    }

    /// Stop the import service, which joins its threads, off the runtime.
    async fn shut_down(self) {
        let import = self.import;
        tokio::task::spawn_blocking(move || import.stop_and_join()).await.unwrap();
    }
}

fn cover_jpeg() -> Vec<u8> {
    let image = image::RgbImage::from_pixel(16, 16, image::Rgb([90, 30, 160]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, image::ImageFormat::Jpeg)
        .expect("encode cover jpeg");
    bytes.into_inner()
}

fn set_modified(path: &Path, modified: std::time::SystemTime) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(modified))
        .expect("set the file's modification time");
}

/// What the import `import_id` reported: the release and album it created, or
/// the error it failed with. Every other event on the stream is skipped.
async fn import_outcome(
    events: &mut bae_import::ImportEvents,
    import_id: &str,
) -> Result<(String, String), String> {
    loop {
        let event = tokio::time::timeout(HANG_GUARD, events.recv())
            .await
            .expect("the import reports its result")
            .expect("the import event stream remains open");
        let ImportEvent::ImportProgress { progress, .. } = event else {
            continue;
        };
        match progress {
            bae_import::ImportProgress::Complete { import_id: completed, id, album_id }
                if completed == import_id =>
            {
                return Ok((id, album_id))
            }
            bae_import::ImportProgress::Failed { import_id: failed, error }
                if failed == import_id =>
            {
                return Err(error)
            }
            _ => {}
        }
    }
}

// ── The album detail while another release imports ──────────────────────────
//
// An import reads its files from a watched folder, which is often a network
// share that also holds the library's other originals; while it reads, that
// volume answers late. What the database holds for an album never waits on
// the volume: the album shows at once, and whether this device still holds
// its originals follows when the volume answers.

/// The import's measuring thread, held by
/// [`ImportServiceHandle::hold_progress_at`] and let go on drop, so a test
/// that fails first does not leave the import worker waiting while the
/// services it runs in are torn down.
struct HeldImport<'a>(&'a ImportServiceHandle);

impl<'a> HeldImport<'a> {
    fn at(import: &'a ImportServiceHandle, phase: bae_import::ImportPhase) -> Self {
        import.hold_progress_at(phase);
        Self(import)
    }
}

impl Drop for HeldImport<'_> {
    fn drop(&mut self) {
        self.0.release_progress();
    }
}

/// The next value the album detail view's read delivers, as the detail
/// view shows it.
async fn next_detail(
    values: &mut coven::ReconfigurableLiveQuery<
        Option<String>,
        bae_views::AlbumDetailProjection,
        bae_store::DbError,
    >,
    waiting_for: &str,
) -> bae_views::album_detail::AlbumDetail {
    let event = tokio::time::timeout(HANG_GUARD, values.next())
        .await
        .unwrap_or_else(|_| panic!("{waiting_for}"))
        .expect("the album detail's read stays open");
    bae_views::album::resolve_album_detail_projection(event.result.expect("the album detail reads"))
        .expect("the album is present")
}

/// The import is held partway through measuring its tracks. The shown
/// album's detail arrives anyway, and says the release plays here from the
/// originals the library records, without asking the disk.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_album_opened_while_another_release_imports_shows_at_once() {
    let fixture = ImportingLibrary::open().await;
    let shown_key = fixture.importable_folder("shown", "Shown Album", "Artist").await;
    let importing_key = fixture.importable_folder("other", "Importing Album", "Artist").await;
    let (_, shown_album) = fixture.import(&shown_key).await.expect("the shown album imports");

    let mut events = fixture.import.every_event();
    let held = HeldImport::at(&fixture.import, bae_import::ImportPhase::MeasuringLoudness);
    let importing = fixture.import.start_import(&importing_key).await.unwrap().expect_queued();
    tokio::time::timeout(HANG_GUARD, fixture.import.progress_held())
        .await
        .expect("the import reaches its measurement");

    let mut values = bae_views::LibraryViews::new(fixture.library.store().clone())
        .subscribe_album_detail(Some(shown_album.clone()));
    let shown =
        next_detail(&mut values, "the album detail arrives while the other import runs").await;
    assert_eq!(shown.album.id, shown_album);
    assert_eq!(
        shown.releases[0].playability,
        bae_views::album_detail::ReleasePlayability::Playable,
        "the library records this device holding the originals"
    );
    drop(held);
    import_outcome(&mut events, &importing).await.expect("the held import finishes");

    drop(values);
    fixture.shut_down().await;
}

// ── The release editor's copy of an imported release ────────────────────────

/// Folders picked from different roots import as one release, each folder a
/// disc, and separating the combination afterwards leaves the release as it
/// was imported and the folders on disk.
#[tokio::test(flavor = "multi_thread")]
async fn selected_folders_from_different_roots_import_as_one_release() {
    let fixture = ImportingLibrary::open().await;
    let first_key = fixture.scanned_folder("first", "Volume B").await;
    let second_key = fixture.scanned_folder("second", "Volume A").await;
    let spare_key = fixture.scanned_folder("spare", "Volume C").await;
    let original_paths: Vec<PathBuf> = [&first_key, &second_key]
        .into_iter()
        .flat_map(|key| {
            let folder = key.test_folder();
            ["01 Track.flac", "02 Track.flac"].map(|name| folder.join(name))
        })
        .collect();
    // Combining takes the selection, not an order: core plays the folders in
    // key order and names the release after the first of them.
    let mut ordered = [first_key.clone(), second_key.clone()];
    ordered.sort();
    let leading = if ordered[0] == first_key { "Volume B" } else { "Volume A" };
    let key = fixture
        .import
        .combine_candidates(vec![second_key.clone(), first_key.clone()])
        .await
        .unwrap();
    assert_eq!(
        fixture.import.candidate_source_folders(&key).await.unwrap(),
        ordered.iter().map(CandidateKey::test_folder).collect::<Vec<_>>()
    );
    assert!(fixture
        .import
        .get_release_candidate(&key)
        .await
        .unwrap()
        .is_some_and(|candidate| candidate.grouping.as_ref() == Some(&key)));
    assert!(fixture.import.get_release_candidate(&first_key).await.unwrap().is_none());
    assert!(fixture.import.get_release_candidate(&second_key).await.unwrap().is_none());
    assert_eq!(fixture.pane(&key).await.draft().unwrap().metadata_draft.album_title, leading);
    // A combined key is not a folder to combine again; separating it first is.
    // Every folder key sorts ahead of the combination key, so the folder in
    // this selection is the one loaded first and the combination is what the
    // call stops on.
    assert!(fixture
        .import
        .combine_candidates(vec![key.clone(), spare_key.clone()])
        .await
        .unwrap_err()
        .to_string()
        .contains("separate an existing combination before combining its folders again"));
    fixture.import.select_candidate_file_tags(key.clone()).await.unwrap();
    fixture
        .import
        .set_candidate_edit_field(
            &key,
            bae_import::DraftFieldEdit::Text {
                field: bae_import::CandidateEditField::AlbumTitle,
                value: "Collected Volumes".into(),
            },
        )
        .await
        .unwrap();
    fixture
        .import
        .set_candidate_album_artists(
            &key,
            vec![bae_import::Credited::plain(bae_import::ArtistAssignment::named(
                "Combined Artist",
            ))],
        )
        .await
        .unwrap();
    let projected = fixture.pane(&key).await;
    assert_eq!(projected.draft().unwrap().metadata_draft.tracks.len(), 4);
    assert_eq!(
        projected
            .draft()
            .unwrap()
            .mapping
            .track_sides
            .iter()
            .map(|side| side.side.clone())
            .collect::<Vec<_>>(),
        [
            bae_types::mediums::TrackSide::Disc { disc: 1 },
            bae_types::mediums::TrackSide::Disc { disc: 2 }
        ],
        "each folder is a disc"
    );

    let (release_id, _) = fixture
        .import(&key)
        .await
        .unwrap_or_else(|error| panic!("combined import failed: {error}"));

    let imported = fixture.release_edit(&release_id).await;
    assert_eq!(imported.edit.album_title, "Collected Volumes");
    assert_eq!(imported.edit.tracks.len(), 4);
    let disc = |disc| bae_types::mediums::TrackSide::Disc { disc };
    assert_eq!(
        imported
            .display
            .sides
            .iter()
            .flat_map(|side| side
                .tracks()
                .map(|track| (side.side.clone(), track.position_text.as_str())))
            .collect::<Vec<_>>(),
        [(disc(1), "1"), (disc(1), "2"), (disc(2), "1"), (disc(2), "2")]
    );
    assert!(fixture.pane(&key).await.is_added());
    fixture.import.separate_candidate(&key).await.unwrap();
    assert_eq!(
        fixture.release_edit(&release_id).await,
        imported,
        "separating source folders preserves the imported copy"
    );
    assert!(original_paths.iter().all(|path| path.is_file()));
    fixture.shut_down().await;
}

/// An imported candidate holds no draft: an edit of it is refused as an
/// import of those files would be, and the library release stays as the
/// import wrote it.
#[tokio::test(flavor = "multi_thread")]
async fn editing_an_imported_candidate_is_refused_and_leaves_its_release() {
    let fixture = ImportingLibrary::open().await;
    let key = fixture.importable_folder("watched", "Album", "Fixture Artist").await;
    let (release_id, _) =
        fixture.import(&key).await.unwrap_or_else(|error| panic!("import failed: {error}"));
    let release_before = fixture.release_edit(&release_id).await;

    let refused = fixture
        .import
        .set_candidate_edit_field(
            &key,
            bae_import::DraftFieldEdit::Text {
                field: bae_import::CandidateEditField::AlbumTitle,
                value: "Edited candidate".into(),
            },
        )
        .await;
    assert_eq!(
        refused.map_err(|error| error.to_string()),
        Err(bae_import::ImportError::FilesAlreadyImported { release_id: release_id.clone() }
            .to_string())
    );
    assert_eq!(fixture.release_edit(&release_id).await, release_before);
    fixture.shut_down().await;
}

// ── The artists an import creates ───────────────────────────────────────────

/// A track artist the import of a Discogs pick creates is found by the
/// library's artist search, with the picture the import fetched and stored.
#[tokio::test(flavor = "multi_thread")]
async fn an_imported_artist_is_searchable_with_its_picture() {
    let images = bae_catalogs::test_image_host::RemoteImageHost::start().await;
    let http = bae_util::http::Http::for_test()
        .serve(bae_catalogs::test_image_host::TEST_IMAGE_HOST, images.origin());
    let fakes = bae_catalogs::fake_catalogs::FakeCatalogs::over(http.clone());
    let fixture = ImportingLibrary::asking(fakes.providers(), http).await;
    fixture
        .library
        .catalogs()
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    let key = fixture.scanned_folder("watched", "Album").await;
    fixture.import.select_candidate_file_tags(key.clone()).await.unwrap();

    let numeric_id = || (uuid::Uuid::new_v4().as_u128() % u128::from(u64::MAX)) as u64;
    let album_artist_id = numeric_id();
    let track_artist_id = numeric_id();
    let source_release_id = numeric_id().to_string();
    fakes.seed_discogs_release(
        &source_release_id,
        serde_json::json!({
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
        .to_string(),
    );
    fakes.seed_discogs_page_lookup(&[(
        bae_catalogs::musicbrainz::DiscogsPage::Release(source_release_id.clone()),
        None,
    )]);
    fixture
        .import
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    source_release_id,
                ),
                partners: vec![],
            },
            bae_import::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    let picture_url = images.serve_image(
        &format!("/discogs-artist-{}.png", uuid::Uuid::new_v4()),
        bae_catalogs::test_image_host::cover_png(),
    );
    fakes.seed_discogs_artist_image(&album_artist_id.to_string(), None);
    fakes.seed_discogs_artist_image(&track_artist_id.to_string(), Some(picture_url));

    fixture.import(&key).await.unwrap_or_else(|error| panic!("import failed: {error}"));

    let mut search = bae_views::LibraryViews::new(fixture.library.store().clone())
        .subscribe_artist_search(bae_views::ArtistSearchView {
            query: bae_views::LibrarySearchQuery::parse("Track Artist"),
            windows: [bae_store::LibraryPageWindow { offset: 0, limit: 50 }].into(),
        });
    let found = search.next().await.expect("the artist search stays open").result.unwrap();
    let result = bae_views::artist_search::resolve_artist_search(found, 0)
        .windows
        .into_iter()
        .flat_map(|window| window.rows)
        .find(|result| {
            result.artist.discogs_artist_id.as_deref() == Some(&track_artist_id.to_string())
        })
        .expect("the track artist is searchable");
    let image = result.image.expect("the searched artist shows its picture");
    let bytes = fixture.library.manager().read_image_bytes(&image).await.unwrap();
    assert!(bytes.is_some_and(|bytes| !bytes.is_empty()), "the picture's bytes read back");
    fixture.shut_down().await;
}

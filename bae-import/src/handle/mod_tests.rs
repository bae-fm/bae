//! The handle's shared fixtures: an import service over a fresh store, folders
//! stored as a scan stores them under a watched root, the handle and key a
//! pane's controls address, and catalog answers picked into the draft. A
//! claimed import's queued status is published at once.

use super::*;
use crate::identification::tests::*;
use crate::tests::import::TestImport;
use crate::PendingStanding;
use bae_catalogs::fake_catalogs::FakeCatalogs;
use bae_catalogs::records::MetadataRef;
use bae_catalogs::test_releases::seed_mb_release_with_catalog;
use bae_catalogs::test_releases::seed_mb_release_with_works;
use bae_identify::AlbumLink;
use bae_identify::LookupChoiceEdit;
use bae_records::{DbAlbum, DbArtist, DbRelease};
use bae_store::Store;
use bae_util::test_channels::HANG_GUARD;
use chrono::Utc;
use std::path::Path;
use tempfile::TempDir;
use uuid::Uuid;

/// What an import service is started from over a fresh store asking no
/// catalog, the store, and the directory holding it.
pub(super) async fn setup_test_import() -> (TestImport, Store, TempDir) {
    setup_test_import_with(
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await
}

/// [`setup_test_import`] asking `providers`.
pub(super) async fn setup_test_import_with(
    providers: bae_catalogs::providers::Providers,
    http: bae_util::http::Http,
) -> (TestImport, Store, TempDir) {
    setup_test_import_over(providers, http, bae_diagnostics::Diagnostics::noop()).await
}

pub(super) async fn setup_test_import_over(
    providers: bae_catalogs::providers::Providers,
    http: bae_util::http::Http,
    diagnostics: bae_diagnostics::Diagnostics,
) -> (TestImport, Store, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Store::new_test(
        crate::TEST_SCHEMA,
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    let parts = TestImport::new(
        db.clone(),
        providers,
        http,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidIds),
        diagnostics,
    );
    (parts, db, temp_dir)
}

pub(crate) fn make_artist(name: &str, discogs_id: Option<&str>, mb_id: Option<&str>) -> DbArtist {
    let now = Utc::now();
    DbArtist {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        sort_name: None,
        discogs_artist_id: discogs_id.map(|s| s.to_string()),
        musicbrainz_artist_id: mb_id.map(|s| s.to_string()),
        created_at: now,
    }
}

/// An album of its own holding one release of one track, crediting the
/// library artist `artist_id`, written as the library writes a release.
pub(super) async fn insert_library_release(
    db: &Store,
    artist_id: &str,
    title: &str,
) -> (DbAlbum, DbRelease) {
    let album = DbAlbum::new_test(title);
    let release = DbRelease::new_test(&album.id, &Uuid::new_v4().to_string());
    bae_records::test_fixtures::test_records(db)
        .insert_album_with_release_and_tracks(
            &album,
            &[bae_records::DbAlbumArtist::new_test(&album.id, artist_id)],
            &release,
            &[bae_records::DbReleaseMedium::new_test(&release.id)],
            &[bae_records::DbTrack::new_test(
                &release.id,
                &Uuid::new_v4().to_string(),
                "Track 1",
                1,
            )],
            &[],
        )
        .await
        .unwrap();
    (album, release)
}

/// What the import `import_id` reported: the release and album it created, or
/// the error it failed with. Every other event on the stream is skipped.
pub(crate) async fn await_import_outcome(
    events: &mut crate::ImportEvents,
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
            crate::ImportProgress::Complete { import_id: completed, id, album_id }
                if completed == import_id =>
            {
                return Ok((id, album_id))
            }
            crate::ImportProgress::Failed { import_id: failed, error } if failed == import_id => {
                return Err(error)
            }
            _ => {}
        }
    }
}

/// Put `key` on the identification queue as the queue admits it, with the
/// files the scan holds it with.
pub(super) async fn admit(handle: &ImportServiceHandle, key: &crate::CandidateKey) {
    let candidate =
        handle.get_release_candidate(key).await.unwrap().expect("the candidate is stored");
    handle
        .admit_identification(
            vec![(key.clone(), crate::CandidateFiles::of(&candidate))],
            crate::Admission::Requested,
        )
        .await
        .unwrap();
}

/// Start `run` of the queued `key`, as the identify driver records its first
/// state.
pub(super) async fn start_run(
    handle: &ImportServiceHandle,
    key: &crate::CandidateKey,
    run: crate::IdentifyRunId,
) {
    let candidate =
        handle.get_release_candidate(key).await.unwrap().expect("the candidate is stored");
    assert!(handle
        .runtime
        .run_started(
            key,
            &crate::CandidateFiles::of(&candidate),
            run,
            &crate::candidate_runtime::tests::triangulating(),
        )
        .await
        .unwrap());
}

pub(super) async fn pick_mb(
    handle: &ImportServiceHandle,
    key: &crate::CandidateKey,
    release_id: &str,
) {
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
}

pub(super) async fn stored_draft(
    handle: &ImportServiceHandle,
    hash: &str,
) -> crate::CandidateDraft {
    crate::preparations::stored_preparation(&handle.services.tables, hash)
        .await
        .unwrap()
        .expect("the candidate is prepared")
        .metadata
        .draft
}

/// A draft read from a release whose second file a sheet carves into two
/// tracks: the release's three tracks lie on `01 Track.flac` and the sheet's
/// two slices.
pub(super) async fn three_track_draft() -> (StoredCandidate, String) {
    let fakes = FakeCatalogs::offline();
    let mut fixture =
        stored_candidate_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    add_sheet(&fixture.handle.services.tables, &mut fixture.candidate, SECOND_FILE_IN_TWO).await;
    fixture
        .handle
        .set_sheet_disc(
            fixture.key.clone(),
            "Disc.cue".to_string(),
            crate::folder_scanner::SheetDisc::Disc { number: 1 },
        )
        .await
        .unwrap();
    seed_mb_release_with_works(
        &fakes,
        "release-with-works",
        "release-with-works",
        &["First Track", "Second Track", "Third Track"],
        &[111_000, 222_000, 333_000],
    );
    pick_mb(&fixture.handle, &fixture.key, "release-with-works").await;
    let hash = fixture.candidate.files.content_hash();
    (fixture, hash)
}

/// A copy of `original` in a sibling folder named `name`, its files copied
/// with their sizes and modification times, so the two folders' files hash
/// alike.
pub(super) fn twin_of(original: &FolderCandidate, name: &str) -> FolderCandidate {
    let folder = original.path.with_file_name(name);
    std::fs::create_dir_all(&folder).unwrap();
    let mut twin = original.clone();
    for candidate_file in &mut twin.files.files {
        let source = candidate_file.file.version.path.clone();
        let target = folder.join(&candidate_file.file.relative_path);
        std::fs::copy(&source, &target).unwrap();
        set_modified(&target, std::fs::metadata(&source).unwrap().modified().unwrap());
        candidate_file.file.version.path = target;
    }
    twin.path = folder.clone();
    twin.file_root = folder;
    twin.name = name.to_string();
    twin.relative_folder = crate::RelativeFolder::test(name);
    assert_eq!(twin.files.content_hash(), original.files.content_hash());
    twin
}

/// Store `candidates`, all under one watched root, as the whole of a fresh
/// scan generation of it.
pub(super) async fn rescan_all_into(
    tables: &crate::tables::ImportTables,
    candidates: &[FolderCandidate],
) {
    let root = candidates[0].watched_folder_path.clone();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    for candidate in candidates {
        crate::store_scan_item(
            tables,
            &bae_diagnostics::Diagnostics::noop(),
            &root,
            generation,
            &crate::folder_scanner::ScanItem::Valid(candidate.clone()),
        )
        .await
        .unwrap()
        .expect("the scan generation is current");
    }
    tables.finish_folder_scan(&root, generation, None).await.unwrap();
}

pub(super) fn catalog(number: &str) -> LookupChoiceEdit {
    LookupChoiceEdit::ToggleCatalog { number: number.to_string() }
}

pub(super) async fn pick(
    handle: &ImportServiceHandle,
    key: &crate::CandidateKey,
    release_id: &str,
) {
    handle
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    release_id.to_string(),
                ),
                partners: vec![],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
}

// The pane's own controls, from the handle down to the next read.
//
// Every one of these writes a row and returns; nothing is handed back to the
// caller. So what each test asserts is what the pane would draw next — the
// `candidate_pane` read — because that is the only path the UI has to what it
// just wrote.

pub(super) fn cover_jpeg() -> Vec<u8> {
    let image = image::RgbImage::from_pixel(16, 16, image::Rgb([90, 30, 160]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, image::ImageFormat::Jpeg)
        .expect("encode cover jpeg");
    bytes.into_inner()
}

/// A watched root holding one folder named `folder_name`, of real audio and one
/// image, scanned into the tables with its draft read from its own tags.
///
/// The folder is stored rather than scanned for: the pane's writes are what is
/// under test, and a real scan would only make when they run less certain.
pub(super) async fn picked_candidate(
    tables: &crate::tables::ImportTables,
    tmp: &TempDir,
    folder_name: &str,
) -> (FolderCandidate, crate::CandidateKey, String) {
    let (candidate, key, hash) = unscanned_candidate(tables, tmp, folder_name).await;
    rescan_into(tables, candidate.clone()).await;
    (candidate, key, hash)
}

/// [`picked_candidate`] as a scan stores a folder whose tags it could not
/// read: a blank draft and no reading, so the first pick is what reads them.
pub(super) async fn unread_candidate(
    tables: &crate::tables::ImportTables,
    tmp: &TempDir,
    folder_name: &str,
) -> (FolderCandidate, crate::CandidateKey, String) {
    let (candidate, key, hash) = unscanned_candidate(tables, tmp, folder_name).await;
    rescan_unread_into(tables, candidate.clone()).await;
    (candidate, key, hash)
}

pub(super) fn set_modified(path: &Path, modified: std::time::SystemTime) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(modified))
        .expect("set the file's modification time");
}

/// [`rescan_into`] as a scan that could not read the folder's tags.
pub(super) async fn rescan_unread_into(
    tables: &crate::tables::ImportTables,
    candidate: FolderCandidate,
) {
    let root = candidate.watched_folder_path.clone();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    let unread = crate::tables::FolderEntryToWrite::Release(crate::tables::ScanItemToWrite {
        item: crate::folder_scanner::ScanItem::Valid(candidate),
        seed: crate::tables::ScanItemSeed::Tags(None),
        folder_date: None,
    });
    tables.save_folder_scan_items(&root, generation, vec![unread]).await.unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();
}

/// The folder [`picked_candidate`] stores, under a watched root, not stored yet.
///
/// Every folder built here is its own files: its files are given a
/// modification time no other folder's has. The files' names and sizes are the
/// same in each, and copying keeps the fixture's time on Windows, where two
/// covers written in one clock tick share theirs too — so two folders would
/// otherwise be one release's files, and a decision about one would answer
/// both.
pub(super) async fn unscanned_candidate(
    tables: &crate::tables::ImportTables,
    tmp: &TempDir,
    folder_name: &str,
) -> (FolderCandidate, crate::CandidateKey, String) {
    use crate::folder_scanner::{
        CandidateFile, CategorizedFiles, FileRole, ReleaseFileScope, ScannedFile,
    };

    static FOLDERS_BUILT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let modified = std::time::UNIX_EPOCH
        + std::time::Duration::from_secs(
            1_700_000_000 + FOLDERS_BUILT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        );
    let root = tmp.path().join("watched");
    let folder = root.join(folder_name);
    std::fs::create_dir_all(&folder).unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac");
    let mut files = Vec::new();
    for (index, name) in ["01 Test Track 1.flac", "02 Test Track 2.flac"].into_iter().enumerate() {
        let relative_path = format!("{:02} Track.flac", index + 1);
        let path = folder.join(&relative_path);
        std::fs::copy(fixtures.join(name), &path).unwrap();
        set_modified(&path, modified);
        let metadata = std::fs::metadata(&path).unwrap();
        let size = metadata.len();
        let modified_at_ns = crate::folder_scanner::file_modified_at_ns(&path, &metadata).unwrap();
        files.push(CandidateFile {
            file: ScannedFile::new(path.clone(), relative_path, size, modified_at_ns)
                .with_test_flac_audio(),
            role: FileRole::Audio,
        });
    }
    let cover_path = folder.join("cover.jpg");
    std::fs::write(&cover_path, cover_jpeg()).unwrap();
    set_modified(&cover_path, modified);
    let cover_metadata = std::fs::metadata(&cover_path).unwrap();
    let cover_modified_at_ns =
        crate::folder_scanner::file_modified_at_ns(&cover_path, &cover_metadata).unwrap();
    files.push(CandidateFile {
        file: ScannedFile::new(
            cover_path.clone(),
            "cover.jpg".to_string(),
            cover_metadata.len(),
            cover_modified_at_ns,
        ),
        role: FileRole::Artwork,
    });

    let candidate = FolderCandidate {
        path: folder.clone(),
        file_root: folder.clone(),
        name: folder_name.to_string(),
        files: CategorizedFiles { files, parts: Vec::new() },
        watched_folder_path: root.clone(),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        relative_folder: crate::RelativeFolder::test(folder_name),
        grouping: None,
    };

    tables.add_watched_import_folder(&root).await.unwrap();

    let key = crate::CandidateKey::folder(&folder);
    let hash = candidate.files.content_hash();
    (candidate, key, hash)
}

/// Store `candidate` as the whole of a fresh scan generation of its own
/// watched root — the stamp every later read of that candidate is checked
/// against.
pub(super) async fn rescan_into(tables: &crate::tables::ImportTables, candidate: FolderCandidate) {
    let root = candidate.watched_folder_path.clone();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    crate::store_scan_item(
        tables,
        &bae_diagnostics::Diagnostics::noop(),
        &root,
        generation,
        &crate::folder_scanner::ScanItem::Valid(candidate),
    )
    .await
    .unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();
}

/// A running import service over a watched root holding one stored candidate —
/// what every control in this file is addressed to.
pub(crate) struct StoredCandidate {
    pub(crate) handle: ImportServiceHandle,
    /// What the service was started from, for the tests that start another
    /// over the same store to read what survived the first one, or ask its
    /// catalog sessions.
    pub(crate) parts: TestImport,
    /// The store the service was started over, for a test that reads or
    /// seeds rows no call reads or writes on its own.
    pub(crate) db: Store,
    pub(super) candidate: FolderCandidate,
    /// The key the pane's controls address the candidate by.
    pub(crate) key: crate::CandidateKey,
    /// Held for its `Drop`: the watched root lives inside it.
    pub(crate) tmp: TempDir,
}

pub(super) async fn stored_candidate() -> StoredCandidate {
    stored_candidate_with(
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await
}

/// A sheet carving the second of [`picked_candidate`]'s two files into two
/// tracks, leaving the first a track of its own.
pub(super) const SECOND_FILE_IN_TWO: &str = concat!(
    "FILE \"02 Track.flac\" WAVE\n",
    "  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n",
    "  TRACK 02 AUDIO\n    INDEX 01 00:00:30\n",
);

/// Write `cue` beside `candidate`'s audio as `Disc.cue` and store the folder
/// as a scan reads it now, so its sheet decisions have a sheet to act on.
pub(super) async fn add_sheet(
    tables: &crate::tables::ImportTables,
    candidate: &mut FolderCandidate,
    cue: &str,
) {
    std::fs::write(candidate.path.join("Disc.cue"), cue).unwrap();
    candidate.files = crate::folder_scanner::collect_release_candidate_files_with_scope(
        &candidate.path,
        candidate.scope,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    rescan_into(tables, candidate.clone()).await;
}

/// [`stored_candidate`] holding a `Disc.cue` of `cue`.
pub(super) async fn stored_candidate_with_sheet(cue: &str) -> StoredCandidate {
    let mut fixture = stored_candidate().await;
    add_sheet(&fixture.handle.services.tables, &mut fixture.candidate, cue).await;
    fixture
}

pub(super) async fn stored_candidate_with(
    providers: bae_catalogs::providers::Providers,
    http: bae_util::http::Http,
) -> StoredCandidate {
    let (parts, db, tmp) = setup_test_import_with(providers, http).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (candidate, key, _hash) = picked_candidate(&tables, &tmp, "Album").await;
    started(parts, db, candidate, key, tmp).await
}

/// [`stored_candidate`] whose tags the scan could not read.
pub(super) async fn unread_stored_candidate() -> StoredCandidate {
    let (parts, db, tmp) = setup_test_import().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (candidate, key, _hash) = unread_candidate(&tables, &tmp, "Album").await;
    started(parts, db, candidate, key, tmp).await
}

/// An import service started from `parts` over `database`, addressed at the
/// stored `candidate`.
pub(super) async fn started(
    parts: TestImport,
    db: Store,
    candidate: FolderCandidate,
    key: crate::CandidateKey,
    tmp: TempDir,
) -> StoredCandidate {
    let handle = parts.start().await.unwrap();
    StoredCandidate { handle, parts, db, candidate, key, tmp }
}

/// The handle, the key its controls address, and the hash its rows are stored
/// under — with the folder's own tags already picked, which is what draws the
/// edit form and the mapping table.
pub(super) async fn pane_fixture() -> (ImportServiceHandle, TempDir, crate::CandidateKey, String) {
    pane_fixture_with(
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await
}

/// [`pane_fixture`] asking `providers`.
pub(super) async fn pane_fixture_with(
    providers: bae_catalogs::providers::Providers,
    http: bae_util::http::Http,
) -> (ImportServiceHandle, TempDir, crate::CandidateKey, String) {
    let fixture = picked_stored_candidate_with(providers, http).await;
    let hash = fixture.candidate.files.content_hash();
    (fixture.handle, fixture.tmp, fixture.key, hash)
}

/// [`stored_candidate_with`] with the folder's own tags already picked.
pub(crate) async fn picked_stored_candidate_with(
    providers: bae_catalogs::providers::Providers,
    http: bae_util::http::Http,
) -> StoredCandidate {
    let fixture = stored_candidate_with(providers, http).await;
    let revision = fixture.handle.select_candidate_file_tags(fixture.key.clone()).await.unwrap();
    assert_eq!(revision, 1);
    fixture
}

pub(super) async fn pane(
    handle: &ImportServiceHandle,
    key: &crate::CandidateKey,
) -> crate::ImportCandidateDetail {
    handle.load_candidate(key).await.unwrap().expect("the picked candidate reads back")
}

pub(crate) async fn shut_down(handle: ImportServiceHandle) {
    tokio::task::spawn_blocking(move || handle.stop_and_join()).await.unwrap();
}

pub(super) struct CountingFileTagReader {
    pub(super) reads: std::sync::atomic::AtomicUsize,
    /// The read, counted from 0, that fails, and how.
    pub(super) fail_on_read: Option<(usize, crate::file_tag_reader::TagReadFailure)>,
    /// Run as the first file is read, before its tags are.
    pub(super) first_read: Option<Box<dyn Fn() + Send + Sync>>,
    pub(super) embedded_cover: Option<Vec<u8>>,
}

impl CountingFileTagReader {
    pub(super) fn immediate() -> Self {
        Self {
            reads: std::sync::atomic::AtomicUsize::new(0),
            fail_on_read: None,
            first_read: None,
            embedded_cover: None,
        }
    }

    /// A reader whose read `fail_on_read` fails at the file itself.
    pub(super) fn failing(fail_on_read: usize) -> Self {
        Self {
            reads: std::sync::atomic::AtomicUsize::new(0),
            fail_on_read: Some((
                fail_on_read,
                crate::file_tag_reader::TagReadFailure::Fault(format!(
                    "fixture tag read {fail_on_read} failed"
                )),
            )),
            first_read: None,
            embedded_cover: None,
        }
    }

    /// A reader whose read `unparseable` finds tags that do not parse.
    pub(super) fn unparseable(unparseable: usize) -> Self {
        Self {
            reads: std::sync::atomic::AtomicUsize::new(0),
            fail_on_read: Some((
                unparseable,
                crate::file_tag_reader::TagReadFailure::Unparseable(format!(
                    "fixture tags {unparseable} do not parse"
                )),
            )),
            first_read: None,
            embedded_cover: None,
        }
    }

    /// A reader held at its first file until the gate opens, and what hears
    /// that it got there.
    pub(super) fn held() -> (Self, std::sync::mpsc::Receiver<()>, crate::tests::gate::Gate) {
        let (gate, held, reached) = crate::tests::gate::closed();
        let reader = Self {
            reads: std::sync::atomic::AtomicUsize::new(0),
            fail_on_read: None,
            first_read: Some(Box::new(move || held.pass())),
            embedded_cover: None,
        };
        (reader, reached, gate)
    }

    pub(super) fn with_embedded_cover(data: Vec<u8>) -> Self {
        Self {
            reads: std::sync::atomic::AtomicUsize::new(0),
            fail_on_read: None,
            first_read: None,
            embedded_cover: Some(data),
        }
    }

    pub(super) fn read_count(&self) -> usize {
        self.reads.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl crate::file_tag_reader::FileTagReader for CountingFileTagReader {
    fn read(
        &self,
        _path: &std::path::Path,
    ) -> Result<crate::file_tag_reader::FileTagRead, crate::file_tag_reader::TagReadFailure> {
        let index = self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if index == 0 {
            if let Some(first_read) = &self.first_read {
                first_read();
            }
        }
        if let Some((_, failure)) = self.fail_on_read.as_ref().filter(|(at, _)| *at == index) {
            return Err(failure.clone());
        }
        Ok(crate::file_tag_reader::FileTagRead {
            tags: crate::file_tag_snapshot::FileTags {
                title: Some(format!("Track Title {}", index + 1)),
                track_artists: vec!["Artist Name".to_string()],
                album_title: Some("Album Title".to_string()),
                album_artists: vec!["Album Artist".to_string()],
                years: vec![2020],
                numbers: crate::track_numbers::TrackNumbers {
                    disc: Some(1),
                    track: Some(u32::try_from(index + 1).unwrap()),
                },
                ..crate::file_tag_snapshot::FileTags::untagged()
            },
            delivery: crate::file_tag_snapshot::DeliveryTags::default(),
            embedded_cover: if index == 0 {
                self.embedded_cover
                    .clone()
                    .map(|data| (data, bae_util::content_type::ContentType::Jpeg))
            } else {
                None
            },
        })
    }
}

pub(super) fn titles(snapshot: &crate::file_tag_snapshot::FileTagSnapshot) -> Vec<Option<&str>> {
    snapshot.files.iter().map(|fact| fact.tags.stated().title.as_deref()).collect()
}

/// The rows of the table that become tracks, in order — what a person edits.
pub(super) fn track_rows(table: &crate::MappingTable) -> Vec<crate::RawTrackEdit> {
    crate::mapping_tracks(table)
}

/// Store a verdict that found nothing for the fixture's candidate.
pub(super) async fn store_not_found(handle: &ImportServiceHandle, hash: &str) {
    let prep = crate::preparations::stored_preparation(&handle.services.tables, hash)
        .await
        .unwrap()
        .expect("the fixture candidate is prepared");
    let stored = handle
        .services
        .tables
        .store_verdict(&crate::tables::NewImportCandidateVerdict {
            content_hash: hash.to_string(),
            file_edit_revision: prep.file_edits.revision,
            candidate_key: crate::CandidateKey::from(Vec::new()),
            verdict: bae_identify::TerminalVerdict::NotFoundAnywhere { ledger: None },
            signals: bae_identify::signals::Signals {
                origin: bae_catalogs::AudioOrigin::default(),
                disc_id: bae_identify::signals::DiscIdSignal::Absent,
                barcode: bae_identify::signals::BarcodeSignal::Absent,
                text: bae_identify::signals::TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            pick: None,
            releases: Vec::new(),
        })
        .await
        .unwrap();
    assert!(stored, "the verdict lands on the fixture candidate");
}

pub(super) async fn standing(
    handle: &ImportServiceHandle,
    key: &crate::CandidateKey,
) -> PendingStanding {
    pane(handle, key).await.live.standing.expect("the candidate is on Found")
}

/// The fixture candidate, picked as `release_id`.
pub(super) async fn picked_fixture(
    release_id: &str,
) -> (ImportServiceHandle, TempDir, crate::CandidateKey) {
    let fakes = FakeCatalogs::offline();
    let (handle, tmp, key, _hash) =
        pane_fixture_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    seed_mb_release_with_catalog(&fakes, release_id, "CAT-1");
    pick(&handle, &key, release_id).await;
    assert_eq!(standing(&handle, &key).await, PendingStanding::Identified);
    (handle, tmp, key)
}

pub(super) const ALBUM: &str = "album-group-1";

/// One MusicBrainz pressing of the album, as the lookup offered it.
pub(super) fn offered_pressing(
    release_id: &str,
    year: i32,
) -> bae_catalogs::search::MetadataResult {
    bae_catalogs::search::MetadataResult {
        source: bae_catalogs::catalog::LookupCatalog::MusicBrainz,
        release_id: release_id.to_string(),
        title: "Album Title".to_string(),
        artist: Some("Artist Name".to_string()),
        year: Some(year),
        labels: Vec::new(),
        area: None,
        status: None,
        packaging: None,
        discogs_details: Vec::new(),
        barcodes: Vec::new(),
        media: bae_types::pressing::StatedMedia::Undescribed,
        links: Vec::new(),
        cover_art: None,
        source_group_id: Some(ALBUM.to_string()),
        album_links: bae_catalogs::album_links::AlbumLinks::NotAsked,
        source_tracks: Some(bae_catalogs::search::SourceTracks::Listed { count: 2 }),
        document_failure: None,
        album_first_year: None,
        track_titles: Vec::new(),
        notes: Vec::new(),
    }
}

/// Store a verdict offering two pressings of one album for the fixture's
/// candidate, each seeded where a read of it finds it, under its own
/// catalog number.
pub(super) async fn store_two_pressings(
    handle: &ImportServiceHandle,
    fakes: &FakeCatalogs,
    hash: &str,
) {
    seed_mb_release_with_catalog(fakes, "mb-pressing-1", "CAT-100");
    seed_mb_release_with_catalog(fakes, "mb-pressing-2", "CAT-200");
    let prep = crate::preparations::stored_preparation(&handle.services.tables, hash)
        .await
        .unwrap()
        .expect("the fixture candidate is prepared");
    let stored = handle
        .services
        .tables
        .store_verdict(&crate::tables::NewImportCandidateVerdict {
            content_hash: hash.to_string(),
            file_edit_revision: prep.file_edits.revision,
            candidate_key: crate::CandidateKey::from(Vec::new()),
            verdict: bae_identify::TerminalVerdict::Found {
                findings: bae_identify::Findings {
                    matches: vec![
                        offered_pressing("mb-pressing-1", 1996),
                        offered_pressing("mb-pressing-2", 2004),
                    ],
                    pressings: vec![0, 1],
                    narrowed_out: Default::default(),
                    medium_conflict: None,
                    named_notes: Vec::new(),
                },
                track_count: 2,
                ledger: None,
            },
            signals: bae_identify::signals::Signals {
                origin: bae_catalogs::AudioOrigin::default(),
                disc_id: bae_identify::signals::DiscIdSignal::Absent,
                barcode: bae_identify::signals::BarcodeSignal::Absent,
                text: bae_identify::signals::TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            pick: None,
            releases: Vec::new(),
        })
        .await
        .unwrap();
    assert!(stored, "the verdict lands on the fixture candidate");
}

pub(super) fn album() -> AlbumLink {
    AlbumLink::new([MetadataRef::new(bae_catalogs::catalog::Catalog::MusicBrainz, ALBUM)])
        .expect("one album")
}

#[tokio::test(flavor = "multi_thread")]
async fn claiming_an_import_publishes_queued_status_immediately() {
    let fixture = Fixture::new("import-queued-status").await;
    let candidate = fixture.disc_id_candidate("Album Title");
    fixture.scan(1).await;
    let mut changes = fixture.import.every_runtime_change_for_test();

    fixture
        .import
        .claim_candidate_for_import(&crate::CandidateKey::folder(&candidate), "import-1")
        .await;

    let change = tokio::time::timeout(HANG_GUARD, changes.recv())
        .await
        .expect("the queued status is published")
        .expect("runtime changes remain open");
    assert!(matches!(
        change,
        crate::CandidateRuntimeChange::Updated { key, runtime }
            if key == crate::CandidateKey::folder(&candidate)
                && runtime.import
                    == Some(crate::ImportInFlight {
                        step: crate::ImportStep::Preparing(
                            crate::PrepareStep::Queued
                        ),
                    })
    ));
}

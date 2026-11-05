//! The fixtures the import service's tests share, and the tests of other
//! modules that run one: a service over a test library, a scan over it, a
//! candidate stored and prepared as a scan and a person would leave it.

use super::*;
use crate::folder_scanner::ScanItem;
use crate::scan_passes::tests::{test_scan_services, TestScan};
use crate::watching::FolderWatcher;
use bae_db::Database;
use tempfile::TempDir;

/// A worker for a test, with the candidate-preparations writer over the same
/// database.
pub(crate) struct TestService {
    pub(crate) service: ImportService,
    pub(crate) preparations: crate::CandidatePreparations,
    /// The library the service imports into, for a test that reads or seeds
    /// rows no call reads or writes on its own.
    pub(crate) database: Database,
    pub(crate) temp: TempDir,
}

impl TestService {
    /// Import's tables in this service's library.
    pub(crate) fn tables(&self) -> &crate::tables::ImportTables {
        &self.service.tables
    }

    /// Report the bugs this service finds to `bugs`.
    pub(crate) fn report_bugs_to(&mut self, bugs: Arc<bae_diagnostics::RecordingBugReporter>) {
        self.service.diagnostics = bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs);
    }

    /// The bus this service announces on.
    pub(crate) fn event_tx(&self) -> &crate::ImportEventBus {
        &self.service.event_tx
    }

    /// The import service's dependencies over this service's library,
    /// announcing on `event_tx`, reading tags through `file_tags` and listings
    /// through `directories`, and serializing folder-state commits on
    /// `folder_state_commit`.
    pub(crate) fn import_services(
        &self,
        event_tx: crate::ImportEventBus,
        file_tags: Arc<dyn crate::file_tag_reader::FileTagReader>,
        directories: Arc<dyn crate::folder_scanner::DirectoryReader>,
        folder_state_commit: crate::FolderStateCommit,
    ) -> crate::ImportServices {
        let service = &self.service;
        crate::ImportServices {
            event_tx,
            database: self.database.clone(),
            tables: service.tables.clone(),
            catalogs: service.catalogs.clone(),
            remote_images: service.remote_images.clone(),
            preparations: self.preparations.clone(),
            clock: service.clock.clone(),
            ids: service.ids.clone(),
            file_tags,
            directories,
            folder_state_commit,
            import_cancels: Default::default(),
            diagnostics: service.diagnostics.clone(),
            closing: tokio_util::sync::CancellationToken::new(),
        }
    }

    /// A scan over this service's library, and the stream it announces on.
    pub(crate) fn scan(&self) -> (TestScan, crate::ImportEvents) {
        self.scan_with(
            Arc::new(crate::file_tag_reader::LoftyFileTagReader),
            Arc::new(crate::folder_scanner::OsDirectoryReader),
        )
    }

    /// The same scan, reading tags through `file_tags` and listings through
    /// `directories`.
    pub(crate) fn scan_with(
        &self,
        file_tags: Arc<dyn crate::file_tag_reader::FileTagReader>,
        directories: Arc<dyn crate::folder_scanner::DirectoryReader>,
    ) -> (TestScan, crate::ImportEvents) {
        let event_tx = crate::ImportEventBus::new(self.service.runtime.clone());
        let events = event_tx.every_event();
        let (fs_tx, fs_rx) = tokio::sync::mpsc::unbounded_channel();
        let scan = TestScan {
            services: test_scan_services(
                self,
                event_tx,
                Arc::new(FolderWatcher::new(fs_tx, bae_diagnostics::Diagnostics::noop())),
                file_tags,
                directories,
            ),
            cancellation: crate::folder_scanner::ScanCancellation::new(),
            _fs_rx: fs_rx,
        };
        (scan, events)
    }
}

pub(crate) async fn setup_import_service() -> TestService {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(
        crate::TEST_SCHEMA,
        db_path.to_str().unwrap(),
        Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();

    let preparations = crate::CandidatePreparations::new(database.clone());
    // The runtime writes its standings into the library the import commits
    // into.
    let tables = crate::tables::ImportTables::new(database.clone());
    let runtime =
        crate::CandidateRuntime::new(tables.clone(), bae_diagnostics::Diagnostics::noop());
    let catalogs = crate::CatalogSessions::new(
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        database.clone(),
        bae_diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
    );
    let (_commands_tx, commands_rx) = tokio::sync::mpsc::unbounded_channel();
    let event_tx = crate::ImportEventBus::new(runtime.clone());
    TestService {
        service: ImportService {
            commands_rx,
            event_tx,
            runtime,
            database: database.clone(),
            tables,
            catalogs,
            remote_images: bae_catalogs::remote_image::RemoteImageCache::for_test(
                bae_util::http::Http::for_test(),
                bae_diagnostics::Diagnostics::noop(),
            ),
            clock: Arc::new(coven::SystemClock),
            ids: Arc::new(coven::UuidIds),
            import_cancels: Default::default(),
            diagnostics: bae_diagnostics::Diagnostics::noop(),
        },
        preparations,
        database,
        temp: temp_dir,
    }
}

impl ImportService {
    /// Queue `import_id` of the candidate at `key` as asking for an import
    /// does — its claim on the candidate and its cancel — for a test that runs
    /// the worker's steps itself.
    pub(crate) async fn queue_import_for_test(&self, key: &crate::CandidateKey, import_id: &str) {
        self.runtime
            .claim_for_import(key, import_id, false)
            .await
            .expect("a test queues an import of a candidate no import owns");
        self.import_cancels.register(key, import_id);
    }
}

pub(crate) async fn prepare_named_candidate(
    service: &ImportService,
    preparations: &crate::CandidatePreparations,
    content_hash: &str,
    watched_folder_path: &std::path::Path,
    candidate_path: &crate::CandidateKey,
    album_title: &str,
) -> u64 {
    let preparation = crate::preparations::stored_preparation(&service.tables, content_hash)
        .await
        .unwrap()
        .expect("the scanned candidate has its initial preparation");
    let mut draft = preparation.metadata.draft;
    draft.album_title = album_title.to_string();
    draft.album_artist_assignments =
        vec![crate::Credited::plain(crate::ArtistAssignment::named("Artist Name"))];
    preparations
        .apply_source(
            watched_folder_path,
            &crate::CandidateAsRead {
                content_hash: content_hash.to_string(),
                file_edit_revision: preparation.file_edits.revision,
                metadata_revision: preparation.metadata_revision,
            },
            candidate_path,
            &crate::CandidateMetadataDraft {
                draft,
                provenance: preparation.metadata.provenance,
                cover: preparation.metadata.cover,
            },
        )
        .await
        .unwrap()
}

/// Store `folder` as a scanned candidate of its own watched root with a named
/// draft, returning its candidate key and what an import of it is queued
/// against: its revisions and the tag reading the scan stored.
pub(crate) async fn store_scanned_candidate(
    test: &TestService,
    folder: &Path,
    name: &str,
) -> (crate::CandidateKey, super::ImportExpectation) {
    let service = &test.service;
    let tables = &test.service.tables;
    let files = crate::folder_scanner::collect_release_candidate_files_with_scope(
        folder,
        crate::ReleaseFileScope::Recursive,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    let content_hash = files.content_hash();
    let candidate_key = crate::CandidateKey::folder(folder);
    tables.add_watched_import_folder(folder).await.unwrap();
    let generation = tables.begin_folder_scan(folder, crate::VolumeKind::Local).await.unwrap();
    crate::store_scan_item(
        tables,
        &bae_diagnostics::Diagnostics::noop(),
        folder,
        generation,
        &ScanItem::Valid(crate::folder_scanner::FolderCandidate {
            path: folder.to_path_buf(),
            file_root: folder.to_path_buf(),
            name: name.to_string(),
            files,
            watched_folder_path: folder.to_path_buf(),
            scope: crate::ReleaseFileScope::Recursive,
            file_edit_revision: 0,
            relative_folder: bae_db::RelativeFolder::test(name),
            grouping: None,
        }),
    )
    .await
    .unwrap()
    .expect("the stored scan generation is current");
    tables.finish_folder_scan(folder, generation, None).await.unwrap();
    let metadata_revision = prepare_named_candidate(
        service,
        &test.preparations,
        &content_hash,
        folder,
        &candidate_key,
        name,
    )
    .await;
    let file_tag_snapshot = tables
        .load_candidate_file_tag_snapshot(folder, &candidate_key)
        .await
        .unwrap()
        .expect("the candidate is stored")
        .snapshot;
    (
        candidate_key,
        super::ImportExpectation {
            candidate: crate::CandidateAsRead {
                content_hash,
                file_edit_revision: 0,
                metadata_revision,
            },
            file_tag_snapshot,
        },
    )
}

pub(crate) fn flac() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac"),
    )
    .unwrap()
}

pub(crate) fn write_test_jpeg(path: &Path) {
    let image = ::image::RgbImage::from_pixel(1, 1, ::image::Rgb([0, 0, 0]));
    image.save(path).unwrap();
}

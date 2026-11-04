//! The pane's commands as the app composes them over the import service and
//! the identification queue.

use super::*;
use bae_import::extraction::{ArtworkAnalysis, ArtworkAnalyzer};
use bae_import::identification::MAX_IN_FLIGHT;
use bae_import::{CandidateKey, ImportServiceHandle};
use bae_util::test_channels::HANG_GUARD;
use bae_util::test_host::FakeHost;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

/// A different barcode per numbered candidate folder, so each asks its own
/// lookup rather than hitting the response cache.
struct PerFolderBarcodeAnalyzer;

impl ArtworkAnalyzer for PerFolderBarcodeAnalyzer {
    fn analyze(&self, path: &Path) -> ArtworkAnalysis {
        let folder = path
            .parent()
            .and_then(|dir| dir.file_name())
            .and_then(|name| name.to_str())
            .expect("a candidate image sits in a named folder");
        let digits: String = folder.chars().filter(|c| c.is_ascii_digit()).collect();
        let ordinal: u32 = digits.parse().expect("a numbered candidate folder");
        ArtworkAnalysis {
            barcodes: vec![bae_types::barcode::with_check_digit(&format!("01234567{ordinal:04}"))],
            text_lines: Vec::new(),
        }
    }
}

/// The app's services over a library whose providers are `provider`, and the
/// import service under them.
struct Library {
    services: AppServices,
    import: ImportServiceHandle,
    database: bae_db::Database,
    provider: FakeHost,
    root: PathBuf,
    _temp: TempDir,
}

impl Library {
    async fn open() -> Self {
        bae_util::test_logs::install();
        let temp = TempDir::new().unwrap();
        let database = bae_db::Database::new_test(
            bae_library::SCHEMA,
            temp.path().join("test.db").to_str().unwrap(),
            Arc::new(coven::SystemClock),
        )
        .await
        .unwrap();
        let provider = FakeHost::start().await;
        let http = bae_util::http::Http::for_test()
            .serve("musicbrainz.org", provider.base_url())
            .serve("coverartarchive.org", provider.base_url())
            .serve("api.discogs.com", provider.base_url());
        let providers = bae_catalogs::providers::Providers::for_test(
            http.clone(),
            Arc::new(bae_catalogs::musicbrainz::MusicBrainz::new(http.clone())),
            Arc::new(bae_catalogs::discogs::client::Discogs::new(http.clone())),
            Arc::new(bae_catalogs::wikidata::Wikidata::new(http)),
        );
        let library = bae_library::test_libraries::TestLibrary::open(
            database.clone(),
            bae_app_dir::AppDir::under_home(temp.path()).unwrap(),
            providers,
        )
        .await;
        let import = library.start_import().await.unwrap();
        let services = AppServices::new(
            LibraryManager::clone(&library),
            bae_views::LibraryViews::new(library.database().clone()),
            library.start_playback().await,
            library.cast_picker(),
            import.clone(),
            import.start_identification_queue(),
        );
        let root = temp.path().join("watched");
        std::fs::create_dir_all(&root).unwrap();
        Self { services, import, database, provider, root, _temp: temp }
    }

    /// A candidate folder of two FLACs and one image, so identification runs
    /// through the artwork's barcode.
    fn barcode_candidate(&self, folder: &str) -> PathBuf {
        let dir = self.root.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["01 Test Track 1.flac", "02 Test Track 2.flac"] {
            std::fs::copy(bae_import::test_fixtures::flac_dir().join(name), dir.join(name))
                .unwrap();
        }
        std::fs::write(dir.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0, 0x00]).unwrap();
        dir
    }

    /// Watch the root and read it, returning once the list holds `expected`
    /// candidates.
    async fn scan(&self, expected: usize) {
        self.services.import_add_watched_folder(self.root.clone()).await.unwrap();
        self.services.import_refresh_watched_folder(self.root.clone()).await.unwrap();
        tokio::time::timeout(
            HANG_GUARD,
            self.import.wait_for_list(bae_import::ImportListView::default(), |projection| {
                projection.summary.counts.pending as usize
                    + projection.summary.counts.done as usize
                    + projection.summary.counts.skipped as usize
                    == expected
            }),
        )
        .await
        .expect("the read lists every candidate");
    }

    /// Whether the queue holds `key` waiting for a slot.
    async fn is_queued(&self, key: &CandidateKey) -> bool {
        self.import
            .candidate_runtime(key)
            .await
            .unwrap()
            .and_then(|runtime| runtime.identification_in_flight())
            == Some(bae_import::IdentificationInFlight::Queued)
    }

    /// Whether identification stored a verdict for `key`.
    async fn identified(&self, key: &CandidateKey) -> bool {
        bae_import::tables::ImportTables::new(self.database.clone())
            .load_import_candidate_states()
            .await
            .unwrap()
            .into_iter()
            .any(|(_, state)| state.candidate_key == *key && state.identify.is_some())
    }
}

/// A person cancelling a waiting candidate from its pane takes it off the
/// queue, stores nothing for it, and puts the pane back on the draft, as its
/// Back does.
#[tokio::test(flavor = "multi_thread")]
async fn cancelling_from_the_pane_takes_a_waiting_candidate_off_the_queue() {
    let library = Library::open().await;
    library.services.extraction_register_analyzer(Arc::new(PerFolderBarcodeAnalyzer));
    // Folders over the cap, each asking the provider a distinct question,
    // with the lookups held so the cap's worth run and the rest wait.
    let mut dirs = Vec::new();
    for index in 0..MAX_IN_FLIGHT + 1 {
        let dir = library.barcode_candidate(&format!("Album {index}"));
        std::fs::write(dir.join(format!("playlist-{index}.m3u")), format!("{index}")).unwrap();
        dirs.push(dir);
    }
    library.provider.route("/release?", 200, "{}");
    library.provider.hold("/release?");
    library.scan(MAX_IN_FLIGHT + 1).await;
    library.provider.wait_for_request("query=barcode", MAX_IN_FLIGHT).await;
    let mut waiting = None;
    for dir in &dirs {
        if library.is_queued(&CandidateKey::folder(dir)).await {
            waiting = Some(dir.clone());
            break;
        }
    }
    let waiting = waiting.expect("one candidate is over the cap and waiting");
    let key = CandidateKey::folder(&waiting);
    library.import.move_candidate_pane(&key, bae_import::PaneMove::Automatic).await.unwrap();

    library.services.pane_cancel_identification(&key).await.unwrap();

    assert!(
        library.import.candidate_runtime(&key).await.unwrap().is_none(),
        "nothing is waiting or running for it"
    );
    assert_eq!(
        library
            .import
            .load_candidate(&key)
            .await
            .unwrap()
            .expect("the candidate is still listed")
            .draft()
            .unwrap()
            .session
            .presentation,
        bae_import::MetadataPresentation::Draft,
        "the pane is back on the draft"
    );
    library.provider.release();
    for dir in dirs.iter().filter(|dir| **dir != waiting) {
        let key = CandidateKey::folder(dir);
        tokio::time::timeout(Duration::from_secs(30), async {
            while !library.identified(&key).await {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("no run stored a verdict for {}", dir.display()));
    }
    assert_eq!(
        library.provider.count_containing("query=barcode"),
        MAX_IN_FLIGHT,
        "the cancelled candidate was never looked up: {:?}",
        library.provider.requests()
    );
    assert!(!library.identified(&key).await);
}

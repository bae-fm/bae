//! Identification-queue tests. Only the providers are fake: a local server per
//! fixture answers their URLs and records each request.

use super::*;
use crate::app::test_library::TestLibrary;
use crate::db::Database;
use crate::identify::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};
use crate::identify::{FolderCheck, VerdictSummary};
use crate::import::extraction::{ArtworkAnalysis, ArtworkAnalyzer};
use crate::import::tables::{
    DbCandidateIdentifyResult, DbImportCandidateState, NewImportCandidateVerdict,
};
use crate::import::{FolderCandidate, ImportCandidateSnapshot};
use bae_catalogs::search::{MetadataResult, SourceTracks};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tempfile::TempDir;

/// The disc ID `store_settled_verdict` seeds, and the file it names.
const SEEDED_DISC_ID: &str = "XwqRcz4RhAqRTfhE5nRxRKF4iFY-";
const SEEDED_DISC_ID_FILE: &str = "Album.log";
const FIXTURE_DISC_ID: &str = "ayQ_jFizitCdB_btUSn6qV6ENaI-";

/// The modification time every copied fixture file carries, since the content
/// hash covers it and `std::fs::copy` keeps it on macOS but not on Linux.
fn fixture_modified_at() -> std::time::SystemTime {
    // 2020-01-01T00:00:00Z.
    std::time::UNIX_EPOCH + Duration::from_secs(1_577_836_800)
}

/// Copy a fixture file into a candidate folder, stamped with
/// [`fixture_modified_at`].
fn copy_fixture(source: &Path, target: &Path) {
    std::fs::copy(source, target).unwrap();
    std::fs::File::options()
        .write(true)
        .open(target)
        .unwrap()
        .set_modified(fixture_modified_at())
        .unwrap();
}

/// Settled signals that found nothing.
fn settled_signals() -> Signals {
    Signals {
        origin: bae_catalogs::AudioOrigin::default(),
        disc_id: DiscIdSignal::Absent,
        barcode: BarcodeSignal::Absent,
        text: TextSignal::Settled { catalogs: Vec::new(), free_text: Vec::new() },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    }
}

// ── The fake provider ───────────────────────────────────────────────────────

/// A local HTTP server standing in for the providers. Routes match by
/// substring of the request target, in the order added; every request is
/// recorded.
struct FakeProvider {
    base_url: String,
    state: Arc<Mutex<FakeState>>,
}

#[derive(Default)]
struct FakeState {
    routes: Vec<(String, u16, Vec<u8>)>,
    requests: Vec<String>,
    /// While set, a request containing any of the needles waits here after
    /// recording itself.
    gate: Option<(Vec<String>, Arc<tokio::sync::Semaphore>)>,
}

impl FakeProvider {
    async fn start() -> Self {
        let listener =
            tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("fake provider binds");
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(FakeState::default()));
        let accept_state = state.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let state = accept_state.clone();
                tokio::spawn(async move { serve_one(stream, state).await });
            }
        });
        FakeProvider { base_url, state }
    }

    /// Answer any request containing `needle` with `status` and `body`; an
    /// earlier route wins over a later one.
    fn route(&self, needle: &str, status: u16, body: impl Into<Vec<u8>>) {
        self.state.lock().unwrap().routes.push((needle.to_string(), status, body.into()));
    }

    /// Answer requests containing `needle` with `status` and `body` from now
    /// on, in place of what the route for it said before.
    fn reroute(&self, needle: &str, status: u16, body: impl Into<Vec<u8>>) {
        let mut state = self.state.lock().unwrap();
        state.routes.retain(|(existing, _, _)| existing != needle);
        state.routes.insert(0, (needle.to_string(), status, body.into()));
    }

    /// Replace every route.
    fn set_routes(&self, routes: Vec<(&str, u16, String)>) {
        let mut state = self.state.lock().unwrap();
        state.routes = routes
            .into_iter()
            .map(|(needle, status, body)| (needle.to_string(), status, body.into_bytes()))
            .collect();
    }

    /// Leave every request matching `needle` unanswered until
    /// [`Self::release`], so a test can act on a lookup in flight.
    fn hold(&self, needle: &str) {
        self.hold_each(&[needle]);
    }

    /// [`Self::hold`] every request matching any of `needles`.
    fn hold_each(&self, needles: &[&str]) {
        self.state.lock().unwrap().gate = Some((
            needles.iter().map(|needle| needle.to_string()).collect(),
            Arc::new(tokio::sync::Semaphore::new(0)),
        ));
    }

    /// Answer one held request, the next to reach the hold if none is
    /// waiting, and go on holding the rest.
    fn let_one_through(&self) {
        if let Some((_, gate)) = self.state.lock().unwrap().gate.as_ref() {
            gate.add_permits(1);
        }
    }

    /// Answer everything held, and let later requests through.
    fn release(&self) {
        if let Some((_, gate)) = self.state.lock().unwrap().gate.take() {
            gate.close();
        }
    }

    fn requests(&self) -> Vec<String> {
        self.state.lock().unwrap().requests.clone()
    }

    fn count_containing(&self, needle: &str) -> usize {
        self.requests().iter().filter(|target| target.contains(needle)).count()
    }
}

/// The id of the next run of `key` whose reported state `accept` answers.
async fn await_run_state(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<ImportEvent>,
    key: &crate::import::CandidateKey,
    accept: impl Fn(IdentifyRunId, &IdentifyState) -> bool,
) -> IdentifyRunId {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match events.recv().await.expect("the import event bus stays open") {
                ImportEvent::IdentifyStateChanged { candidate_key, run, state, .. }
                    if candidate_key == key.clone() && accept(run, &state) =>
                {
                    return run
                }
                _ => continue,
            }
        }
    })
    .await
    .expect("a run of the candidate reports the awaited state")
}

/// Every event `events` holds so far.
fn drain_events(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<ImportEvent>,
) -> Vec<ImportEvent> {
    let mut drained = Vec::new();
    loop {
        match events.try_recv() {
            Ok(event) => drained.push(event),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => return drained,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                panic!("the import event bus closed while draining ready events")
            }
        }
    }
}

async fn wait_for_request(provider: &FakeProvider, needle: &str, count: usize) {
    if tokio::time::timeout(Duration::from_secs(10), async {
        while provider.count_containing(needle) < count {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .is_err()
    {
        panic!(
            "the provider received no request containing {needle:?} (wanted {count}); \
             requests so far: {:?}",
            provider.requests()
        );
    }
}

include!("tests/provider.rs");

// ── The fixture ─────────────────────────────────────────────────────────────

/// The candidate audio: two real FLACs, so the fast pass has durations to
/// measure.
const FLAC_FIXTURES: [&str; 2] = ["01 Test Track 1.flac", "02 Test Track 2.flac"];

/// Reads one barcode off every image, for a folder with no LOG or CUE.
struct BarcodeAnalyzer {
    barcode: String,
}

impl ArtworkAnalyzer for BarcodeAnalyzer {
    fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
        ArtworkAnalysis { barcodes: vec![self.barcode.clone()], text_lines: Vec::new() }
    }
}

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

/// An analyzer held at each image behind a [`crate::test_gate::Gate`].
struct GatedAnalyzer(crate::test_gate::Held);

struct CountingAnalyzer {
    calls: Arc<AtomicUsize>,
}

impl ArtworkAnalyzer for CountingAnalyzer {
    fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
        self.calls.fetch_add(1, Ordering::Relaxed);
        ArtworkAnalysis::empty()
    }
}

impl ArtworkAnalyzer for GatedAnalyzer {
    fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
        self.0.pass();
        ArtworkAnalysis::empty()
    }
}

struct Fixture {
    manager: TestLibrary,
    /// The store under `manager`, for a test that breaks a row directly.
    database: Database,
    /// The writer the handle uses, for tests that write a candidate directly.
    preparations: crate::import::CandidatePreparations,
    import: ImportServiceHandle,
    provider: FakeProvider,
    /// The catalog sessions the library and its import ask through, which a
    /// test stores a Discogs key with.
    catalogs: crate::import::CatalogSessions,
    /// The MusicBrainz client those sessions ask, which a test seeds answers
    /// into and reads the waiting requests of.
    musicbrainz: Arc<bae_catalogs::musicbrainz::MusicBrainz>,
    /// The services the queue runs on, for the tests that drive one step of it
    /// directly.
    context: Context,
    /// The queue, started the first time a test reaches for it, so a test can
    /// set a candidate up before a found release's run reads it.
    identification: OnceLock<IdentificationHandle>,
    root: PathBuf,
    _temp: TempDir,
}

/// The fixed instant every stored row is stamped with.
fn fixed_now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2024-03-01T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc)
}

impl Fixture {
    async fn new(name: &str) -> Self {
        Self::with_ids(name, Arc::new(coven::SequentialIds::new())).await
    }

    /// A fixture that makes UUIDs, which an import's release rows need.
    async fn importing(name: &str) -> Self {
        Self::with_ids(name, Arc::new(coven::UuidIds)).await
    }

    async fn with_ids(_name: &str, ids: coven::IdSourceRef) -> Self {
        bae_util::test_logs::install();
        let temp = TempDir::new().unwrap();
        let clock: coven::ClockRef = Arc::new(coven::FixedClock::new((fixed_now()).into()));
        let database =
            Database::new_test(temp.path().join("test.db").to_str().unwrap(), clock.clone())
                .await
                .unwrap();
        let preparations = crate::import::CandidatePreparations::new(database.clone());

        let provider = FakeProvider::start().await;
        // Discogs goes to the fake too, so no test reaches the real API.
        let http = bae_util::http::Http::for_test()
            .serve("musicbrainz.org", &provider.base_url)
            .serve("coverartarchive.org", &provider.base_url)
            .serve("api.discogs.com", &provider.base_url);
        // Production request spacing: admission order is part of what these
        // tests measure.
        let musicbrainz = Arc::new(bae_catalogs::musicbrainz::MusicBrainz::new(http.clone()));
        let providers = bae_catalogs::providers::Providers::for_test(
            http.clone(),
            musicbrainz.clone(),
            Arc::new(bae_catalogs::discogs::client::Discogs::new(http.clone())),
            Arc::new(bae_catalogs::wikidata::Wikidata::new(http)),
        );
        // No Discogs key, so lookups ask MusicBrainz alone unless a test calls
        // `use_discogs`.
        let manager = TestLibrary::open_with(
            database.clone(),
            bae_app_dir::AppDir::under_home(temp.path()).unwrap(),
            providers,
            clock,
            ids,
            bae_diagnostics::Diagnostics::noop(),
        )
        .await;
        let catalogs = manager.catalogs().clone();

        let import = manager.start_import().await.unwrap();

        let root = temp.path().join("watched");
        std::fs::create_dir_all(&root).unwrap();

        let context = Context {
            import: import.clone(),
            database: database.clone(),
            tables: crate::import::tables::ImportTables::new(database.clone()),
            diagnostics: bae_diagnostics::Diagnostics::noop(),
        };
        Fixture {
            manager,
            database,
            preparations,
            import,
            provider,
            catalogs,
            musicbrainz,
            context,
            identification: OnceLock::new(),
            root,
            _temp: temp,
        }
    }

    /// The running queue, started on first use.
    fn identification(&self) -> &IdentificationHandle {
        self.identification.get_or_init(|| self.import.start_identification_queue())
    }

    /// A candidate folder with two FLACs and a rip log, so the disc ID computes.
    fn disc_id_candidate(&self, folder: &str) -> PathBuf {
        let dir = self.candidate_dir(folder);
        copy_fixture(Path::new("tests/fixtures/logs/test_album.log"), &dir.join("test_album.log"));
        dir
    }

    /// A candidate folder with two FLACs and one image and no LOG or CUE, so
    /// identification runs through the artwork barcode.
    fn barcode_candidate(&self, folder: &str) -> PathBuf {
        let dir = self.candidate_dir(folder);
        std::fs::write(dir.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0, 0x00]).unwrap();
        dir
    }

    fn candidate_dir(&self, folder: &str) -> PathBuf {
        // Part by part, so a nested folder is spelled as the scan spells its
        // key on every host.
        let dir = folder.split('/').fold(self.root.clone(), |dir, part| dir.join(part));
        std::fs::create_dir_all(&dir).unwrap();
        for name in FLAC_FIXTURES {
            copy_fixture(&Path::new("tests/fixtures/flac").join(name), &dir.join(name));
        }
        dir
    }

    fn probed_total_ms(&self, dir: &Path) -> u64 {
        FLAC_FIXTURES
            .iter()
            .map(|name| {
                bae_audio::probe_audio_from_path(&dir.join(name))
                    .expect("fixture FLAC probes")
                    .duration
                    .as_millis() as u64
            })
            .sum()
    }

    /// Copy the whole cue_flac fixture into `<root>/<name>` and return it.
    fn seed_cue_album(&self, name: &str) -> PathBuf {
        let dir = self.root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for file in [
            "Test Album.cue",
            "Test Album.flac",
            "02 Test Artist - Track Two (White Noise).flac",
            "03 Test Artist - Track Three (Brown Noise).flac",
        ] {
            copy_fixture(&Path::new("tests/fixtures/cue_flac").join(file), &dir.join(file));
        }
        dir
    }

    /// Watch the root and read it, returning once the read is over — by which
    /// point every release it found has been handed to automatic
    /// identification — and the list holds `expected` candidates.
    async fn scan(&self, expected: usize) {
        let root = self.root.to_path_buf();
        self.import.add_watched_folder(root).await.unwrap();
        self.rescan(&self.import, expected).await;
    }

    /// Read the watched root again through `import`, as a launch does, and
    /// wait as [`Self::scan`] does. A refresh answers once its read is over,
    /// which is what makes the wait exact rather than a matter of time.
    async fn rescan(&self, import: &ImportServiceHandle, expected: usize) {
        self.read_root(import, &self.root).await;
        self.await_listed(import, expected).await;
    }

    /// Read the watched folder `root` through `import`, returning once the
    /// read is over.
    async fn read_root(&self, import: &ImportServiceHandle, root: &Path) {
        import.refresh_watched_folder(root.to_path_buf()).await.unwrap();
    }

    /// The list holds `expected` candidates, as the reads already over left it.
    async fn await_listed(&self, import: &ImportServiceHandle, expected: usize) {
        tokio::time::timeout(
            Duration::from_secs(30),
            import.wait_for_list(crate::import::ImportListView::default(), |projection| {
                projection.summary.counts.pending as usize
                    + projection.summary.counts.done as usize
                    + projection.summary.counts.skipped as usize
                    == expected
            }),
        )
        .await
        .expect("the completed read lists every fixture candidate");
    }

    /// Ask to identify `dir`, as a person does.
    fn start_explicit_lookup(&self, dir: &Path) {
        self.identification().rerun_identify(crate::import::CandidateKey::folder(dir));
    }

    /// Ask to identify `dir` and wait until its run is registered.
    async fn start_explicit_lookup_and_await_run(&self, dir: &Path) {
        let key = crate::import::CandidateKey::folder(dir);
        self.start_explicit_lookup(dir);
        tokio::time::timeout(Duration::from_secs(10), async {
            while !self.import.is_identifying(&key) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("identify registers the driver for the opened candidate");
    }

    /// The candidate's row once a run has stored its verdict, waited for with
    /// a bound.
    async fn await_identified_row(&self, dir: &Path) -> DbImportCandidateState {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if let Some(row) = self.stored_for(dir).await {
                    if row.identify.is_some() {
                        return row;
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "no run stored a verdict for {}; requests so far: {:?}",
                dir.display(),
                self.provider.requests()
            )
        })
    }

    fn count_release_lookups(&self, release_id: &str) -> usize {
        self.provider.count_containing(&format!("/release/{release_id}?"))
    }

    fn context(&self) -> Context {
        self.context.clone()
    }

    /// Start the queue if needed, and wait until every release found so far is
    /// admitted and every automatic job has ended.
    async fn drain_automatic(&self) {
        tokio::time::timeout(
            Duration::from_secs(30),
            self.identification().automatic_drained_for_test(),
        )
        .await
        .expect("the automatic admission's queue drains");
    }

    /// The same, as a task a test can watch while it acts on the queue.
    fn drain_automatic_task(&self) -> tokio::task::JoinHandle<()> {
        let identification = self.identification().clone();
        tokio::spawn(async move { identification.automatic_drained_for_test().await })
    }

    /// Where the queue has `key`, as the runtime publishes it.
    async fn identification_status(
        &self,
        key: &crate::import::CandidateKey,
    ) -> Option<crate::import::IdentificationStatus> {
        self.import.standing_facts_for_test(key).await.identification
    }

    /// The first of `dirs` the queue holds waiting.
    async fn first_queued(&self, dirs: &[PathBuf]) -> Option<PathBuf> {
        for dir in dirs {
            if self.identification_status(&crate::import::CandidateKey::folder(dir)).await
                == Some(crate::import::IdentificationStatus::Queued)
            {
                return Some(dir.clone());
            }
        }
        None
    }

    async fn stored(&self) -> BTreeMap<String, DbImportCandidateState> {
        self.context.tables.load_import_candidate_states().await.unwrap().into_iter().collect()
    }

    /// A candidate folder's content hash, read off disk.
    fn content_hash(&self, dir: &Path) -> String {
        crate::import::folder_scanner::collect_release_candidate_files_with_scope(
            dir,
            crate::import::ReleaseFileScope::Recursive,
            &crate::import::folder_scanner::StoredCandidateEdits::none(),
        )
        .expect("the candidate folder is readable")
        .content_hash()
    }

    async fn stored_for(&self, dir: &Path) -> Option<DbImportCandidateState> {
        let hash = self.content_hash(dir);
        self.stored().await.remove(&hash)
    }

    async fn identified_for(&self, dir: &Path) -> Option<DbCandidateIdentifyResult> {
        self.stored_for(dir).await.and_then(|row| row.identify)
    }

    /// One candidate's pane as it reads back from the tables.
    async fn pane(&self, dir: &Path) -> Option<crate::import::ImportCandidateDetail> {
        self.import.load_candidate(&crate::import::CandidateKey::folder(dir)).await.unwrap()
    }

    /// The stored MusicBrainz release, if a fetch stored one.
    async fn stored_release(
        &self,
        release_id: &str,
    ) -> Option<bae_catalogs::source_release::SourceRelease> {
        let tables = self.context.tables.clone();
        tables
            .load_source_release(&bae_catalogs::records::LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                release_id,
            ))
            .await
            .unwrap()
    }

    /// The stored Discogs release, if a fetch stored one.
    async fn stored_discogs_release(
        &self,
        release_id: &str,
    ) -> Option<bae_catalogs::source_release::SourceRelease> {
        let tables = self.context.tables.clone();
        tables
            .load_source_release(&bae_catalogs::records::LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::Discogs,
                release_id,
            ))
            .await
            .unwrap()
    }

    /// Store a fake Discogs key, so lookups ask both providers.
    async fn use_discogs(&self) {
        self.catalogs
            .store_discogs_key("test-discogs-token", crate::preferences::DiscogsValidation::Valid)
            .await
            .expect("the fake Discogs key is stored");
    }

    /// Store a release directly, as a settle step would have.
    async fn archive(&self, release_id: &str, group_id: &str, track_lengths: &[u64]) {
        self.context
            .tables
            .save_source_release(&stored_pressing(release_id, group_id, track_lengths))
            .await
            .unwrap();
    }

    /// Store a release whose group failed to fetch.
    async fn archive_missing_its_group(
        &self,
        release_id: &str,
        group_id: &str,
        track_lengths: &[u64],
    ) {
        let mut release = stored_pressing(release_id, group_id, track_lengths);
        release.unfetched.push(bae_catalogs::source_release::UnfetchedDocument {
            document: bae_catalogs::records::PayloadSource::MusicBrainzReleaseGroup,
            key: group_id.to_string(),
            reason: bae_catalogs::source_release::UnfetchedReason::Failed,
        });
        self.context.tables.save_source_release(&release).await.unwrap();
    }

    /// Store the verdict a settled lead produces, without running the pipeline.
    async fn store_settled_verdict(&self, dir: &Path, release_id: &str, group_id: &str) {
        self.store_settled_verdict_listing(
            dir,
            release_id,
            group_id,
            SourceTracks::Listed { count: 2 },
        )
        .await;
    }

    /// `store_settled_verdict`, with the lead stating `source_tracks`.
    async fn store_settled_verdict_listing(
        &self,
        dir: &Path,
        release_id: &str,
        group_id: &str,
        source_tracks: SourceTracks,
    ) {
        self.store_verdict_settled_on(
            dir,
            release_id,
            group_id,
            source_tracks,
            SettledDraft::Picked,
        )
        .await;
    }

    /// The verdict a settled lead produces with no pick behind it: the lead
    /// names `release_id`, and the draft is left as it is.
    async fn store_settled_lead_without_its_pick(
        &self,
        dir: &Path,
        release_id: &str,
        group_id: &str,
    ) {
        self.store_verdict_settled_on(
            dir,
            release_id,
            group_id,
            SourceTracks::Listed { count: 2 },
            SettledDraft::Untouched,
        )
        .await;
    }

    async fn store_verdict_settled_on(
        &self,
        dir: &Path,
        release_id: &str,
        group_id: &str,
        source_tracks: SourceTracks,
        settled_draft: SettledDraft,
    ) {
        let candidate = self
            .import
            .answerable_candidate(&crate::import::CandidateKey::folder(dir))
            .await
            .expect("the candidate state is readable")
            .expect("the scanned candidate is answerable");
        let mut draft = candidate.blank_draft();
        draft.album_title = "Album".to_string();
        draft.album_artist_assignments =
            vec![crate::import::Credited::plain(crate::import::ArtistAssignment::Credit {
                credit: bae_catalogs::credits::ArtistCredit {
                    credited_as: None,
                    name: "Artist".to_string(),
                    sort_name: None,
                    musicbrainz_artist_id: None,
                    discogs_artist_id: None,
                },
            })];
        for (index, track) in draft.tracks.iter_mut().enumerate() {
            track.edit.title = format!("Track {}", index + 1);
        }
        let verdict = TerminalVerdict::Found {
            findings: crate::identify::Findings {
                matches: vec![MetadataResult {
                    source: bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    release_id: release_id.to_string(),
                    title: "Album".to_string(),
                    artist: Some("Artist".to_string()),
                    year: None,
                    labels: Vec::new(),
                    area: None,
                    status: None,
                    packaging: None,
                    discogs_details: Vec::new(),
                    barcodes: Vec::new(),
                    media: bae_types::pressing::StatedMedia::Undescribed,
                    links: Vec::new(),
                    cover_art: None,
                    source_group_id: Some(group_id.to_string()),
                    album_links: bae_catalogs::album_links::AlbumLinks::NotAsked,
                    source_tracks: Some(source_tracks),
                    document_failure: None,
                    album_first_year: None,
                    track_titles: Vec::new(),
                    notes: Vec::new(),
                }],
                pressings: vec![0],
                narrowed_out: crate::identify::NarrowedOut::default(),
                medium_conflict: None,
                named_notes: Vec::new(),
            },
            track_count: 2,
            ledger: None,
        };
        let wrote = self
            .import
            .save_candidate_verdict_if_current(
                &crate::import::CandidateKey::folder(dir),
                IdentifyRunId::for_test(1),
                &NewImportCandidateVerdict {
                    content_hash: self.content_hash(dir),
                    file_edit_revision: 0,
                    candidate_key: crate::import::CandidateKey::folder(dir),
                    verdict,
                    // A disc ID naming its log, for the evidence chip.
                    signals: Signals {
                        disc_id: DiscIdSignal::Computed {
                            discs: vec![crate::identify::signals::ComputedDiscId {
                                disc_id: SEEDED_DISC_ID.to_string(),
                                source_file: Some(SEEDED_DISC_ID_FILE.to_string()),
                            }],
                        },
                        ..settled_signals()
                    },
                    pick: matches!(settled_draft, SettledDraft::Picked).then(|| {
                        let record = bae_catalogs::records::LookupRef::new(
                            bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                            release_id.to_string(),
                        );
                        crate::import::tables::VerdictPick {
                            link: crate::identify::PressingLink {
                                record: record.clone(),
                                partners: vec![],
                            },
                            metadata: crate::import::CandidateMetadataDraft {
                                draft,
                                provenance: Some(
                                    crate::import::MetadataProvenance::ExternalRelease { record },
                                ),
                                cover: None,
                                assets: crate::import::CandidatePreparedAssets::default(),
                            },
                        }
                    }),
                    releases: Vec::new(),
                },
            )
            .await
            .unwrap();
        assert!(
            matches!(wrote, crate::import::VerdictWrite::Stored { .. }),
            "the seeded verdict lands"
        );
    }

    /// Whether a stored row's verdict is auto-importable, and the check
    /// against the folder it failed.
    async fn judgement_for(&self, dir: &Path) -> (bool, Option<FolderCheck>) {
        let row = self.stored_for(dir).await.expect("a row was stored");
        VerdictSummary::of(&identify_result(&row).verdict, false).judgement()
    }
}

/// The release a lone MusicBrainz release document extracts to.
fn stored_pressing(
    release_id: &str,
    group_id: &str,
    track_lengths: &[u64],
) -> bae_catalogs::source_release::SourceRelease {
    bae_catalogs::payloads::ReleasePayloads::for_test(
        bae_catalogs::records::LookupRef::new(
            bae_catalogs::catalog::LookupCatalog::MusicBrainz,
            release_id,
        ),
        release_json(release_id, group_id, track_lengths),
        Vec::new(),
    )
    .extract()
    .unwrap()
}

/// Whether a seeded settled verdict carries the pick a settle writes with it.
enum SettledDraft {
    Picked,
    Untouched,
}

/// The identify half of a stored row, which every row identification wrote has.
fn identify_result(
    row: &DbImportCandidateState,
) -> &crate::import::tables::DbCandidateIdentifyResult {
    row.identify.as_ref().expect("a row identification wrote carries its identify result")
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(identification) = self.identification.get() {
            identification.stop();
        }
        self.import.stop_and_join();
    }
}

// ── 1. A candidate nobody selected acquires a verdict ────────────────────────

include!("tests/identification.rs");
include!("tests/lookup_choices.rs");
include!("tests/settling.rs");
include!("tests/catalog_searches.rs");
include!("tests/disc_id_answers.rs");
include!("tests/applying_the_pick.rs");
include!("tests/fitting_the_folder.rs");
include!("tests/metadata_modes.rs");
include!("tests/imports_and_progress.rs");
include!("tests/persistence.rs");
include!("tests/stored_picks.rs");
include!("tests/settled_panes.rs");
include!("tests/candidate_decisions.rs");
include!("tests/cancellation.rs");
include!("tests/cancelled.rs");
include!("tests/requested.rs");
include!("tests/admissions.rs");
include!("tests/automatic.rs");
include!("tests/row_live_state.rs");
include!("tests/stored_standings.rs");
include!("tests/auto_import.rs");
include!("tests/reading_the_pick.rs");

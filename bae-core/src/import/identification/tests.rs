//! Identification-queue tests.
//!
//! Every one of them drives the real pipeline — folder scan, extraction,
//! identify reducer, the real MusicBrainz client — and fakes only the provider,
//! at the wire. Each fixture's providers send MusicBrainz, Cover Art Archive and
//! Discogs requests to its own local server, which answers the same URLs the
//! live services do and counts what was asked for, so "did the queue re-fetch
//! this?" is answered by request counts rather than by a stub the queue was
//! handed. Nothing is shared between fixtures, so the tests run in parallel.

use super::*;
use crate::config::{Config, ConfigHandle};
use crate::db::{
    Database, DbCandidateIdentifyResult, DbImportCandidateState, NewImportCandidateVerdict,
};
use crate::identify::ready::{classify, NeedsYou, QueueClassification};
use crate::import::search::{MetadataResult, SourceTracks};
use crate::import::{FolderCandidate, ImportCandidateSnapshot};
use crate::library::LibraryManager;
use crate::signals::{ArtworkAnalysis, ArtworkAnalyzer, DetectedBarcode};
use crate::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex, OnceLock};
use std::time::Duration;
use tempfile::TempDir;

/// Settled signals carrying `durations` and nothing found — what a verdict a
/// test seeds stores beside itself.
/// The disc ID `store_settled_verdict` seeds, and the candidate file it names
/// — what the resumed pane's evidence points at.
const SEEDED_DISC_ID: &str = "XwqRcz4RhAqRTfhE5nRxRKF4iFY-";
const SEEDED_DISC_ID_FILE: &str = "Album.log";
const FIXTURE_DISC_ID: &str = "ayQ_jFizitCdB_btUSn6qV6ENaI-";

/// The modification time every copied fixture file carries. A candidate's
/// content hash covers each file's modification time, and two folders built
/// from the same fixtures are the same candidate only when their copies agree
/// on it. `std::fs::copy` keeps the source's time on macOS and stamps the
/// current time on Linux, so the fixture sets it rather than inheriting
/// whichever the platform gives.
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

fn settled_signals(durations: crate::import::probe::SourceDurations) -> Signals {
    Signals {
        disc_id: DiscIdSignal::Absent { track_count: 0 },
        barcode: BarcodeSignal::Absent,
        text: TextSignal::Settled {
            catalogs: Vec::new(),
            free_text: Vec::new(),
        },
        text_pool: Vec::new(),
        durations,
    }
}

// ── The fake provider ───────────────────────────────────────────────────────

/// A local HTTP server standing in for MusicBrainz. Routes are matched by
/// substring against the request target in the order they were added, so
/// `"/discid/"` catches any disc ID and `"/release/mb-1?"` catches one release
/// lookup. Every request is recorded, whether or not it matched.
struct FakeProvider {
    base_url: String,
    state: Arc<Mutex<FakeState>>,
}

#[derive(Default)]
struct FakeState {
    routes: Vec<(String, u16, String)>,
    requests: Vec<String>,
    /// While set, a request whose target contains the needle records itself
    /// and then waits here, so a test acts on a lookup that is genuinely in
    /// flight.
    gate: Option<(String, Arc<tokio::sync::Semaphore>)>,
}

impl FakeProvider {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("fake provider binds");
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

    /// Answer any request whose target contains `needle` with `status` + `body`.
    /// A later route never shadows an earlier one, so a test can add a specific
    /// route first and a catch-all after it.
    fn route(&self, needle: &str, status: u16, body: impl Into<String>) {
        self.state
            .lock()
            .unwrap()
            .routes
            .push((needle.to_string(), status, body.into()));
    }

    /// Replace every route. Used to flip the provider from failing to healthy
    /// between two sweep passes.
    fn set_routes(&self, routes: Vec<(&str, u16, String)>) {
        let mut state = self.state.lock().unwrap();
        state.routes = routes
            .into_iter()
            .map(|(needle, status, body)| (needle.to_string(), status, body))
            .collect();
    }

    /// Leave every request matching `needle` unanswered until
    /// [`Self::release`], so a test can act on a lookup that is in flight. A
    /// rendezvous rather than a delay: what the test does next never has to
    /// beat a clock, which is the whole class of flake a loaded machine finds.
    fn hold(&self, needle: &str) {
        self.state.lock().unwrap().gate =
            Some((needle.to_string(), Arc::new(tokio::sync::Semaphore::new(0))));
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
        self.requests()
            .iter()
            .filter(|target| target.contains(needle))
            .count()
    }
}

/// The id of the next run of `key` whose broadcast state `accept` answers.
///
/// The rendezvous a restart needs: a run ends at its own verdict, so a test
/// that releases a held lookup before the replacement run exists is racing the
/// run it means to supersede. Waiting for the replacement's first state is
/// waiting for the cancel that `IdentifyServiceHandle::start` does first.
async fn await_run_state(
    events: &mut tokio::sync::broadcast::Receiver<ImportEvent>,
    key: &str,
    accept: impl Fn(IdentifyRunId, &IdentifyState) -> bool,
) -> IdentifyRunId {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match events
                .recv()
                .await
                .expect("the import event bus stays open")
            {
                ImportEvent::IdentifyStateChanged {
                    candidate_key,
                    run,
                    state,
                    ..
                } if candidate_key == key && accept(run, &state) => return run,
                _ => continue,
            }
        }
    })
    .await
    .expect("a run of the candidate broadcasts the awaited state")
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

/// Where the candidate audio comes from. The two FLACs are real files with real
/// durations, so the probe in the fast pass has something to measure.
const FLAC_FIXTURES: [&str; 2] = ["01 Test Track 1.flac", "02 Test Track 2.flac"];

/// A barcode-only analyzer: the folder gets a barcode signal without a LOG or
/// CUE, so the disc-ID pipe is skipped and identification goes through the
/// search endpoint — the path that carries no lengths.
struct BarcodeAnalyzer {
    barcode: String,
}

impl ArtworkAnalyzer for BarcodeAnalyzer {
    fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
        ArtworkAnalysis {
            barcodes: vec![DetectedBarcode {
                payload: self.barcode.clone(),
                region: None,
            }],
            text_lines: Vec::new(),
        }
    }
}

/// A different barcode per candidate folder, so a flood of candidates is a
/// flood of distinct lookups. Candidates that ask the same question of the
/// provider are answered once from the response cache and queue nothing.
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
            barcodes: vec![DetectedBarcode {
                payload: format!("012345678{ordinal:04}"),
                region: None,
            }],
            text_lines: Vec::new(),
        }
    }
}

/// An OCR stub held between entry and completion, so a test can act while a
/// candidate is genuinely mid-extraction without depending on scheduling.
struct GatedAnalyzer {
    started: Arc<Barrier>,
    release: Arc<Barrier>,
}

struct SlowAnalyzer {
    delay: Duration,
}

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
        self.started.wait();
        self.release.wait();
        ArtworkAnalysis::empty()
    }
}

impl ArtworkAnalyzer for SlowAnalyzer {
    fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
        std::thread::sleep(self.delay);
        ArtworkAnalysis::empty()
    }
}

struct Fixture {
    manager: LibraryManager,
    /// The writer the handle uses, for tests that write a candidate directly.
    preparations: crate::import::CandidatePreparations,
    import: ImportServiceHandle,
    provider: FakeProvider,
    /// The services the queue runs on, for the tests that drive one step of it
    /// directly.
    context: Context,
    /// The queue itself, started the first time a test reaches for it.
    ///
    /// Started late on purpose: the loop admits candidates as the scan
    /// announces them, so a fixture that started one in `new` would identify
    /// every test's fixtures before the test had said what it was testing.
    identification: OnceLock<IdentificationHandle>,
    root: PathBuf,
    _temp: TempDir,
}

/// The fixed instant every stored row is stamped with. A deterministic clock
/// rather than wall time, so `identified_at` is an assertable value.
fn fixed_now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2024-03-01T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc)
}

impl Fixture {
    async fn new(name: &str) -> Self {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_test_writer()
            .with_ansi(false)
            .try_init();
        let temp = TempDir::new().unwrap();
        let clock: coven::ClockRef = Arc::new(coven::FixedClock(fixed_now()));
        let ids: coven::IdRef = Arc::new(coven::SequentialIdProvider::new(name));
        let database = Database::new_test(
            temp.path().join("test.db").to_str().unwrap(),
            clock.clone(),
            ids.clone(),
        )
        .await
        .unwrap();
        let preparations = crate::import::CandidatePreparations::new(database.clone());
        let library_dir = coven::StoreDir::new(temp.path());
        let library_id = format!("sweep-{name}-{}", uuid::Uuid::new_v4());
        let config = Config::with_defaults(
            library_id.clone(),
            "test-device".to_string(),
            library_dir,
            "Test Library".to_string(),
        );
        crate::config::install_test_keyring();
        let provider = FakeProvider::start().await;
        // Discogs goes to the fake whether or not this test enables it, so no
        // fixture can spend its fake key on the real API.
        let http = crate::util::http::Http::for_test()
            .serve("musicbrainz.org", &provider.base_url)
            .serve("coverartarchive.org", &provider.base_url)
            .serve("api.discogs.com", &provider.base_url);
        // No Discogs key is seeded, so Discogs operations are unavailable and a
        // lookup asks MusicBrainz alone. A test about a pressing both sources
        // carry seeds one with `use_discogs`.
        let manager = LibraryManager::new(
            database,
            crate::config::AppDir::under_home(temp.path()),
            Arc::new(ConfigHandle::new(config)),
            clock,
            ids,
            crate::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            crate::import::cover_art::RemoteImageCache::for_test(http.clone()),
            // The production request spacing: the queue's admission order is
            // part of what these tests measure.
            crate::providers::Providers::new(http),
        );

        let import = manager
            .start_import_service(tokio::runtime::Handle::current())
            .await
            .unwrap();

        let root = temp.path().join("watched");
        std::fs::create_dir_all(&root).unwrap();

        let context = Context {
            import: import.clone(),
            library_manager: manager.clone(),
        };
        Fixture {
            manager,
            preparations,
            import,
            provider,
            context,
            identification: OnceLock::new(),
            root,
            _temp: temp,
        }
    }

    /// The running queue, started on first use.
    fn identification(&self) -> &IdentificationHandle {
        self.identification
            .get_or_init(|| super::start(self.import.clone(), self.manager.clone()))
    }

    /// A candidate folder with two real FLACs, and a rip log so the disc ID
    /// computes — the free path.
    fn disc_id_candidate(&self, folder: &str) -> PathBuf {
        let dir = self.candidate_dir(folder);
        copy_fixture(
            Path::new("tests/fixtures/logs/test_album.log"),
            &dir.join("test_album.log"),
        );
        dir
    }

    /// A candidate folder with two real FLACs and one image, and no LOG or CUE:
    /// no disc ID, so identification runs through the artwork barcode and the
    /// search endpoint.
    fn barcode_candidate(&self, folder: &str) -> PathBuf {
        let dir = self.candidate_dir(folder);
        std::fs::write(dir.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0, 0x00]).unwrap();
        dir
    }

    fn candidate_dir(&self, folder: &str) -> PathBuf {
        let dir = self.root.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        for name in FLAC_FIXTURES {
            copy_fixture(
                &Path::new("tests/fixtures/flac").join(name),
                &dir.join(name),
            );
        }
        dir
    }

    /// What every fixture FLAC in `dir` plays for, as the fast pass measures
    /// it — the durations a stored verdict carries.
    fn probed_durations(&self, dir: &Path) -> crate::import::probe::SourceDurations {
        crate::import::probe::SourceDurations::new(
            FLAC_FIXTURES
                .iter()
                .map(|name| crate::import::probe::SourceDuration {
                    audio: crate::import::AudioFile::Standalone {
                        file_id: (*name).to_string(),
                    },
                    duration_ms: u64::try_from(
                        crate::audio_codec::probe_audio_from_path(dir.join(name).to_str().unwrap())
                            .expect("fixture FLAC probes")
                            .duration
                            .as_millis(),
                    )
                    .expect("fixture duration fits u64"),
                })
                .collect(),
        )
    }

    fn probed_total_ms(&self, dir: &Path) -> u64 {
        FLAC_FIXTURES
            .iter()
            .map(|name| {
                crate::audio_codec::probe_audio_from_path(dir.join(name).to_str().unwrap())
                    .expect("fixture FLAC probes")
                    .duration
                    .as_millis() as u64
            })
            .sum()
    }

    /// Copy the whole cue_flac fixture — the sheet, its container, and the two
    /// loose reference tracks — into `<root>/<name>`, and return that folder.
    fn seed_cue_album(&self, name: &str) -> PathBuf {
        let dir = self.root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for file in [
            "Test Album.cue",
            "Test Album.flac",
            "02 Test Artist - Track Two (White Noise).flac",
            "03 Test Artist - Track Three (Brown Noise).flac",
        ] {
            copy_fixture(
                &Path::new("tests/fixtures/cue_flac").join(file),
                &dir.join(file),
            );
        }
        dir
    }

    /// Watch the root and wait for the scan to surface every candidate, so a
    /// sweep started after this sees a populated queue.
    async fn scan(&self, expected: usize) {
        let root = self.root.to_string_lossy().into_owned();
        self.import.add_watched_folder(root.clone()).await.unwrap();
        self.import.refresh_watched_folder(root).await.unwrap();
        tokio::time::timeout(
            Duration::from_secs(10),
            self.import
                .wait_for_list(crate::import::ImportListView::default(), |projection| {
                    projection.summary.counts.pending as usize
                        + projection.summary.counts.done as usize
                        + projection.summary.counts.skipped as usize
                        == expected
                }),
        )
        .await
        .expect("the completed scan surfaces every fixture candidate");
    }

    /// Ask to identify `dir`, through the one entry point a person's request
    /// reaches.
    fn start_explicit_lookup(&self, dir: &Path) {
        self.identification()
            .rerun_identify(dir.to_string_lossy().into_owned());
    }

    /// Open `dir` and wait until identify has registered the driver for it.
    /// Registration happens on a spawned task, so a caller that needs the run
    /// to exist before it acts waits for it rather than guessing a delay.
    async fn start_explicit_lookup_and_await_run(&self, dir: &Path) {
        let key = dir.to_string_lossy().into_owned();
        self.start_explicit_lookup(dir);
        tokio::time::timeout(Duration::from_secs(10), async {
            while !self.import.is_identifying(&key) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("identify registers the driver for the opened candidate");
    }

    /// Wait for identification to write an answer for `dir`, polling because
    /// the writer is a detached task rather than something the caller awaits.
    /// The candidate's row once a run has stored its verdict. Bounded: a run
    /// that never stores is the failure, and a wait with no end hides it
    /// behind the harness's silence until someone kills the job.
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
        self.provider
            .count_containing(&format!("/release/{release_id}?"))
    }

    fn context(&self) -> Context {
        self.context.clone()
    }

    /// Run the automatic admission and wait for everything it is responsible
    /// for to end — the whole of what one pass over the queue was. Assertions
    /// land after finished work rather than after a sleep.
    async fn sweep_once(&self) {
        tokio::time::timeout(
            Duration::from_secs(30),
            self.identification().identify_the_queue_for_test(),
        )
        .await
        .expect("the automatic admission's queue drains");
    }

    /// The same, as a task a test can watch while it acts on the queue.
    fn sweep(&self) -> tokio::task::JoinHandle<()> {
        let identification = self.identification().clone();
        tokio::spawn(async move { identification.identify_the_queue_for_test().await })
    }

    /// Whether the queue holds `key` — what the runtime says about it, which is
    /// the queue's published state and the only thing outside it can read.
    fn identification_status(&self, key: &str) -> Option<crate::import::IdentificationStatus> {
        crate::import::TriageRuntimeFacts::of(&self.import.candidate_runtime(key)?).identification
    }

    async fn stored(&self) -> BTreeMap<String, DbImportCandidateState> {
        self.manager
            .load_import_candidate_states()
            .await
            .unwrap()
            .into_iter()
            .collect()
    }

    /// A candidate folder's content hash, read off disk. Take it before a test
    /// removes the folder — there is nothing to hash afterwards.
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
        self.manager
            .load_import_candidate(&dir.to_string_lossy())
            .await
            .unwrap()
            .map(|projection| {
                projection.resolve(&crate::import::triage::TriageRuntimeFacts::default())
            })
    }

    /// The archived MusicBrainz document for a release, if one was written.
    async fn archived(&self, release_id: &str) -> Option<String> {
        self.manager
            .source_release_payload_for_test(crate::import::PayloadSource::MusicBrainz, release_id)
            .await
            .unwrap()
    }

    /// The archived Discogs document for a release, if one was written.
    async fn archived_discogs(&self, release_id: &str) -> Option<String> {
        self.manager
            .source_release_payload_for_test(crate::import::PayloadSource::Discogs, release_id)
            .await
            .unwrap()
    }

    /// Configure the fake Discogs key, so lookups ask both providers rather
    /// than MusicBrainz alone. The client is already pointed at this fixture's
    /// fake provider.
    fn use_discogs(&self) {
        self.manager
            .set_discogs_key(
                "test-discogs-token",
                crate::config::DiscogsValidation::Valid,
            )
            .expect("the fake Discogs key is stored");
    }

    /// Archive a release's documents directly, as a settle step would have — for
    /// a test that needs them present without anything having fetched them.
    async fn archive(&self, release_id: &str, group_id: &str, track_lengths: &[u64]) {
        let now = crate::db::DbSourceReleasePayload {
            source: crate::import::PayloadSource::MusicBrainz,
            source_release_id: release_id.to_string(),
            json: release_json(release_id, group_id, track_lengths),
            fetched_at: fixed_now(),
        };
        self.manager
            .save_source_release_payloads_for_test(&[now])
            .await
            .unwrap();
    }

    /// Store the verdict a settled lead produces, without running the pipeline.
    /// The lead lists as many tracks as the fixture folder holds.
    async fn store_settled_verdict(
        &self,
        dir: &Path,
        release_id: &str,
        group_id: &str,
        probed_total_ms: u64,
    ) {
        self.store_settled_verdict_listing(
            dir,
            release_id,
            group_id,
            probed_total_ms,
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
        probed_total_ms: u64,
        source_tracks: SourceTracks,
    ) {
        let candidate = self
            .import
            .answerable_candidate(&dir.to_string_lossy())
            .await
            .expect("the candidate state is readable")
            .expect("the scanned candidate is sweepable");
        let mut draft = candidate.blank_source().draft;
        draft.album_title = "Album".to_string();
        draft.album_artist_assignments = vec![crate::import::ArtistAssignment::New {
            seed: crate::import::NewArtistSeed {
                name: "Artist".to_string(),
                sort_name: None,
                musicbrainz_artist_id: None,
                discogs_artist_id: None,
            },
        }];
        for (index, track) in draft.tracks.iter_mut().enumerate() {
            track.edit.title = format!("Track {}", index + 1);
        }
        let verdict = TerminalVerdict::Found {
            matches: vec![MetadataResult {
                source: crate::import::Catalog::MusicBrainz,
                release_id: release_id.to_string(),
                title: "Album".to_string(),
                artist: Some("Artist".to_string()),
                year: None,
                format: None,
                label: None,
                catalog_number: None,
                country: None,
                barcodes: Vec::new(),
                media: crate::import::search::StatedMedia::Undescribed,
                links: Vec::new(),
                cover_art: None,
                source_group_id: Some(group_id.to_string()),
                album_links: crate::import::album_links::AlbumLinks::NotAsked,
                source_tracks: Some(source_tracks),
            }],
            track_count: 2,
            provenance: vec![crate::identify::combine::LookupProvenance {
                by_disc_id: true,
                by_barcode: false,
                by_catalog: false,
                by_search: false,
            }],
            pressings: vec![0],
            narrowed_out: Vec::new(),
            narrowed_out_provenance: Vec::new(),
            narrowed_out_pressings: Vec::new(),
            ledger: None,
        };
        let wrote = self
            .import
            .save_candidate_verdict_if_current(
                &dir.to_string_lossy(),
                IdentifyRunId::for_test(1),
                &NewImportCandidateVerdict {
                    candidate: crate::import::CandidateAsRead {
                        content_hash: self.content_hash(dir),
                        file_edit_revision: 0,
                        metadata_revision: 0,
                    },
                    folder_path: dir.to_string_lossy().into_owned(),
                    verdict,
                    // A computed disc ID that names the log it came from, so
                    // what reads this row back has a file to put the evidence
                    // chip on.
                    signals: Signals {
                        disc_id: DiscIdSignal::Computed {
                            disc_id: SEEDED_DISC_ID.to_string(),
                            track_count: 2,
                            source_file: Some(SEEDED_DISC_ID_FILE.to_string()),
                        },
                        ..settled_signals(crate::import::probe::SourceDurations::totalling(
                            probed_total_ms,
                        ))
                    },
                    metadata: Some(crate::import::CandidateMetadataDraft {
                        draft,
                        source_discogs_artist_ids: Default::default(),
                        provenance: Some(crate::import::MetadataProvenance::ExternalRelease {
                            record: crate::import::MetadataRef::new(
                                crate::import::Catalog::MusicBrainz,
                                release_id.to_string(),
                            ),
                            partners: vec![],
                        }),
                        cover: None,
                        assets: crate::import::CandidatePreparedAssets::default(),
                    }),
                },
            )
            .await
            .unwrap();
        assert!(wrote, "the seeded verdict lands");
    }

    /// The classification a sidebar would derive from a stored row — the stored
    /// verdict, never a stored classification.
    async fn classification_for(&self, dir: &Path) -> QueueClassification {
        let row = self.stored_for(dir).await.expect("a row was stored");
        classify(&identify_result(&row).verdict)
    }
}

/// The identify half of a stored row, which every assertion here is about. A
/// row identification wrote always has one; a row with none was written by the
/// binding editor, which these tests never invoke.
fn identify_result(row: &DbImportCandidateState) -> &crate::db::DbCandidateIdentifyResult {
    row.identify
        .as_ref()
        .expect("a row the sweep wrote carries its identify result")
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
include!("tests/metadata_modes.rs");
include!("tests/imports_and_progress.rs");
include!("tests/persistence.rs");
include!("tests/stored_picks.rs");
include!("tests/settled_panes.rs");
include!("tests/persistence_late.rs");
include!("tests/candidate_decisions.rs");
include!("tests/cancellation.rs");
include!("tests/requested.rs");
include!("tests/admissions.rs");

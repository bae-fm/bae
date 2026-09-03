//! Sweep tests.
//!
//! Every one of them drives the real pipeline — folder scan, extraction,
//! identify reducer, the shared rate limiter, the real MusicBrainz client — and
//! fakes only the provider, at the wire. `set_base_url_for_test` points the
//! client at a local server that answers the same URLs the live service does and
//! counts what was asked for, so "did the sweep re-fetch this?" is answered by
//! request counts rather than by a stub the sweep was handed.
//!
//! The MusicBrainz base URL, its rate limiter, and its release cache are all
//! process-wide, so these tests are `#[serial]`.

use super::*;
use crate::config::{Config, ConfigHandle};
use crate::db::{
    Database, DbCandidateIdentifyResult, DbImportCandidateState, NewImportCandidateVerdict,
};
use crate::identify::ready::{classify, NeedsYou, QueueClassification};
use crate::import::search::{MetadataResult, SourceTracks};
use crate::library::LibraryManager;
use crate::signals::{ArtworkAnalysis, ArtworkAnalyzer};
use crate::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};
use serial_test::serial;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::Duration;
use tempfile::TempDir;

/// Settled signals carrying `durations` and nothing found — what a verdict a
/// test seeds stores beside itself.
/// The disc ID `store_settled_verdict` seeds, and the candidate file it names
/// — what the resumed pane's evidence points at.
const SEEDED_DISC_ID: &str = "XwqRcz4RhAqRTfhE5nRxRKF4iFY-";
const SEEDED_DISC_ID_FILE: &str = "Album.log";

fn settled_signals(durations: crate::import::probe::SourceDurations) -> Signals {
    Signals {
        disc_id: DiscIdSignal::Absent { track_count: 0 },
        barcode: BarcodeSignal::Absent,
        text: TextSignal::Settled {
            catalogs: Vec::new(),
            free_text: Vec::new(),
        },
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

async fn wait_for_request(provider: &FakeProvider, needle: &str, count: usize) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while provider.count_containing(needle) < count {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("provider received the expected request");
}

async fn serve_one(mut stream: tokio::net::TcpStream, state: Arc<Mutex<FakeState>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    // One request per connection: every response says `Connection: close`, so
    // headers always arrive in the first read or two and there is no pipelining
    // to unpick.
    while !buffer.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    }
    let head = String::from_utf8_lossy(&buffer).to_string();
    let target = head
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();

    let (status, body, gate) = {
        let mut state = state.lock().unwrap();
        state.requests.push(target.clone());
        let (status, body) = state
            .routes
            .iter()
            .find(|(needle, _, _)| target.contains(needle.as_str()))
            .map(|(_, status, body)| (*status, body.clone()))
            .unwrap_or((404, "{}".to_string()));
        let gate = state
            .gate
            .as_ref()
            .filter(|(needle, _)| target.contains(needle.as_str()))
            .map(|(_, gate)| gate.clone());
        (status, body, gate)
    };
    if let Some(gate) = gate {
        // The hold ends by closing the semaphore, so this wait only ever ends
        // — it never takes a permit, and no release can be missed.
        let _ = gate.acquire().await;
    }

    let response = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

// ── Canned MusicBrainz payloads ─────────────────────────────────────────────

/// A release as the disc-ID and release-lookup endpoints return it: one medium
/// whose tracks carry `length`, which is what makes the disc-ID path free.
fn release_json(release_id: &str, group_id: &str, track_lengths: &[u64]) -> String {
    let tracks: Vec<String> = track_lengths
        .iter()
        .enumerate()
        .map(|(i, length)| {
            format!(
                r#"{{"position":{},"number":"{}","title":"Track {}","length":{length}}}"#,
                i + 1,
                i + 1,
                i + 1
            )
        })
        .collect();
    format!(
        r#"{{"id":"{release_id}","title":"Album","artist-credit":[{{"name":"Artist"}}],
            "release-group":{{"id":"{group_id}"}},
            "media":[{{"tracks":[{}]}}],"relations":[],
            "cover-art-archive":{{"front":false,"darkened":false}}}}"#,
        tracks.join(",")
    )
}

/// A release under a title and artist of its own — for a test that has to tell
/// two releases apart by what a row shows.
fn titled_release_json(release_id: &str, group_id: &str, title: &str, artist: &str) -> String {
    format!(
        r#"{{"id":"{release_id}","title":"{title}",
            "artist-credit":[{{"name":"{artist}"}}],
            "release-group":{{"id":"{group_id}"}},
            "media":[{{"tracks":[
                {{"position":1,"number":"1","title":"Track Title 1","length":180000}}
            ]}}],"relations":[],
            "cover-art-archive":{{"front":true,"darkened":false}}}}"#
    )
}

fn discid_json(release_id: &str, group_id: &str, track_lengths: &[u64]) -> String {
    format!(
        r#"{{"releases":[{}]}}"#,
        release_json(release_id, group_id, track_lengths)
    )
}

/// A search hit as `ws/2/release?query=…` returns it: no `media`, hence no
/// lengths and no count, so the Ready rule has nothing to check until the lead
/// is settled.
fn search_json(release_id: &str, group_id: &str) -> String {
    format!(
        r#"{{"releases":[{{"id":"{release_id}","title":"Album",
            "artist-credit":[{{"name":"Artist"}}],
            "release-group":{{"id":"{group_id}"}},"label-info":[]}}]}}"#
    )
}

// ── The fixture ─────────────────────────────────────────────────────────────

/// Where the candidate audio comes from. The two FLACs are real files with real
/// durations, so the probe in the fast pass has something to measure and the
/// Ready rule has a total to compare.
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
            barcodes: vec![self.barcode.clone()],
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
        ArtworkAnalysis {
            barcodes: Vec::new(),
            text_lines: Vec::new(),
        }
    }
}

impl ArtworkAnalyzer for GatedAnalyzer {
    fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
        self.started.wait();
        self.release.wait();
        ArtworkAnalysis {
            barcodes: Vec::new(),
            text_lines: Vec::new(),
        }
    }
}

impl ArtworkAnalyzer for SlowAnalyzer {
    fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
        std::thread::sleep(self.delay);
        ArtworkAnalysis {
            barcodes: Vec::new(),
            text_lines: Vec::new(),
        }
    }
}

struct Fixture {
    manager: LibraryManager,
    import: ImportServiceHandle,
    identify: IdentifyServiceHandle,
    extraction: ExtractionServiceHandle,
    provider: FakeProvider,
    /// One context for the fixture's whole life, so consecutive `sweep_once`
    /// calls are the same sweep — which is what a second pass after a failed
    /// first one actually is.
    context: SweepContext,
    /// The handle over that same context, for the selection entry point.
    sweep: QueueSweepHandle,
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
        let library_dir = coven::StoreDir::new(temp.path());
        let library_id = format!("sweep-{name}-{}", uuid::Uuid::new_v4());
        let config = Config::with_defaults(
            library_id.clone(),
            "test-device".to_string(),
            library_dir,
            "Test Library".to_string(),
        );
        crate::config::install_test_keyring();
        // No Discogs key is seeded, so Discogs operations are unavailable and the
        // barcode lookup is MusicBrainz-only — one provider to fake.
        let manager = LibraryManager::new(
            database,
            Arc::new(ConfigHandle::new(config)),
            clock,
            ids,
            crate::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            crate::import::cover_art::RemoteImageCache::for_test(),
        );

        let import = manager
            .start_import_service(tokio::runtime::Handle::current())
            .await
            .unwrap();
        let (identify, extraction) = import.start_candidate_services();

        let provider = FakeProvider::start().await;
        crate::musicbrainz::set_base_url_for_test(Some(provider.base_url.clone()));
        crate::import::cover_art::set_base_url_for_test(Some(provider.base_url.clone()));
        crate::musicbrainz::reset_rate_limiter_for_test();

        let root = temp.path().join("watched");
        std::fs::create_dir_all(&root).unwrap();

        let context = SweepContext {
            import: import.clone(),
            identify: identify.clone(),
            extraction: extraction.clone(),
            library_manager: manager.clone(),
            ours: Arc::new(Mutex::new(HashSet::new())),
        };
        let tasks = tokio_util::task::TaskTracker::new();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let runtime_handle = runtime.handle().clone();
        let completion_tasks = tasks.clone();
        let executor_thread = std::thread::spawn(move || {
            runtime.block_on(completion_tasks.wait());
        });
        let sweep = QueueSweepHandle::new(
            context.clone(),
            CancellationToken::new(),
            tasks,
            runtime_handle,
            executor_thread,
        );
        Fixture {
            manager,
            import,
            identify,
            extraction,
            provider,
            context,
            sweep,
            root,
            _temp: temp,
        }
    }

    /// A candidate folder with two real FLACs, and a rip log so the disc ID
    /// computes — the free path.
    fn disc_id_candidate(&self, folder: &str) -> PathBuf {
        let dir = self.candidate_dir(folder);
        std::fs::copy(
            Path::new("tests/fixtures/test_album.log"),
            dir.join("test_album.log"),
        )
        .unwrap();
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
            std::fs::copy(Path::new("tests/fixtures/flac").join(name), dir.join(name)).unwrap();
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

    /// Enter Lookup for `dir` through the one explicit entry point core exposes.
    fn start_explicit_lookup(&self, dir: &Path) {
        self.sweep
            .identify_for_explicit_lookup(dir.to_string_lossy().into_owned());
    }

    /// Open `dir` and wait until identify has registered the driver for it.
    /// Registration happens on a spawned task, so a caller that needs the run
    /// to exist before it acts waits for it rather than guessing a delay.
    async fn start_explicit_lookup_and_await_run(&self, dir: &Path) {
        let key = dir.to_string_lossy().into_owned();
        self.start_explicit_lookup(dir);
        tokio::time::timeout(Duration::from_secs(10), async {
            while !self.identify.is_running(&key) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("identify registers the driver for the opened candidate");
    }

    /// Wait for identification to write an answer for `dir`, polling because
    /// the writer is a detached task rather than something the caller awaits.
    async fn await_identified_row(&self, dir: &Path) -> DbImportCandidateState {
        loop {
            if let Some(row) = self.stored_for(dir).await {
                if row.identify.is_some() {
                    return row;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn count_release_lookups(&self, release_id: &str) -> usize {
        self.provider
            .count_containing(&format!("/release/{release_id}?"))
    }

    fn context(&self) -> SweepContext {
        self.context.clone()
    }

    /// Run one sweep pass to completion. Calling `run_pass` directly rather than
    /// through the bus-driven loop keeps the test's assertions after a finished
    /// pass rather than after a sleep.
    async fn sweep_once(&self) {
        let token = CancellationToken::new();
        tokio::time::timeout(
            Duration::from_secs(30),
            run_pass_for_test(&self.context(), &token),
        )
        .await
        .expect("a sweep pass finishes");
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
    async fn store_settled_verdict(
        &self,
        dir: &Path,
        release_id: &str,
        group_id: &str,
        probed_total_ms: u64,
    ) {
        let candidate = self
            .import
            .sweepable_candidate(&dir.to_string_lossy())
            .await
            .expect("the candidate state is readable")
            .expect("the scanned candidate is sweepable");
        let source_draft = crate::import::pane::blank_candidate_source(&candidate.files);
        let mut edit = source_draft.edit;
        edit.album_title = "Album".to_string();
        edit.album_artist_assignments = vec![crate::import::ArtistAssignment::New {
            seed: crate::import::NewArtistSeed {
                name: "Artist".to_string(),
                sort_name: None,
                musicbrainz_artist_id: None,
                discogs_artist_id: None,
            },
        }];
        for (index, track) in edit.tracks.iter_mut().enumerate() {
            track.title = format!("Track {}", index + 1);
        }
        let verdict = TerminalVerdict::Found {
            matches: vec![MetadataResult {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: release_id.to_string(),
                title: "Album".to_string(),
                artist: Some("Artist".to_string()),
                year: None,
                format: None,
                label: None,
                catalog_number: None,
                country: None,
                cover_art: None,
                source_group_id: Some(group_id.to_string()),
                source_tracks: Some(SourceTracks::Listed {
                    count: 2,
                    total_duration_ms: Some(probed_total_ms),
                }),
            }],
            track_count: 2,
            provenance: vec![crate::identify::combine::ResultProvenance {
                by_disc_id: true,
                by_barcode: false,
                by_catalog: false,
            }],
            matched_barcode: None,
        };
        let wrote = self
            .import
            .save_candidate_verdict_if_current(
                &dir.to_string_lossy(),
                &NewImportCandidateVerdict {
                    content_hash: self.content_hash(dir),
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
                    expected_edit_revision: 0,
                    expected_metadata_revision: 0,
                    metadata: crate::import::CandidateMetadataDraft {
                        edit,
                        track_mappings: source_draft.track_mappings,
                        source_discogs_artist_ids: Default::default(),
                        provenance: Some(crate::import::MetadataProvenance::ExternalRelease {
                            source: crate::import::MetadataSource::MusicBrainz,
                            release_id: release_id.to_string(),
                        }),
                        cover: None,
                        assets: crate::import::CandidatePreparedAssets::default(),
                    },
                },
            )
            .await
            .unwrap();
        assert!(wrote, "the seeded verdict lands");
    }

    /// The classification a sidebar would derive from a stored row — the stored
    /// verdict plus a live library check, never a stored classification.
    async fn classification_for(&self, dir: &Path) -> QueueClassification {
        let row = self.stored_for(dir).await.expect("a row was stored");
        let identify = identify_result(&row);
        let verdict = identify.verdict.clone();
        let matches: Vec<MetadataResult> = match &verdict {
            TerminalVerdict::Found { matches, .. } => matches.clone(),
            _ => Vec::new(),
        };
        let checks: Vec<crate::db::LibraryCheck> =
            matches.iter().map(crate::db::LibraryCheck::from).collect();
        let statuses = self
            .manager
            .check_releases_in_library(&checks)
            .await
            .unwrap();
        classify(&verdict, identify.probed_total_duration_ms, &statuses)
    }
}

/// The identify half of a stored row, which every sweep assertion is about. A
/// row the sweep wrote always has one; a row with none was written by the
/// binding editor, which these tests never invoke.
fn identify_result(row: &DbImportCandidateState) -> &crate::db::DbCandidateIdentifyResult {
    row.identify
        .as_ref()
        .expect("a row the sweep wrote carries its identify result")
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // The base URL is process-wide; leaving it pointed at a dead port would
        // make the next test's live-service assumption silently wrong.
        crate::musicbrainz::set_base_url_for_test(None);
        crate::import::cover_art::set_base_url_for_test(None);
        self.sweep.stop();
        self.import.stop_and_join();
    }
}

// ── 1. A candidate nobody selected acquires a verdict ────────────────────────

include!("tests/identification.rs");
include!("tests/metadata_modes.rs");
include!("tests/imports_and_progress.rs");
include!("tests/persistence.rs");
include!("tests/persistence_late.rs");
include!("tests/candidate_decisions.rs");

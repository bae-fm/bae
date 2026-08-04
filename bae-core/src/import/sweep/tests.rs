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
use crate::db::{Database, DbImportCandidateState, NewImportCandidateVerdict};
use crate::identify::ready::{classify, NeedsYou, QueueClassification};
use crate::import::cover_art::CoverArtArchiveClient;
use crate::import::search::{MetadataResult, SourceTracks};
use crate::keys::StoreKeys;
use crate::library::LibraryManager;
use crate::signals::{ArtworkAnalysis, ArtworkAnalyzer, ExtractionService};
use serial_test::serial;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

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
            "media":[{{"tracks":[{}]}}],"relations":[]}}"#,
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
            ]}}],"relations":[]}}"#
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

/// An OCR stub that takes its time, so a test can act while a candidate is
/// genuinely mid-extraction.
struct SlowAnalyzer {
    delay: Duration,
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
    cover_art: CoverArtArchiveClient,
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
        // No Discogs key is seeded, so `discogs_client()` yields `None` and the
        // barcode lookup is MusicBrainz-only — one provider to fake.
        let manager = LibraryManager::new(
            database,
            Arc::new(ConfigHandle::new(config)),
            StoreKeys::bind(library_id),
            clock,
            ids,
            crate::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            crate::import::cover_art::CoverArtArchiveClient::hermetic(),
        );

        let cover_art = manager.cover_art_archive_for_test();
        let import =
            crate::import::ImportService::start(tokio::runtime::Handle::current(), manager.clone())
                .await
                .unwrap();
        let identify = IdentifyServiceHandle::new(
            manager.clone(),
            tokio::runtime::Handle::current(),
            import.event_sender_for_test(),
        );
        let extraction = ExtractionService::start(
            tokio::runtime::Handle::current(),
            import.event_sender_for_test(),
            manager.clone(),
        );

        let provider = FakeProvider::start().await;
        crate::musicbrainz::set_base_url_for_test(Some(provider.base_url.clone()));
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
        let sweep = QueueSweepHandle {
            context: context.clone(),
            token: CancellationToken::new(),
            tasks,
            runtime_handle,
            executor_thread: Arc::new(Mutex::new(Some(executor_thread))),
        };
        Fixture {
            manager,
            import,
            identify,
            extraction,
            cover_art,
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

    /// Sum of the fixture audio's real probed durations — what the sweep stores
    /// and the Ready rule compares against.
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
        let mut events = self.import.subscribe_events();
        self.import
            .add_watched_folder(self.root.to_string_lossy().into_owned())
            .await
            .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let event = tokio::time::timeout(remaining, events.recv())
                .await
                .expect("scan finishes")
                .expect("bus stays open");
            if matches!(event, ImportEvent::Scan(ScanEvent::Finished))
                && self.import.get_import_candidates().folder_candidates.len() == expected
            {
                return;
            }
        }
    }

    /// What `ImportView.selectCandidate` does, through the one entry point core
    /// exposes for it.
    fn select(&self, dir: &Path) {
        self.sweep
            .identify_for_selection(dir.to_string_lossy().into_owned());
    }

    /// Wait for a row to exist for `dir`, polling because the writer is a
    /// detached task rather than something the caller awaits.
    async fn await_row(&self, dir: &Path) -> DbImportCandidateState {
        loop {
            if let Some(row) = self.stored_for(dir).await {
                return row;
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

    /// The archived MusicBrainz document for a release, if one was written.
    async fn archived(&self, release_id: &str) -> Option<String> {
        self.manager
            .database_for_test()
            .load_source_release_payloads(&[(
                crate::import::PayloadSource::MusicBrainz,
                release_id.to_string(),
            )])
            .await
            .unwrap()
            .remove(&(
                crate::import::PayloadSource::MusicBrainz,
                release_id.to_string(),
            ))
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
        let cover = crate::db::DbSourceReleasePayload {
            source: crate::import::PayloadSource::CoverArtRelease,
            source_release_id: release_id.to_string(),
            json: serde_json::to_string(&Some(crate::import::cover_art::RemoteCover {
                url: "https://caa.example/front.jpg".to_string(),
                thumbnail_url: "https://caa.example/front-250.jpg".to_string(),
                label: "Cover Art Archive".to_string(),
                source: crate::import::MetadataSource::MusicBrainz,
            }))
            .unwrap(),
            fetched_at: fixed_now(),
        };
        let group_cover = crate::db::DbSourceReleasePayload {
            source: crate::import::PayloadSource::CoverArtReleaseGroup,
            source_release_id: group_id.to_string(),
            json: "null".to_string(),
            fetched_at: fixed_now(),
        };
        self.manager
            .database_for_test()
            .save_source_release_payloads(&[now, cover, group_cover])
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
            group: crate::identify::combine::GroupKey {
                source: crate::import::MetadataSource::MusicBrainz,
                source_group_id: group_id.to_string(),
            },
            provenance: vec![crate::identify::combine::ResultProvenance {
                by_disc_id: true,
                by_barcode: false,
                matches_catalog: false,
            }],
        };
        let wrote = self
            .import
            .save_candidate_verdict_if_current(
                &dir.to_string_lossy(),
                &NewImportCandidateVerdict {
                    content_hash: self.content_hash(dir),
                    folder_path: dir.to_string_lossy().into_owned(),
                    verdict: serde_json::to_string(&verdict).unwrap(),
                    probed_total_duration_ms: probed_total_ms as i64,
                    expected_edit_revision: 0,
                    identity_pick: Some(
                        serde_json::to_string(&crate::import::IdentityPick::Release {
                            source: crate::import::MetadataSource::MusicBrainz,
                            release_id: release_id.to_string(),
                            claim: crate::import::ClaimLevel::Exact,
                        })
                        .unwrap(),
                    ),
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
        let verdict: TerminalVerdict =
            serde_json::from_str(&identify.verdict).expect("verdict decodes");
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
        classify(
            &verdict,
            identify.probed_total_duration_ms as u64,
            &statuses,
        )
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
        self.sweep.stop();
        self.import.stop_and_join();
    }
}

// ── 1. A candidate nobody selected acquires a verdict ────────────────────────

/// The whole point of the task: nothing is selected, no view is open, and the
/// candidate still ends up with a stored verdict that classifies as Ready.
///
/// The provider answers the disc-ID lookup with exactly one release whose track
/// lengths are the fixture audio's own, so the Ready rule's every clause is
/// exercised for real: one match, not in the library, counts agreeing, totals
/// agreeing.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_candidate_nobody_selected_acquires_a_verdict() {
    let fixture = Fixture::new("acquires-verdict").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .cover_art
        .seed_lookup(Some("mb-ready-1"), Some("rg-ready-1"), None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json(
            "mb-ready-1",
            "rg-ready-1",
            &[probed / 2, probed - probed / 2],
        ),
    );
    fixture.provider.route(
        "/release/mb-ready-1?",
        200,
        release_json(
            "mb-ready-1",
            "rg-ready-1",
            &[probed / 2, probed - probed / 2],
        ),
    );
    fixture.scan(1).await;

    // Nobody selects anything. The sweep is the only actor.
    fixture.sweep_once().await;

    let row = fixture.stored_for(&dir).await.expect("a verdict is stored");
    assert_eq!(
        row.folder_path,
        dir.to_string_lossy(),
        "the row names where the candidate was last seen"
    );
    let identify = identify_result(&row);
    assert_eq!(
        identify.probed_total_duration_ms as u64, probed,
        "the probed total rode the fast pass into the row"
    );
    assert_eq!(
        identify.identified_at,
        fixed_now(),
        "the row is stamped from the injected clock"
    );
    assert_eq!(
        fixture.classification_for(&dir).await,
        QueueClassification::Ready,
        "one match, not in the library, counts and totals agreeing"
    );
}

// ── 2. A stored verdict is not re-fetched ───────────────────────────────────

/// The second launch is instant because a candidate whose content hash already
/// has a verdict is never handed to the pipeline again. Two passes over the same
/// queue, and the provider sees requests only in the first.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_stored_verdict_is_not_re_fetched() {
    let fixture = Fixture::new("not-re-fetched").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .cover_art
        .seed_lookup(Some("mb-cached-1"), Some("rg-cached-1"), None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-cached-1", "rg-cached-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-cached-1?",
        200,
        release_json("mb-cached-1", "rg-cached-1", &[probed, 0]),
    );
    fixture.scan(1).await;

    fixture.sweep_once().await;
    let after_first = fixture.provider.requests().len();
    assert!(
        after_first > 0,
        "the first pass has to actually ask the provider"
    );
    assert!(fixture.stored_for(&dir).await.is_some());

    fixture.sweep_once().await;

    assert_eq!(
        fixture.provider.requests().len(),
        after_first,
        "the second pass asked the provider for nothing: {:?}",
        fixture.provider.requests()
    );
}

// ── 3. A transport failure leaves no row and is retried ─────────────────────

/// A lookup that never got an answer is not a verdict. Nothing is written, so
/// the next pass asks again — and there is no attempt counter or backoff row to
/// stop it from succeeding when the provider comes back.
///
/// The failing response is a 400 rather than a 5xx so the client's own retry
/// policy stays out of it; what is under test is what the sweep does with a
/// failure, not how many times the client repeats one.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_transport_failure_leaves_no_row_and_is_retried() {
    let fixture = Fixture::new("failure-retried").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .cover_art
        .seed_lookup(Some("mb-retry-1"), Some("rg-retry-1"), None);
    fixture
        .provider
        .set_routes(vec![("/discid/", 400, "{}".to_string())]);
    fixture.scan(1).await;

    fixture.sweep_once().await;
    assert!(
        fixture.stored_for(&dir).await.is_none(),
        "a failed lookup must leave no row — a stored failure is a stored answer"
    );

    fixture.provider.set_routes(vec![
        (
            "/discid/",
            200,
            discid_json("mb-retry-1", "rg-retry-1", &[probed, 0]),
        ),
        (
            "/release/mb-retry-1?",
            200,
            release_json("mb-retry-1", "rg-retry-1", &[probed, 0]),
        ),
    ]);
    fixture.sweep_once().await;

    assert!(
        fixture.stored_for(&dir).await.is_some(),
        "the candidate is retried, and the retry stores"
    );
}

// ── 4. The interactive path is not delayed by the sweep ─────────────────────

/// The pair to the limiter's own priority test, from the producer's side. With
/// the sweep's background lookups queued on the shared limiter, a search the
/// user typed is admitted next rather than after all of them.
///
/// Eight candidates saturate the limiter's background queue at the sweep's
/// concurrency cap. Without priority the interactive search waits out every
/// queued background call at one second each; with it, one interval.
///
/// Wall time, not the deterministic clock, and deliberately: the fake provider
/// is a real socket, so `start_paused` would leave the runtime idle while a
/// response is in flight and auto-advance straight into the request's own
/// `API_TIMEOUT` — every lookup would time out before the server answered. What
/// the clock would otherwise buy is bought instead by bracketing the
/// measurement with assertions that background work really was in flight, so a
/// sweep that had died cannot make this pass by doing nothing.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn the_interactive_path_is_not_delayed_by_the_sweep() {
    let fixture = Fixture::new("interactive-not-delayed").await;
    let mut dirs = Vec::new();
    for i in 0..8 {
        let dir = fixture.disc_id_candidate(&format!("Album {i}"));
        std::fs::write(
            dir.join(format!("playlist-{i}.m3u")),
            format!("candidate {i}"),
        )
        .unwrap();
        dirs.push(dir);
    }
    let probed = fixture.probed_total_ms(&dirs[0]);
    for i in 0..8 {
        fixture
            .cover_art
            .seed_lookup(Some(&format!("mb-flood-{i}")), None, None);
    }
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-flood-0", "rg-flood-0", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-flood-0?",
        200,
        release_json("mb-flood-0", "rg-flood-0", &[probed, 0]),
    );
    fixture
        .provider
        .route("/release?", 200, search_json("mb-typed", "rg-typed"));
    fixture
        .cover_art
        .seed_lookup(Some("mb-typed"), Some("rg-typed"), None);
    fixture.scan(8).await;

    let context = fixture.context();
    let token = CancellationToken::new();
    let sweep_token = token.clone();
    let sweep = tokio::spawn(async move { run_pass_for_test(&context, &sweep_token).await });

    // Let the sweep take the first slot and stack the rest behind it.
    tokio::time::sleep(Duration::from_millis(1_200)).await;
    let background_before = fixture.provider.count_containing("/discid/");
    assert!(
        (1..8).contains(&background_before),
        "the sweep must be mid-flight when the search is timed — {background_before} of 8 \
         lookups done means there is no background queue to be admitted ahead of"
    );

    let started = std::time::Instant::now();
    let typed = crate::import::search::search_mb(
        &fixture.cover_art,
        crate::musicbrainz::ReleaseSearchParams {
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            ..Default::default()
        },
        CallPriority::Interactive,
    )
    .await
    .expect("the typed search succeeds");
    let waited = started.elapsed();

    // Still running, so the search really was admitted past a live background
    // queue rather than into an idle limiter. (Its count does not rise across
    // the measurement, and must not: the whole point is that the interactive
    // call took the slot the sweep would have had.)
    assert!(
        !sweep.is_finished(),
        "the sweep must still be mid-pass across the measurement — a sweep that \
         died would make this pass by doing nothing"
    );
    token.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(20), sweep).await;

    assert_eq!(typed.len(), 1);
    assert!(
        waited < Duration::from_millis(2_000),
        "an interactive search waited {waited:?} behind the sweep; \
         with priority it is admitted within about one interval"
    );
}

// ── 5. Totals decide, not per-track lengths ─────────────────────────────────

fn found_verdict(track_count: u32, source: Option<SourceTracks>) -> TerminalVerdict {
    TerminalVerdict::Found {
        matches: vec![MetadataResult {
            source: crate::import::MetadataSource::MusicBrainz,
            release_id: "mb-1".to_string(),
            title: "Album".to_string(),
            artist: None,
            year: None,
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            cover_art: None,
            source_group_id: Some("rg-1".to_string()),
            source_tracks: source,
        }],
        track_count,
        group: crate::identify::GroupKey {
            source: crate::import::MetadataSource::MusicBrainz,
            source_group_id: "rg-1".to_string(),
        },
        provenance: vec![crate::identify::ResultProvenance {
            by_disc_id: true,
            by_barcode: false,
            matches_catalog: false,
        }],
    }
}

/// The gate is total against total. A rip that splits a continuous piece
/// differently from the source has per-track lengths that disagree everywhere
/// and a total that agrees exactly — and it is a correct match, so it is Ready.
/// A release that is genuinely a different edition differs in the total, and is
/// not.
///
/// The per-track half is enforced by the type, not by the comparison: what the
/// source contributes is one summed total, parsed out of its response by
/// `mb_source_tracks`, so there are no per-track lengths for a future gate to
/// reach for. This drives that parse rather than hand-building the total.
#[test]
fn totals_decide_not_per_track_lengths() {
    use crate::musicbrainz::MbReleaseResponse;

    let source_response: MbReleaseResponse =
        serde_json::from_str(&release_json("mb-1", "rg-1", &[200_000, 100_000, 300_000])).unwrap();
    let source = crate::import::search::mb_source_tracks(&source_response);
    assert_eq!(
        source,
        SourceTracks::Listed {
            count: 3,
            total_duration_ms: Some(600_000)
        }
    );

    // The rip splits the same 600 s across three tracks differently. Every
    // per-track length disagrees; the total does not.
    let rip_total = 100_000 + 300_000 + 200_000;
    assert_eq!(
        classify(&found_verdict(3, Some(source.clone())), rip_total, &[]),
        QueueClassification::Ready,
        "a different split of the same running time is the same record"
    );

    // A different edition — one track longer by a minute — is not absorbed.
    let different_edition = rip_total + 60_000;
    let QueueClassification::NeedsYou(NeedsYou::DurationsDisagree { tolerance_ms, .. }) = classify(
        &found_verdict(3, Some(source.clone())),
        different_edition,
        &[],
    ) else {
        panic!("a minute of difference must not be admitted");
    };

    // The tolerance's own edges, so a change to it fails here rather than
    // silently widening what gets imported unattended.
    assert_eq!(tolerance_ms, 5_000, "3 tracks sit on the floor");
    assert_eq!(
        classify(
            &found_verdict(3, Some(source.clone())),
            600_000 + tolerance_ms,
            &[]
        ),
        QueueClassification::Ready,
        "exactly at the tolerance still agrees"
    );
    assert!(
        matches!(
            classify(
                &found_verdict(3, Some(source)),
                600_000 + tolerance_ms + 1,
                &[]
            ),
            QueueClassification::NeedsYou(NeedsYou::DurationsDisagree { .. })
        ),
        "one millisecond past it does not"
    );
}

/// The count is checked before the totals, and separately: two different
/// tracklists can add up to the same running time.
#[test]
fn a_count_disagreement_is_named_as_one() {
    let source = SourceTracks::Listed {
        count: 12,
        total_duration_ms: Some(600_000),
    };
    assert_eq!(
        classify(&found_verdict(11, Some(source)), 600_000, &[]),
        QueueClassification::NeedsYou(NeedsYou::TrackCountDisagrees {
            local: 11,
            source: 12
        })
    );
}

// ── 6. Settling a lead ──────────────────────────────────────────────────────

/// One release lookup per settled lead, whichever signal found it.
///
/// The disc-ID response already carries a tracklist, but not the rest of what
/// opening the candidate needs — the release-level relations the commit maps,
/// the release group, the cover options. A lead is settled by fetching the
/// release itself, once, and both candidates in this pass cost exactly that.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn settling_a_lead_costs_one_release_lookup_whichever_signal_found_it() {
    let fixture = Fixture::new("settle-lead").await;
    fixture
        .extraction
        .register_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    let disc_dir = fixture.disc_id_candidate("From Disc Id");
    let barcode_dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&disc_dir);

    for (id, group) in [("mb-disc-1", "rg-disc-1"), ("mb-barcode-1", "rg-barcode-1")] {
        fixture.cover_art.seed_lookup(Some(id), Some(group), None);
        fixture.provider.route(
            &format!("/release/{id}?"),
            200,
            release_json(id, group, &[probed, 0]),
        );
    }
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-disc-1", "rg-disc-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release?",
        200,
        search_json("mb-barcode-1", "rg-barcode-1"),
    );
    fixture.scan(2).await;

    fixture.sweep_once().await;

    assert_eq!(
        fixture.count_release_lookups("mb-disc-1"),
        1,
        "the disc-ID lead is settled once: {:?}",
        fixture.provider.requests()
    );
    assert_eq!(
        fixture.count_release_lookups("mb-barcode-1"),
        1,
        "and so is the search lead: {:?}",
        fixture.provider.requests()
    );
    assert_eq!(
        fixture.classification_for(&disc_dir).await,
        QueueClassification::Ready
    );
    assert_eq!(
        fixture.classification_for(&barcode_dir).await,
        QueueClassification::Ready
    );
}

/// The write ordering, from the outside: a stored verdict's lead always has its
/// documents alongside it, because the settle step writes them first and the
/// verdict is not written at all when it fails.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_settled_verdict_never_stores_without_its_documents() {
    let fixture = Fixture::new("settle-ordering").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .cover_art
        .seed_lookup(Some("mb-order-1"), Some("rg-order-1"), None);
    // The disc-ID lookup answers; the release lookup that settles the lead does
    // not. The verdict is reachable, and must still not be stored.
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-order-1", "rg-order-1", &[probed, 0]),
    );
    fixture.provider.route("/release/mb-order-1?", 400, "{}");
    fixture.scan(1).await;

    fixture.sweep_once().await;

    assert!(
        fixture.stored_for(&dir).await.is_none(),
        "a lead whose documents could not be fetched stores no verdict"
    );
    assert!(
        fixture.archived("mb-order-1").await.is_none(),
        "and nothing half-written is left behind"
    );

    // The provider comes back, and now both land together.
    fixture.provider.set_routes(vec![
        (
            "/discid/",
            200,
            discid_json("mb-order-1", "rg-order-1", &[probed, 0]),
        ),
        (
            "/release/mb-order-1?",
            200,
            release_json("mb-order-1", "rg-order-1", &[probed, 0]),
        ),
    ]);
    fixture.sweep_once().await;

    assert!(fixture.stored_for(&dir).await.is_some(), "the retry stores");
    assert!(
        fixture.archived("mb-order-1").await.is_some(),
        "with the documents the verdict rests on"
    );
}

/// Opening a candidate settles it too. A person's own run answers the candidate
/// for good, and "answered" means the next launch opens it with no network —
/// so the same step runs here, before the verdict is written.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn opening_a_candidate_settles_its_lead_before_storing_the_verdict() {
    let fixture = Fixture::new("interactive-settles").await;
    fixture
        .extraction
        .register_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .cover_art
        .seed_lookup(Some("mb-interactive-1"), Some("rg-interactive-1"), None);
    fixture.provider.route(
        "/release?",
        200,
        search_json("mb-interactive-1", "rg-interactive-1"),
    );
    fixture.provider.route(
        "/release/mb-interactive-1?",
        200,
        release_json("mb-interactive-1", "rg-interactive-1", &[probed, 0]),
    );
    fixture.scan(1).await;

    // Exactly what `selectCandidate` does.
    fixture.select(&dir);
    let row = tokio::time::timeout(Duration::from_secs(20), fixture.await_row(&dir))
        .await
        .expect("the selection recorder stores the verdict");

    let verdict: TerminalVerdict = serde_json::from_str(&identify_result(&row).verdict).unwrap();
    let TerminalVerdict::Found { matches, .. } = &verdict else {
        panic!("expected a single-match Found, got {verdict:?}");
    };
    assert!(
        matches[0].source_tracks.is_some(),
        "the lead was settled before the verdict was written"
    );
    assert!(
        fixture.archived("mb-interactive-1").await.is_some(),
        "and its documents are archived under the release they describe"
    );
    assert_eq!(
        fixture.classification_for(&dir).await,
        QueueClassification::Ready,
        "so the row is admitted on evidence that was actually checked"
    );

    // A later sweep pass finds nothing left to buy.
    let after_selection = fixture.provider.requests().len();
    fixture.sweep_once().await;
    assert_eq!(
        fixture.provider.requests().len(),
        after_selection,
        "a settled row is finished: {:?}",
        fixture.provider.requests()
    );
}

/// The receipt for "so ready it is offline": a candidate whose lead is settled
/// opens with nothing routed and no cover-art answer seeded — the hermetic
/// client would panic on a live lookup, and the provider would answer 404. The
/// release id is this test's own, so no session cache holds it either.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_settled_candidate_opens_with_the_provider_gone() {
    let fixture = Fixture::new("offline-open").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;

    // Nothing is routed and nothing is seeded: the archived documents are the
    // only place this release exists.
    fixture
        .archive("mb-offline-1", "rg-offline-1", &[probed, 0])
        .await;
    fixture
        .store_settled_verdict(&dir, "mb-offline-1", "rg-offline-1", probed)
        .await;
    let before = fixture.provider.requests().len();

    let prefetch = fixture
        .import
        .prefetch_release(
            &dir.to_string_lossy(),
            "mb-offline-1",
            crate::import::MetadataSource::MusicBrainz,
            crate::import::ClaimLevel::Exact,
        )
        .await
        .expect("a settled candidate opens from what identification archived");

    assert_eq!(prefetch.detail.release_id, "mb-offline-1");
    assert_eq!(prefetch.detail.tracks.len(), 2);
    assert_eq!(prefetch.seed.tracks.len(), 2);
    assert_eq!(
        prefetch.detail.cover_art.len(),
        1,
        "the archive's answer for the pressing rides along"
    );
    assert_eq!(
        fixture.provider.requests().len(),
        before,
        "opening it reached the wire for nothing: {:?}",
        fixture.provider.requests()
    );
}

/// A settled lead whose documents are missing is a broken invariant, not a cold
/// cache. Opening it fails loudly rather than re-fetching and hiding the break.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_settled_lead_with_no_documents_fails_loud() {
    let fixture = Fixture::new("offline-miss").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    fixture
        .store_settled_verdict(&dir, "mb-missing-1", "rg-missing-1", probed)
        .await;

    let error = fixture
        .import
        .prefetch_release(
            &dir.to_string_lossy(),
            "mb-missing-1",
            crate::import::MetadataSource::MusicBrainz,
            crate::import::ClaimLevel::Exact,
        )
        .await
        .expect_err("a settled lead with nothing archived must not silently re-fetch");

    assert!(
        matches!(&error, crate::import::ImportError::Internal { detail }
            if detail.contains("mb-missing-1")),
        "unexpected error: {error}"
    );
}

/// A pick identification never made — another pressing on the list, a manual
/// search hit — fetches, and archives what it fetched, so opening it again is
/// local too.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_pick_outside_the_verdict_archives_what_it_fetched() {
    let fixture = Fixture::new("manual-pick").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    fixture
        .cover_art
        .seed_lookup(Some("mb-manual-1"), Some("rg-manual-1"), None);
    fixture.provider.route(
        "/release/mb-manual-1?",
        200,
        release_json("mb-manual-1", "rg-manual-1", &[probed, 0]),
    );

    assert!(
        fixture.archived("mb-manual-1").await.is_none(),
        "nothing has fetched this release yet"
    );

    fixture
        .import
        .prefetch_release(
            &dir.to_string_lossy(),
            "mb-manual-1",
            crate::import::MetadataSource::MusicBrainz,
            crate::import::ClaimLevel::Exact,
        )
        .await
        .expect("a manual pick fetches");

    assert!(
        fixture.archived("mb-manual-1").await.is_some(),
        "and archives the release it fetched"
    );

    // Re-opening it costs nothing.
    let before = fixture.provider.requests().len();
    fixture
        .import
        .prefetch_release(
            &dir.to_string_lossy(),
            "mb-manual-1",
            crate::import::MetadataSource::MusicBrainz,
            crate::import::ClaimLevel::Exact,
        )
        .await
        .expect("re-opening reads what the first open archived");
    assert_eq!(
        fixture.provider.requests().len(),
        before,
        "the second open reached the wire: {:?}",
        fixture.provider.requests()
    );
}

/// Skipped is a decision the user already made, so automatic identification
/// excludes it until the user explicitly unskips it.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_skipped_candidate_is_not_swept() {
    let fixture = Fixture::new("skipped").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .cover_art
        .seed_lookup(Some("mb-skipped-1"), Some("rg-skipped-1"), None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-skipped-1", "rg-skipped-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-skipped-1?",
        200,
        release_json("mb-skipped-1", "rg-skipped-1", &[probed, 0]),
    );
    fixture.scan(1).await;
    fixture
        .import
        .set_candidate_skipped(dir.to_string_lossy().into_owned(), true)
        .await
        .unwrap();

    fixture.sweep_once().await;

    assert!(
        fixture.provider.requests().is_empty(),
        "a skipped candidate costs the provider nothing: {:?}",
        fixture.provider.requests()
    );
    assert!(
        fixture.stored_for(&dir).await.is_none(),
        "and leaves no row"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn unskipping_a_stored_candidate_mid_pass_counts_it_immediately() {
    let fixture = Fixture::new("unskip-mid-pass").await;
    fixture
        .extraction
        .register_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    let stored = fixture.barcode_candidate("Stored");
    let running = fixture.disc_id_candidate("Running");
    std::fs::write(running.join("notes.txt"), "distinct candidate").unwrap();
    let probed = fixture.probed_total_ms(&running);
    fixture
        .cover_art
        .seed_lookup(Some("mb-unskip-stored"), Some("rg-unskip-stored"), None);
    fixture
        .cover_art
        .seed_lookup(Some("mb-unskip-running"), Some("rg-unskip-running"), None);
    fixture.provider.route(
        "/release?",
        200,
        search_json("mb-unskip-stored", "rg-unskip-stored"),
    );
    fixture.provider.route(
        "/release/mb-unskip-stored?",
        200,
        release_json("mb-unskip-stored", "rg-unskip-stored", &[probed, 0]),
    );
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-unskip-running", "rg-unskip-running", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-unskip-running?",
        200,
        release_json("mb-unskip-running", "rg-unskip-running", &[probed, 0]),
    );
    fixture.scan(2).await;
    fixture.select(&stored);
    fixture.await_row(&stored).await;
    fixture
        .import
        .set_candidate_skipped(stored.to_string_lossy().into_owned(), true)
        .await
        .unwrap();

    fixture.provider.hold("/discid/");
    let mut events = fixture.import.subscribe_events();
    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    fixture
        .import
        .set_candidate_skipped(stored.to_string_lossy().into_owned(), false)
        .await
        .unwrap();
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(15), pass)
        .await
        .expect("pass finishes after unskip")
        .unwrap();

    let row = fixture
        .stored_for(&stored)
        .await
        .expect("stored row remains");
    let verdict: TerminalVerdict = serde_json::from_str(&identify_result(&row).verdict).unwrap();
    assert!(matches!(&verdict, TerminalVerdict::Found { matches, .. }
        if matches[0].source_tracks.is_some()));
    let progress: Vec<_> = drain_events(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            ImportEvent::QueueIdentifyProgress { identified, total } => Some((identified, total)),
            _ => None,
        })
        .collect();
    assert!(
        progress.contains(&(1, 2)),
        "the stored unskipped candidate is counted immediately: {progress:?}"
    );
    assert_eq!(progress.last(), Some(&(2, 2)), "{progress:?}");
}

/// What starting an import does to a candidate, in the order the import
/// service does it: [`ImportServiceHandle::claim_candidate_for_import`] before
/// the command is queued, and the worker's first `ImportProgress` some time
/// after that — after the folder re-walk, and behind every import already
/// queued ahead of it. Tests that model the start as the event alone are
/// modelling the second half of it only.
async fn start_import_for(fixture: &Fixture, candidate: &Path) {
    let candidate_key = candidate.to_string_lossy().into_owned();
    fixture
        .import
        .claim_candidate_for_import(&candidate_key)
        .await;
    crate::import::handle::send_event(
        &fixture.import.event_tx,
        ImportEvent::ImportProgress {
            candidate_key,
            progress: crate::import::ImportProgress::Started {
                id: "release-importing".to_string(),
                import_id: "import-running".to_string(),
            },
        },
    );
}

/// An import started mid-pass takes its candidate away from the sweep: no
/// verdict is stored for it, and it stops counting towards the queue's total.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn an_import_start_mid_pass_removes_the_candidate_from_work_and_progress() {
    let fixture = Fixture::new("import-mid-pass").await;
    let remaining = fixture.disc_id_candidate("Remaining");
    let importing = fixture.disc_id_candidate("Importing");
    std::fs::write(importing.join("notes.txt"), "distinct candidate").unwrap();
    let importing_hash = fixture.content_hash(&importing);
    let probed = fixture.probed_total_ms(&remaining);
    fixture
        .cover_art
        .seed_lookup(Some("mb-import-progress"), Some("rg-import-progress"), None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-import-progress", "rg-import-progress", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-import-progress?",
        200,
        release_json("mb-import-progress", "rg-import-progress", &[probed, 0]),
    );
    fixture.scan(2).await;
    fixture.provider.hold("/discid/");

    let mut events = fixture.import.subscribe_events();
    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    // Starting an import, in the order the import service really does it: the
    // candidate is claimed before the command is queued, and the worker's
    // first progress event comes back some time after that.
    start_import_for(&fixture, &importing).await;
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(15), pass)
        .await
        .expect("pass finishes after import ownership changes")
        .unwrap();

    assert!(!fixture.stored().await.contains_key(&importing_hash));
    assert!(fixture.stored_for(&remaining).await.is_some());
    let progress: Vec<_> = drain_events(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            ImportEvent::QueueIdentifyProgress { identified, total } => Some((identified, total)),
            _ => None,
        })
        .collect();
    assert_eq!(progress.last(), Some(&(1, 1)), "{progress:?}");
}

/// A re-scan lands while an import owns a candidate. The scan announces every
/// candidate it walks, import or no import, and the pass must not count one
/// back in that an import has taken away — the queue's total would climb back
/// past what the sweep is responsible for and never come down, because nothing
/// announces the candidate again once the import finishes with it.
///
/// This is the same sequence CI hits on every non-macOS runner: the OS watcher
/// delivers the folder's own change events late enough that the re-scan they
/// trigger arrives inside the pass rather than after it. Driven here from the
/// bus instead of the filesystem, so the ordering is the test's and not the
/// watcher backend's.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_rescan_does_not_count_back_a_candidate_an_import_owns() {
    let fixture = Fixture::new("import-rescan").await;
    let remaining = fixture.disc_id_candidate("Remaining");
    let importing = fixture.disc_id_candidate("Importing");
    std::fs::write(importing.join("notes.txt"), "distinct candidate").unwrap();
    let probed = fixture.probed_total_ms(&remaining);
    fixture
        .cover_art
        .seed_lookup(Some("mb-import-rescan"), Some("rg-import-rescan"), None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-import-rescan", "rg-import-rescan", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-import-rescan?",
        200,
        release_json("mb-import-rescan", "rg-import-rescan", &[probed, 0]),
    );
    fixture.scan(2).await;
    fixture.provider.hold("/discid/");

    let mut events = fixture.import.subscribe_events();
    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    start_import_for(&fixture, &importing).await;
    // …and then the scan re-announces it, exactly as a watcher-triggered pass
    // over the same folder does.
    let claimed = match fixture.import.get_candidate(&importing.to_string_lossy()) {
        Some(ImportCandidateSnapshot::Folder { candidate, .. }) => candidate,
        other => panic!("the claimed candidate is still a folder candidate: {other:?}"),
    };
    crate::import::handle::send_event(
        &fixture.import.event_tx,
        ImportEvent::Scan(ScanEvent::FolderCandidate {
            candidate: claimed,
            skipped: false,
            is_added: false,
        }),
    );
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(15), pass)
        .await
        .expect("pass finishes after the re-scan")
        .unwrap();

    let progress: Vec<_> = drain_events(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            ImportEvent::QueueIdentifyProgress { identified, total } => Some((identified, total)),
            _ => None,
        })
        .collect();
    assert_eq!(progress.last(), Some(&(1, 1)), "{progress:?}");
}

/// The same import start, one step later in the candidate's life — and the
/// step where the pass's own bookkeeping can no longer help.
///
/// The verdict has settled and the pass is buying its tracklist, so the
/// candidate is in neither `in_flight` nor `pending`: the `ImportProgress` the
/// worker sends finds nothing to detach and cancels nothing, and the write is
/// already on its way. What stops the row is the claim — the write takes the
/// folder-state commit lock the claim was taken under, re-reads the candidate,
/// and finds an import owns it.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn an_import_started_while_a_verdict_is_in_flight_stores_nothing() {
    let fixture = Fixture::new("import-mid-write").await;
    fixture
        .extraction
        .register_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    let dir = fixture.barcode_candidate("From Barcode");
    let hash = fixture.content_hash(&dir);
    fixture
        .cover_art
        .seed_lookup(Some("mb-mid-write"), Some("rg-mid-write"), None);
    fixture.provider.route(
        "/release?",
        200,
        search_json("mb-mid-write", "rg-mid-write"),
    );
    fixture.provider.route(
        "/release/mb-mid-write?",
        200,
        release_json("mb-mid-write", "rg-mid-write", &[1, 1]),
    );
    fixture.scan(1).await;
    // A search result carries no tracklist, so the pass buys one before it can
    // store anything. Holding that lookup puts the import start exactly inside
    // the window between a settled verdict and its row.
    fixture.provider.hold("/release/mb-mid-write?");

    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    wait_for_request(&fixture.provider, "/release/mb-mid-write?", 1).await;
    start_import_for(&fixture, &dir).await;
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(20), pass)
        .await
        .expect("pass finishes after the import claims the candidate")
        .unwrap();

    assert!(
        !fixture.stored().await.contains_key(&hash),
        "the verdict was already bought and paid for, and is still not stored: {:?}",
        fixture.stored().await.keys().collect::<Vec<_>>()
    );
}

/// Progress crosses as an event carrying both numbers. The total is the sweep's
/// own count of what it is responsible for, so a view renders "n of m" without
/// counting the rows it happens to be holding — and the second pass opens at the
/// full count rather than starting over at zero.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn progress_carries_both_counts() {
    let fixture = Fixture::new("progress").await;
    let first = fixture.disc_id_candidate("Album One");
    // A second folder with a differing file makes a second content hash; the
    // two would otherwise share one row.
    let second = fixture.disc_id_candidate("Album Two");
    std::fs::write(second.join("notes.txt"), "different bytes").unwrap();
    let probed = fixture.probed_total_ms(&first);
    fixture
        .cover_art
        .seed_lookup(Some("mb-prog-1"), Some("rg-prog-1"), None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-prog-1", "rg-prog-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-prog-1?",
        200,
        release_json("mb-prog-1", "rg-prog-1", &[probed, 0]),
    );
    fixture.scan(2).await;

    let mut events = fixture.import.subscribe_events();
    fixture.sweep_once().await;
    let mut progress = Vec::new();
    for event in drain_events(&mut events) {
        if let ImportEvent::QueueIdentifyProgress { identified, total } = event {
            progress.push((identified, total));
        }
    }
    assert_eq!(
        progress.first(),
        Some(&(0, 2)),
        "planning announces the whole queue before any of it is answered"
    );
    assert_eq!(
        progress.last(),
        Some(&(2, 2)),
        "and every verdict advances the count: {progress:?}"
    );

    let mut events = fixture.import.subscribe_events();
    fixture.sweep_once().await;
    let replanned = loop {
        match events.try_recv() {
            Ok(ImportEvent::QueueIdentifyProgress { identified, total }) => {
                break (identified, total)
            }
            Ok(_) => continue,
            Err(e) => panic!("the second pass must announce progress too: {e}"),
        }
    };
    assert_eq!(
        replanned,
        (2, 2),
        "a pass over an answered queue opens at the full count"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn identified_progress_is_emitted_after_the_verdict_is_committed() {
    let fixture = Fixture::new("progress-after-commit").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .cover_art
        .seed_lookup(Some("mb-progress-commit"), Some("rg-progress-commit"), None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-progress-commit", "rg-progress-commit", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-progress-commit?",
        200,
        release_json("mb-progress-commit", "rg-progress-commit", &[probed, 0]),
    );
    fixture.scan(1).await;

    let mut events = fixture.import.subscribe_events();
    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });

    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .expect("identified progress arrives")
            .expect("event bus remains open");
        if matches!(
            event,
            ImportEvent::QueueIdentifyProgress {
                identified: 1,
                total: 1
            }
        ) {
            assert!(
                fixture.stored_for(&dir).await.is_some(),
                "the DB row must be readable before progress exposes the verdict"
            );
            break;
        }
    }

    pass.await.expect("sweep pass joins");
}

/// The sweep drives the whole queue through `IdentifyStateChanged` and
/// `SignalsUpdated`, and every one of those would otherwise invalidate a
/// candidate in both UIs. A run a person started still does.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn only_a_watched_run_invalidates_the_ui() {
    let fixture = Fixture::new("ui-invalidation").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .cover_art
        .seed_lookup(Some("mb-ui-1"), Some("rg-ui-1"), None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-ui-1", "rg-ui-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-ui-1?",
        200,
        release_json("mb-ui-1", "rg-ui-1", &[probed, 0]),
    );
    fixture.scan(1).await;

    let mut events = fixture.import.subscribe_events();
    fixture.sweep_once().await;
    let (swept_candidate_events, swept_watched) = count_candidate_events(&mut events);
    assert!(
        swept_candidate_events > 0,
        "the sweep does emit per-candidate events — otherwise this proves nothing"
    );
    assert_eq!(
        swept_watched, 0,
        "but none of them claims a person is watching, so the UI bus renders nothing"
    );

    // The same pipeline, opened by a person.
    let mut events = fixture.import.subscribe_events();
    fixture.select(&dir);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (_, selected_watched) = count_candidate_events(&mut events);
    assert!(
        selected_watched > 0,
        "an opened candidate keeps re-rendering exactly as it does today"
    );
}

/// `(per-candidate events, of which watched)` — the two the UI bus branches on.
fn count_candidate_events(
    events: &mut tokio::sync::broadcast::Receiver<ImportEvent>,
) -> (usize, usize) {
    let mut total = 0;
    let mut watched = 0;
    for event in drain_events(events) {
        let priority = match event {
            ImportEvent::IdentifyStateChanged { priority, .. }
            | ImportEvent::SignalsUpdated { priority, .. } => priority,
            _ => continue,
        };
        total += 1;
        if priority == CallPriority::Interactive {
            watched += 1;
        }
    }
    (total, watched)
}

fn drain_events(events: &mut tokio::sync::broadcast::Receiver<ImportEvent>) -> Vec<ImportEvent> {
    let mut drained = Vec::new();
    loop {
        match events.try_recv() {
            Ok(event) => drained.push(event),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => return drained,
            Err(error) => panic!("import event bus failed while draining ready events: {error}"),
        }
    }
}

/// A candidate that vanishes while it is being identified must not wedge the
/// pass. The signals service cancels extraction on `CandidateRemoved` and
/// nothing cancels identify, so the driver would sit in `Triangulating`
/// forever holding a slot — and because the outer loop only takes another
/// `ScanEvent::Finished` between passes, a stalled pass silently ends sweeping
/// for the whole session.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_candidate_removed_mid_flight_does_not_wedge_the_sweep() {
    let fixture = Fixture::new("removed-mid-flight").await;
    fixture.extraction.register_analyzer(Arc::new(SlowAnalyzer {
        delay: Duration::from_millis(400),
    }));
    let dir = fixture.barcode_candidate("Vanishing");
    let hash = fixture.content_hash(&dir);
    fixture.scan(1).await;

    // Start the pass and let extraction get as far as its (slow) OCR pass, so
    // the candidate is genuinely mid-flight when the folder goes.
    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    tokio::time::sleep(Duration::from_millis(150)).await;
    std::fs::remove_dir_all(&dir).unwrap();
    // What the folder watcher does when a candidate's directory goes: re-scan
    // the root and reconcile, which emits `CandidateRemoved` for the one that
    // is no longer there.
    fixture.import.scan_watched_folders().unwrap();

    tokio::time::timeout(Duration::from_secs(10), pass)
        .await
        .expect("the pass must finish rather than wait on a candidate that is gone")
        .unwrap();

    assert!(
        !fixture.stored().await.contains_key(&hash),
        "a candidate that vanished mid-identification learned nothing"
    );
    // And the sweep is still alive to the queue: a later pass runs.
    fixture.sweep_once().await;
}

/// A candidate the sweep is done with leaves nothing of the sweep's behind.
///
/// `run_driver` only ends via `Cancelled`, so a settled driver the sweep does
/// not cancel parks a task, a bus-relay task, and a live broadcast receiver that
/// every later `IdentifyStateChanged` — a whole `IdentifyState`, result vectors
/// and all — is deep-cloned into. Over a queue swept unattended on every launch
/// that fan-out is quadratic in its size. The sweep never toggles a signal or
/// re-runs, so it has no use for the driver once the verdict is written.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_finished_candidate_leaves_no_driver_behind() {
    let fixture = Fixture::new("no-driver-left").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .cover_art
        .seed_lookup(Some("mb-drv-1"), Some("rg-drv-1"), None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-drv-1", "rg-drv-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-drv-1?",
        200,
        release_json("mb-drv-1", "rg-drv-1", &[probed, 0]),
    );
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();

    fixture.sweep_once().await;

    assert!(
        fixture.stored_for(&dir).await.is_some(),
        "the candidate really was identified"
    );
    assert!(
        !fixture.identify.is_running(&key),
        "and its driver is gone rather than parked for a toggle the sweep will never send"
    );
    assert!(
        fixture.context().ours.lock().unwrap().is_empty(),
        "the sweep holds no ownership of a candidate it has finished with"
    );
}

/// The case the ownership guard exists for, end to end: the sweep fails a
/// candidate, the user then opens it, and the next pass must not take it back.
///
/// `identify.start` supersedes, so taking it would cancel their Interactive run
/// and restart it in the background. This only holds because the sweep gives up
/// ownership when it finishes with a candidate — a set that only ever grows
/// would claim this one forever.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_candidate_the_sweep_failed_then_the_user_opened_is_left_alone() {
    let fixture = Fixture::new("failed-then-opened").await;
    fixture.extraction.register_analyzer(Arc::new(SlowAnalyzer {
        delay: Duration::from_millis(2_000),
    }));
    let dir = fixture.disc_id_candidate("Album");
    // One image, so the user's run stays in flight on a slow OCR pass while the
    // second sweep pass runs.
    std::fs::write(dir.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0, 0x00]).unwrap();
    fixture
        .provider
        .set_routes(vec![("/discid/", 400, "{}".to_string())]);
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();

    fixture.sweep_once().await;
    assert!(
        fixture.stored_for(&dir).await.is_none(),
        "the first pass learned nothing"
    );

    // The user opens it.
    fixture.select(&dir);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(fixture.identify.is_running(&key), "their run is in flight");

    let lookups_before = fixture.provider.count_containing("/discid/");
    let mut events = fixture.import.subscribe_events();
    fixture.sweep_once().await;
    let (candidate_events, watched) = count_candidate_events(&mut events);

    assert_eq!(
        candidate_events - watched,
        0,
        "the sweep started no background run for a candidate the user has open"
    );
    assert!(
        fixture.context().ours.lock().unwrap().is_empty(),
        "and claimed no ownership of it: it did not take the candidate back"
    );
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        lookups_before,
        "nor spent a background lookup on it"
    );
    assert!(
        fixture.identify.is_running(&key),
        "their run is still the one registered — it was not cancelled and \
         restarted underneath them"
    );
}

/// The guard the priority exists for. A candidate someone has open is left
/// alone — `identify.start` supersedes, so taking it would cancel their
/// Interactive run and restart it in the background.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn the_sweep_leaves_a_candidate_the_user_has_open_alone() {
    let fixture = Fixture::new("user-owns-it").await;
    // A slow OCR pass keeps the user's run in flight across the sweep.
    fixture.extraction.register_analyzer(Arc::new(SlowAnalyzer {
        delay: Duration::from_millis(1_500),
    }));
    let dir = fixture.barcode_candidate("Opened");
    fixture.scan(1).await;

    fixture.select(&dir);
    // Let identify register the user's driver before the sweep plans.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        fixture.identify.is_running(&dir.to_string_lossy()),
        "the user's run is in flight"
    );

    fixture.sweep_once().await;

    assert!(
        !fixture
            .context()
            .ours
            .lock()
            .unwrap()
            .contains(dir.to_string_lossy().as_ref()),
        "the sweep never took ownership of a candidate it does not own"
    );
    assert!(
        fixture.identify.is_running(&dir.to_string_lossy()),
        "and it did not cancel the run out from under them"
    );
}

/// Teardown writes nothing. The token is re-checked immediately before the
/// write, so a cancellation landing during the settle lookup that precedes it
/// cannot leave a row behind.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_cancelled_candidate_writes_no_row() {
    let fixture = Fixture::new("cancelled-writes-nothing").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .cover_art
        .seed_lookup(Some("mb-cancel-1"), Some("rg-cancel-1"), None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-cancel-1", "rg-cancel-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-cancel-1?",
        200,
        release_json("mb-cancel-1", "rg-cancel-1", &[probed, 0]),
    );
    fixture.scan(1).await;
    // Hold the disc-ID response, so the cancel lands while the candidate is
    // genuinely mid-identification rather than racing a pass that already
    // finished.
    fixture.provider.hold("/discid/");

    let context = fixture.context();
    let token = CancellationToken::new();
    let pass_token = token.clone();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &pass_token).await });
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    token.cancel();
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(10), pass)
        .await
        .expect("a cancelled pass returns")
        .unwrap();

    assert!(
        fixture.stored_for(&dir).await.is_none(),
        "a cancelled candidate writes no row: {:?}",
        fixture.stored().await.keys().collect::<Vec<_>>()
    );

    // `save` itself refuses under a cancelled token, whatever reached it.
    let verdict = TerminalVerdict::NotFoundAnywhere;
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(
        !save(
            &fixture.context(),
            &cancelled,
            "/x",
            "hash-x",
            "/x",
            &verdict,
            0,
            0,
        )
        .await,
        "the write is gated on the token, not only the lookup before it"
    );
    assert!(fixture.stored().await.is_empty());
}

#[test]
fn a_verdict_that_no_longer_decodes_is_rejected() {
    let stale = synthetic_candidate("/b", 222);
    let row = row_with_verdict(
        &stale,
        r#"{"ShapeFromAnOlderBuild":{"whatever":1}}"#.to_string(),
    );
    assert!(decode(&row).is_err());
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_malformed_verdict_on_a_late_candidate_aborts_without_panicking() {
    let fixture = Fixture::new("malformed-late-row").await;
    let running = fixture.disc_id_candidate("Running");
    let probed = fixture.probed_total_ms(&running);
    fixture.cover_art.seed_lookup(
        Some("mb-malformed-running"),
        Some("rg-malformed-running"),
        None,
    );
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-malformed-running", "rg-malformed-running", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-malformed-running?",
        200,
        release_json("mb-malformed-running", "rg-malformed-running", &[probed, 0]),
    );
    fixture.scan(1).await;

    fixture.provider.hold("/discid/");

    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    let late = fixture.disc_id_candidate("Late");
    std::fs::write(late.join("late-playlist.m3u"), "late identity").unwrap();
    fixture
        .manager
        .save_import_candidate_verdict(&NewImportCandidateVerdict {
            content_hash: fixture.content_hash(&late),
            folder_path: late.to_string_lossy().into_owned(),
            verdict: r#"{"ShapeFromAnOlderBuild":{"whatever":1}}"#.to_string(),
            probed_total_duration_ms: 0,
            expected_edit_revision: 0,
            identity_pick: None,
        })
        .await
        .unwrap();
    fixture
        .import
        .refresh_watched_folder(fixture.root.to_string_lossy().into_owned())
        .await
        .unwrap();
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(10), pass)
        .await
        .expect("malformed late row aborts the pass")
        .expect("malformed late row is handled without panic");
    assert!(!fixture
        .identify
        .is_running(running.to_string_lossy().as_ref()));
}

#[test]
fn duplicate_content_hashes_share_one_identify_job() {
    let first = synthetic_candidate("/first", 321);
    let second = synthetic_candidate("/second", 321);
    assert_eq!(first.files.content_hash(), second.files.content_hash());

    let planned = plan(vec![first.clone(), second.clone()], &HashMap::new(), 2);
    assert_eq!(planned.identify.len(), 1);
    assert_eq!(planned.identify[0].candidates.len(), 2);
    assert_eq!(planned.identified, 0);

    let stored = HashMap::from([(
        first.files.content_hash(),
        row_with_verdict(
            &first,
            serde_json::to_string(&TerminalVerdict::NotFoundAnywhere).unwrap(),
        ),
    )]);
    let planned = plan(vec![first, second], &stored, 2);
    assert!(planned.identify.is_empty());
    assert_eq!(planned.identified, 2);
}

// ── Synthetic candidates, for the pure planning tests ───────────────────────

fn synthetic_candidate(path: &str, size: u64) -> FolderCandidate {
    use crate::import::folder_scanner::{CandidateFile, CategorizedFiles, FileRole, ScannedFile};
    FolderCandidate {
        path: PathBuf::from(path),
        file_root: PathBuf::from(path),
        name: path.trim_start_matches('/').to_string(),
        files: CategorizedFiles {
            files: vec![CandidateFile {
                proposed_audio: true,
                file: ScannedFile::new(
                    PathBuf::from(format!("{path}/01.flac")),
                    "01.flac".to_string(),
                    size,
                ),
                role: FileRole::Audio,
            }],
            format_label: "FLAC".to_string(),
        },
        watched_folder_path: "/".to_string(),
        scope: crate::import::folder_scanner::ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: path.trim_start_matches('/').to_string(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    }
}

fn row_with_verdict(candidate: &FolderCandidate, verdict: String) -> DbImportCandidateState {
    DbImportCandidateState {
        content_hash: candidate.files.content_hash(),
        folder_path: candidate.path.to_string_lossy().into_owned(),
        identify: Some(crate::db::DbCandidateIdentifyResult {
            verdict,
            probed_total_duration_ms: 0,
            identified_at: fixed_now(),
        }),
        file_edits: Default::default(),
        identity_pick: None,
    }
}

// ── 10. Selection resumes a stored verdict ──────────────────────────────────

/// A several-match verdict, as identification stores one: the pressing is the
/// open question, so no match carries a settled tracklist.
fn multi_match_verdict(release_ids: &[&str], group_id: &str) -> TerminalVerdict {
    TerminalVerdict::Found {
        matches: release_ids
            .iter()
            .map(|release_id| MetadataResult {
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
                source_tracks: None,
            })
            .collect(),
        track_count: 2,
        group: crate::identify::GroupKey {
            source: crate::import::MetadataSource::MusicBrainz,
            source_group_id: group_id.to_string(),
        },
        provenance: release_ids
            .iter()
            .map(|_| crate::identify::ResultProvenance {
                by_disc_id: true,
                by_barcode: false,
                matches_catalog: false,
            })
            .collect(),
    }
}

/// Selecting an answered candidate stands its stored verdict back up as the
/// identify state — every stored match, at `Interactive`, with the provider
/// gone. This is what makes clicking a "several matches" row show those
/// matches instantly instead of re-running the whole pipeline.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn selecting_an_answered_candidate_resumes_its_verdict_with_the_provider_gone() {
    let fixture = Fixture::new("resume-answered").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.scan(1).await;

    // Nothing is routed and nothing is seeded: any lookup would 404 its way
    // to a different state than the stored one.
    let verdict = multi_match_verdict(&["mb-resume-1", "mb-resume-2"], "rg-resume-1");
    let wrote = fixture
        .import
        .save_candidate_verdict_if_current(
            &dir.to_string_lossy(),
            &NewImportCandidateVerdict {
                content_hash: fixture.content_hash(&dir),
                folder_path: dir.to_string_lossy().into_owned(),
                verdict: serde_json::to_string(&verdict).unwrap(),
                probed_total_duration_ms: fixture.probed_total_ms(&dir) as i64,
                expected_edit_revision: 0,
                identity_pick: None,
            },
        )
        .await
        .unwrap();
    assert!(wrote, "the seeded verdict lands");
    let mut events = fixture.import.subscribe_events();

    fixture.select(&dir);

    let state = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match events.recv().await.expect("bus stays open") {
                ImportEvent::IdentifyStateChanged { state, .. } => return state,
                _ => continue,
            }
        }
    })
    .await
    .expect("the resumed state is broadcast");
    let IdentifyState::Found { matches, .. } = &state else {
        panic!("expected the stored Found back, got {state:?}");
    };
    assert_eq!(
        matches
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["mb-resume-1", "mb-resume-2"],
        "every stored match is in the resumed state"
    );
    assert!(
        fixture.provider.requests().is_empty(),
        "resuming reached the wire for nothing: {:?}",
        fixture.provider.requests()
    );
}

/// A driver being torn down after settling broadcasts `Idle` on its way out —
/// the sweep cancels its own drivers once they settle. The recorded runtime
/// keeps the terminal state: the candidate's answer doesn't stop being its
/// answer because the machinery that produced it exited. A genuine mid-run
/// cancel still resets.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_settled_runs_teardown_does_not_blank_its_recorded_state() {
    let fixture = Fixture::new("teardown-keeps-state").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();

    let not_in_library = |result: &MetadataResult| crate::db::LibraryStatus {
        release_id: result.release_id.clone(),
        release_in_library: false,
        album_in_library: false,
        album_title: None,
        album_id: None,
    };
    let found =
        multi_match_verdict(&["mb-teardown-1"], "rg-teardown-1").resume_state(&not_in_library);
    let changed = |state: IdentifyState| ImportEvent::IdentifyStateChanged {
        candidate_key: key.clone(),
        toolbar: Vec::new(),
        state,
        priority: CallPriority::Background,
    };

    fixture.import.record_candidate_event(&changed(found));
    fixture
        .import
        .record_candidate_event(&changed(IdentifyState::Idle));
    let Some(ImportCandidateSnapshot::Folder { runtime, .. }) = fixture.import.get_candidate(&key)
    else {
        panic!("the scanned candidate is readable");
    };
    let IdentifyState::Found { matches, .. } = &runtime.identify_state else {
        panic!(
            "a terminal state survives its driver's teardown, got {:?}",
            runtime.identify_state
        );
    };
    assert_eq!(matches[0].release_id, "mb-teardown-1");

    // A mid-run cancel is a different fact and still resets: the run was
    // abandoned, not answered.
    let triangulating = IdentifyState::Triangulating {
        discid: crate::identify::DiscidProgress::Computing,
        barcode: crate::identify::BarcodeProgress::Scanning,
        context: crate::identify::state::SignalsContext {
            disc_id: crate::signals::DiscIdSignal::Absent { track_count: 0 },
            barcode_codes: Vec::new(),
            had_barcode_source: false,
            catalogs: Vec::new(),
            excluded: Default::default(),
            discid_results: Vec::new(),
            barcode_results: Vec::new(),
            discid_failure: None,
            barcode_failure: None,
            matched_barcode: None,
            track_count: 0,
        },
    };
    fixture
        .import
        .record_candidate_event(&changed(triangulating));
    fixture
        .import
        .record_candidate_event(&changed(IdentifyState::Idle));
    let Some(ImportCandidateSnapshot::Folder { runtime, .. }) = fixture.import.get_candidate(&key)
    else {
        panic!("the scanned candidate is readable");
    };
    assert!(
        matches!(runtime.identify_state, IdentifyState::Idle),
        "a cancelled mid-run state resets as before, got {:?}",
        runtime.identify_state
    );
}

/// Re-run on a candidate whose driver is gone starts a fresh interactive run
/// instead of no-op'ing — the stored answer is what a re-run exists to
/// replace, so it is not consulted.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_rerun_with_no_driver_runs_identification_again() {
    let fixture = Fixture::new("rerun-no-driver").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    fixture
        .store_settled_verdict(&dir, "mb-rerun-1", "rg-rerun-1", probed)
        .await;
    fixture
        .cover_art
        .seed_lookup(Some("mb-rerun-2"), Some("rg-rerun-2"), None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-rerun-2", "rg-rerun-2", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-rerun-2?",
        200,
        release_json("mb-rerun-2", "rg-rerun-2", &[probed, 0]),
    );

    fixture
        .sweep
        .rerun_for_selection(dir.to_string_lossy().into_owned());

    wait_for_request(&fixture.provider, "/discid/", 1).await;
}

// ── 11. Re-stating a file decision changes nothing ──────────────────────────

/// The disc menu and the role picker fire on every selection, including of
/// the item already in force. A decision that re-states what is already true
/// writes nothing — above all it does not clear the stored verdict, which
/// would re-identify a folder whose shape did not change and blank the pane
/// over it.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn restating_a_file_decision_changes_nothing() {
    let fixture = Fixture::new("edit-noop").await;
    let dir = fixture.root.join("Album");
    std::fs::create_dir_all(&dir).unwrap();
    for name in [
        "Test Album.cue",
        "Test Album.flac",
        "02 Test Artist - Track Two (White Noise).flac",
        "03 Test Artist - Track Three (Brown Noise).flac",
    ] {
        std::fs::copy(
            Path::new("tests/fixtures/cue_flac").join(name),
            dir.join(name),
        )
        .unwrap();
    }
    fixture.scan(1).await;
    fixture
        .store_settled_verdict(&dir, "mb-noop-1", "rg-noop-1", 1_000)
        .await;
    let key = dir.to_string_lossy().into_owned();
    let mut events = fixture.import.subscribe_events();

    // The sheet already carves disc one, the loose file is already audio, and
    // the sheet already binds its own container.
    fixture
        .import
        .set_sheet_disc(
            key.clone(),
            "Test Album.cue".to_string(),
            crate::import::folder_scanner::SheetDisc::Disc { number: 1 },
        )
        .await
        .unwrap();
    fixture
        .import
        .set_file_role(
            key.clone(),
            "02 Test Artist - Track Two (White Noise).flac".to_string(),
            crate::import::folder_scanner::FileRoleChoice::Audio,
        )
        .await
        .unwrap();
    fixture
        .import
        .set_sheet_binding(
            key.clone(),
            "Test Album.cue".to_string(),
            Some("Test Album.flac".to_string()),
        )
        .await
        .unwrap();

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate's row remains");
    assert!(
        row.identify.is_some(),
        "a re-stated decision must not clear the stored verdict"
    );
    assert!(
        !drain_events(&mut events).iter().any(|event| matches!(
            event,
            ImportEvent::Scan(ScanEvent::CandidateBindingChanged { .. })
        )),
        "and must not announce a changed candidate"
    );

    // A genuinely different decision still lands and still clears.
    fixture
        .import
        .set_sheet_disc(
            key,
            "Test Album.cue".to_string(),
            crate::import::folder_scanner::SheetDisc::Ignored,
        )
        .await
        .unwrap();
    assert!(
        fixture
            .stored_for(&dir)
            .await
            .is_none_or(|row| row.identify.is_none()),
        "a real change clears the verdict as before"
    );
}

// ── 12. The pick command and the answer query serve one payload ─────────────

/// Deciding an identity persists it, the row carries it back, and reading it
/// — the whole of "resume" — returns the same seeded answer with the
/// provider gone. A settled single match wrote the same record, so a Ready
/// candidate answers identically.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_pick_reads_back_as_the_same_answer() {
    let fixture = Fixture::new("pick-answer").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();

    // The settled verdict wrote the pick; nothing is routed, so everything
    // below is served from what identification archived.
    fixture
        .archive("mb-answer-1", "rg-answer-1", &[probed, 0])
        .await;
    fixture
        .store_settled_verdict(&dir, "mb-answer-1", "rg-answer-1", probed)
        .await;

    let resumed = fixture
        .import
        .candidate_answer(key.clone())
        .await
        .expect("the stored decision reads back")
        .expect("a settled single match is a decision");
    let crate::import::DecidedIdentity::Release {
        release_id,
        prefetch,
        ..
    } = &resumed
    else {
        panic!("expected the settled release back, got Unknown");
    };
    assert_eq!(release_id, "mb-answer-1");
    assert_eq!(prefetch.detail.tracks.len(), 2);
    // Identification settling on one match is a pick, so it claims the
    // pressing exactly as a click on that release would.
    assert_eq!(prefetch.claim.level, crate::import::ClaimLevel::Exact);

    // The row carries the same decision for the sidebar's resume trigger.
    let queue = crate::import::triage::load(&fixture.import, &fixture.manager)
        .await
        .expect("the triage queue loads");
    let picked = queue
        .sections
        .iter()
        .flat_map(|section| &section.entries)
        .find_map(|entry| match entry {
            crate::import::triage::TriageEntry::Candidate(row) if row.candidate_key == key => {
                row.picked.clone()
            }
            _ => None,
        })
        .expect("the row carries the decision");
    assert_eq!(
        picked,
        crate::import::IdentityPick::Release {
            source: crate::import::MetadataSource::MusicBrainz,
            release_id: "mb-answer-1".to_string(),
            claim: crate::import::ClaimLevel::Exact,
        }
    );

    // A person deciding Unknown replaces the record, and the query returns
    // exactly what the command did.
    let decided = fixture
        .import
        .pick_candidate_identity(key.clone(), crate::import::IdentityPick::Unknown)
        .await
        .expect("deciding Unknown succeeds");
    assert!(matches!(
        decided,
        crate::import::DecidedIdentity::Unknown { .. }
    ));
    let resumed = fixture
        .import
        .candidate_answer(key)
        .await
        .expect("the replaced decision reads back")
        .expect("Unknown is a decision");
    assert!(matches!(
        resumed,
        crate::import::DecidedIdentity::Unknown { .. }
    ));
    assert!(
        fixture.provider.requests().is_empty(),
        "every answer came from the archive: {:?}",
        fixture.provider.requests()
    );
}

/// The sidebar row leads with the identity the candidate is settled on. A
/// manual search settles it on a release identification never named, and the
/// pick is the only record of that — a row reading the stored verdict alone
/// goes on showing the folder name and a placeholder while the pane shows the
/// release, with nothing to move it off.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_picked_release_is_what_the_row_leads_with() {
    let fixture = Fixture::new("pick-row").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();

    // Identification settled on one release; the user searched and picked
    // another, whose documents the search archived.
    fixture
        .archive("mb-answer-1", "rg-answer-1", &[probed, 0])
        .await;
    fixture
        .store_settled_verdict(&dir, "mb-answer-1", "rg-answer-1", probed)
        .await;
    // The picked release is one identification never fetched, which is what a
    // manual search result is: its documents are archived by the pick itself.
    fixture.provider.route(
        "/release/mb-picked-1",
        200,
        titled_release_json(
            "mb-picked-1",
            "rg-picked-1",
            "Picked Album Title",
            "Picked Artist Name",
        ),
    );
    fixture.cover_art.seed_lookup(
        Some("mb-picked-1"),
        Some("rg-picked-1"),
        Some(crate::import::cover_art::RemoteCover {
            url: "https://caa.example/picked.jpg".to_string(),
            thumbnail_url: "https://caa.example/picked-250.jpg".to_string(),
            label: "Cover Art Archive".to_string(),
            source: crate::import::MetadataSource::MusicBrainz,
        }),
    );

    // Read the queue on the event the surfaces refresh on, not after the pick
    // has finished settling: the row has to be right the moment it lands.
    let mut events = fixture.import.event_sender_for_test().subscribe();
    let picking = {
        let import = fixture.import.clone();
        let key = key.clone();
        tokio::spawn(async move {
            import
                .pick_candidate_identity(
                    key,
                    crate::import::IdentityPick::Release {
                        source: crate::import::MetadataSource::MusicBrainz,
                        release_id: "mb-picked-1".to_string(),
                        claim: crate::import::ClaimLevel::Exact,
                    },
                )
                .await
        })
    };
    loop {
        let event = events.recv().await.expect("the pick raises an event");
        if matches!(
            &event,
            crate::import::ImportEvent::Scan(super::super::handle::ScanEvent::CandidateIdentityPicked {
                candidate_key,
            }) if *candidate_key == key
        ) {
            break;
        }
    }

    let queue = crate::import::triage::load(&fixture.import, &fixture.manager)
        .await
        .expect("the triage queue loads");
    picking
        .await
        .expect("the pick task runs")
        .expect("picking the searched release succeeds");
    let matched = queue
        .sections
        .iter()
        .flat_map(|section| &section.entries)
        .find_map(|entry| match entry {
            crate::import::triage::TriageEntry::Candidate(row) if row.candidate_key == key => {
                row.matched.clone()
            }
            _ => None,
        })
        .expect("the row leads with the release the pick settled it on");
    assert_eq!(matched.release_id, "mb-picked-1");
    assert_eq!(matched.title, "Picked Album Title");
    assert_eq!(matched.artist.as_deref(), Some("Picked Artist Name"));
    assert_eq!(
        matched.cover_thumbnail_url.as_deref(),
        Some("https://caa.example/picked-250.jpg")
    );
}

/// Lowering the claim is a decision like any other: it is written with the
/// pick, so the answer, the row's resume record and the identity a bulk import
/// would commit all come back at the album level after a restart. The evidence
/// here is a disc ID that matched one release — the sharpest there is — and it
/// still does not move the claim back.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_lowered_claim_reads_back_lowered() {
    let fixture = Fixture::new("pick-lowered").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();

    fixture
        .archive("mb-answer-1", "rg-answer-1", &[probed, 0])
        .await;
    fixture
        .store_settled_verdict(&dir, "mb-answer-1", "rg-answer-1", probed)
        .await;

    let lowered = crate::import::IdentityPick::Release {
        source: crate::import::MetadataSource::MusicBrainz,
        release_id: "mb-answer-1".to_string(),
        claim: crate::import::ClaimLevel::Approximate,
    };
    let decided = fixture
        .import
        .pick_candidate_identity(key.clone(), lowered.clone())
        .await
        .expect("lowering the claim succeeds");
    let crate::import::DecidedIdentity::Release { prefetch, .. } = &decided else {
        panic!("expected the picked release back, got Unknown");
    };
    assert_eq!(prefetch.claim.level, crate::import::ClaimLevel::Approximate);
    // The evidence is untouched: it says what identified the release, not what
    // the user claims about it.
    assert_eq!(
        prefetch.claim.evidence,
        crate::import::ClaimEvidence::DiscIdAlone
    );

    // The query serves what the command did, which is what a restart reads.
    let resumed = fixture
        .import
        .candidate_answer(key.clone())
        .await
        .expect("the lowered decision reads back")
        .expect("a lowered claim is still a decision");
    let crate::import::DecidedIdentity::Release { prefetch, .. } = &resumed else {
        panic!("expected the picked release back, got Unknown");
    };
    assert_eq!(prefetch.claim.level, crate::import::ClaimLevel::Approximate);

    // And the row carries it both ways: the pick the pane reopens on, and the
    // identity a bulk import of this row would commit.
    let queue = crate::import::triage::load(&fixture.import, &fixture.manager)
        .await
        .expect("the triage queue loads");
    let row = queue
        .sections
        .iter()
        .flat_map(|section| &section.entries)
        .find_map(|entry| match entry {
            crate::import::triage::TriageEntry::Candidate(row) if row.candidate_key == key => {
                Some(row.clone())
            }
            _ => None,
        })
        .expect("the row is in the queue");
    assert_eq!(row.picked, Some(lowered));
    assert_eq!(
        row.claim,
        Some(crate::import::IdentityChoice::Approximate {
            release_ref: crate::import::MetadataRef::new(
                "mb-answer-1",
                crate::import::MetadataSource::MusicBrainz
            ),
        })
    );
}

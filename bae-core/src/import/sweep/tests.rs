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
use crate::db::{Database, DbImportCandidateState};
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
    /// Held before answering, so a test can act while a request is in flight.
    delay: Duration,
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

    /// Make every later response take `delay` to arrive.
    fn set_delay(&self, delay: Duration) {
        self.state.lock().unwrap().delay = delay;
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

    let (status, body, delay) = {
        let mut state = state.lock().unwrap();
        state.requests.push(target.clone());
        let (status, body) = state
            .routes
            .iter()
            .find(|(needle, _, _)| target.contains(needle.as_str()))
            .map(|(_, status, body)| (*status, body.clone()))
            .unwrap_or((404, "{}".to_string()));
        (status, body, state.delay)
    };
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
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

fn discid_json(release_id: &str, group_id: &str, track_lengths: &[u64]) -> String {
    format!(
        r#"{{"releases":[{}]}}"#,
        release_json(release_id, group_id, track_lengths)
    )
}

/// A search hit as `ws/2/release?query=…` returns it: no `media`, hence no
/// lengths and no count — the shape that forces the paid lookup.
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
            StoreKeys::new(library_id),
            clock,
            ids,
            crate::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
        );

        let cover_art = CoverArtArchiveClient::hermetic();
        let import = crate::import::ImportService::start(
            tokio::runtime::Handle::current(),
            manager.clone(),
            cover_art.clone(),
        );
        let identify = IdentifyServiceHandle::new(
            manager.clone(),
            tokio::runtime::Handle::current(),
            import.event_sender_for_test(),
            cover_art.clone(),
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
        let sweep = QueueSweepHandle {
            context: context.clone(),
            token: CancellationToken::new(),
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
            .identify_for_selection(dir.to_string_lossy().into_owned(), dir.to_path_buf());
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
        tokio::time::timeout(Duration::from_secs(30), run_pass(&self.context(), &token))
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
        crate::import::folder_scanner::collect_release_candidate_files(dir)
            .expect("the candidate folder is readable")
            .content_hash()
    }

    async fn stored_for(&self, dir: &Path) -> Option<DbImportCandidateState> {
        let hash = self.content_hash(dir);
        self.stored().await.remove(&hash)
    }

    /// The classification a sidebar would derive from a stored row — the stored
    /// verdict plus a live library check, never a stored classification.
    async fn classification_for(&self, dir: &Path) -> QueueClassification {
        let row = self.stored_for(dir).await.expect("a row was stored");
        let verdict: TerminalVerdict = serde_json::from_str(&row.verdict).expect("verdict decodes");
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
        classify(&verdict, row.probed_total_duration_ms as u64, &statuses)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // The base URL is process-wide; leaving it pointed at a dead port would
        // make the next test's live-service assumption silently wrong.
        crate::musicbrainz::set_base_url_for_test(None);
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
        .seed_lookup(Some("mb-ready-1"), None, None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json(
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
    assert_eq!(
        row.probed_total_duration_ms as u64, probed,
        "the probed total rode the fast pass into the row"
    );
    assert_eq!(
        row.identified_at,
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
        .seed_lookup(Some("mb-cached-1"), None, None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-cached-1", "rg-cached-1", &[probed, 0]),
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
        .seed_lookup(Some("mb-retry-1"), None, None);
    fixture
        .provider
        .set_routes(vec![("/discid/", 400, "{}".to_string())]);
    fixture.scan(1).await;

    fixture.sweep_once().await;
    assert!(
        fixture.stored_for(&dir).await.is_none(),
        "a failed lookup must leave no row — a stored failure is a stored answer"
    );

    fixture.provider.set_routes(vec![(
        "/discid/",
        200,
        discid_json("mb-retry-1", "rg-retry-1", &[probed, 0]),
    )]);
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
        dirs.push(fixture.disc_id_candidate(&format!("Album {i}")));
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
    fixture
        .provider
        .route("/release?", 200, search_json("mb-typed", "rg-typed"));
    fixture.cover_art.seed_lookup(Some("mb-typed"), None, None);
    fixture.scan(8).await;

    let context = fixture.context();
    let token = CancellationToken::new();
    let sweep_token = token.clone();
    let sweep = tokio::spawn(async move { run_pass(&context, &sweep_token).await });

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

// ── 6. The paid lookup, and who never pays it ───────────────────────────────

/// One `lookup_release_by_id` for a single match that arrived without a
/// tracklist, and none at all for one that arrived with it.
///
/// Both candidates are swept in the same pass, so the assertion is about which
/// of them cost a release lookup, not about whether release lookups happen.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn the_paid_lookup_is_spent_only_on_a_non_disc_id_single_result() {
    let fixture = Fixture::new("paid-lookup").await;
    fixture
        .extraction
        .register_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    let disc_dir = fixture.disc_id_candidate("From Disc Id");
    let barcode_dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&disc_dir);

    for id in ["mb-disc-1", "mb-barcode-1"] {
        fixture.cover_art.seed_lookup(Some(id), None, None);
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
    fixture.provider.route(
        "/release/mb-barcode-1?",
        200,
        release_json("mb-barcode-1", "rg-barcode-1", &[probed, 0]),
    );
    fixture.scan(2).await;

    fixture.sweep_once().await;

    assert_eq!(
        fixture.count_release_lookups("mb-disc-1"),
        0,
        "a disc-ID match already carries its tracklist; buying it again is the \
         cost the free path exists to avoid: {:?}",
        fixture.provider.requests()
    );
    assert_eq!(
        fixture.count_release_lookups("mb-barcode-1"),
        1,
        "a search result carries no lengths, so it costs exactly one lookup: {:?}",
        fixture.provider.requests()
    );
    assert_eq!(
        fixture.classification_for(&disc_dir).await,
        QueueClassification::Ready
    );
    assert_eq!(
        fixture.classification_for(&barcode_dir).await,
        QueueClassification::Ready,
        "the paid lookup is what admits the barcode match"
    );
}

/// Opening a candidate never buys anything. The interactive path drives the
/// same pipeline to the same `Found`, and the release lookup the sweep would
/// have spent is not spent here — a person waits for their candidate's identity,
/// never for a background classification.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn an_interactively_opened_candidate_never_pays_for_the_lookup() {
    let fixture = Fixture::new("interactive-no-paid").await;
    fixture
        .extraction
        .register_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    let dir = fixture.barcode_candidate("From Barcode");
    fixture
        .cover_art
        .seed_lookup(Some("mb-interactive-1"), None, None);
    fixture.provider.route(
        "/release?",
        200,
        search_json("mb-interactive-1", "rg-interactive-1"),
    );
    fixture.provider.route(
        "/release/mb-interactive-1?",
        200,
        release_json("mb-interactive-1", "rg-interactive-1", &[1, 1]),
    );
    fixture.scan(1).await;

    // Exactly what `selectCandidate` does.
    let key = dir.to_string_lossy().into_owned();
    let mut events = fixture.import.subscribe_events();
    fixture.select(&dir);

    let found = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match events.recv().await.expect("bus stays open") {
                ImportEvent::IdentifyStateChanged {
                    candidate_key,
                    state,
                    ..
                } if candidate_key == key && state.is_terminal() => return state,
                _ => continue,
            }
        }
    })
    .await
    .expect("the interactive run settles");

    assert!(
        matches!(found, IdentifyState::Found { .. }),
        "the interactive path reaches the same single match, got {found:?}"
    );
    assert_eq!(
        fixture.count_release_lookups("mb-interactive-1"),
        0,
        "nothing on an opened candidate's path buys the source's tracklist: {:?}",
        fixture.provider.requests()
    );
    // The verdict itself does persist — opening a candidate answers it for
    // good, not only for this session — with the tracklist still unbought.
    let row = tokio::time::timeout(Duration::from_secs(5), fixture.await_row(&dir))
        .await
        .expect("the selection recorder stores the verdict");
    let verdict: TerminalVerdict = serde_json::from_str(&row.verdict).unwrap();
    let TerminalVerdict::Found { matches, .. } = &verdict else {
        panic!("expected a single-match Found, got {verdict:?}");
    };
    assert_eq!(matches[0].source_tracks, None, "nobody paid for it here");
    assert_eq!(
        fixture.classification_for(&dir).await,
        QueueClassification::NeedsYou(NeedsYou::SourceLengthsUnknown),
        "so it is not admitted to Ready on evidence nobody checked"
    );
}

/// The row a person's own run wrote still owes the paid lookup, and the sweep
/// pays it without re-identifying anything: one release lookup, no disc-ID or
/// search request, and the candidate is promoted to Ready.
///
/// Without the top-up this row would be "answered" forever and stay at
/// `SourceLengthsUnknown` — skipped by every later pass, never reaching the one
/// lookup that would settle it.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn the_sweep_tops_up_a_verdict_an_interactive_run_left_unverified() {
    let fixture = Fixture::new("top-up").await;
    fixture
        .extraction
        .register_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .cover_art
        .seed_lookup(Some("mb-topup-1"), None, None);
    fixture
        .provider
        .route("/release?", 200, search_json("mb-topup-1", "rg-topup-1"));
    fixture.provider.route(
        "/release/mb-topup-1?",
        200,
        release_json("mb-topup-1", "rg-topup-1", &[probed, 0]),
    );
    fixture.scan(1).await;

    fixture.select(&dir);
    tokio::time::timeout(Duration::from_secs(20), fixture.await_row(&dir))
        .await
        .expect("the interactive run stores an unverified verdict");
    assert_eq!(
        fixture.count_release_lookups("mb-topup-1"),
        0,
        "the person's own path bought nothing"
    );
    let after_selection = fixture.provider.requests().len();

    fixture.sweep_once().await;

    assert_eq!(
        fixture.count_release_lookups("mb-topup-1"),
        1,
        "the sweep pays for the tracklist exactly once"
    );
    let bought: Vec<String> = fixture.provider.requests().split_off(after_selection);
    assert!(
        bought
            .iter()
            .all(|target| target.starts_with("/release/") || target.starts_with("/release-group/")),
        "the sweep identifies nothing again — no disc-ID lookup, no search: {bought:?}"
    );
    assert_eq!(
        bought.len(),
        2,
        "one `lookup_release_by_id`, which is the release plus the release-group \
         hop it makes when the release carries no Discogs url-rel: {bought:?}"
    );
    assert_eq!(
        fixture.classification_for(&dir).await,
        QueueClassification::Ready,
        "the top-up is what promotes it"
    );

    fixture.sweep_once().await;
    assert_eq!(
        fixture.count_release_lookups("mb-topup-1"),
        1,
        "and a finished row is never topped up again"
    );
}

/// Skipped is a decision the user already made, so the sweep spends no rate
/// limit re-asking about it. Whether it should eventually be swept is an open
/// question in the roadmap; this pins the conservative half until it is
/// answered.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_skipped_candidate_is_not_swept() {
    let fixture = Fixture::new("skipped").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .cover_art
        .seed_lookup(Some("mb-skipped-1"), None, None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-skipped-1", "rg-skipped-1", &[probed, 0]),
    );
    fixture.scan(1).await;
    fixture
        .import
        .set_candidate_skipped(dir.to_string_lossy().into_owned(), true)
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
    fixture.cover_art.seed_lookup(Some("mb-prog-1"), None, None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-prog-1", "rg-prog-1", &[probed, 0]),
    );
    fixture.scan(2).await;

    let mut events = fixture.import.subscribe_events();
    fixture.sweep_once().await;
    let mut progress = Vec::new();
    while let Ok(event) = events.try_recv() {
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

/// The sweep drives the whole queue through `IdentifyStateChanged` and
/// `SignalsUpdated`, and every one of those would otherwise invalidate a
/// candidate in both UIs. A run a person started still does.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn only_a_watched_run_invalidates_the_ui() {
    let fixture = Fixture::new("ui-invalidation").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.cover_art.seed_lookup(Some("mb-ui-1"), None, None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-ui-1", "rg-ui-1", &[probed, 0]),
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
    while let Ok(event) = events.try_recv() {
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
    let pass = tokio::spawn(async move { run_pass(&context, &token).await });
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
    fixture.cover_art.seed_lookup(Some("mb-drv-1"), None, None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-drv-1", "rg-drv-1", &[probed, 0]),
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
/// write, so a cancellation landing during the paid lookup that precedes it
/// cannot leave a row behind.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_cancelled_candidate_writes_no_row() {
    let fixture = Fixture::new("cancelled-writes-nothing").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .cover_art
        .seed_lookup(Some("mb-cancel-1"), None, None);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-cancel-1", "rg-cancel-1", &[probed, 0]),
    );
    fixture.scan(1).await;
    // Hold the disc-ID response, so the cancel lands while the candidate is
    // genuinely mid-identification rather than racing a pass that already
    // finished.
    fixture.provider.set_delay(Duration::from_millis(2_000));

    let context = fixture.context();
    let token = CancellationToken::new();
    let pass_token = token.clone();
    let pass = tokio::spawn(async move { run_pass(&context, &pass_token).await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        1,
        "the lookup is in flight when the cancel lands"
    );
    token.cancel();
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
        !save(&fixture.context(), &cancelled, "hash-x", "/x", &verdict, 0).await,
        "the write is gated on the token, not only the lookup before it"
    );
    assert!(fixture.stored().await.is_empty());
}

/// A row this build can no longer decode is absent, not broken: the candidate is
/// identified again and the row overwritten. Greenfield — there is no fallback
/// decoder to reach for, and treating it as an error would strand the candidate
/// with no way back.
#[test]
fn a_verdict_that_no_longer_decodes_reads_as_absent() {
    let good = synthetic_candidate("/a", 111);
    let stale = synthetic_candidate("/b", 222);
    let mut stored = HashMap::new();
    stored.insert(
        good.files.content_hash(),
        row_with_verdict(
            &good,
            serde_json::to_string(&TerminalVerdict::NotFoundAnywhere).unwrap(),
        ),
    );
    stored.insert(
        stale.files.content_hash(),
        row_with_verdict(
            &stale,
            r#"{"ShapeFromAnOlderBuild":{"whatever":1}}"#.to_string(),
        ),
    );

    let planned = plan(vec![good, stale], &stored, 2);
    assert_eq!(planned.identified, 1, "only the decodable row counts");
    let re_identified: Vec<String> = planned
        .identify
        .iter()
        .map(|c| c.path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        re_identified,
        vec!["/b".to_string()],
        "an undecodable row is absent, so its candidate is identified again"
    );
    assert!(planned.top_up.is_empty());
}

/// The top-up predicate, at the level where it is decided. "Nobody asked"
/// (`None`) owes a lookup; "asked, and the source lists nothing" does not — that
/// distinction is what stops the sweep re-buying the same empty answer on every
/// launch.
#[test]
fn only_an_unasked_single_match_owes_the_paid_lookup() {
    let asked = SourceTracks::Listed {
        count: 2,
        total_duration_ms: Some(10_000),
    };
    assert!(owes_source_tracks(&found_verdict(2, None)));
    assert!(!owes_source_tracks(&found_verdict(2, Some(asked))));
    assert!(
        !owes_source_tracks(&found_verdict(2, Some(SourceTracks::Nothing))),
        "the source answered; there is nothing left to ask"
    );
    assert!(
        !owes_source_tracks(&TerminalVerdict::NotFoundAnywhere),
        "only a single match is worth paying for"
    );
}

// ── Synthetic candidates, for the pure planning tests ───────────────────────

fn synthetic_candidate(path: &str, size: u64) -> FolderCandidate {
    use crate::import::folder_scanner::{AudioContent, CategorizedFiles, ScannedFile};
    FolderCandidate {
        path: PathBuf::from(path),
        name: path.trim_start_matches('/').to_string(),
        files: CategorizedFiles {
            audio: AudioContent::TrackFiles {
                tracks: vec![ScannedFile::new(
                    PathBuf::from(format!("{path}/01.flac")),
                    "01.flac".to_string(),
                    size,
                )],
                format_label: "FLAC".to_string(),
            },
            artwork: Vec::new(),
            documents: Vec::new(),
            unpaired_cue_sheets: Vec::new(),
        },
        watched_folder_path: "/".to_string(),
        skipped: false,
        is_added: false,
    }
}

fn row_with_verdict(candidate: &FolderCandidate, verdict: String) -> DbImportCandidateState {
    DbImportCandidateState {
        content_hash: candidate.files.content_hash(),
        folder_path: candidate.path.to_string_lossy().into_owned(),
        verdict,
        probed_total_duration_ms: 0,
        identified_at: fixed_now(),
    }
}

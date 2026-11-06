//! The identification queue's shared fixture: an import service whose
//! providers alone are fake (a local server per fixture answers their URLs and
//! records each request), the Disc ID answers its rip draws, and the reads of
//! the list and the event bus its tests share. Also one run end to end: a
//! candidate nobody selected acquires a verdict.

use super::*;
use crate::extraction::{ArtworkAnalysis, ArtworkAnalyzer};
use crate::tables::{DbCandidateIdentifyResult, DbImportCandidateState};
use bae_catalogs::search::MetadataResult;
use bae_catalogs::test_releases::mb_release_json;
use bae_identify::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};
use bae_identify::{FolderCheck, VerdictSummary};
use bae_store::Store;
use bae_util::test_channels::HANG_GUARD;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tempfile::TempDir;

/// The disc ID `store_settled_verdict` seeds, and the file it names.
pub(crate) const SEEDED_DISC_ID: &str = "XwqRcz4RhAqRTfhE5nRxRKF4iFY-";

pub(crate) const SEEDED_DISC_ID_FILE: &str = "Album.log";

pub(crate) const FIXTURE_DISC_ID: &str = "ayQ_jFizitCdB_btUSn6qV6ENaI-";

/// The modification time every copied fixture file carries, since the content
/// hash covers it and `std::fs::copy` keeps it on macOS but not on Linux.
pub(crate) fn fixture_modified_at() -> std::time::SystemTime {
    // 2020-01-01T00:00:00Z.
    std::time::UNIX_EPOCH + Duration::from_secs(1_577_836_800)
}

/// Copy a fixture file into a candidate folder, stamped with
/// [`fixture_modified_at`].
pub(crate) fn copy_fixture(source: &Path, target: &Path) {
    std::fs::copy(source, target).unwrap();
    std::fs::File::options()
        .write(true)
        .open(target)
        .unwrap()
        .set_modified(fixture_modified_at())
        .unwrap();
}

/// Settled signals that found nothing.
pub(crate) fn settled_signals() -> Signals {
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

/// The candidate audio: two real FLACs, so the fast pass has durations to
/// measure.
pub(crate) const FLAC_FIXTURES: [&str; 2] = ["01 Test Track 1.flac", "02 Test Track 2.flac"];

/// Reads one barcode off every image, for a folder with no LOG or CUE.
pub(crate) struct BarcodeAnalyzer {
    pub(crate) barcode: String,
}

impl ArtworkAnalyzer for BarcodeAnalyzer {
    fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
        ArtworkAnalysis { barcodes: vec![self.barcode.clone()], text_lines: Vec::new() }
    }
}

/// A different barcode per numbered candidate folder, so each asks its own
/// lookup rather than hitting the response cache.
pub(crate) struct PerFolderBarcodeAnalyzer;

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

pub(crate) struct Fixture {
    /// What the fixture's import service was started from, for a test that
    /// starts another over the same store or writes a preference.
    pub(crate) parts: crate::tests::import::TestImport,
    /// The store under `import`, for a test that breaks a row directly.
    pub(crate) db: Store,
    pub(crate) import: ImportServiceHandle,
    pub(crate) provider: bae_util::test_host::FakeHost,
    /// The MusicBrainz client those sessions ask, which a test seeds answers
    /// into and reads the waiting requests of.
    pub(crate) musicbrainz: Arc<bae_catalogs::musicbrainz::MusicBrainz>,
    /// The clock the providers' waits between requests pass on: each passes
    /// as it is armed until a test holds them.
    request_waits: Arc<coven::PollingClock>,
    /// The time `request_waits` moves, which a test moves past a held wait.
    request_time: Arc<coven::FixedClock>,
    /// The services the queue runs on, for the tests that drive one step of it
    /// directly.
    pub(super) context: Context,
    /// The queue, started the first time a test reaches for it, so a test can
    /// set a candidate up before a found release's run reads it.
    pub(crate) identification: OnceLock<IdentificationHandle>,
    /// Every bug the import service reported.
    pub(crate) bugs: Arc<bae_diagnostics::RecordingBugReporter>,
    pub(crate) root: PathBuf,
    pub(crate) _temp: TempDir,
}

/// The fixed instant every stored row is stamped with.
pub(crate) fn fixed_now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2024-03-01T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc)
}

/// Pull `query` until a value satisfies `wanted` and return it. A query that
/// never reaches it is bounded by the hang guard.
pub(crate) async fn live_until<T: Clone + PartialEq + Send + 'static>(
    query: &mut coven::LiveQuery<T, bae_store::DbError>,
    wanted: impl Fn(&T) -> bool,
) -> T {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let value = query.next().await.expect("the query stays open").expect("the query reads");
            if wanted(&value) {
                return value;
            }
        }
    })
    .await
    .expect("the query reaches the value")
}

impl Fixture {
    pub(crate) async fn new(name: &str) -> Self {
        Self::with_ids(name, Arc::new(coven::SequentialIds::new())).await
    }

    /// A fixture that makes UUIDs, which an import's release rows need.
    pub(crate) async fn importing(name: &str) -> Self {
        Self::with_ids(name, Arc::new(coven::UuidIds)).await
    }

    pub(crate) async fn with_ids(_name: &str, ids: coven::IdSourceRef) -> Self {
        bae_util::test_logs::install();
        let temp = TempDir::new().unwrap();
        let clock: coven::ClockRef = Arc::new(coven::FixedClock::new((fixed_now()).into()));
        let db = Store::new_test(
            crate::TEST_SCHEMA,
            temp.path().join("test.db").to_str().unwrap(),
            clock.clone(),
        )
        .await
        .unwrap();

        let provider = bae_util::test_host::FakeHost::start().await;
        let request_time = Arc::new(coven::FixedClock::new(std::time::UNIX_EPOCH));
        let request_waits = Arc::new(coven::PollingClock::new(request_time.clone()));
        // Discogs goes to the fake too, so no test reaches the real API.
        let http = bae_util::http::Http::for_test()
            .serve("musicbrainz.org", provider.base_url())
            .serve("coverartarchive.org", provider.base_url())
            .serve("api.discogs.com", provider.base_url())
            .on_clock(request_waits.clone());
        // Production request spacing, on a clock that passes each wait as it
        // is armed until a test holds the waits: admission order is part of
        // what these tests measure.
        let musicbrainz = Arc::new(bae_catalogs::musicbrainz::MusicBrainz::new(http.clone()));
        let providers = bae_catalogs::providers::Providers::for_test(
            http.clone(),
            musicbrainz.clone(),
            Arc::new(bae_catalogs::discogs::client::Discogs::new(http.clone())),
            Arc::new(bae_catalogs::wikidata::Wikidata::new(http.clone())),
        );
        // No Discogs key, so lookups ask MusicBrainz alone unless a test calls
        // `use_discogs`.
        let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
        let parts = crate::tests::import::TestImport::new(
            db.clone(),
            providers,
            http,
            clock,
            ids,
            bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone()),
        );
        let import = parts.start().await.unwrap();

        let root = temp.path().join("watched");
        std::fs::create_dir_all(&root).unwrap();

        let context = Context {
            import: import.clone(),
            store: db.clone(),
            tables: crate::tables::ImportTables::new(db.clone()),
            diagnostics: bae_diagnostics::Diagnostics::noop(),
            catalogs: parts.catalogs().clone(),
        };
        Fixture {
            parts,
            db,
            import,
            provider,
            musicbrainz,
            request_waits,
            request_time,
            context,
            identification: OnceLock::new(),
            bugs,
            root,
            _temp: temp,
        }
    }

    /// Hold every wait between provider requests armed from now on until the
    /// test passes it, so calls queue behind the one whose turn is next.
    pub(crate) fn hold_request_waits(&self) {
        self.request_waits.hold_waits();
    }

    /// Move the requests' clock to the end of the latest wait armed on it.
    pub(crate) async fn pass_armed_request_wait(&self) {
        let (_, end) = tokio::time::timeout(HANG_GUARD, self.request_time.sleep_begun_after(0))
            .await
            .expect("a wait is armed on the requests' clock");
        self.request_time.set(std::time::UNIX_EPOCH + end);
    }

    /// Pass every wait already armed, and each later one as it is armed.
    pub(crate) fn release_request_waits(&self) {
        self.request_waits.release_waits();
        // An hour on is past any wait a lookup arms.
        let now = coven::Clock::now(self.request_time.as_ref());
        self.request_time.set(now + Duration::from_secs(3600));
    }

    /// Whether releases found from now on are identified on their own.
    pub(crate) async fn set_identify_automatically(&self, enabled: bool) {
        self.parts.edit_preferences(move |prefs| prefs.identification.automatic = enabled).await;
    }

    /// Whether what a run settles on as needing nothing is imported at once.
    pub(crate) async fn set_import_when_identified(&self, enabled: bool) {
        self.parts
            .edit_preferences(move |prefs| {
                prefs.identification.import_when_identified = enabled;
            })
            .await;
    }

    /// The running queue, started on first use.
    pub(crate) fn identification(&self) -> &IdentificationHandle {
        self.identification.get_or_init(|| self.import.start_identification_queue())
    }

    /// A candidate folder with two FLACs and a rip log, so the disc ID computes.
    pub(crate) fn disc_id_candidate(&self, folder: &str) -> PathBuf {
        let dir = self.candidate_dir(folder);
        copy_fixture(Path::new("tests/fixtures/logs/test_album.log"), &dir.join("test_album.log"));
        dir
    }

    /// A candidate folder with two FLACs and one image and no LOG or CUE, so
    /// identification runs through the artwork barcode.
    pub(crate) fn barcode_candidate(&self, folder: &str) -> PathBuf {
        let dir = self.candidate_dir(folder);
        std::fs::write(dir.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0, 0x00]).unwrap();
        dir
    }

    pub(crate) fn candidate_dir(&self, folder: &str) -> PathBuf {
        // Part by part, so a nested folder is spelled as the scan spells its
        // key on every host.
        let dir = folder.split('/').fold(self.root.clone(), |dir, part| dir.join(part));
        std::fs::create_dir_all(&dir).unwrap();
        for name in FLAC_FIXTURES {
            copy_fixture(&Path::new("tests/fixtures/flac").join(name), &dir.join(name));
        }
        dir
    }

    pub(crate) fn probed_total_ms(&self, dir: &Path) -> u64 {
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

    /// Watch the root and read it, returning once the read is over — by which
    /// point every release it found has been handed to automatic
    /// identification — and the list holds `expected` candidates.
    pub(crate) async fn scan(&self, expected: usize) {
        let root = self.root.to_path_buf();
        self.import.add_watched_folder(root).await.unwrap();
        self.rescan(&self.import, expected).await;
    }

    /// Read the watched root again through `import`, as a launch does, and
    /// wait as [`Self::scan`] does. A refresh answers once its read is over,
    /// which is what makes the wait exact rather than a matter of time.
    pub(crate) async fn rescan(&self, import: &ImportServiceHandle, expected: usize) {
        self.read_root(import, &self.root).await;
        self.await_listed(import, expected).await;
    }

    /// Read the watched folder `root` through `import`, returning once the
    /// read is over.
    pub(crate) async fn read_root(&self, import: &ImportServiceHandle, root: &Path) {
        import.refresh_watched_folder(root.to_path_buf()).await.unwrap();
    }

    /// The list holds `expected` candidates, as the reads already over left it.
    pub(crate) async fn await_listed(&self, import: &ImportServiceHandle, expected: usize) {
        tokio::time::timeout(
            HANG_GUARD,
            import.wait_for_list(crate::ImportListView::default(), |projection| {
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
    pub(crate) fn start_explicit_lookup(&self, dir: &Path) {
        self.identification().rerun_identify(crate::CandidateKey::folder(dir));
    }

    /// Ask to identify `dir` and wait until the queue has taken the request
    /// and started its run.
    pub(crate) async fn start_explicit_lookup_and_await_run(&self, dir: &Path) {
        let key = crate::CandidateKey::folder(dir);
        self.start_explicit_lookup(dir);
        tokio::time::timeout(HANG_GUARD, self.identification().commands_taken_for_test())
            .await
            .expect("the queue takes the request");
        assert!(self.import.is_identifying(&key), "identify registers the driver for {key}");
    }

    /// The candidate's row once a run has stored its verdict, followed
    /// through the state table's live read.
    pub(crate) async fn await_identified_row(&self, dir: &Path) -> DbImportCandidateState {
        self.await_state(dir, |row| row.identify.is_some()).await
    }

    /// The candidate's row once `wanted` holds of it, followed through the
    /// state table's live read.
    pub(crate) async fn await_state(
        &self,
        dir: &Path,
        wanted: impl Fn(&DbImportCandidateState) -> bool,
    ) -> DbImportCandidateState {
        let hash = self.content_hash(dir);
        let mut states = self.context.tables.subscribe_import_candidate_states();
        let mut stored =
            live_until(&mut states, |states| states.get(&hash).is_some_and(&wanted)).await;
        stored.remove(&hash).expect("the wanted row is in the state it satisfied")
    }

    /// Wait until `key`'s runtime, as its live read has it, satisfies
    /// `wanted`.
    pub(crate) async fn await_runtime(
        &self,
        key: &crate::CandidateKey,
        wanted: impl Fn(&crate::CandidateRuntimeSnapshot) -> bool,
    ) {
        let mut runtime = self.context.tables.subscribe_candidate_runtime(key);
        live_until(&mut runtime, |runtime| runtime.as_ref().is_some_and(&wanted)).await;
    }

    /// Wait until nothing is recorded as running for `key`.
    pub(crate) async fn await_no_runtime(&self, key: &crate::CandidateKey) {
        let mut runtime = self.context.tables.subscribe_candidate_runtime(key);
        live_until(&mut runtime, Option::is_none).await;
    }

    /// Wait until no candidate of `keys` stands identifying.
    pub(crate) async fn await_not_standing(&self, keys: &[crate::CandidateKey]) {
        self.await_facts(|facts| {
            keys.iter().all(|key| facts.get(key).is_none_or(|facts| facts.identification.is_none()))
        })
        .await;
    }

    pub(crate) fn count_release_lookups(&self, release_id: &str) -> usize {
        self.provider.count_containing(&format!("/release/{release_id}?"))
    }

    pub(super) fn context(&self) -> Context {
        self.context.clone()
    }

    /// Import's tables over the fixture's store.
    pub(crate) fn tables(&self) -> crate::tables::ImportTables {
        self.context.tables.clone()
    }

    /// Start the queue if needed, and wait until every release found so far is
    /// admitted and every automatic job has ended.
    pub(crate) async fn drain_automatic(&self) {
        tokio::time::timeout(HANG_GUARD, self.identification().automatic_drained_for_test())
            .await
            .expect("the automatic admission's queue drains");
    }

    /// The same, as a task a test can watch while it acts on the queue.
    pub(crate) fn drain_automatic_task(&self) -> tokio::task::JoinHandle<()> {
        let identification = self.identification().clone();
        tokio::spawn(async move { identification.automatic_drained_for_test().await })
    }

    /// Where the queue has `key`, as the runtime publishes it.
    pub(crate) async fn identification_status(
        &self,
        key: &crate::CandidateKey,
    ) -> Option<crate::IdentificationStatus> {
        self.import.standing_facts_for_test(key).await.identification
    }

    /// The first of `dirs` the queue holds waiting.
    pub(crate) async fn first_queued(&self, dirs: &[PathBuf]) -> Option<PathBuf> {
        for dir in dirs {
            if self.identification_status(&crate::CandidateKey::folder(dir)).await
                == Some(crate::IdentificationStatus::Queued)
            {
                return Some(dir.clone());
            }
        }
        None
    }

    pub(crate) async fn stored(&self) -> BTreeMap<String, DbImportCandidateState> {
        self.context.tables.load_import_candidate_states().await.unwrap().into_iter().collect()
    }

    /// A candidate folder's content hash, read off disk.
    pub(crate) fn content_hash(&self, dir: &Path) -> String {
        crate::folder_scanner::collect_release_candidate_files_with_scope(
            dir,
            crate::ReleaseFileScope::Recursive,
            &crate::folder_scanner::StoredCandidateEdits::none(),
        )
        .expect("the candidate folder is readable")
        .content_hash()
    }

    pub(crate) async fn stored_for(&self, dir: &Path) -> Option<DbImportCandidateState> {
        let hash = self.content_hash(dir);
        self.stored().await.remove(&hash)
    }

    pub(crate) async fn identified_for(&self, dir: &Path) -> Option<DbCandidateIdentifyResult> {
        self.stored_for(dir).await.and_then(|row| row.identify)
    }

    /// One candidate's pane as it reads back from the tables.
    pub(crate) async fn pane(&self, dir: &Path) -> Option<crate::ImportCandidateDetail> {
        self.import.load_candidate(&crate::CandidateKey::folder(dir)).await.unwrap()
    }

    /// The stored MusicBrainz release, if a fetch stored one.
    pub(crate) async fn stored_release(
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

    /// Store a fake Discogs key, so lookups ask both providers.
    pub(crate) async fn use_discogs(&self) {
        self.parts
            .catalogs()
            .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
            .await
            .expect("the fake Discogs key is stored");
    }

    /// Store a release directly, as a settle step would have.
    pub(crate) async fn archive(&self, release_id: &str, group_id: &str, track_lengths: &[u64]) {
        self.context
            .tables
            .save_source_release(&stored_pressing(release_id, group_id, track_lengths))
            .await
            .unwrap();
    }

    /// Whether a stored row's verdict is auto-importable, and the check
    /// against the folder it failed.
    pub(crate) async fn judgement_for(&self, dir: &Path) -> (bool, Option<FolderCheck>) {
        let row = self.stored_for(dir).await.expect("a row was stored");
        VerdictSummary::of(&identify_result(&row).verdict, false).judgement()
    }

    /// Route a disc-ID match for `dir` to one release listing `tracks` tracks
    /// (the fixture folder holds two).
    pub(crate) fn route_disc_id_match(
        &self,
        dir: &Path,
        release_id: &str,
        group_id: &str,
        tracks: usize,
    ) {
        let probed = self.probed_total_ms(dir);
        let mut lengths = vec![0; tracks];
        lengths[0] = probed;
        self.provider.route("/discid/", 200, discid_json(release_id, group_id, &lengths));
        self.provider.route(
            &format!("/release/{release_id}?"),
            200,
            mb_release_json(release_id, group_id, &lengths),
        );
    }

    /// An automatic run's auto-importable verdict for `dir`, stored while
    /// importing when identified is off.
    pub(crate) async fn identify_ready(&self, dir: &Path, release_id: &str, group_id: &str) {
        self.route_disc_id_match(dir, release_id, group_id, 2);
        self.scan(1).await;
        self.drain_automatic().await;
        assert_eq!(self.judgement_for(dir).await, (true, None));
    }

    /// Wait until the standings, as their live read has them, satisfy `wanted`.
    pub(crate) async fn await_facts(
        &self,
        wanted: impl Fn(
            &std::collections::HashMap<crate::CandidateKey, crate::TriageRuntimeFacts>,
        ) -> bool,
    ) {
        let mut facts = self.context.tables.subscribe_candidate_facts();
        live_until(&mut facts, wanted).await;
    }

    /// Wait until the runtime says `status` of `key`, followed through its
    /// standing's live read.
    pub(crate) async fn await_identification_status(
        &self,
        key: &crate::CandidateKey,
        status: crate::IdentificationStatus,
    ) {
        self.await_facts(|facts| {
            facts.get(key).is_some_and(|facts| facts.identification.as_ref() == Some(&status))
        })
        .await;
    }

    /// Let the held read answer and wait for the queue to be done.
    pub(crate) async fn finish_reading(&self) {
        self.provider.release();
        tokio::time::timeout(HANG_GUARD, self.identification().drained_for_test())
            .await
            .expect("the settle ends");
    }

    /// The stored Discogs release, if a fetch stored one.
    pub(crate) async fn stored_discogs_release(
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
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(identification) = self.identification.get() {
            identification.stop();
        }
        self.import.stop_and_join();
    }
}

/// The release a lone MusicBrainz release document extracts to.
pub(crate) fn stored_pressing(
    release_id: &str,
    group_id: &str,
    track_lengths: &[u64],
) -> bae_catalogs::source_release::SourceRelease {
    bae_catalogs::payloads::ReleasePayloads::for_test(
        bae_catalogs::records::LookupRef::new(
            bae_catalogs::catalog::LookupCatalog::MusicBrainz,
            release_id,
        ),
        mb_release_json(release_id, group_id, track_lengths),
        Vec::new(),
    )
    .extract()
    .unwrap()
}

/// The identify half of a stored row, which every row identification wrote has.
pub(crate) fn identify_result(
    row: &DbImportCandidateState,
) -> &crate::tables::DbCandidateIdentifyResult {
    row.identify.as_ref().expect("a row identification wrote carries its identify result")
}

pub(crate) fn discid_json(release_id: &str, group_id: &str, track_lengths: &[u64]) -> String {
    let mut release: serde_json::Value =
        serde_json::from_str(&mb_release_json(release_id, group_id, track_lengths))
            .expect("release fixture parses");
    release["media"][0]["discs"] = serde_json::json!([{ "id": FIXTURE_DISC_ID }]);
    serde_json::json!({ "releases": [release] }).to_string()
}

/// The same disc-ID answer for a release that prints a barcode. MusicBrainz
/// states a release's barcode on every answer that names it, so a disc-ID
/// result carries the code that pairs it with the Discogs record of the same
/// pressing.
pub(crate) fn discid_json_stating_barcode(
    release_id: &str,
    group_id: &str,
    track_lengths: &[u64],
    barcode: &str,
) -> String {
    let mut answer: serde_json::Value =
        serde_json::from_str(&discid_json(release_id, group_id, track_lengths))
            .expect("the disc ID fixture parses");
    answer["releases"][0]["barcode"] = serde_json::json!(barcode);
    answer.to_string()
}

/// `discid_json` for a release that carries `catalog_number`.
pub(crate) fn discid_json_with_catalog(
    release_id: &str,
    group_id: &str,
    track_lengths: &[u64],
    catalog_number: &str,
) -> String {
    let mut answer: serde_json::Value =
        serde_json::from_str(&discid_json(release_id, group_id, track_lengths))
            .expect("the disc ID fixture parses");
    answer["releases"][0]["label-info"] = serde_json::json!([{ "catalog-number": catalog_number }]);
    answer.to_string()
}

/// A small PNG, which is what an image host serves.
pub(crate) fn cover_png() -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(16, 16)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("the test cover encodes");
    bytes.into_inner()
}

/// A Disc ID answered by two pressings of the album the folder holds, so the
/// run offers both and picks neither. The first states a disambiguation, and
/// its own page adds an annotation.
pub(crate) async fn two_pressings_fixture(name: &str) -> (Fixture, PathBuf) {
    let fixture = Fixture::new(name).await;
    let dir = fixture.disc_id_candidate("Album");
    route_two_pressings(&fixture, fixture.probed_total_ms(&dir));
    (fixture, dir)
}

/// The routes of [`two_pressings_fixture`], for any fixture.
pub(crate) fn route_two_pressings(fixture: &Fixture, probed: u64) {
    let pressing = |release_id: &str, country: &str| {
        let mut release: serde_json::Value =
            serde_json::from_str(&mb_release_json(release_id, "rg-disc", &[probed, 0])).unwrap();
        release["media"][0]["discs"] = serde_json::json!([{ "id": FIXTURE_DISC_ID }]);
        release["country"] = serde_json::json!(country);
        release
    };
    let mut first = pressing("mb-disc-de", "DE");
    first["disambiguation"] = serde_json::json!("club edition");
    let second = pressing("mb-disc-gb", "GB");
    let mut document = first.clone();
    document["annotation"] = serde_json::json!("Matrix: A1");
    fixture.provider.route(
        "/discid/",
        200,
        serde_json::json!({ "releases": [first, second.clone()] }).to_string(),
    );
    fixture.provider.route("/release/mb-disc-de?", 200, document.to_string());
    fixture.provider.route("/release/mb-disc-gb?", 200, second.to_string());
    fixture.provider.route(
        "/release-group/rg-disc?",
        200,
        serde_json::json!({
            "id": "rg-disc", "title": "Album", "first-release-date": "1981", "relations": []
        })
        .to_string(),
    );
}

/// The id of the next run of `key` whose reported state `accept` answers.
pub(crate) async fn await_run_state(
    events: &mut crate::ImportEvents,
    key: &crate::CandidateKey,
    accept: impl Fn(IdentifyRunId, &IdentifyState) -> bool,
) -> IdentifyRunId {
    tokio::time::timeout(HANG_GUARD, async {
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
pub(crate) fn drain_events(events: &mut crate::ImportEvents) -> Vec<ImportEvent> {
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

/// Nothing imported `key`, asked once the queue is done with it: the worker
/// runs every import it was handed before it stops, so `events` then holds
/// every report there will be.
pub(crate) async fn assert_no_import(
    fixture: &Fixture,
    events: &mut crate::ImportEvents,
    key: &crate::CandidateKey,
    after: &str,
) {
    let import = fixture.import.clone();
    tokio::task::spawn_blocking(move || import.stop_and_join())
        .await
        .expect("the import worker stops");
    for event in drain_events(events) {
        if let ImportEvent::ImportProgress { candidate_key, progress } = event {
            assert_ne!(
                &candidate_key, key,
                "nothing imports {key} after {after}, got {progress:?}"
            );
        }
    }
}

/// Stand in for starting an import: the claim, then the worker's first
/// `ImportProgress`, in that order.
pub(crate) async fn start_import_for(fixture: &Fixture, candidate: &Path) {
    let candidate_key = crate::CandidateKey::folder(candidate);
    fixture.import.claim_candidate_for_import(&candidate_key, "import-running").await;
    fixture.import.emit_event_for_test(ImportEvent::ImportProgress {
        candidate_key,
        progress: crate::ImportProgress::Preparing {
            import_id: "import-running".to_string(),
            step: crate::PrepareStep::ValidatingSourceFiles,
        },
    });
}

/// A list subscription over the Pending tab, asking for `windows`.
pub(crate) async fn list_subscription(
    fixture: &Fixture,
    windows: bae_store::LibraryPageWindows,
) -> crate::LiveImportList {
    fixture
        .import
        .open_list(crate::ImportListView::default(), windows)
        .await
        .expect("the list opens")
}

pub(crate) fn page_window(offset: u64, limit: u64) -> bae_store::LibraryPageWindows {
    std::iter::once(bae_store::LibraryPageWindow { offset, limit }).collect()
}

/// The list's next snapshot.
pub(crate) async fn next_page(list: &crate::LiveImportList) -> crate::ImportListSnapshot {
    tokio::time::timeout(HANG_GUARD, list.next())
        .await
        .expect("the list delivers")
        .expect("the list answers")
}

/// How many of Found's rows a snapshot counts under `filter`.
pub(crate) fn counted(snapshot: &crate::ImportListSnapshot, filter: crate::PendingFilter) -> u32 {
    snapshot
        .pending_filter_entries
        .iter()
        .find(|entry| entry.filter == filter)
        .expect("every entry is counted")
        .count
}

/// What is running for each candidate row a snapshot's page holds, by key.
pub(crate) fn page_live(
    snapshot: &crate::ImportListSnapshot,
) -> BTreeMap<crate::CandidateKey, crate::CandidateLiveState> {
    snapshot
        .windows
        .iter()
        .flat_map(|window| &window.rows)
        .filter_map(|item| match item {
            crate::ImportListItem::Candidate { row, live, .. } => {
                Some((row.candidate_key.clone(), live.clone()))
            }
            _ => None,
        })
        .collect()
}

pub(crate) fn running(phase: crate::ImportPhase, percent: Option<u8>) -> crate::ImportProgress {
    crate::ImportProgress::Progress {
        id: "release-1".to_string(),
        percent,
        phase,
        import_id: "import-1".to_string(),
    }
}

pub(crate) fn reported(key: &crate::CandidateKey, progress: crate::ImportProgress) -> ImportEvent {
    ImportEvent::ImportProgress { candidate_key: key.clone(), progress }
}

/// Each candidate row a snapshot's page holds, with its live state, by key.
pub(crate) fn page_rows(
    snapshot: &crate::ImportListSnapshot,
) -> BTreeMap<crate::CandidateKey, (crate::TriageRow, crate::CandidateLiveState)> {
    snapshot
        .windows
        .iter()
        .flat_map(|window| &window.rows)
        .filter_map(|item| match item {
            crate::ImportListItem::Candidate { row, live, .. } => {
                Some((row.candidate_key.clone(), (row.clone(), live.clone())))
            }
            _ => None,
        })
        .collect()
}

/// The barcode both sources print on the sleeve, as each of them spaces it.
/// Only the digits are comparable, which is what pairs the two records.
pub(crate) const PAIRED_BARCODE: &str = "0123456789012";

pub(crate) const PAIRED_BARCODE_AS_DISCOGS_PRINTS_IT: &str = "012 345 678901 2";

/// A several-match verdict, as identification stores one: the pressing is the
/// open question, so no match carries a settled tracklist.
pub(crate) fn multi_match_verdict(release_ids: &[&str], group_id: &str) -> TerminalVerdict {
    TerminalVerdict::Found {
        findings: bae_identify::Findings {
            matches: release_ids
                .iter()
                .map(|release_id| MetadataResult {
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
                    source_tracks: None,
                    document_failure: None,
                    album_first_year: None,
                    track_titles: Vec::new(),
                    notes: Vec::new(),
                })
                .collect(),
            // Each release is a pressing of its own: the group lists several, and
            // which one the folder is, is the open question.
            pressings: (0..release_ids.len() as u32).collect(),
            narrowed_out: bae_identify::NarrowedOut::default(),
            medium_conflict: None,
            named_notes: Vec::new(),
        },
        track_count: 2,
        ledger: None,
    }
}

/// The list's row for one candidate and its live state, as [`queue_row`]
/// finds it.
pub(crate) async fn queue_item(
    fixture: &Fixture,
    key: &crate::CandidateKey,
) -> (crate::TriageRow, crate::CandidateLiveState) {
    for tab in [crate::TriageTab::Pending, crate::TriageTab::Skipped] {
        let view = crate::ImportListView { tab, ..crate::ImportListView::default() };
        let projection = fixture.import.wait_for_list(view, |_| true).await;
        let row =
            projection.windows.iter().flat_map(|window| &window.rows).find_map(|item| match item {
                crate::ImportListItem::Candidate { row, live, .. }
                    if row.candidate_key == key.clone() =>
                {
                    Some((row.clone(), live.clone()))
                }
                _ => None,
            });
        if let Some(row) = row {
            return row;
        }
    }
    panic!("the row is in the queue");
}

/// The whole point of the task: nothing is selected, no view is open, and the
/// candidate still ends up with a stored verdict that is auto-importable.
///
/// The provider answers the disc-ID lookup with exactly one release listing as
/// many tracks as the fixture holds, so every clause of the auto-import check is
/// exercised for real: one match, counts agreeing.
#[tokio::test(flavor = "multi_thread")]
async fn a_candidate_nobody_selected_acquires_a_verdict() {
    let fixture = Fixture::new("acquires-verdict").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-auto-1", "rg-auto-1", &[probed / 2, probed - probed / 2]),
    );
    fixture.provider.route(
        "/release/mb-auto-1?",
        200,
        mb_release_json("mb-auto-1", "rg-auto-1", &[probed / 2, probed - probed / 2]),
    );
    fixture.scan(1).await;

    // Nobody asks: it is identified because it was found.
    fixture.drain_automatic().await;

    let row = fixture.stored_for(&dir).await.expect("a verdict is stored");
    assert_eq!(
        row.candidate_key,
        crate::CandidateKey::folder(&dir),
        "the row names where the candidate was last seen"
    );
    assert_eq!(fixture.judgement_for(&dir).await, (true, None), "one match, counts agreeing");
}

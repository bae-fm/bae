//! What the runtime holds apart from the table: how far each key's import
//! is, and the files each scanned key was recorded with, so a scan that
//! drops or reshapes a key ends its work. Fixtures the runtime's other
//! tests share.

use super::*;
use crate::folder_scanner::{CategorizedFiles, InvalidCandidate};
use crate::identification::tests::*;
use crate::types::{ImportPhase, ImportProgress, PrepareStep};
use crate::CandidateFiles;
use crate::IdentifyRunId;
use crate::ImportInFlight;
use crate::ImportStep;
use std::path::PathBuf;

pub(super) fn empty_categorized() -> CategorizedFiles {
    CategorizedFiles { files: Vec::new(), parts: Vec::new() }
}

pub(super) fn folder_candidate(key: &crate::CandidateKey, watched: &str) -> FolderCandidate {
    let path = key.test_folder();
    FolderCandidate {
        relative_folder: crate::RelativeFolder::of(path.strip_prefix(watched).unwrap()).unwrap(),
        name: format!("Candidate {}", path.display()),
        file_root: path.clone(),
        path,
        files: empty_categorized(),
        watched_folder_path: PathBuf::from(watched),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        grouping: None,
    }
}

pub(super) fn scanned(candidate: FolderCandidate) -> ImportEvent {
    ImportEvent::Scan(ScanEvent::FolderCandidate { candidate, found_while_automatic: false })
}

pub(super) fn progress(key: &crate::CandidateKey, percent: u8) -> ImportEvent {
    ImportEvent::ImportProgress {
        candidate_key: key.clone(),
        progress: ImportProgress::Progress {
            id: "rel1".to_string(),
            percent: Some(percent),
            phase: ImportPhase::MeasuringLoudness,
            import_id: "imp-1".to_string(),
        },
    }
}

/// Claim `key` for the import every [`progress`] reports on, and take it up
/// as the worker does: its first step written, then reported.
pub(super) async fn claim(runtime: &CandidateRuntime, key: &crate::CandidateKey) {
    runtime.claim_for_import(key, "imp-1", false).await.unwrap();
    runtime
        .move_import(key, "imp-1", Some(ImportStep::Preparing(PrepareStep::ValidatingSourceFiles)))
        .await
        .unwrap();
    runtime.record_event(&ImportEvent::ImportProgress {
        candidate_key: key.clone(),
        progress: ImportProgress::Preparing {
            import_id: "imp-1".to_string(),
            step: PrepareStep::ValidatingSourceFiles,
        },
    });
}

/// Move `key`'s import to measuring loudness, as the worker writes it before
/// it starts, and report `percent` of it, as the measuring does.
pub(super) async fn measuring(runtime: &CandidateRuntime, key: &crate::CandidateKey, percent: u8) {
    runtime
        .move_import(key, "imp-1", Some(ImportStep::Running(ImportPhase::MeasuringLoudness)))
        .await
        .unwrap();
    runtime.record_event(&progress(key, percent));
}

pub(crate) fn signals_context(track_count: u32) -> bae_identify::state::SignalsContext {
    bae_identify::state::SignalsContext {
        origin: bae_catalogs::AudioOrigin::default(),
        providers: Vec::new(),
        artwork: bae_identify::signals::ArtworkScan::Absent,
        disc: bae_identify::state::DiscIdEvidence {
            signal: bae_identify::signals::DiscIdSignal::Absent,
            ..Default::default()
        },
        barcode: Default::default(),
        catalog: Default::default(),
        search: Default::default(),
        text: Default::default(),
        text_settled: true,
        audio: bae_identify::signals::AudioFacts { track_count, ..Default::default() },
        documents: bae_identify::documents::DocumentReading::Pending,
        pressings: Vec::new(),
        isrc: Default::default(),
        track_titles: Vec::new(),
    }
}

pub(crate) fn run(number: u64) -> crate::IdentifyRunId {
    crate::IdentifyRunId::for_test(number)
}

/// A run's state partway through, which the runtime records as running.
pub(crate) fn triangulating() -> bae_identify::IdentifyState {
    bae_identify::IdentifyState::Triangulating {
        discid: bae_identify::DiscidProgress::Computing,
        barcode: bae_identify::BarcodeProgress::Scanning,
        catalog: bae_identify::CatalogProgress::Skipped,
        isrc: bae_identify::IsrcProgress::Skipped,
        search: bae_identify::SearchProgress::Pending,
        context: signals_context(9),
    }
}

pub(crate) fn manual_only() -> bae_identify::IdentifyState {
    bae_identify::IdentifyState::ManualOnly {
        ledger: None,
        track_count: 9,
        context: signals_context(9),
    }
}

/// `state` as its run's rows read it back, with no signals stored beside it.
pub(crate) fn published(state: bae_identify::IdentifyState) -> bae_identify::PublishedRun {
    bae_identify::PublishedRun {
        outcome: bae_identify::RunOutcome::of(&state).expect("a test publishes a run's state"),
        signals: None,
    }
}

/// Store `key` as a candidate the scan holds under `/watch` with `revision`
/// of its file decisions, as the scan would, and return the files it is
/// identified as.
pub(super) async fn scanned_at(
    runtime: &CandidateRuntime,
    key: &crate::CandidateKey,
    revision: u64,
) -> CandidateFiles {
    let files =
        CandidateFiles { content_hash: format!("hash:{key}"), file_edit_revision: revision };
    runtime
        .execute_local_sql_for_test(&format!(
            "INSERT OR IGNORE INTO watched_import_folders (path, position)
                 VALUES (CAST('/watch' AS BLOB), 0);
             INSERT OR IGNORE INTO folder_scan_roots (watched_folder_path, generation, status, volume)
                 VALUES (CAST('/watch' AS BLOB), 1, 'complete', 'local');
             DELETE FROM scan_candidate WHERE path = X'{key}';
             INSERT INTO scan_candidate (watched_folder_path, path, generation, kind, name,
                 relative_folder, folder, file_root, scope, content_hash, file_edit_revision)
                 VALUES (CAST('/watch' AS BLOB), X'{key}', 1, 'valid', 'Candidate', X'{relative}',
                     X'{key}', X'{key}', 'recursive', '{hash}', {revision});",
            key = hex::encode(key.as_bytes()),
            relative = hex::encode(
                crate::RelativeFolder::of(key.test_folder().strip_prefix("/watch").unwrap())
                    .unwrap()
                    .as_bytes()
            ),
            hash = files.content_hash,
        ))
        .await;
    files
}

pub(super) async fn scanned_key(
    runtime: &CandidateRuntime,
    key: &crate::CandidateKey,
) -> CandidateFiles {
    scanned_at(runtime, key, 0).await
}

/// Make every write of a candidate's standing that one of `failing` names —
/// a statement (`INSERT`, `UPDATE`, `DELETE`) and the condition on its row,
/// `NEW` or `OLD`, that it fails for — fail, as a full disk fails it.
pub(crate) async fn fail_standing_writes(directory: &std::path::Path, failing: &[(&str, &str)]) {
    let triggers: String = failing
        .iter()
        .enumerate()
        .map(|(index, (statement, when))| {
            format!(
                "CREATE TRIGGER fail_standing_write_{index} BEFORE {statement} \
                 ON import_candidate_standing WHEN {when} \
                 BEGIN SELECT RAISE(ABORT, 'the disk is full'); END;"
            )
        })
        .collect();
    add_to_library_schema(directory, "import_candidate_standing", triggers).await;
}

/// Run `statements`, a schema change, on the library under `directory`, the
/// database holding `table`, from a connection of its own: no write of the
/// library makes a schema change.
pub(crate) async fn add_to_library_schema(
    directory: &std::path::Path,
    table: &'static str,
    statements: String,
) {
    let directory = directory.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut found = false;
        let mut dirs = vec![directory];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                let is_database =
                    std::fs::read(&path).is_ok_and(|bytes| bytes.starts_with(b"SQLite format 3\0"));
                if !is_database {
                    continue;
                }
                let connection = rusqlite::Connection::open(&path).unwrap();
                let holds_table: bool = connection
                    .query_row(
                        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = ?)",
                        [table],
                        |row| row.get(0),
                    )
                    .unwrap();
                if !holds_table {
                    continue;
                }
                connection.execute_batch(&statements).unwrap();
                found = true;
            }
        }
        assert!(found, "the library under the directory holds {table}");
    })
    .await
    .unwrap();
}

/// Report `state` as `run` of `key` does: the run started with it stored,
/// then its answer written — the way the identify driver reports.
pub(super) async fn report(
    runtime: &CandidateRuntime,
    key: &crate::CandidateKey,
    files: &CandidateFiles,
    run: crate::IdentifyRunId,
    state: bae_identify::IdentifyState,
) {
    assert!(runtime.run_started(key, files, run, &state).await.unwrap());
    if state.is_terminal() {
        assert!(runtime.run_answered(key, run, state.picks_unattended()).await.unwrap());
    }
}

/// What the table says `key`'s identification is.
pub(super) async fn identification(
    runtime: &CandidateRuntime,
    key: &crate::CandidateKey,
) -> Option<crate::IdentificationStatus> {
    runtime.standing_facts(key).await.unwrap().identification
}

pub(crate) fn extracted_signals() -> bae_identify::signals::Signals {
    bae_identify::signals::Signals {
        origin: bae_catalogs::AudioOrigin::default(),
        disc_id: bae_identify::signals::DiscIdSignal::Absent,
        barcode: bae_identify::signals::BarcodeSignal::Settled { codes: Vec::new() },
        text: bae_identify::signals::TextSignal::Settled {
            catalogs: Vec::new(),
            free_text: Vec::new(),
        },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    }
}

/// An import's step is the one its claim holds, per key, and how far through
/// it the import is is held per key apart from it.
#[tokio::test]
async fn an_import_s_step_is_its_claim_s_and_its_percent_is_held_per_key() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    claim(&runtime, key).await;
    claim(&runtime, &crate::CandidateKey::test("/watch/a/rel2")).await;

    measuring(&runtime, key, 42).await;

    assert_eq!(
        runtime.snapshot(key).await.unwrap().and_then(|runtime| runtime.import),
        Some(ImportInFlight { step: ImportStep::Running(ImportPhase::MeasuringLoudness) })
    );
    assert_eq!(
        runtime
            .snapshot(&crate::CandidateKey::test("/watch/a/rel2"))
            .await
            .unwrap()
            .and_then(|runtime| runtime.import),
        Some(ImportInFlight { step: ImportStep::Preparing(PrepareStep::ValidatingSourceFiles) }),
        "another key's progress is its own"
    );
    assert_eq!(
        runtime.percents(),
        HashMap::from([(key.clone(), 42)]),
        "only the key that reported a percent has one"
    );
}

/// Every way an import ends is a row by the time the event reaches here — the
/// release the worker committed, the failure it wrote, or the claim a cancel
/// ended — so the key stops being in flight.
#[tokio::test]
async fn a_finished_import_leaves_nothing_in_flight() {
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let endings = [
        ImportProgress::Complete {
            id: "rel1".to_string(),
            import_id: "imp-1".to_string(),
            album_id: "alb".to_string(),
        },
        ImportProgress::RemoteUploadQueued {
            id: "rel1".to_string(),
            import_id: "imp-1".to_string(),
            album_id: "alb".to_string(),
        },
        ImportProgress::Failed {
            error: "no space left".to_string(),
            import_id: "imp-1".to_string(),
        },
    ];
    for ending in endings {
        let runtime = CandidateRuntime::for_test().await;
        claim(&runtime, key).await;
        measuring(&runtime, key, 42).await;

        runtime.move_import(key, "imp-1", None).await.unwrap();
        runtime.record_event(&ImportEvent::ImportProgress {
            candidate_key: key.clone(),
            progress: ending.clone(),
        });
        assert!(runtime.snapshot(key).await.unwrap().is_none(), "{ending:?} left something behind");
        assert!(runtime.percents().is_empty(), "{ending:?} left a percent");
    }
}

/// An import's claim stands whatever the scan reports about its candidate.
#[tokio::test]
async fn a_claim_survives_the_scan_reporting_the_key() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    claim(&runtime, key).await;
    runtime.record_event(&progress(key, 42));

    runtime.record_event(&scanned(folder_candidate(key, "/watch/a")));

    assert!(runtime.snapshot(key).await.unwrap().is_some_and(|runtime| runtime.import.is_some()));
}

/// A rescan reporting the same files keeps the key's work; one reporting
/// other files ends it. Its search ends in the scan's own write (see the
/// standings' ending tests).
#[tokio::test]
async fn a_rescan_reporting_new_files_ends_the_key_s_work() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    runtime.record_event(&scanned(folder_candidate(key, "/watch/a")));
    let work = runtime.start_work(CandidateWork::Identify, key.clone(), |token, _| token);

    runtime.record_event(&scanned(folder_candidate(key, "/watch/a")));
    assert!(!work.is_cancelled(), "an unchanged re-scan keeps the work");

    let mut reshaped = folder_candidate(key, "/watch/a");
    reshaped.file_edit_revision = 1;
    runtime.record_event(&ImportEvent::Scan(ScanEvent::CandidateBindingChanged {
        candidate: reshaped,
    }));
    assert!(work.is_cancelled());
}

// The scan dropping or reshaping a candidate ends the work in flight for it:
// that work read files the candidate no longer has.

/// Start extraction and an identify run on `key`, returning their tokens.
fn start_both(
    runtime: &CandidateRuntime,
    key: &crate::CandidateKey,
) -> [tokio_util::sync::CancellationToken; 2] {
    [CandidateWork::Extraction, CandidateWork::Identify]
        .map(|work| runtime.start_work(work, key.clone(), |token, _| token))
}

fn invalid(candidate: &FolderCandidate) -> ImportEvent {
    ImportEvent::Scan(ScanEvent::InvalidCandidate(InvalidCandidate {
        path: candidate.path.clone(),
        name: "rel1".to_string(),
        watched_folder_path: candidate.watched_folder_path.clone(),
        relative_folder: crate::RelativeFolder::test("rel1"),
        grouping: candidate.grouping.clone(),
        files: Vec::new(),
        tags: None,
    }))
}

#[tokio::test]
async fn a_candidate_leaving_or_reshaped_ends_its_work() {
    for grouping in [None, Some(crate::CandidateKey::grouping("group-1"))] {
        let mut candidate =
            folder_candidate(&crate::CandidateKey::test("/watch/a/rel1"), "/watch/a");
        candidate.grouping = grouping;
        let key = candidate.key();
        let mut reshaped = candidate.clone();
        reshaped.file_edit_revision = 1;
        let departures = [
            ("reshaped", scanned(reshaped.clone())),
            (
                "reshaped while not yet actionable",
                ImportEvent::Scan(ScanEvent::CandidateDiscovered { candidate: reshaped }),
            ),
            (
                "rebound",
                ImportEvent::Scan(ScanEvent::CandidateBindingChanged {
                    candidate: candidate.clone(),
                }),
            ),
            ("invalid", invalid(&candidate)),
            (
                "removed",
                ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key: key.clone() }),
            ),
        ];
        for (name, departure) in departures {
            let runtime = CandidateRuntime::for_test().await;
            runtime.record_event(&scanned(candidate.clone()));
            let tokens = start_both(&runtime, &key);
            let other = start_both(&runtime, &crate::CandidateKey::test("/watch/a/other"));

            runtime.record_event(&departure);

            assert!(
                tokens.iter().all(|token| token.is_cancelled()),
                "{key} {name}: its work goes on"
            );
            assert!(
                other.iter().all(|token| !token.is_cancelled()),
                "{key} {name}: another key's work ended"
            );
        }
    }
}

/// A candidate's standing ends with it: a folder whose answer failed to
/// save, removed and found again with the same files, starts with no
/// standing — no failure on its row.
#[tokio::test(flavor = "multi_thread")]
async fn a_folder_removed_and_found_again_starts_with_no_standing() {
    let fixture = Fixture::new("standing-ends-with-its-candidate").await;
    fixture.set_identify_automatically(false).await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.scan(1).await;
    let files =
        crate::CandidateFiles { content_hash: fixture.content_hash(&dir), file_edit_revision: 0 };
    let failure = crate::SaveFailure::NotWritten { error: "the disk is full".to_string() };
    fixture
        .import
        .fail_identification(&key, IdentifyRunId::for_test(1), &files, failure.clone())
        .await
        .unwrap();
    assert_eq!(
        fixture.identification_status(&key).await,
        Some(crate::IdentificationStatus::FinalizationFailed { failure })
    );

    let away = fixture.root.with_file_name("away-from-the-watched-folder");
    std::fs::rename(&dir, &away).unwrap();
    fixture.rescan(&fixture.import, 0).await;
    assert_eq!(
        fixture.identification_status(&key).await,
        None,
        "the scan that dropped the folder ended its standing"
    );
    std::fs::rename(&away, &dir).unwrap();
    fixture.rescan(&fixture.import, 1).await;
    assert_eq!(fixture.content_hash(&dir), files.content_hash, "the same files");

    assert_eq!(fixture.identification_status(&key).await, None);
    let list = list_subscription(&fixture, page_window(0, 50)).await;
    let (_, live) = &page_rows(&next_page(&list).await)[&key];
    assert_eq!(live.facts.identification, None, "the row shows no failure");
}

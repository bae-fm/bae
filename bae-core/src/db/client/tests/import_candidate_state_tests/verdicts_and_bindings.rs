use super::super::*;
use crate::identify::{ResultProvenance, TerminalVerdict};
use crate::import::folder_registry::host_root;
use crate::import::folder_scanner::{CandidateFile, CategorizedFiles, FileRole, ScannedFile};
use crate::import::search::MetadataResult;
use coven::FixedClock;
use std::path::PathBuf;

/// The instant `empty_db`'s injected clock always returns. Fixed rather
/// than `SystemClock` so `identified_at` can be asserted exactly — which is
/// why `save_import_candidate_verdict` stamps it from the injected clock
/// instead of taking it from the caller.
fn fixed_identified_at() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-01-15T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

async fn empty_db() -> (Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(FixedClock(fixed_identified_at())),
        Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    (db, tmp)
}

/// A folder of plain track files (no track sheet) named
/// `(relative_path, size)`.
fn track_files_candidate(files: &[(&str, u64)]) -> CategorizedFiles {
    CategorizedFiles {
        files: files
            .iter()
            .map(|(name, size)| CandidateFile {
                file: ScannedFile::new(
                    PathBuf::from(*name),
                    name.to_string(),
                    *size,
                    1,
                    format!("{size:064x}"),
                )
                .with_test_flac_audio(),
                role: FileRole::Audio,
                proposed_audio: true,
            })
            .collect(),
    }
}

fn sample_verdict() -> TerminalVerdict {
    TerminalVerdict::Found {
        matches: vec![MetadataResult {
            source: MetadataSource::MusicBrainz,
            release_id: "rel-1".to_string(),
            title: "Album".to_string(),
            artist: Some("Artist".to_string()),
            year: Some(1999),
            format: Some("CD".to_string()),
            label: Some("Label".to_string()),
            catalog_number: Some("CAT-1".to_string()),
            country: Some("US".to_string()),
            cover_art: None,
            source_group_id: Some("group-1".to_string()),
            source_tracks: None,
        }],
        track_count: 11,
        provenance: vec![ResultProvenance {
            by_disc_id: true,
            by_barcode: true,
            by_catalog: true,
        }],
        matched_barcode: Some("5099969394522".to_string()),
    }
}

/// Settled signals with nothing found and a stated total — what a verdict
/// carries when a test cares only about the numbers stored beside it.
fn sample_signals(probed_total_duration_ms: u64) -> crate::signals::Signals {
    crate::signals::Signals {
        disc_id: crate::signals::DiscIdSignal::Absent { track_count: 0 },
        barcode: crate::signals::BarcodeSignal::Absent,
        text: crate::signals::TextSignal::Settled {
            catalogs: Vec::new(),
            free_text: Vec::new(),
        },
        durations: crate::import::probe::SourceDurations::totalling(probed_total_duration_ms),
    }
}

fn new_candidate_row(
    content_hash: &str,
    folder_path: &str,
    verdict: &TerminalVerdict,
    probed_total_duration_ms: u64,
) -> NewImportCandidateVerdict {
    NewImportCandidateVerdict {
        content_hash: content_hash.to_string(),
        folder_path: folder_path.to_string(),
        verdict: verdict.clone(),
        signals: sample_signals(probed_total_duration_ms),
        expected_edit_revision: 0,
        expected_metadata_revision: 0,
        metadata: crate::import::CandidateMetadataDraft {
            edit: metadata_draft("", ""),
            provenance: None,
            cover: None,
        },
    }
}

/// Save a verdict, read it back, and check the provenance survived the JSON
/// round trip along with everything else — a stripped `by_disc_id` or a
/// dropped catalog number wouldn't show up in a looser comparison.
#[tokio::test]
async fn round_trip_preserves_the_verdict_including_provenance() {
    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let verdict = sample_verdict();
    let row = new_candidate_row(&hash, "/music/Some Album", &verdict, 2_700_000);

    db.save_import_candidate_verdict(&row).await.unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    let loaded_row = loaded
        .get(&hash)
        .expect("row present under its content hash");
    assert_eq!(loaded_row.folder_path, "/music/Some Album");
    let identify = loaded_row
        .identify
        .as_ref()
        .expect("a stored verdict reads back as an identify result");
    assert_eq!(identify.probed_total_duration_ms, 2_700_000);
    // Stamped by the write path from the injected clock, not something
    // `new_candidate_row` had any way to supply.
    assert_eq!(identify.identified_at, fixed_identified_at());
    assert_eq!(
        identify.verdict, verdict,
        "the verdict must round-trip exactly, provenance included"
    );
}

/// Resizing one file changes `content_hash`, which is the whole
/// invalidation mechanism: the new hash finds nothing (so it gets
/// re-identified) while the old row is left behind, unreachable, under its
/// own key.
#[tokio::test]
async fn resizing_a_file_orphans_the_old_row_under_a_new_hash() {
    let (db, _tmp) = empty_db().await;
    let original = track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let original_hash = original.content_hash();
    let row = new_candidate_row(
        &original_hash,
        "/music/Some Album",
        &sample_verdict(),
        2_700_000,
    );
    db.save_import_candidate_verdict(&row).await.unwrap();

    let resized = track_files_candidate(&[("01 Track.flac", 999_999), ("02 Track.flac", 234_567)]);
    let resized_hash = resized.content_hash();
    assert_ne!(
        original_hash, resized_hash,
        "resizing a file must change the hash"
    );

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert!(
        loaded.contains_key(&original_hash),
        "the old row is still present under its own key"
    );
    assert!(
        !loaded.contains_key(&resized_hash),
        "the new hash must find no row -- the candidate needs re-identifying"
    );
}

fn release_pick(release_id: &str) -> crate::import::MetadataProvenance {
    crate::import::MetadataProvenance::ExternalRelease {
        source: crate::import::MetadataSource::MusicBrainz,
        release_id: release_id.to_string(),
    }
}

#[tokio::test]
async fn every_metadata_provenance_variant_survives_a_database_reopen() {
    let (db, tmp) = empty_db().await;
    let cases = [
        (
            track_files_candidate(&[("01 Track.flac", 100_001)]).content_hash(),
            "/music/Candidate A",
            release_pick("release-a"),
        ),
        (
            track_files_candidate(&[("01 Track.flac", 100_002)]).content_hash(),
            "/music/Candidate B",
            crate::import::MetadataProvenance::FileTags,
        ),
    ];

    for (content_hash, folder_path, provenance) in &cases {
        db.save_import_candidate_failure(
            content_hash,
            folder_path,
            0,
            &crate::import::ImportFailure::error_only("test anchor", fixed_identified_at()),
        )
            .await
            .unwrap();
        db.replace_candidate_metadata(
            content_hash,
            folder_path,
            &metadata_draft("Album", "Artist"),
            Some(provenance),
        )
            .await
            .unwrap();
        db.clear_import_candidate_failure(content_hash).await.unwrap();
    }
    drop(db);

    let path = tmp.path().join("test.db");
    let reopened = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(FixedClock(fixed_identified_at())),
        Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let loaded = reopened.load_import_candidate_states().await.unwrap();

    for (content_hash, _, provenance) in &cases {
        assert_eq!(
            loaded
                .get(content_hash)
                .expect("the candidate state survives the database reopen")
                .metadata_provenance
                .as_ref(),
            Some(provenance)
        );
    }
}

fn found_nothing() -> TerminalVerdict {
    TerminalVerdict::NotFoundAnywhere
}

/// A pick identification derived from its own single match belongs to that
/// verdict. When a re-run settles on anything else, the pick goes with the
/// verdict that made it — otherwise the candidate keeps naming a release it
/// is no longer identified as, and an import would commit that release
/// against a folder nothing now matches.
#[tokio::test]
async fn a_re_run_that_settles_elsewhere_drops_the_pick_its_own_earlier_verdict_made() {
    let (db, _tmp) = empty_db().await;
    let hash = track_files_candidate(&[("01 Track.flac", 123_456)]).content_hash();

    let mut settled = new_candidate_row(&hash, "/music/Album", &sample_verdict(), 2_700_000);
    settled.metadata.provenance = Some(release_pick("mb-rel-1"));
    db.save_import_candidate_verdict(&settled).await.unwrap();

    let mut re_run = new_candidate_row(&hash, "/music/Album", &found_nothing(), 2_700_000);
    re_run.expected_metadata_revision = 1;
    assert_eq!(re_run.metadata.provenance, None, "nothing was found to pick");
    db.save_import_candidate_verdict(&re_run).await.unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the row is still there")
            .metadata_provenance,
        None,
        "the pick the superseded verdict made must not outlive it"
    );
}

/// A person's pick is not identification's to revise. A later run's signals
/// turning up nothing says nothing about a release they chose by hand, so
/// the choice stands and the pane reopens on it.
#[tokio::test]
async fn a_re_run_that_finds_nothing_leaves_a_person_s_pick_alone() {
    let (db, _tmp) = empty_db().await;
    let hash = track_files_candidate(&[("01 Track.flac", 123_456)]).content_hash();

    let initial = new_candidate_row(&hash, "/music/Album", &found_nothing(), 2_700_000);
    db.save_import_candidate_verdict(&initial).await.unwrap();
    db.replace_candidate_metadata(
        &hash,
        "/music/Album",
        &metadata_draft("Album", "Artist"),
        Some(&release_pick("mb-rel-chosen")),
    )
        .await
        .unwrap();

    let mut re_run = new_candidate_row(&hash, "/music/Album", &found_nothing(), 2_700_000);
    re_run.expected_metadata_revision = 2;
    db.save_import_candidate_verdict(&re_run).await.unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the row is still there")
            .metadata_provenance,
        Some(release_pick("mb-rel-chosen")),
        "a verdict must not unmake a choice a person made"
    );
}

/// And a re-run that settles on a *different* single match replaces the
/// pick its predecessor made, rather than leaving the older release named.
#[tokio::test]
async fn a_re_run_that_settles_elsewhere_replaces_the_pick_it_made() {
    let (db, _tmp) = empty_db().await;
    let hash = track_files_candidate(&[("01 Track.flac", 123_456)]).content_hash();

    let mut first = new_candidate_row(&hash, "/music/Album", &sample_verdict(), 2_700_000);
    first.metadata.provenance = Some(release_pick("mb-rel-first"));
    db.save_import_candidate_verdict(&first).await.unwrap();

    let mut second = new_candidate_row(&hash, "/music/Album", &sample_verdict(), 2_700_000);
    second.expected_metadata_revision = 1;
    second.metadata.provenance = Some(release_pick("mb-rel-second"));
    db.save_import_candidate_verdict(&second).await.unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the row is still there")
            .metadata_provenance,
        Some(release_pick("mb-rel-second")),
        "the live verdict's own conclusion is the one that stands"
    );
}

/// A file decision clears the verdict, and identification's pick goes with
/// it: that pick was the verdict's conclusion about a folder shape the
/// decision just changed. The person's own pick is untouched — their choice
/// names a release, not a shape.
#[tokio::test]
async fn a_file_decision_clears_identification_s_pick_and_keeps_a_person_s() {
    let (db, _tmp) = empty_db().await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = candidate.content_hash();
    let edits = crate::import::folder_scanner::CandidateFileEdits::default();

    let mut settled = new_candidate_row(&hash, "/music/Album", &sample_verdict(), 2_700_000);
    settled.metadata.provenance = Some(release_pick("mb-rel-derived"));
    db.save_import_candidate_verdict(&settled).await.unwrap();
    db.save_import_candidate_file_edits(&hash, "/music/Album", 0, &edits, &[])
        .await
        .unwrap();
    let loaded = db.load_import_candidate_states().await.unwrap();
    let row = loaded.get(&hash).expect("the row is still there");
    assert!(row.identify.is_none(), "the decision clears the verdict");
    assert_eq!(
        row.metadata_provenance, None,
        "and the pick that verdict concluded goes with it"
    );

    db.replace_candidate_metadata(
        &hash,
        "/music/Album",
        &metadata_draft("Album", "Artist"),
        Some(&release_pick("mb-rel-chosen")),
    )
        .await
        .unwrap();
    db.save_import_candidate_file_edits(&hash, "/music/Album", 1, &edits, &[])
        .await
        .unwrap();
    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the row is still there")
            .metadata_provenance,
        Some(release_pick("mb-rel-chosen")),
        "a file decision must not unmake a choice a person made"
    );
}

/// Same files, same relative paths and sizes, under a different parent
/// directory: `content_hash` never looks at the absolute path, so the row
/// saved for the folder at its old location is still the row found for it
/// at the new one.
#[tokio::test]
async fn a_moved_folder_hashes_identically_and_keeps_its_row() {
    let (db, _tmp) = empty_db().await;
    let at_old_location =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = at_old_location.content_hash();
    let row = new_candidate_row(
        &hash,
        "/music/Old Location/Some Album",
        &sample_verdict(),
        2_700_000,
    );
    db.save_import_candidate_verdict(&row).await.unwrap();

    let mut at_new_location = at_old_location.clone();
    for entry in &mut at_new_location.files {
        entry.file.path = PathBuf::from("/music/New Location/Some Album")
            .join(&entry.file.relative_path);
    }
    assert_eq!(
        hash,
        at_new_location.content_hash(),
        "a moved folder must hash identically to itself before the move"
    );

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert!(
        loaded.contains_key(&at_new_location.content_hash()),
        "the row saved before the move must still be reachable after it"
    );
}

/// A transport failure writes no row. Driven through the real identify
/// reducer (a disc-ID lookup that fails over the network, no barcode
/// source to fall back on) rather than hand-built, so this actually
/// exercises the guard in `identify::verdict::TerminalVerdict::try_from` —
/// the one thing standing between "nothing was learned" and a permanent
/// `NotFoundAnywhere` row. If that guard is ever weakened or removed, this
/// starts writing a row and fails.
#[tokio::test]
async fn no_row_is_written_for_a_transport_failure() {
    use crate::identify::state::step as identify_step;
    use crate::identify::{IdentifyEvent, IdentifyState};
    use crate::signals::{BarcodeSignal, DiscIdSignal, LookupFailure, Signals, TextSignal};

    let (db, _tmp) = empty_db().await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = candidate.content_hash();

    let (state, _) = identify_step(IdentifyState::Idle, IdentifyEvent::Started);
    let (state, _) = identify_step(
        state,
        IdentifyEvent::SignalsUpdated {
            signals: Signals {
                disc_id: DiscIdSignal::Computed {
                    disc_id: "disc-hash".to_string(),
                    track_count: 1,
                    source_file: None,
                },
                barcode: BarcodeSignal::Absent,
                text: TextSignal::Settled {
                    catalogs: vec![],
                    free_text: vec![],
                },
                durations: crate::import::probe::SourceDurations::default(),
            },
        },
    );
    let (state, _) = identify_step(
        state,
        IdentifyEvent::DiscidLookupFailed {
            failure: LookupFailure::Provider { status: Some(503) },
            track_count: 1,
        },
    );

    // Exactly the shape a scheduler will use: only a successful conversion
    // ever reaches `save_import_candidate_verdict`.
    if let Ok(verdict) = TerminalVerdict::try_from(state) {
        let row = new_candidate_row(&hash, "/music/Some Album", &verdict, 0);
        db.save_import_candidate_verdict(&row).await.unwrap();
    }

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert!(
        !loaded.contains_key(&hash),
        "a transport failure teaches nothing -- absence is the retry signal"
    );
}

/// A binding the user cleared survives a relaunch: it is stored under the
/// candidate's content hash, read back from a cold database, and the scan
/// that follows reports the folder as they settled it rather than as its
/// filenames read.
///
/// The scan is the point — a binding that round-tripped through SQLite but
/// never reached a folder's roles would be a stored value nothing consumes.
#[tokio::test]
async fn a_cleared_binding_survives_a_relaunch() {
    use crate::import::folder_scanner::{
        collect_release_candidate_files_with_scope, CandidateFileEdits, SheetBindingEdits,
        StoredCandidateEdits, UserSheetBinding,
    };

    let (db, _tmp) = empty_db().await;
    let folder = walkthrough_folder();
    let scanned = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
    assert_eq!(
        scanned.track_count(),
        12,
        "the unique same-stem audio is bound automatically"
    );
    let root = folder.path().to_string_lossy().into_owned();
    db.add_watched_import_folder(&root).await.unwrap();
    let generation = db.begin_folder_scan(&root).await.unwrap();
    let candidate = crate::import::folder_scanner::FolderCandidate {
        path: folder.path().to_path_buf(),
        file_root: folder.path().to_path_buf(),
        name: "Release".to_string(),
        files: scanned.clone(),
        watched_folder_path: root.clone(),
        scope: crate::import::ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: String::new(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    };
    db.save_folder_scan_item(
        &root,
        generation,
        &crate::import::folder_scanner::ScanItem::Valid(candidate),
    )
    .await
    .unwrap();
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();

    let mut edits = SheetBindingEdits::default();
    edits.set("cd.cue".to_string(), UserSheetBinding::Cleared);
    let candidate_edits = CandidateFileEdits {
        sheet_bindings: edits,
        ..Default::default()
    };
    let mut settled = scanned.clone();
    settled
        .apply_candidate_file_edits(&candidate_edits)
        .unwrap();
    db.save_import_candidate_file_edits(
        &scanned.content_hash(),
        &folder.path().to_string_lossy(),
        0,
        &candidate_edits,
        &[(folder.path().to_string_lossy().into_owned(), settled)],
    )
    .await
    .unwrap();

    let current = db
        .load_candidate_file_edits(&scanned.content_hash())
        .await
        .unwrap();
    assert_eq!(current.revision, 1);
    assert_eq!(
        current.sheet_bindings.get("cd.cue"),
        Some(&UserSheetBinding::Cleared)
    );
    assert_eq!(
        db.load_candidate_file_edits("missing").await.unwrap(),
        CandidateFileEdits::default()
    );

    let restored = db.load_folder_scan_snapshots().await.unwrap();
    let crate::import::folder_scanner::ScanItem::Valid(restored_candidate) = &restored[0].items[0]
    else {
        panic!("the persisted candidate keeps its valid variant");
    };
    assert_eq!(restored_candidate.file_edit_revision, 1);
    assert_eq!(restored_candidate.track_count(), 1);
    assert!(restored_candidate.files.bound_sheets().is_empty());

    // A subsequent scan reads the same decisions and derives the same
    // shape as the candidate restored before that scan.
    let stored = db.load_stored_candidate_edits().await.unwrap();
    let reopened = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::import::ReleaseFileScope::Recursive,
        &stored,
    )
    .unwrap();

    assert_eq!(
        reopened.track_count(),
        1,
        "the cleared binding read back from disk is the one the scan applies"
    );
    assert!(reopened.bound_sheets().is_empty());
}

/// The pair that makes re-identification correct rather than incidental:
/// changing a binding leaves the row's key alone, **and** clears the
/// verdict stored under it.
///
/// The hash covers files and never role decisions, so the edit addresses
/// the same row rather than orphaning it — and that row's verdict was
/// derived from the shape the folder no longer has, so the queue must
/// answer the candidate again instead of trusting it.
#[tokio::test]
async fn changing_a_binding_keeps_the_hash_and_clears_the_verdict() {
    use crate::import::folder_scanner::{
        collect_release_candidate_files_with_scope, CandidateFileEdits, SheetBindingEdits,
        StoredCandidateEdits, UserSheetBinding,
    };

    let (db, _tmp) = empty_db().await;
    let folder = walkthrough_folder();
    let proposed = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
    assert_eq!(proposed.track_count(), 12);
    let hash = proposed.content_hash();

    db.save_import_candidate_verdict(&new_candidate_row(
        &hash,
        &folder.path().to_string_lossy(),
        &sample_verdict(),
        2_700_000,
    ))
    .await
    .unwrap();
    assert!(
        db.load_import_candidate_states()
            .await
            .unwrap()
            .get(&hash)
            .expect("the verdict is stored")
            .identify
            .is_some(),
        "the candidate starts out identified"
    );

    let mut edits = SheetBindingEdits::default();
    edits.set("cd.cue".to_string(), UserSheetBinding::Cleared);
    db.save_import_candidate_file_edits(
        &hash,
        &folder.path().to_string_lossy(),
        0,
        &CandidateFileEdits {
            sheet_bindings: edits,
            ..Default::default()
        },
        &[],
    )
    .await
    .unwrap();

    let unbound = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::import::ReleaseFileScope::Recursive,
        &db.load_stored_candidate_edits().await.unwrap(),
    )
    .unwrap();
    assert_eq!(
        unbound.track_count(),
        1,
        "the folder really did change shape -- otherwise this proves nothing"
    );
    assert_eq!(
        unbound.content_hash(),
        hash,
        "the hash covers files, never role decisions, so the row stays addressable"
    );

    let row = db
        .load_import_candidate_states()
        .await
        .unwrap()
        .remove(&hash)
        .expect("the row is still found under the unchanged hash");
    assert!(
        row.identify.is_none(),
        "the stored verdict described the folder before the binding; it must be cleared \
         so the queue identifies the candidate again"
    );
    assert_eq!(
        row.file_edits.sheet_bindings.get("cd.cue"),
        Some(&UserSheetBinding::Cleared),
        "the decision that cleared the verdict is what the row now holds"
    );
}

#[tokio::test]
async fn folder_release_decision_is_idempotent_and_root_scoped() {
    use crate::import::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey};

    let (db, _tmp) = empty_db().await;
    let other = host_root("/other/library");
    let key = FolderReleaseDecisionKey {
        watched_folder_path: host_root("/mounted/library"),
        relative_folder_path: "Collection/Release Wrapper".to_string(),
    };
    db.add_watched_import_folder(&key.watched_folder_path)
        .await
        .unwrap();
    db.add_watched_import_folder(&other).await.unwrap();

    db.set_folder_release_decision(&key, FolderReleaseDecision::CombineAsOneRelease, crate::import::folder_scanner::FolderReleaseDecisionAuthor::User)
        .await
        .unwrap();
    db.set_folder_release_decision(&key, FolderReleaseDecision::CombineAsOneRelease, crate::import::folder_scanner::FolderReleaseDecisionAuthor::User)
        .await
        .unwrap();
    db.set_folder_release_decision(
        &FolderReleaseDecisionKey {
            watched_folder_path: other,
            relative_folder_path: key.relative_folder_path.clone(),
        },
        FolderReleaseDecision::KeepAsSeparateReleases,
        crate::import::folder_scanner::FolderReleaseDecisionAuthor::User,
    )
    .await
    .unwrap();

    let decisions = db
        .load_folder_release_decisions(&key.watched_folder_path)
        .await
        .unwrap();
    assert_eq!(
        decisions.get(&key.relative_folder_path),
        Some((

            FolderReleaseDecision::CombineAsOneRelease,

            crate::import::folder_scanner::FolderReleaseDecisionAuthor::User,

        ))
    );
}

use super::super::*;
use crate::identify::{GroupKey, ResultProvenance, TerminalVerdict};
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
                file: ScannedFile::new(PathBuf::from(*name), name.to_string(), *size),
                role: FileRole::Audio,
                proposed_audio: true,
            })
            .collect(),
        format_label: "FLAC".to_string(),
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
        group: GroupKey {
            source: MetadataSource::MusicBrainz,
            source_group_id: "group-1".to_string(),
        },
        provenance: vec![ResultProvenance {
            by_disc_id: true,
            by_barcode: false,
            matches_catalog: true,
        }],
    }
}

fn new_candidate_row(
    content_hash: &str,
    folder_path: &str,
    verdict: &TerminalVerdict,
    probed_total_duration_ms: i64,
) -> NewImportCandidateVerdict {
    NewImportCandidateVerdict {
        content_hash: content_hash.to_string(),
        folder_path: folder_path.to_string(),
        verdict: serde_json::to_string(verdict).unwrap(),
        probed_total_duration_ms,
        expected_edit_revision: 0,
        identity_pick: None,
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
    let loaded_verdict: TerminalVerdict = serde_json::from_str(&identify.verdict).unwrap();
    assert_eq!(
        loaded_verdict, verdict,
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

fn release_pick(release_id: &str) -> String {
    serde_json::to_string(&crate::import::IdentityPick::Release {
        source: crate::import::MetadataSource::MusicBrainz,
        release_id: release_id.to_string(),
        claim: crate::import::ClaimLevel::Exact,
    })
    .expect("the pick encodes")
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
    settled.identity_pick = Some(release_pick("mb-rel-1"));
    db.save_import_candidate_verdict(&settled).await.unwrap();

    let re_run = new_candidate_row(&hash, "/music/Album", &found_nothing(), 2_700_000);
    assert_eq!(re_run.identity_pick, None, "nothing was found to pick");
    db.save_import_candidate_verdict(&re_run).await.unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the row is still there")
            .identity_pick,
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

    db.save_candidate_identity_pick(&hash, "/music/Album", &release_pick("mb-rel-chosen"))
        .await
        .unwrap();

    let re_run = new_candidate_row(&hash, "/music/Album", &found_nothing(), 2_700_000);
    db.save_import_candidate_verdict(&re_run).await.unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the row is still there")
            .identity_pick,
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
    first.identity_pick = Some(release_pick("mb-rel-first"));
    db.save_import_candidate_verdict(&first).await.unwrap();

    let mut second = new_candidate_row(&hash, "/music/Album", &sample_verdict(), 2_700_000);
    second.identity_pick = Some(release_pick("mb-rel-second"));
    db.save_import_candidate_verdict(&second).await.unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the row is still there")
            .identity_pick,
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
    settled.identity_pick = Some(release_pick("mb-rel-derived"));
    db.save_import_candidate_verdict(&settled).await.unwrap();
    db.save_import_candidate_file_edits(&hash, "/music/Album", 0, &edits, &[])
        .await
        .unwrap();
    let loaded = db.load_import_candidate_states().await.unwrap();
    let row = loaded.get(&hash).expect("the row is still there");
    assert!(row.identify.is_none(), "the decision clears the verdict");
    assert_eq!(
        row.identity_pick, None,
        "and the pick that verdict concluded goes with it"
    );

    db.save_candidate_identity_pick(&hash, "/music/Album", &release_pick("mb-rel-chosen"))
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
            .identity_pick,
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

    let at_new_location = CategorizedFiles {
        files: vec![
            CandidateFile {
                file: ScannedFile::new(
                    PathBuf::from("/music/New Location/Some Album/01 Track.flac"),
                    "01 Track.flac".to_string(),
                    123_456,
                ),
                role: FileRole::Audio,
                proposed_audio: true,
            },
            CandidateFile {
                file: ScannedFile::new(
                    PathBuf::from("/music/New Location/Some Album/02 Track.flac"),
                    "02 Track.flac".to_string(),
                    234_567,
                ),
                role: FileRole::Audio,
                proposed_audio: true,
            },
        ],
        format_label: "FLAC".to_string(),
    };
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
                },
                barcode: BarcodeSignal::Absent,
                text: TextSignal::Settled {
                    catalogs: vec![],
                    free_text: vec![],
                },
                probed_total_duration_ms: 0,
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

/// A binding the user set survives a relaunch: it is stored under the
/// candidate's content hash, read back from a cold database, and the scan
/// that follows reports the folder as they settled it rather than as its
/// filenames read.
///
/// The scan is the point — a binding that round-tripped through SQLite but
/// never reached a folder's roles would be a stored value nothing consumes.
#[tokio::test]
async fn a_binding_survives_a_relaunch() {
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
    assert_eq!(scanned.track_count(), 1, "unbound, the image is one track");
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
        &[],
    )
    .await
    .unwrap();
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();

    let mut edits = SheetBindingEdits::default();
    edits.set(
        "cd.cue".to_string(),
        UserSheetBinding::Describes {
            file_id: "cd.flac".to_string(),
        },
    );
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
        Some(&UserSheetBinding::Describes {
            file_id: "cd.flac".to_string()
        })
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
    assert_eq!(restored_candidate.track_count(), 12);
    assert_eq!(
        restored_candidate.files.bound_sheets()[0].audio.file_name,
        "cd.flac"
    );

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
        12,
        "the binding read back from disk is the one the scan applies"
    );
    assert_eq!(reopened.bound_sheets()[0].audio.file_name, "cd.flac");
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
    let unbound = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
    let hash = unbound.content_hash();

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
    edits.set(
        "cd.cue".to_string(),
        UserSheetBinding::Describes {
            file_id: "cd.flac".to_string(),
        },
    );
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

    let bound = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::import::ReleaseFileScope::Recursive,
        &db.load_stored_candidate_edits().await.unwrap(),
    )
    .unwrap();
    assert_eq!(
        bound.track_count(),
        12,
        "the folder really did change shape -- otherwise this proves nothing"
    );
    assert_eq!(
        bound.content_hash(),
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
        Some(&UserSheetBinding::Describes {
            file_id: "cd.flac".to_string()
        }),
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

    db.set_folder_release_decision(&key, FolderReleaseDecision::CombineAsOneRelease)
        .await
        .unwrap();
    db.set_folder_release_decision(&key, FolderReleaseDecision::CombineAsOneRelease)
        .await
        .unwrap();
    db.set_folder_release_decision(
        &FolderReleaseDecisionKey {
            watched_folder_path: other,
            relative_folder_path: key.relative_folder_path.clone(),
        },
        FolderReleaseDecision::KeepAsSeparateReleases,
    )
    .await
    .unwrap();

    let decisions = db
        .load_folder_release_decisions(&key.watched_folder_path)
        .await
        .unwrap();
    assert_eq!(
        decisions.get(&key.relative_folder_path),
        Some(FolderReleaseDecision::CombineAsOneRelease)
    );
}

fn scanned_candidate(root: &str, name: &str) -> crate::import::folder_scanner::ScanItem {
    use crate::import::folder_scanner::{FolderCandidate, ReleaseFileScope, ScanItem};

    let path = PathBuf::from(root).join(name);
    ScanItem::Valid(FolderCandidate {
        path: path.clone(),
        file_root: path,
        name: name.to_string(),
        files: track_files_candidate(&[("01.flac", 123)]),
        watched_folder_path: root.to_string(),
        scope: ReleaseFileScope::Direct,
        file_edit_revision: 0,
        display_path: name.to_string(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    })
}

#[tokio::test]
async fn folder_scan_cache_writes_progressively_and_prunes_only_on_success() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    let first = scanned_candidate(root, "First");
    let second = scanned_candidate(root, "Second");
    db.add_watched_import_folder(root).await.unwrap();

    let generation = db.begin_folder_scan(root).await.unwrap();
    assert!(db
        .save_folder_scan_item(root, generation, &first, &[])
        .await
        .unwrap());
    assert!(db
        .finish_folder_scan(root, generation, Some("share disconnected"))
        .await
        .unwrap());

    let generation = db.begin_folder_scan(root).await.unwrap();
    assert!(db
        .save_folder_scan_item(root, generation, &second, &[])
        .await
        .unwrap());
    assert!(db
        .finish_folder_scan(root, generation, Some("directory unreadable"))
        .await
        .unwrap());
    let failed = db.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].items.len(), 2);
    assert!(matches!(
        &failed[0].status,
        crate::import::FolderScanStatus::Failed { error }
            if error == "directory unreadable"
    ));

    let generation = db.begin_folder_scan(root).await.unwrap();
    assert!(db
        .save_folder_scan_item(root, generation, &second, &[])
        .await
        .unwrap());
    assert!(db.finish_folder_scan(root, generation, None).await.unwrap());
    let complete = db.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(complete[0].items.len(), 1);
    assert_eq!(complete[0].items[0].persisted_key(), second.persisted_key());
    assert_eq!(
        complete[0].status,
        crate::import::FolderScanStatus::Complete
    );

    assert!(
        !db.save_folder_scan_item(root, generation - 1, &first, &[])
            .await
            .unwrap(),
        "a superseded generation cannot overwrite the stored snapshot"
    );
}

#[tokio::test]
async fn folder_scan_item_rejects_a_mismatched_embedded_root_without_changing_the_snapshot() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    let existing = scanned_candidate(root, "Existing");
    db.save_folder_scan_item(root, generation, &existing, &[])
        .await
        .unwrap();

    let mismatched = scanned_candidate(&host_root("/other/library"), "Injected");
    let error = db
        .save_folder_scan_item(root, generation, &mismatched, &[])
        .await
        .unwrap_err();

    assert!(error.to_string().contains("does not belong"));
    let snapshot = db.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].items.len(), 1);
    assert_eq!(
        snapshot[0].items[0].persisted_key(),
        existing.persisted_key()
    );
}

#[tokio::test]
async fn imported_content_hash_lookup_uses_its_partial_index() {
    let (db, _tmp) = empty_db().await;
    let plan = db.content_hash_query_plan_for_test().await.unwrap();

    assert!(
        plan.iter()
            .any(|detail| detail.contains("idx_releases_content_hash")),
        "query plan did not use the content-hash index: {plan:?}"
    );
}

#[tokio::test]
async fn folder_decisions_remove_contradictory_scan_rows_before_failed_rescan() {
    use crate::import::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey};

    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    for name in ["Box/CD1", "Box/CD2"] {
        let item = scanned_candidate(root, name);
        db.save_folder_scan_item(root, generation, &item, &[])
            .await
            .unwrap();
    }
    let key = FolderReleaseDecisionKey {
        watched_folder_path: root.to_string(),
        relative_folder_path: "Box".to_string(),
    };
    let (combine_generation, combine_removals) = db
        .set_folder_release_decisions(&[(key.clone(), FolderReleaseDecision::CombineAsOneRelease)])
        .await
        .unwrap();
    assert_eq!(
        combine_removals,
        vec![
            scanned_candidate(root, "Box/CD1").persisted_key(),
            scanned_candidate(root, "Box/CD2").persisted_key(),
        ]
    );
    db.finish_folder_scan(root, combine_generation, Some("share disconnected"))
        .await
        .unwrap();
    assert!(db.load_folder_scan_snapshots().await.unwrap()[0]
        .items
        .is_empty());
    assert_eq!(
        db.load_folder_release_decisions(root)
            .await
            .unwrap()
            .get("Box"),
        Some(FolderReleaseDecision::CombineAsOneRelease)
    );

    let generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item(root, generation, &scanned_candidate(root, "Box"), &[])
        .await
        .unwrap();
    let (separate_generation, separate_removals) = db
        .set_folder_release_decisions(&[(key, FolderReleaseDecision::KeepAsSeparateReleases)])
        .await
        .unwrap();
    assert_eq!(
        separate_removals,
        vec![scanned_candidate(root, "Box").persisted_key()]
    );
    db.finish_folder_scan(root, separate_generation, Some("share disconnected"))
        .await
        .unwrap();
    assert!(db.load_folder_scan_snapshots().await.unwrap()[0]
        .items
        .is_empty());
    assert_eq!(
        db.load_folder_release_decisions(root)
            .await
            .unwrap()
            .get("Box"),
        Some(FolderReleaseDecision::KeepAsSeparateReleases)
    );
}

#[tokio::test]
async fn removed_and_readded_root_rejects_items_from_its_old_registration() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    let old_generation = db.begin_folder_scan(root).await.unwrap();

    db.remove_watched_import_folder(root).await.unwrap();
    db.add_watched_import_folder(root).await.unwrap();
    let new_generation = db.begin_folder_scan(root).await.unwrap();
    assert!(new_generation > old_generation);

    assert!(!db
        .save_folder_scan_item(root, old_generation, &scanned_candidate(root, "Old"), &[],)
        .await
        .unwrap());
    assert!(db.load_folder_scan_snapshots().await.unwrap()[0]
        .items
        .is_empty());
}

#[tokio::test]
async fn folder_decision_failure_rolls_back_decision_entries_and_generation() {
    use crate::import::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey};

    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item(root, generation, &scanned_candidate(root, "Box/CD1"), &[])
        .await
        .unwrap();
    db.call(|conn| {
        conn.execute(
            "UPDATE folder_scan_generation_sequence SET last_generation = ?",
            [i64::MAX],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let result = db
        .set_folder_release_decisions(&[(
            FolderReleaseDecisionKey {
                watched_folder_path: root.to_string(),
                relative_folder_path: "Box".to_string(),
            },
            FolderReleaseDecision::CombineAsOneRelease,
        )])
        .await;
    assert!(result.is_err());

    assert!(db
        .load_folder_release_decisions(root)
        .await
        .unwrap()
        .get("Box")
        .is_none());
    let snapshots = db.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(snapshots[0].generation, generation);
    assert_eq!(snapshots[0].items.len(), 1);
    assert_eq!(
        snapshots[0].items[0].persisted_key(),
        scanned_candidate(root, "Box/CD1").persisted_key()
    );
}

#[tokio::test]
async fn removing_watched_root_cascades_all_local_folder_state() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    db.set_import_candidate_skipped(root, "Collection/Release", true)
        .await
        .unwrap();
    db.set_folder_release_decision(
        &crate::import::folder_scanner::FolderReleaseDecisionKey {
            watched_folder_path: root.to_string(),
            relative_folder_path: "Collection".to_string(),
        },
        crate::import::folder_scanner::FolderReleaseDecision::KeepAsSeparateReleases,
    )
    .await
    .unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item(root, generation, &scanned_candidate(root, "Release"), &[])
        .await
        .unwrap();

    assert!(db.remove_watched_import_folder(root).await.unwrap());
    assert!(db
        .load_import_folder_registry()
        .await
        .unwrap()
        .watched_folders()
        .is_empty());
    assert_eq!(
        db.load_folder_release_decisions(root)
            .await
            .unwrap()
            .get("Collection"),
        None
    );
    assert!(db.load_folder_scan_snapshots().await.unwrap().is_empty());
}

#[tokio::test]
async fn watched_root_overlap_uses_paths_not_sql_patterns() {
    let (db, _tmp) = empty_db().await;
    for root in ["/music/100%", "/music/name_value"] {
        assert!(db
            .add_watched_import_folder(&host_root(root))
            .await
            .unwrap());
    }
    for child in ["/music/100%/child", "/music/name_value/child"] {
        let error = db
            .add_watched_import_folder(&host_root(child))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("cannot overlap"), "{child}");
    }
}

#[tokio::test]
async fn watched_root_order_survives_middle_removal_and_later_add() {
    let (db, _tmp) = empty_db().await;
    for root in ["/one", "/two", "/three"] {
        db.add_watched_import_folder(&host_root(root))
            .await
            .unwrap();
    }
    db.remove_watched_import_folder(&host_root("/two"))
        .await
        .unwrap();
    db.add_watched_import_folder(&host_root("/four"))
        .await
        .unwrap();
    let paths: Vec<_> = db
        .load_import_folder_registry()
        .await
        .unwrap()
        .watched_folders()
        .into_iter()
        .map(|folder| folder.path)
        .collect();
    assert_eq!(
        paths,
        vec![host_root("/one"), host_root("/three"), host_root("/four")]
    );
}

/// However the folder was spelled on the way in, one row exists and it is
/// keyed by the canonical spelling — so a second spelling of a folder
/// already watched is recognized as the same folder rather than added
/// beside it.
#[tokio::test]
async fn watched_root_spellings_settle_on_one_row() {
    let (db, _tmp) = empty_db().await;
    let canonical = host_root("/music/rips");
    assert!(db.add_watched_import_folder(&canonical).await.unwrap());

    // The last of these is the drive-lettered, forward-slashed form a
    // `bae://import` link and a `file://` folder drop hand over on Windows.
    #[cfg(windows)]
    const URL_SPELLINGS: &[&str] = &["C:/music/rips"];
    #[cfg(not(windows))]
    const URL_SPELLINGS: &[&str] = &[];

    let spellings = [
        host_root("/music/rips/"),
        host_root("/music//rips"),
        host_root("/music/./rips"),
    ];

    for spelling in spellings
        .iter()
        .map(String::as_str)
        .chain(URL_SPELLINGS.iter().copied())
    {
        assert!(
            !db.add_watched_import_folder(spelling).await.unwrap(),
            "{spelling} is the folder already watched, not a new one"
        );
    }
    let paths: Vec<_> = db
        .load_import_folder_registry()
        .await
        .unwrap()
        .watched_folders()
        .into_iter()
        .map(|folder| folder.path)
        .collect();
    assert_eq!(paths, vec![canonical]);
}

/// `..` never becomes a key: rewriting it without reading the filesystem
/// is wrong across a symlink, so it is refused instead.
#[tokio::test]
async fn watched_root_rejects_a_path_climbing_out_of_itself() {
    let (db, _tmp) = empty_db().await;
    let path = host_root("/music/../rips");
    assert!(db.add_watched_import_folder(&path).await.is_err(), "{path}");
}

#[tokio::test]
async fn corrupt_relative_folder_keys_fail_when_loaded() {
    let (db, _tmp) = empty_db().await;
    let root = host_root("/mounted/library");
    db.add_watched_import_folder(&root).await.unwrap();
    assert!(db
        .set_import_candidate_skipped(&root, "a//b", true)
        .await
        .is_err());
    assert!(db
        .set_folder_release_decision(
            &crate::import::folder_scanner::FolderReleaseDecisionKey {
                watched_folder_path: root.clone(),
                relative_folder_path: "a/./b".to_string(),
            },
            crate::import::folder_scanner::FolderReleaseDecision::CombineAsOneRelease,
        )
        .await
        .is_err());
    let stored_root = root.clone();
    db.call(move |conn| {
        conn.execute(
            "INSERT INTO skipped_import_candidates VALUES (?, 'a//b')",
            params![stored_root],
        )?;
        conn.execute(
            "INSERT INTO folder_release_decisions VALUES (?, 'a/./b', 'combine_as_one_release')",
            params![stored_root],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    assert!(db.load_import_folder_registry().await.is_err());
    assert!(db.load_folder_release_decisions(&root).await.is_err());
}

#[tokio::test]
async fn corrupt_scan_entry_identity_and_generation_fail_when_loaded() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    let item = scanned_candidate(root, "Release");
    db.save_folder_scan_item(root, generation, &item, &[])
        .await
        .unwrap();
    // A key naming a folder the stored item does not: the entry no longer
    // identifies its own item.
    let other_key = scanned_candidate(root, "Other").persisted_key();
    db.call(move |conn| {
        conn.execute(
            "UPDATE folder_scan_entries SET entry_key = ?",
            params![other_key],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    assert!(db.load_folder_scan_snapshots().await.is_err());

    // Key restored, so only the generation is now wrong.
    let item_key = item.persisted_key();
    db.call(move |conn| {
        conn.execute(
            "UPDATE folder_scan_entries SET entry_key = ?, generation = ?",
            params![item_key, i64::try_from(generation + 1).unwrap()],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    assert!(db.load_folder_scan_snapshots().await.is_err());
}

/// A disc assignment the user set survives a relaunch: it is stored under
/// the candidate's content hash, read back from a cold database, and the
/// scan that follows lays the discs down as they settled them rather than
/// in the order the cue filenames read.
#[tokio::test]
async fn a_disc_assignment_survives_a_relaunch() {
    use crate::import::folder_scanner::{
        collect_release_candidate_files_with_scope, CandidateFileEdits, SheetDisc, SheetDiscEdits,
        StoredCandidateEdits,
    };

    let (db, _tmp) = empty_db().await;
    let folder = two_sheet_folder();
    let scanned = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
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
        &[],
    )
    .await
    .unwrap();
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();

    // The rip named its sheets the other way round: `alpha.cue` is disc two.
    let mut sheet_discs = SheetDiscEdits::default();
    sheet_discs.set("alpha.cue".to_string(), SheetDisc::Disc { number: 2 });
    sheet_discs.set("beta.cue".to_string(), SheetDisc::Disc { number: 1 });
    let candidate_edits = CandidateFileEdits {
        sheet_discs,
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
        current.sheet_discs.get("alpha.cue"),
        Some(SheetDisc::Disc { number: 2 })
    );

    // A subsequent scan reads the same decisions, so the folder's audio
    // comes out in the order the user settled rather than in path order.
    let stored = db.load_stored_candidate_edits().await.unwrap();
    let reopened = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::import::ReleaseFileScope::Recursive,
        &stored,
    )
    .unwrap();
    assert_eq!(
        reopened
            .carving_sheets()
            .iter()
            .map(|sheet| (sheet.file.relative_path.as_str(), sheet.disc))
            .collect::<Vec<_>>(),
        vec![
            ("alpha.cue", SheetDisc::Disc { number: 2 }),
            ("beta.cue", SheetDisc::Disc { number: 1 }),
        ],
    );
}

/// Two bound single-track sheets, each naming the audio beside it.
fn two_sheet_folder() -> tempfile::TempDir {
    let tmp = tempfile::TempDir::new().unwrap();
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for stem in ["alpha", "beta"] {
        std::fs::copy(
            fixtures.join("tests/fixtures/cue_flac/Test Album.flac"),
            tmp.path().join(format!("{stem}.flac")),
        )
        .unwrap();
        std::fs::write(
            tmp.path().join(format!("{stem}.cue")),
            format!(
                "PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\n\
                 FILE \"{stem}.flac\" WAVE\n  TRACK 01 AUDIO\n    \
                 TITLE \"Track Title\"\n    INDEX 01 00:00:00\n",
            ),
        )
        .unwrap();
    }
    tmp
}

/// The walkthrough folder on disk: a twelve-track sheet written against a
/// WAV, the FLAC it was actually encoded to, and the rip log.
fn walkthrough_folder() -> tempfile::TempDir {
    let tmp = tempfile::TempDir::new().unwrap();
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::copy(
        fixtures.join("tests/fixtures/cue_flac/Test Album.flac"),
        tmp.path().join("cd.flac"),
    )
    .unwrap();
    std::fs::copy(
        fixtures.join("tests/fixtures/test_album.log"),
        tmp.path().join("rip.log"),
    )
    .unwrap();
    let mut cue =
        String::from("PERFORMER \"Test Artist\"\nTITLE \"Album\"\nFILE \"cd.wav\" WAVE\n");
    for track in 1..=12 {
        cue.push_str(&format!(
            "  TRACK {track:02} AUDIO\n    TITLE \"Track {track:02}\"\n    INDEX 01 {:02}:00:00\n",
            (track - 1) * 5,
        ));
    }
    std::fs::write(tmp.path().join("cd.cue"), cue).unwrap();
    tmp
}

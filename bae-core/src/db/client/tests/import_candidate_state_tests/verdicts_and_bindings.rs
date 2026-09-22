use super::super::*;
use crate::identify::{
    DiscIdFile, DiscIdFileKind, DiscIdStepView, IdentifyRunView, LookupProvenance, LookupView,
    TerminalVerdict,
};
use crate::import::folder_scanner::{CandidateFile, CategorizedFiles, FileRole, ScannedFile};
use crate::import::search::MetadataResult;
use crate::import::watched_folder::host_root;
use coven::FixedClock;
use std::path::PathBuf;

/// The instant `empty_db`'s injected clock always returns. Fixed rather
/// than `SystemClock` so `identified_at` can be asserted exactly — which is
/// why `CandidatePreparations::store_verdict` stamps it from the injected clock
/// instead of taking it from the caller.
/// A folder of plain track files (no track sheet) named
/// `(relative_path, size)`.
fn track_files_candidate(files: &[(&str, u64)]) -> CategorizedFiles {
    CategorizedFiles {
        files: files
            .iter()
            .map(|(name, size)| CandidateFile {
                file: ScannedFile::new(PathBuf::from(*name), name.to_string(), *size, 1)
                    .with_test_flac_audio(),
                role: FileRole::Audio,
                proposed_audio: true,
            })
            .collect(),
    }
}

/// The ledger the sample verdict's run recorded: one disc ID, read off a rip
/// log, that named the release the verdict settled on.
fn sample_ledger() -> IdentifyRunView {
    IdentifyRunView {
        providers: vec![Catalog::MusicBrainz],
        disc_id: DiscIdStepView::Read {
            disc_id: "disc-1".to_string(),
            source: Some(DiscIdFile {
                kind: DiscIdFileKind::Log,
                file: "rip/Album.LOG".to_string(),
            }),
            lookup: LookupView::Found {
                count: 1,
                groups: crate::import::release_group::group_results(
                    crate::import::release_group::unranked(vec![sample_match()]),
                ),
            },
        },
        barcode: crate::identify::BarcodeStepView::Absent,
        catalog: crate::identify::CatalogStepView::NoneFound,
        search: crate::identify::SearchStepView::NotNeeded,
    }
}

fn sample_match() -> MetadataResult {
    MetadataResult {
        source: Catalog::MusicBrainz,
        release_id: "rel-1".to_string(),
        title: "Album".to_string(),
        artist: Some("Artist".to_string()),
        year: Some(1999),
        format: Some("CD".to_string()),
        label: Some("Label".to_string()),
        catalog_number: Some("CAT-1".to_string()),
        country: Some("US".to_string()),
        barcodes: Vec::new(),
        media: crate::import::search::StatedMedia::Undescribed,
        links: Vec::new(),
        cover_art: None,
        source_group_id: Some("group-1".to_string()),
        source_tracks: None,
    }
}

fn sample_verdict() -> TerminalVerdict {
    TerminalVerdict::Found {
        matches: vec![sample_match()],
        track_count: 11,
        provenance: vec![LookupProvenance {
            by_disc_id: true,
            by_barcode: true,
            by_catalog: true,
            by_search: false,
        }],
        pressings: vec![0],
        narrowed_out: Vec::new(),
        narrowed_out_provenance: Vec::new(),
        narrowed_out_pressings: Vec::new(),
        ledger: Some(sample_ledger()),
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
        text_pool: Vec::new(),
        durations: crate::import::probe::SourceDurations::totalling(probed_total_duration_ms),
    }
}

/// The same row, concluding one release: what a run that settled writes, and
/// the only shape that replaces the draft it lands on.
fn concluding(mut row: NewImportCandidateVerdict, release_id: &str) -> NewImportCandidateVerdict {
    row.metadata = Some(crate::import::CandidateMetadataDraft {
        draft: candidate_draft("", ""),
        source_discogs_artist_ids: Default::default(),
        provenance: Some(release_pick(release_id)),
        cover: None,
        assets: crate::import::CandidatePreparedAssets::default(),
    });
    row
}

fn new_candidate_row(
    content_hash: &str,
    folder_path: &str,
    verdict: &TerminalVerdict,
    probed_total_duration_ms: u64,
) -> NewImportCandidateVerdict {
    NewImportCandidateVerdict {
        candidate: as_read(content_hash, 0),
        folder_path: folder_path.to_string(),
        verdict: verdict.clone(),
        signals: sample_signals(probed_total_duration_ms),
        metadata: None,
    }
}

/// Save a verdict, read it back, and check the provenance and the run's
/// ledger survived the JSON round trip along with everything else — a
/// stripped `by_disc_id`, a dropped catalog number, or a ledger cell that
/// lost its release cards wouldn't show up in a looser comparison.
#[tokio::test]
async fn round_trip_preserves_the_verdict_including_provenance() {
    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let verdict = sample_verdict();
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict, 2_700_000);
    store_candidate_state(&db, &candidate, &row.folder_path).await;

    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    let loaded_row = loaded
        .get(&hash)
        .expect("row present under its content hash");
    assert_eq!(loaded_row.folder_path, host_root("/music/Some Album"));
    let identify = loaded_row
        .identify
        .as_ref()
        .expect("a stored verdict reads back as an identify result");
    assert_eq!(identify.probed_total_duration_ms, 2_700_000);
    // Stamped by the write path from the injected clock, not something
    // `new_candidate_row` had any way to supply.
    assert_eq!(identify.identified_at, fixed_now());
    assert_eq!(
        identify.verdict, verdict,
        "the verdict must round-trip exactly, provenance included"
    );
}

/// Every barcode, every medium entry — stated or not — and every link a
/// match carries store and read back, so the rows a stored verdict groups
/// into are the rows the run grouped into.
#[tokio::test]
async fn round_trip_preserves_the_evidence_the_rows_are_paired_by() {
    use crate::import::search::StatedMedia;
    use crate::import::{Catalog, MetadataRef};

    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let mut musicbrainz = sample_match();
    musicbrainz.barcodes = vec!["012345678905".to_string()];
    musicbrainz.media = StatedMedia::PerMedium(vec![Some("CD".to_string()), None]);
    musicbrainz.links = vec![
        MetadataRef::new(Catalog::Discogs, "42"),
        MetadataRef::new(Catalog::Discogs, "43"),
    ];
    let mut discogs = sample_match();
    discogs.source = Catalog::Discogs;
    discogs.release_id = "42".to_string();
    discogs.source_group_id = Some("7".to_string());
    discogs.barcodes = vec!["0 12345 67890 5".to_string(), "5051961234567".to_string()];
    discogs.media = StatedMedia::Descriptors(vec!["CD".to_string(), "Album".to_string()]);
    let mut undescribed = sample_match();
    undescribed.release_id = "rel-2".to_string();
    undescribed.year = Some(2001);
    undescribed.media = StatedMedia::Undescribed;
    let matches = vec![musicbrainz, discogs, undescribed];
    let verdict = TerminalVerdict::Found {
        provenance: matches
            .iter()
            .map(|_| LookupProvenance {
                by_disc_id: true,
                by_barcode: false,
                by_catalog: false,
                by_search: false,
            })
            .collect(),
        pressings: crate::import::release_group::form_rows(&matches),
        matches: matches.clone(),
        track_count: 11,
        narrowed_out: Vec::new(),
        narrowed_out_provenance: Vec::new(),
        narrowed_out_pressings: Vec::new(),
        ledger: None,
    };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict, 2_700_000);
    store_candidate_state(&db, &candidate, &row.folder_path).await;
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    let stored = &loaded[&hash]
        .identify
        .as_ref()
        .expect("a stored verdict reads back")
        .verdict;
    assert_eq!(*stored, verdict);
    let TerminalVerdict::Found {
        matches: stored_matches,
        ..
    } = stored
    else {
        panic!("the verdict found releases");
    };
    let live = crate::import::release_group::group_results(crate::import::release_group::unranked(
        matches.clone(),
    ));
    let replayed = crate::import::release_group::group_results(
        crate::import::release_group::unranked(stored_matches.clone()),
    );
    assert_eq!(replayed, live);
    assert_eq!(live.len(), 1, "the link joins the two groups on one card");
    assert_eq!(
        live[0]
            .pressings
            .iter()
            .map(|pressing| pressing.releases.len())
            .collect::<Vec<_>>(),
        vec![2, 1],
        "the linked pair is one row, the other release its own"
    );
    assert_eq!(crate::import::release_group::pressing_count(matches), 2);
}

/// The candidate's own text stores and reads back whole — every line, in the
/// order the pass read it, each still naming where it came from. It is what
/// the rows are judged and ordered against, so a resumed candidate has to be
/// able to say exactly what it said while the run went.
#[tokio::test]
async fn the_candidate_s_text_round_trips_line_by_line() {
    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let pool = vec![
        crate::signals::TextLine {
            text: "AC-DC - Dirty Deeds Done Dirt Cheap [16033-2]".to_string(),
            origin: crate::signals::SignalOrigin::FolderName,
            file: None,
            region: None,
        },
        crate::signals::TextLine {
            text: "Atlantic Records, Inc.".to_string(),
            origin: crate::signals::SignalOrigin::Artwork,
            file: Some("back.jpg".to_string()),
            region: crate::signals::ImageRegion::new(0.1, 0.2, 0.3, 0.4),
        },
    ];
    let mut row = new_candidate_row(
        &hash,
        &host_root("/music/Some Album"),
        &sample_verdict(),
        2_700_000,
    );
    row.signals.text_pool = pool.clone();
    store_candidate_state(&db, &candidate, &row.folder_path).await;

    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    let stored = loaded
        .get(&hash)
        .expect("row present under its content hash")
        .signals
        .as_ref()
        .expect("a stored verdict reads its signals back");
    assert_eq!(stored.text_pool, pool);
}

/// A verdict recorded with no ledger reads back with none: the column is
/// empty, and the pane draws the settled lists without a run beside them.
#[tokio::test]
async fn a_verdict_with_no_ledger_reads_back_without_one() {
    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let TerminalVerdict::Found {
        matches,
        track_count,
        provenance,
        pressings,
        narrowed_out,
        narrowed_out_provenance,
        narrowed_out_pressings,
        ..
    } = sample_verdict()
    else {
        panic!("the sample verdict is a found one");
    };
    let verdict = TerminalVerdict::Found {
        matches,
        track_count,
        provenance,
        pressings,
        narrowed_out,
        narrowed_out_provenance,
        narrowed_out_pressings,
        ledger: None,
    };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict, 2_700_000);
    store_candidate_state(&db, &candidate, &row.folder_path).await;

    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    let identify = loaded
        .get(&hash)
        .expect("row present under its content hash")
        .identify
        .as_ref()
        .expect("a stored verdict reads back as an identify result");
    assert_eq!(identify.verdict, verdict);
}

/// The releases agreement narrowed out are stored under the same verdict as
/// its matches and read back apart from them: they are what the run rejected,
/// never what it settled on, so a reader that merged the two lists would
/// offer a rejected release as an answer.
#[tokio::test]
async fn a_verdict_round_trips_its_narrowed_out_releases_apart_from_its_matches() {
    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let TerminalVerdict::Found {
        matches,
        track_count,
        provenance,
        pressings,
        ..
    } = sample_verdict()
    else {
        panic!("the sample verdict is a found one");
    };
    let mut left_out = matches[0].clone();
    left_out.release_id = "rel-narrowed".to_string();
    let verdict = TerminalVerdict::Found {
        matches,
        track_count,
        provenance,
        pressings,
        narrowed_out: vec![left_out],
        narrowed_out_provenance: vec![LookupProvenance {
            by_disc_id: true,
            by_barcode: false,
            by_catalog: false,
            by_search: false,
        }],
        narrowed_out_pressings: vec![0],
        ledger: Some(sample_ledger()),
    };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict, 2_700_000);
    store_candidate_state(&db, &candidate, &row.folder_path).await;

    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    let identify = loaded
        .get(&hash)
        .expect("row present under its content hash")
        .identify
        .as_ref()
        .expect("a stored verdict reads back as an identify result");
    assert_eq!(identify.verdict, verdict);
    let TerminalVerdict::Found {
        matches,
        narrowed_out,
        ..
    } = &identify.verdict
    else {
        panic!("a found verdict reads back as one");
    };
    assert_eq!(
        matches
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["rel-1"]
    );
    assert_eq!(
        narrowed_out
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["rel-narrowed"]
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
        &host_root("/music/Some Album"),
        &sample_verdict(),
        2_700_000,
    );
    store_candidate_state(&db, &original, &row.folder_path).await;
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

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
        record: crate::import::MetadataRef::new(
            crate::import::Catalog::MusicBrainz,
            release_id.to_string(),
        ),
        partners: vec![],
    }
}

#[tokio::test]
async fn every_metadata_provenance_variant_survives_a_database_reopen() {
    let (db, tmp) = empty_db().await;
    let cases = [
        (
            track_files_candidate(&[("01 Track.flac", 100_001)]).content_hash(),
            &host_root("/music/Candidate A"),
            100_001,
            release_pick("release-a"),
        ),
        (
            track_files_candidate(&[("01 Track.flac", 100_002)]).content_hash(),
            &host_root("/music/Candidate B"),
            100_002,
            crate::import::MetadataProvenance::FileMetadata,
        ),
    ];

    for (content_hash, folder_path, size, provenance) in &cases {
        let candidate = track_files_candidate(&[("01 Track.flac", *size)]);
        assert_eq!(&candidate.content_hash(), content_hash);
        store_candidate_state(&db, &candidate, folder_path).await;
        let row = new_candidate_row(content_hash, folder_path, &sample_verdict(), 1_000);
        crate::import::CandidatePreparations::new(db.clone())
            .store_verdict(&row)
            .await
            .unwrap();
        crate::import::CandidatePreparations::new(db.clone())
            .replace_metadata(
                content_hash,
                folder_path,
                &metadata_draft("Album", "Artist"),
                Some(provenance),
            )
            .await
            .unwrap();
    }
    drop(db);

    let path = tmp.path().join("test.db");
    let reopened = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(FixedClock(fixed_now())),
        Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let loaded = reopened.load_import_candidate_states().await.unwrap();

    for (content_hash, _, _, provenance) in &cases {
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
    TerminalVerdict::NotFoundAnywhere { ledger: None }
}

/// A re-run that settles on no release replaces the result and nothing else.
/// The draft it lands on stands whoever wrote it — an earlier run included —
/// because "we looked again and found nothing" says what the candidate is not,
/// and a release already named is not unmade by that.
#[tokio::test]
async fn a_re_run_that_finds_nothing_leaves_the_draft_an_earlier_run_wrote() {
    let (db, _tmp) = empty_db().await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = store_candidate_state(&db, &candidate, &host_root("/music/Album")).await;

    let settled = concluding(
        new_candidate_row(
            &hash,
            &host_root("/music/Album"),
            &sample_verdict(),
            2_700_000,
        ),
        "mb-rel-1",
    );
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&settled)
        .await
        .unwrap();

    let mut re_run = new_candidate_row(
        &hash,
        &host_root("/music/Album"),
        &found_nothing(),
        2_700_000,
    );
    re_run.candidate.metadata_revision = 1;
    assert!(re_run.metadata.is_none(), "nothing was found to pick");
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&re_run)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the row is still there")
            .metadata_provenance,
        Some(release_pick("mb-rel-1")),
        "a run that concluded no release wrote no draft"
    );
}

/// A run that found nothing says what the candidate is not. That is no reason
/// to unmake a release somebody chose, so the choice stands and the pane
/// reopens on it.
#[tokio::test]
async fn a_re_run_that_finds_nothing_leaves_a_person_s_pick_alone() {
    let (db, _tmp) = empty_db().await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = store_candidate_state(&db, &candidate, &host_root("/music/Album")).await;

    let initial = new_candidate_row(
        &hash,
        &host_root("/music/Album"),
        &found_nothing(),
        2_700_000,
    );
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&initial)
        .await
        .unwrap();
    crate::import::CandidatePreparations::new(db.clone())
        .replace_metadata(
            &hash,
            &host_root("/music/Album"),
            &metadata_draft("Album", "Artist"),
            Some(&release_pick("mb-rel-chosen")),
        )
        .await
        .unwrap();

    let mut re_run = new_candidate_row(
        &hash,
        &host_root("/music/Album"),
        &found_nothing(),
        2_700_000,
    );
    re_run.candidate.metadata_revision = 2;
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&re_run)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the row is still there")
            .metadata_provenance,
        Some(release_pick("mb-rel-chosen")),
        "a verdict that concluded no release must not unmake a choice a person made"
    );
}

/// And a re-run that settles on a *different* single match replaces the
/// pick its predecessor made, rather than leaving the older release named.
#[tokio::test]
async fn a_re_run_that_settles_elsewhere_replaces_the_pick_it_made() {
    let (db, _tmp) = empty_db().await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = store_candidate_state(&db, &candidate, &host_root("/music/Album")).await;

    let first = concluding(
        new_candidate_row(
            &hash,
            &host_root("/music/Album"),
            &sample_verdict(),
            2_700_000,
        ),
        "mb-rel-first",
    );
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&first)
        .await
        .unwrap();

    let mut second = concluding(
        new_candidate_row(
            &hash,
            &host_root("/music/Album"),
            &sample_verdict(),
            2_700_000,
        ),
        "mb-rel-second",
    );
    second.candidate.metadata_revision = 1;
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&second)
        .await
        .unwrap();

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

/// File decisions invalidate identification results, while applied metadata
/// keeps its provenance regardless of who applied it.
#[tokio::test]
async fn a_file_decision_preserves_applied_metadata_from_either_author() {
    let (db, _tmp) = empty_db().await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = candidate.content_hash();
    let edits = crate::import::folder_scanner::CandidateFileEdits::default();
    store_candidate_state(&db, &candidate, &host_root("/music/Album")).await;

    let settled = concluding(
        new_candidate_row(
            &hash,
            &host_root("/music/Album"),
            &sample_verdict(),
            2_700_000,
        ),
        "mb-rel-derived",
    );
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&settled)
        .await
        .unwrap();
    let (metadata_revision, mapping_preparation) = current_mapping_preparation(&db, &hash).await;
    crate::import::CandidatePreparations::new(db.clone())
        .store_file_decisions(
            &as_read(&hash, metadata_revision),
            &host_root("/music/Album"),
            &edits,
            &[(host_root("/music/Album"), candidate.clone())],
            &mapping_preparation,
        )
        .await
        .unwrap();
    let loaded = db.load_import_candidate_states().await.unwrap();
    let row = loaded.get(&hash).expect("the row is still there");
    assert!(row.identify.is_none(), "the decision clears the verdict");
    assert_eq!(
        row.metadata_provenance,
        Some(release_pick("mb-rel-derived")),
        "the applied metadata keeps its source"
    );

    crate::import::CandidatePreparations::new(db.clone())
        .replace_metadata(
            &hash,
            &host_root("/music/Album"),
            &metadata_draft("Album", "Artist"),
            Some(&release_pick("mb-rel-chosen")),
        )
        .await
        .unwrap();
    let (metadata_revision, mapping_preparation) = current_mapping_preparation(&db, &hash).await;
    crate::import::CandidatePreparations::new(db.clone())
        .store_file_decisions(
            &crate::import::CandidateAsRead {
                content_hash: hash.clone(),
                file_edit_revision: 1,
                metadata_revision,
            },
            &host_root("/music/Album"),
            &edits,
            &[(host_root("/music/Album"), candidate.clone())],
            &mapping_preparation,
        )
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
        &host_root("/music/Old Location/Some Album"),
        &sample_verdict(),
        2_700_000,
    );
    store_candidate_state(&db, &at_old_location, &row.folder_path).await;
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let mut at_new_location = at_old_location.clone();
    for entry in &mut at_new_location.files {
        entry.file.path = PathBuf::from(host_root("/music/New Location/Some Album"))
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

/// A transport failure is a stored terminal answer rather than absence that
/// an automatic sweep interprets as permission to retry.
#[tokio::test]
async fn a_transport_failure_round_trips_as_a_failed_verdict() {
    use crate::identify::state::step as identify_step;
    use crate::identify::{IdentifyEvent, IdentifyState};
    use crate::signals::{BarcodeSignal, DiscIdSignal, LookupFailure, Signals, TextSignal};

    let (db, _tmp) = empty_db().await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = candidate.content_hash();
    store_candidate_state(&db, &candidate, &host_root("/music/Some Album")).await;

    let (state, _) = identify_step(
        IdentifyState::Idle,
        IdentifyEvent::Started {
            providers: vec![crate::import::Catalog::MusicBrainz],
            choices: crate::import::LookupChoices::default(),
            title_search: None,
        },
    );
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
                text_pool: Vec::new(),
                durations: crate::import::probe::SourceDurations::default(),
            },
            artwork: crate::signals::ArtworkScan::Absent,
        },
    );
    let (state, _) = identify_step(
        state,
        IdentifyEvent::DiscidLookupFailed {
            failure: LookupFailure::Provider { status: Some(503) },
            track_count: 1,
        },
    );

    let verdict = TerminalVerdict::try_from(state).expect("the failure is terminal");
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict, 0);
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    let loaded = loaded
        .get(&hash)
        .and_then(|state| state.identify.as_ref())
        .expect("the failed verdict is stored");
    assert_eq!(
        loaded.verdict, verdict,
        "the provider failure survives the database boundary"
    );
}

include!("bindings.rs");

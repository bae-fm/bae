mod preparation;
// What the pane stores under a candidate: measured durations, the settled
// signals, the failure an import left, the cover, and the metadata and track
// rows the user typed.

use crate::import::folder_scanner::{CandidateFileEdits, FileRoleChoice};
use crate::import::probe::{SourceDuration, SourceDurations};
use crate::import::{
    ArtistAssignment, AudioFile, CandidateEditField, CandidateTrackEdit, CoverSelection,
    ExistingArtist, ImportFailure, NewArtistSeed, RawPressingEdit, RawReleaseEdit, RawTrackEdit,
    TrackArtistAssignments,
};
use crate::signals::{
    BarcodeSignal, DiscIdSignal, LookupFailure, SignalOrigin, Signals, SourcedValue, TextSignal,
};

#[path = "pane_rows/artist_identity_conflicts.rs"]
mod artist_identity_conflicts;

fn pane_candidate() -> CategorizedFiles {
    track_files_candidate(&[("01 Track.flac", 111), ("CDImage.flac", 222)])
}
fn pane_candidate_path() -> String {
    PathBuf::from(host_root("/music"))
        .join("Album")
        .to_string_lossy()
        .into_owned()
}

fn file_unit(file_id: &str, duration_ms: u64) -> SourceDuration {
    SourceDuration {
        audio: AudioFile::Standalone {
            file_id: file_id.to_string(),
        },
        duration_ms,
    }
}

fn slice_unit(index: u32, duration_ms: u64) -> SourceDuration {
    SourceDuration {
        audio: AudioFile::SheetSlice {
            file_id: "CDImage.flac".to_string(),
            sheet_id: "CDImage.cue".to_string(),
            index,
        },
        duration_ms,
    }
}

fn signals_with(durations: SourceDurations) -> Signals {
    Signals {
        disc_id: DiscIdSignal::Absent { track_count: 2 },
        barcode: BarcodeSignal::Absent,
        text: TextSignal::Settled {
            catalogs: Vec::new(),
            free_text: Vec::new(),
        },
        durations,
    }
}

/// Store a verdict for `hash` carrying `signals`, and say whether it landed.
async fn store_verdict(db: &Database, hash: &str, signals: Signals) -> bool {
    db.save_import_candidate_verdict(&NewImportCandidateVerdict {
        content_hash: hash.to_string(),
        folder_path: pane_candidate_path(),
        verdict: sample_verdict(),
        signals,
        expected_edit_revision: 0,
        expected_metadata_revision: 0,
        metadata: crate::import::CandidateMetadataDraft {
            edit: crate::import::pane::blank_candidate_draft(&pane_candidate()),
            track_mappings: crate::import::pane::blank_candidate_source(&pane_candidate())
                .track_mappings,
            source_discogs_artist_ids: Default::default(),
            provenance: None,
            cover: None,
            assets: crate::import::CandidatePreparedAssets::default(),
        },
    })
    .await
    .unwrap()
}

fn edited_row(id: &str, title: &str, file: Option<AudioFile>) -> CandidateTrackEdit {
    CandidateTrackEdit::edited(RawTrackEdit {
        id: id.to_string(),
        title: title.to_string(),
        artist_assignments: TrackArtistAssignments::Explicit(vec![new_artist("Artist Name")]),
        side: 1,
        track_number: Some(1),
        file,
    })
}

fn new_artist(name: &str) -> ArtistAssignment {
    ArtistAssignment::New {
        seed: NewArtistSeed {
            name: name.to_string(),
            sort_name: None,
            musicbrainz_artist_id: None,
            discogs_artist_id: None,
        },
    }
}

fn existing_artist() -> DbArtist {
    DbArtist {
        id: bae_test_support::test_uuid("library-artist"),
        name: "Library Artist".to_string(),
        sort_name: Some("Artist, Library".to_string()),
        discogs_artist_id: Some("discogs-library".to_string()),
        musicbrainz_artist_id: Some("mb-library".to_string()),
        created_at: fixed_identified_at(),
    }
}

async fn stored_pane_candidate(db: &Database) -> (CategorizedFiles, String) {
    let root = host_root("/music");
    let item = scanned_candidate(&root, "Album");
    let crate::import::folder_scanner::ScanItem::Valid(candidate) = &item else {
        unreachable!("the fixture creates a valid candidate")
    };
    let files = candidate.files.clone();
    let hash = files.content_hash();
    db.add_watched_import_folder(&root).await.unwrap();
    let generation = db.begin_folder_scan(&root).await.unwrap();
    db.save_folder_scan_item(&root, generation, &item)
        .await
        .unwrap()
        .expect("the current scan accepts the candidate");
    (files, hash)
}

async fn store_candidate_state(
    db: &Database,
    files: &CategorizedFiles,
    folder_path: &str,
) -> String {
    use crate::import::folder_scanner::{FolderCandidate, ReleaseFileScope, ScanItem};

    let path = PathBuf::from(folder_path);
    let root = path
        .parent()
        .expect("the candidate fixture has a watched root")
        .to_string_lossy()
        .into_owned();
    let name = path
        .file_name()
        .expect("the candidate fixture has a folder name")
        .to_string_lossy()
        .into_owned();
    let item = ScanItem::Valid(FolderCandidate {
        path: path.clone(),
        file_root: path,
        name: name.clone(),
        files: files.clone(),
        watched_folder_path: root.clone(),
        scope: ReleaseFileScope::Direct,
        file_edit_revision: 0,
        display_path: name,
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    });
    let hash = files.content_hash();
    db.add_watched_import_folder(&root).await.unwrap();
    let generation = db.begin_folder_scan(&root).await.unwrap();
    db.save_folder_scan_item(&root, generation, &item)
        .await
        .unwrap()
        .expect("the current scan accepts the candidate");
    hash
}

fn metadata_draft(title: &str, artist: &str) -> RawReleaseEdit {
    RawReleaseEdit {
        album_title: title.to_string(),
        album_artist_assignments: if artist.is_empty() {
            Vec::new()
        } else {
            vec![new_artist(artist)]
        },
        album_year: String::new(),
        pressing: RawPressingEdit {
            year: String::new(),
            format: String::new(),
            label: String::new(),
            catalog_number: String::new(),
            country: String::new(),
            barcode: String::new(),
        },
        tracks: vec![RawTrackEdit {
            id: "candidate-track-0".to_string(),
            title: "Track title".to_string(),
            artist_assignments: TrackArtistAssignments::AlbumArtists,
            side: 1,
            track_number: Some(1),
            file: None,
        }],
    }
}

#[tokio::test]
async fn a_verdict_cannot_create_state_for_an_absent_candidate() {
    let (db, _tmp) = empty_db().await;
    let hash = pane_candidate().content_hash();

    assert!(!store_verdict(&db, &hash, signals_with(SourceDurations::default())).await);
    assert!(db
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .is_none());
}

/// The verdict stores its derived total without duplicating per-file scan facts.
#[tokio::test]
async fn verdict_stores_only_the_derived_total() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let durations = SourceDurations::new(vec![
        file_unit("01 Track.flac", 180_000),
        file_unit("CDImage.flac", 600_000),
        slice_unit(0, 200_000),
        slice_unit(1, 400_000),
    ]);

    assert!(store_verdict(&db, &hash, signals_with(durations.clone())).await);

    let state = db
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .expect("the verdict wrote a row");
    assert!(state.signals.unwrap().durations.units.is_empty());
    assert_eq!(
        state
            .identify
            .expect("the verdict reads back")
            .probed_total_duration_ms,
        780_000,
        "the column is the sum the same write derived"
    );
}

/// Every settled shape of every signal comes back as it went in, including
/// each way a lookup can fail.
#[tokio::test]
async fn every_settled_signal_shape_round_trips() {
    let cases: Vec<(&str, DiscIdSignal, BarcodeSignal, TextSignal)> = vec![
        (
            "computed disc ID, settled barcodes and text",
            DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                track_count: 11,
                // The rip log it was derived from rides with it.
                source_file: Some("rip.log".to_string()),
            },
            BarcodeSignal::Settled {
                codes: vec![
                    // The image OCR read it off rides with it, so a surface can
                    // put the barcode on that image.
                    SourcedValue::in_file(
                        "0123456789012".to_string(),
                        SignalOrigin::Artwork,
                        "Scans/back.jpg".to_string(),
                    ),
                    SourcedValue::new("9876543210987".to_string(), SignalOrigin::CueSheet),
                ],
            },
            TextSignal::Settled {
                catalogs: vec![SourcedValue::new(
                    "CAT-1".to_string(),
                    SignalOrigin::FolderName,
                )],
                free_text: vec!["Album Title".to_string(), "Artist Name".to_string()],
            },
        ),
        (
            "absent everywhere",
            DiscIdSignal::Absent { track_count: 0 },
            BarcodeSignal::Absent,
            TextSignal::Settled {
                catalogs: Vec::new(),
                free_text: Vec::new(),
            },
        ),
        (
            "a network failure and a provider one",
            DiscIdSignal::Failed {
                failure: LookupFailure::Network,
                track_count: 3,
            },
            BarcodeSignal::Failed {
                failure: LookupFailure::Provider { status: Some(503) },
                codes: vec![SourcedValue::new(
                    "0123456789012".to_string(),
                    SignalOrigin::Filename,
                )],
            },
            TextSignal::Failed {
                failure: LookupFailure::Timeout,
                catalogs: vec![SourcedValue::new(
                    "CAT-2".to_string(),
                    SignalOrigin::TextFile,
                )],
                free_text: vec!["Some Line".to_string()],
            },
        ),
        (
            "a diagnostic, an artwork failure, and a provider with no status",
            DiscIdSignal::Failed {
                failure: LookupFailure::Diagnostic {
                    detail: "the release was not found".to_string(),
                },
                track_count: 1,
            },
            BarcodeSignal::Failed {
                failure: LookupFailure::ArtworkAnalysis,
                codes: Vec::new(),
            },
            TextSignal::Failed {
                failure: LookupFailure::Provider { status: None },
                catalogs: Vec::new(),
                free_text: Vec::new(),
            },
        ),
    ];

    for (what, disc_id, barcode, text) in cases {
        let (db, _tmp) = empty_db().await;
        let (_, hash) = stored_pane_candidate(&db).await;
        let signals = Signals {
            disc_id,
            barcode,
            text,
            durations: SourceDurations::new(vec![file_unit("01 Track.flac", 1_000)]),
        };

        assert!(store_verdict(&db, &hash, signals.clone()).await, "{what}");

        let stored = db
            .load_import_candidate_state(&hash)
            .await
            .unwrap()
            .unwrap()
            .signals
            .unwrap_or_else(|| panic!("{what}: the signals read back"));
        assert_eq!(
            stored,
            Signals {
                durations: SourceDurations::default(),
                ..signals
            },
            "{what}"
        );
    }
}

/// A signal still scanning is artwork OCR mid-flight, which no verdict can
/// have reached. The write refuses it rather than storing a half-read
/// extraction, and the whole transaction — verdict included — rolls back.
#[tokio::test]
async fn a_scanning_signal_is_refused_and_writes_nothing() {
    for scanning in [
        Signals {
            disc_id: DiscIdSignal::Absent { track_count: 0 },
            barcode: BarcodeSignal::Scanning { codes: Vec::new() },
            text: TextSignal::Settled {
                catalogs: Vec::new(),
                free_text: Vec::new(),
            },
            durations: SourceDurations::default(),
        },
        Signals {
            disc_id: DiscIdSignal::Absent { track_count: 0 },
            barcode: BarcodeSignal::Absent,
            text: TextSignal::Scanning {
                catalogs: Vec::new(),
                free_text: Vec::new(),
            },
            durations: SourceDurations::default(),
        },
    ] {
        let (db, _tmp) = empty_db().await;
        let candidate = pane_candidate();
        let hash = store_candidate_state(&db, &candidate, &pane_candidate_path()).await;

        let error = db
            .save_import_candidate_verdict(&NewImportCandidateVerdict {
                content_hash: hash.clone(),
                folder_path: pane_candidate_path(),
                verdict: sample_verdict(),
                signals: scanning,
                expected_edit_revision: 0,
                expected_metadata_revision: 0,
                metadata: crate::import::CandidateMetadataDraft {
                    edit: crate::import::pane::blank_candidate_draft(&pane_candidate()),
                    track_mappings: Default::default(),
                    source_discogs_artist_ids: Default::default(),
                    provenance: None,
                    cover: None,
                    assets: crate::import::CandidatePreparedAssets::default(),
                },
            })
            .await
            .expect_err("a scanning signal is not storable");
        assert!(
            error.to_string().contains("still scanning"),
            "{error} should name what was refused"
        );
        let state = db
            .load_import_candidate_state(&hash)
            .await
            .unwrap()
            .expect("the discovered candidate remains");
        assert!(state.identify.is_none(), "the refused write left no verdict");
    }
}

/// A failed import is recorded on the candidate draft that was created during
/// discovery. Queueing the next attempt clears it.
#[tokio::test]
async fn a_failure_on_a_discovered_candidate_is_replaced_then_cleared() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;

    db.save_import_candidate_failure(
        &hash,
        0,
        &ImportFailure::error_only("the folder vanished", fixed_identified_at()),
    )
    .await
    .unwrap();

    let failure = db
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .failure
        .expect("the failure is stored");
    assert_eq!(failure.error, "the folder vanished");
    assert_eq!(failure.failed_at, fixed_identified_at());
    assert!(db
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .metadata_draft
        .is_blank());

    db.save_import_candidate_failure(
        &hash,
        0,
        &ImportFailure::error_only("the disc would not read", fixed_identified_at()),
    )
    .await
    .unwrap();
    assert_eq!(
        db.load_import_candidate_pane_rows(&hash)
            .await
            .unwrap()
            .failure
            .unwrap()
            .error,
        "the disc would not read",
        "the second failure replaces the first"
    );

    db.clear_import_candidate_failure(&hash).await.unwrap();
    assert!(db
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .failure
        .is_none());
}

#[tokio::test]
async fn an_active_import_omits_its_previous_persisted_failure_from_the_detail() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    db.save_import_candidate_failure(
        &hash,
        0,
        &ImportFailure::error_only("the prior attempt failed", fixed_identified_at()),
    )
    .await
    .unwrap();

    let key = pane_candidate_path();
    let detail = db
        .load_import_candidate(&key)
        .await
        .unwrap()
        .expect("the stored candidate has a detail")
        .resolve(&crate::import::TriageRuntimeFacts {
            identification: None,
            importing: true,
        });

    assert!(matches!(
        detail.row.import_status,
        Some(crate::import::TriageImportStatus::Importing)
    ));
    assert!(detail.failure.is_none());
}

/// Both kinds of cover choice come back as they went in.
#[tokio::test]
async fn a_cover_choice_round_trips_in_both_shapes() {
    for cover in [
        CoverSelection::Local("cover.jpg".to_string()),
        CoverSelection::Remote(
            "https://example.invalid/front".to_string(),
            MetadataSource::Discogs,
        ),
    ] {
        let (db, _tmp) = empty_db().await;
        let (_, hash) = stored_pane_candidate(&db).await;

        db.save_import_candidate_cover(&hash, &cover).await.unwrap();

        assert_eq!(
            db.load_import_candidate_pane_rows(&hash)
                .await
                .unwrap()
                .cover,
            Some(cover)
        );
    }
}

#[tokio::test]
async fn a_remote_cover_round_trips_the_exact_prepared_bytes() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let cover = CoverSelection::Remote(
        "https://example.invalid/image".to_string(),
        MetadataSource::Discogs,
    );
    let image = crate::import::cover_art::RemoteImage {
        bytes: vec![1, 2, 3, 4],
        content_type: crate::util::content_type::ContentType::Jpeg,
    };

    db.save_import_candidate_prepared_cover(
        &host_root("/music"),
        &pane_candidate_path(),
        &hash,
        0,
        0,
        &cover,
        Some(&image),
    )
        .await
        .unwrap();

    assert_eq!(
        db.load_import_candidate_prepared_assets(&hash)
            .await
            .unwrap()
            .remote_cover,
        Some(image)
    );
}

#[tokio::test]
async fn a_remote_cover_without_exact_bytes_writes_nothing() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let cover = CoverSelection::Remote(
        "https://example.invalid/image".to_string(),
        MetadataSource::Discogs,
    );

    db.save_import_candidate_prepared_cover(
        &host_root("/music"),
        &pane_candidate_path(),
        &hash,
        0,
        0,
        &cover,
        None,
    )
        .await
        .expect_err("a remote selection requires its exact bytes");

    let state = db
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .expect("the candidate remains");
    assert_eq!(state.metadata_revision, 0);
    assert_eq!(
        db.load_import_candidate_pane_rows(&hash)
            .await
            .unwrap()
            .cover,
        None
    );
}

#[tokio::test]
async fn a_stale_remote_cover_write_leaves_the_current_selection_and_bytes() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let current_cover = CoverSelection::Remote(
        "https://example.invalid/current".to_string(),
        MetadataSource::Discogs,
    );
    let current_image = crate::import::cover_art::RemoteImage {
        bytes: vec![1, 2, 3],
        content_type: crate::util::content_type::ContentType::Jpeg,
    };
    db.save_import_candidate_prepared_cover(
        &host_root("/music"),
        &pane_candidate_path(),
        &hash,
        0,
        0,
        &current_cover,
        Some(&current_image),
    )
        .await
        .unwrap();

    let stale_cover = CoverSelection::Remote(
        "https://example.invalid/stale".to_string(),
        MetadataSource::MusicBrainz,
    );
    let stale_image = crate::import::cover_art::RemoteImage {
        bytes: vec![4, 5, 6],
        content_type: crate::util::content_type::ContentType::Png,
    };
    db.save_import_candidate_prepared_cover(
        &host_root("/music"),
        &pane_candidate_path(),
        &hash,
        0,
        0,
        &stale_cover,
        Some(&stale_image),
    )
        .await
        .expect_err("revision zero is stale after the first selection");

    let rows = db.load_import_candidate_pane_rows(&hash).await.unwrap();
    let assets = db
        .load_import_candidate_prepared_assets(&hash)
        .await
        .unwrap();
    assert_eq!(rows.cover, Some(current_cover));
    assert_eq!(assets.remote_cover, Some(current_image));
}

#[tokio::test]
async fn metadata_replacement_replaces_the_complete_artist_asset_set() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let mut draft = metadata_draft("Release Title", "Artist Name");
    let first = crate::import::PreparedArtistImage::Nothing {
        discogs_artist_id: "101".to_string(),
    };
    draft.album_artist_assignments[0] = crate::import::ArtistAssignment::New {
        seed: crate::import::NewArtistSeed {
            name: "Artist Name".to_string(),
            sort_name: None,
            musicbrainz_artist_id: None,
            discogs_artist_id: Some("101".to_string()),
        },
    };
    let revision = db
        .replace_candidate_metadata_prepared(
            &host_root("/music"),
            &hash,
            &pane_candidate_path(),
            0,
            0,
            &crate::import::CandidateMetadataDraft {
                edit: draft.clone(),
                track_mappings: Default::default(),
                source_discogs_artist_ids: Default::default(),
                provenance: Some(release_pick("release-1")),
                cover: None,
                assets: crate::import::CandidatePreparedAssets {
                    remote_cover: None,
                    artist_images: vec![first],
                },
            },
        )
        .await
        .unwrap();

    draft.album_artist_assignments[0] = crate::import::ArtistAssignment::New {
        seed: crate::import::NewArtistSeed {
            name: "Replacement Artist".to_string(),
            sort_name: None,
            musicbrainz_artist_id: None,
            discogs_artist_id: Some("202".to_string()),
        },
    };
    let second = crate::import::PreparedArtistImage::Nothing {
        discogs_artist_id: "202".to_string(),
    };
    let revision = db
        .replace_candidate_metadata_prepared(
            &host_root("/music"),
            &hash,
            &pane_candidate_path(),
            0,
            revision,
            &crate::import::CandidateMetadataDraft {
                edit: draft,
                track_mappings: Default::default(),
                source_discogs_artist_ids: Default::default(),
                provenance: Some(release_pick("release-2")),
                cover: None,
                assets: crate::import::CandidatePreparedAssets {
                    remote_cover: None,
                    artist_images: vec![second.clone()],
                },
            },
        )
        .await
        .unwrap();

    assert_eq!(
        db.load_import_candidate_prepared_assets(&hash)
            .await
            .unwrap()
            .artist_images,
        vec![second]
    );

    let third_assignment = crate::import::ArtistAssignment::New {
        seed: crate::import::NewArtistSeed {
            name: "Edited Artist".to_string(),
            sort_name: None,
            musicbrainz_artist_id: None,
            discogs_artist_id: Some("303".to_string()),
        },
    };
    let third = crate::import::PreparedArtistImage::Nothing {
        discogs_artist_id: "303".to_string(),
    };
    db.replace_import_candidate_album_artists_prepared(
        &host_root("/music"),
        &pane_candidate_path(),
        &hash,
        0,
        revision,
        &[third_assignment],
        &std::collections::BTreeSet::new(),
        std::slice::from_ref(&third),
    )
    .await
    .unwrap();

    assert_eq!(
        db.load_import_candidate_prepared_assets(&hash)
            .await
            .unwrap()
            .artist_images,
        vec![third]
    );
}

#[tokio::test]
async fn preparation_round_trips_source_only_artist_answers() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let source_ids = std::collections::BTreeSet::from(["role-artist".to_string()]);
    let answer = crate::import::PreparedArtistImage::Nothing {
        discogs_artist_id: "role-artist".to_string(),
    };

    db.replace_candidate_metadata_prepared(
        &host_root("/music"),
        &hash,
        &pane_candidate_path(),
        0,
        0,
        &crate::import::CandidateMetadataDraft {
            edit: metadata_draft("Release Title", "Artist Name"),
            track_mappings: Default::default(),
            source_discogs_artist_ids: source_ids.clone(),
            provenance: Some(release_pick("release-with-role")),
            cover: None,
            assets: crate::import::CandidatePreparedAssets {
                remote_cover: None,
                artist_images: vec![answer.clone()],
            },
        },
    )
    .await
    .unwrap();

    let preparation = db
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the candidate is prepared");
    assert_eq!(preparation.source_discogs_artist_ids, source_ids);
    assert_eq!(preparation.assets.artist_images, vec![answer]);

    db.replace_import_candidate_album_artists_prepared(
        &host_root("/music"),
        &pane_candidate_path(),
        &hash,
        preparation.file_edit_revision,
        preparation.metadata_revision,
        &preparation.metadata_draft.album_artist_assignments,
        &std::collections::BTreeSet::new(),
        &[],
    )
    .await
    .unwrap();

    let preparation = db
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the candidate remains prepared");
    assert!(preparation.source_discogs_artist_ids.is_empty());
    assert!(preparation.assets.artist_images.is_empty());
}

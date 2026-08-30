// What the pane stores under a candidate: measured durations, the settled
// signals, the failure an import left, the cover, and the metadata and track
// rows the user typed.

use crate::import::probe::{SourceDurations, SourceDuration};
use crate::import::folder_scanner::{CandidateFileEdits, FileRoleChoice};
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

fn file_unit(file_id: &str, duration_ms: Option<u64>) -> SourceDuration {
    SourceDuration {
        audio: AudioFile::Standalone {
            file_id: file_id.to_string(),
        },
        duration_ms,
    }
}

fn slice_unit(index: u32, duration_ms: Option<u64>) -> SourceDuration {
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
        folder_path: "/music/Album".to_string(),
        verdict: sample_verdict(),
        signals,
        expected_edit_revision: 0,
        expected_metadata_revision: 0,
        metadata: crate::import::CandidateMetadataDraft {
            edit: crate::import::pane::blank_candidate_draft(&pane_candidate()),
            provenance: None,
            cover: None,
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

fn metadata_draft(title: &str, artist: &str) -> RawReleaseEdit {
    RawReleaseEdit {
        album_title: title.to_string(),
        album_artist_assignments: if artist.is_empty() {
            Vec::new()
        } else {
            vec![new_artist(artist)]
        },
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

/// The verdict stores its derived total without duplicating per-file scan facts.
#[tokio::test]
async fn verdict_stores_only_the_derived_total() {
    let (db, _tmp) = empty_db().await;
    let candidate = pane_candidate();
    let hash = candidate.content_hash();
    let durations = SourceDurations::new(vec![
        file_unit("01 Track.flac", Some(180_000)),
        file_unit("CDImage.flac", Some(600_000)),
        slice_unit(0, Some(200_000)),
        slice_unit(1, None),
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

/// A file that would not open zeroes the total: a partial sum reads as "the
/// durations disagree", which is a wrong answer where the honest one is that
/// nobody knows.
#[tokio::test]
async fn a_file_with_no_length_makes_the_total_unknown() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let durations = SourceDurations::new(vec![
        file_unit("01 Track.flac", Some(180_000)),
        file_unit("CDImage.flac", None),
    ]);

    assert!(store_verdict(&db, &hash, signals_with(durations)).await);

    let state = db.load_import_candidate_state(&hash).await.unwrap().unwrap();
    assert_eq!(
        state.identify.unwrap().probed_total_duration_ms,
        0,
        "and the column says the same"
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
        let hash = pane_candidate().content_hash();
        let signals = Signals {
            disc_id,
            barcode,
            text,
            durations: SourceDurations::new(vec![file_unit("01 Track.flac", Some(1_000))]),
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
        let hash = pane_candidate().content_hash();

        let error = db
            .save_import_candidate_verdict(&NewImportCandidateVerdict {
                content_hash: hash.clone(),
                folder_path: "/music/Album".to_string(),
                verdict: sample_verdict(),
                signals: scanning,
                expected_edit_revision: 0,
                expected_metadata_revision: 0,
                metadata: crate::import::CandidateMetadataDraft {
                    edit: crate::import::pane::blank_candidate_draft(&pane_candidate()),
                    provenance: None,
                    cover: None,
                },
            })
            .await
            .expect_err("a scanning signal is not storable");
        assert!(
            error.to_string().contains("still scanning"),
            "{error} should name what was refused"
        );
        assert!(
            db.load_import_candidate_state(&hash)
                .await
                .unwrap()
                .is_none(),
            "the refused write left no row behind"
        );
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
        "/music/Album",
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
        "/music/Album",
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
        "/music/Album",
        0,
        &ImportFailure::error_only("the prior attempt failed", fixed_identified_at()),
    )
    .await
    .unwrap();

    let key = format!("{}/Album", host_root("/music"));
    let detail = db
        .load_import_candidate(&key)
        .await
        .unwrap()
        .expect("the stored candidate has a detail")
        .resolve(&crate::import::TriageRuntimeFacts {
            identify_phase: None,
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
            db.load_import_candidate_pane_rows(&hash).await.unwrap().cover,
            Some(cover)
        );
    }
}

/// A pane edit with no candidate row under it is a defect, not a state to
/// absorb: the form is drawn only under a pick, and a pick writes that row.
#[tokio::test]
async fn a_pane_edit_without_a_candidate_row_is_refused() {
    let (db, _tmp) = empty_db().await;
    let hash = pane_candidate().content_hash();

    for error in [
        db.save_import_candidate_cover(&hash, &CoverSelection::Local("cover.jpg".to_string()))
            .await
            .expect_err("a cover with nothing picked"),
        db.save_import_candidate_edit_field(&hash, CandidateEditField::Year, "1991")
            .await
            .expect_err("a field with nothing picked"),
        db.save_import_candidate_track_edit(&hash, &edited_row("import-track-0", "Track Title", None))
            .await
            .expect_err("a row with nothing picked"),
    ] {
        assert!(
            error.to_string().contains("no candidate state row"),
            "{error} should say what is missing"
        );
    }
}

/// Field writes update the one complete draft and leave its other values intact.
#[tokio::test]
async fn draft_field_writes_change_only_the_named_fields() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let seed = metadata_draft("Seeded Title", "Artist Name");
    db.replace_candidate_metadata(&hash, "/music/Album", &seed, Some(&release_pick("rel-1")))
        .await
        .unwrap();

    db.save_import_candidate_edit_field(&hash, CandidateEditField::Year, "1991")
        .await
        .unwrap();
    db.save_import_candidate_edit_field(&hash, CandidateEditField::AlbumTitle, "Album Title")
        .await
        .unwrap();

    let stored = db
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .metadata_draft;
    assert_eq!(stored.album_title, "Album Title");
    assert_eq!(stored.pressing.year, "1991");
    assert_eq!(stored.album_artist_assignments, seed.album_artist_assignments);
    assert_eq!(stored.tracks, seed.tracks);
}

#[tokio::test]
async fn existing_artist_assignments_resolve_the_canonical_artist_row() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let existing = existing_artist();
    db.insert_artist(&existing).await.unwrap();

    let assignments = vec![
        ArtistAssignment::existing(existing.into()),
        ArtistAssignment::New {
            seed: NewArtistSeed {
                name: "New Artist".to_string(),
                sort_name: Some("Artist, New".to_string()),
                musicbrainz_artist_id: Some("mb-new".to_string()),
                discogs_artist_id: Some("discogs-new".to_string()),
            },
        },
    ];
    db.replace_import_candidate_album_artists(&hash, &assignments)
        .await
        .unwrap();

    let stored = db.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert_eq!(stored.metadata_draft.album_artist_assignments, assignments);

    let explicit_empty = CandidateTrackEdit::edited(RawTrackEdit {
        id: "candidate-track-0".to_string(),
        title: "Track Title".to_string(),
        artist_assignments: TrackArtistAssignments::Explicit(Vec::new()),
        side: 1,
        track_number: Some(1),
        file: None,
    });
    db.save_import_candidate_track_edit(&hash, &explicit_empty)
        .await
        .unwrap();
    assert_eq!(
        db.load_import_candidate_pane_rows(&hash)
            .await
            .unwrap()
            .metadata_draft
            .tracks[0]
            .artist_assignments,
        TrackArtistAssignments::Explicit(Vec::new())
    );
}

#[tokio::test]
async fn an_existing_artist_assignment_to_a_missing_row_is_rejected() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;

    let error = db
        .replace_import_candidate_album_artists(
            &hash,
            &[ArtistAssignment::existing(ExistingArtist {
                artist_id: bae_test_support::test_uuid("missing-artist"),
                name: "Missing Artist".to_string(),
                sort_name: None,
                musicbrainz_artist_id: None,
                discogs_artist_id: None,
            })],
        )
        .await
        .expect_err("a missing referenced artist cannot be stored");
    assert!(
        error.to_string().contains("FOREIGN KEY constraint failed"),
        "the database rejects the broken reference: {error}"
    );
}

/// A track metadata edit and its physical mapping are stored through their
/// independent tables and rejoin in the candidate pane.
#[tokio::test]
async fn a_track_row_round_trips_metadata_and_mapping() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    db.replace_candidate_metadata(
        &hash,
        "/music/Album",
        &metadata_draft("Album", "Artist"),
        Some(&release_pick("rel-1")),
    )
        .await
        .unwrap();
    let edit = edited_row(
        "candidate-track-0",
        "Edited title",
        Some(AudioFile::SheetSlice {
            file_id: "CDImage.flac".to_string(),
            sheet_id: "CDImage.cue".to_string(),
            index: 4,
        }),
    );
    db.save_import_candidate_track_edit(&hash, &edit).await.unwrap();

    let stored = db
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap();
    assert_eq!(stored.metadata_draft.tracks[0].title, "Edited title");
    assert_eq!(stored.track_mappings.len(), 1);
    assert_eq!(stored.track_mappings[0].file, edit.file().cloned());
}

/// A file decision reshapes the folder, so the slice measurements, the
/// extracted signals and the row edits go with it. The whole files' own
/// lengths stay — those are facts about bytes the hash still covers — and so
/// do the metadata and cover, which belong to the pick.
#[tokio::test]
async fn a_file_decision_clears_what_the_reshaped_folder_invalidates() {
    let (db, _tmp) = empty_db().await;
    let (files, hash) = stored_pane_candidate(&db).await;
    let durations = SourceDurations::new(vec![
        file_unit("01 Track.flac", Some(180_000)),
        file_unit("CDImage.flac", Some(600_000)),
        slice_unit(0, Some(200_000)),
    ]);
    assert!(store_verdict(&db, &hash, signals_with(durations)).await);
    db.replace_candidate_metadata(
        &hash,
        "/music/Album",
        &metadata_draft("Album", "Artist"),
        Some(&release_pick("rel-1")),
    )
        .await
        .unwrap();
    db.save_import_candidate_edit_field(&hash, CandidateEditField::Year, "1991")
        .await
        .unwrap();
    db.save_import_candidate_cover(&hash, &CoverSelection::Local("cover.jpg".to_string()))
        .await
        .unwrap();
    db.save_import_candidate_track_edit(&hash, &edited_row("candidate-track-0", "Track Title", None))
        .await
        .unwrap();
    let mut edits = CandidateFileEdits::default();
    edits.file_roles.set(
        "CDImage.flac".to_string(),
        FileRoleChoice::NotATrack,
    );
    let mut settled = files;
    settled.apply_candidate_file_edits(&edits).unwrap();
    db.save_import_candidate_file_edits(
        &hash,
        "/music/Album",
        0,
        &edits,
        &[("/music/Album".to_string(), settled)],
    )
        .await
        .unwrap();

    let state = db.load_import_candidate_state(&hash).await.unwrap().unwrap();
    assert!(state.signals.is_none(), "the disc ID is recomputed");

    let pane = db.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert!(pane.track_mappings.is_empty(), "the table's physical rows are a new set");
    assert_eq!(pane.metadata_draft.pressing.year, "1991", "the draft survives");
    assert_eq!(
        pane.cover,
        Some(CoverSelection::Local("cover.jpg".to_string())),
        "and so does its cover"
    );
}

/// Applying or clearing metadata replaces the whole draft and removes artist
/// assignments owned by the prior source, while every physical decision stays.
#[tokio::test]
async fn metadata_apply_and_clear_preserve_every_physical_decision() {
    let (db, _tmp) = empty_db().await;
    let (files, hash) = stored_pane_candidate(&db).await;
    let old_draft = metadata_draft("Old album", "Replacement Artist");
    db.replace_candidate_metadata(
        &hash,
        "/music/Album",
        &old_draft,
        Some(&release_pick("rel-1")),
    )
        .await
        .unwrap();
    let mut file_edits = CandidateFileEdits::default();
    file_edits.file_roles.set(
        "CDImage.flac".to_string(),
        FileRoleChoice::NotATrack,
    );
    let mut settled = files;
    settled.apply_candidate_file_edits(&file_edits).unwrap();
    db.save_import_candidate_file_edits(
        &hash,
        "/music/Album",
        0,
        &file_edits,
        &[("/music/Album".to_string(), settled)],
    )
        .await
        .unwrap();
    db.save_import_candidate_cover(&hash, &CoverSelection::Local("cover.jpg".to_string()))
        .await
        .unwrap();
    let mapping = edited_row(
        "candidate-track-0",
        "Track Title",
        Some(AudioFile::Standalone {
            file_id: "01 Track.flac".to_string(),
        }),
    );
    db.save_import_candidate_track_edit(&hash, &mapping)
        .await
        .unwrap();

    let new_draft = metadata_draft("New album", "New Artist");
    let applied_revision = db.replace_candidate_metadata(
        &hash,
        "/music/Album",
        &new_draft,
        Some(&release_pick("rel-2")),
    )
        .await
        .unwrap();
    let applied = db.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert_eq!(applied.metadata_draft, new_draft);
    assert_eq!(applied.track_mappings[0].file, mapping.file().cloned());
    assert_eq!(applied_revision, 4);
    assert_eq!(applied.cover, None, "applying a source clears a local cover");
    assert_eq!(
        db.load_import_candidate_state(&hash)
            .await
            .unwrap()
            .unwrap()
            .file_edits
            .file_roles,
        file_edits.file_roles
    );

    db.save_import_candidate_cover(
        &hash,
        &CoverSelection::Remote(
            "https://example.invalid/cover".to_string(),
            MetadataSource::MusicBrainz,
        ),
    )
    .await
    .unwrap();
    let blank = crate::import::pane::blank_candidate_draft(&pane_candidate());
    let cleared_revision = db.replace_candidate_metadata(&hash, "/music/Album", &blank, None)
        .await
        .unwrap();
    let cleared = db.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert!(cleared.metadata_draft.is_blank());
    assert_eq!(cleared.track_mappings[0].file, mapping.file().cloned());
    assert_eq!(cleared_revision, 6);
    assert_eq!(cleared.cover, None, "clearing removes a remote cover");
}

#[tokio::test]
async fn metadata_revision_advances_for_every_draft_and_cover_mutation() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;

    assert_eq!(
        db.replace_candidate_metadata(
            &hash,
            "/music/Album",
            &metadata_draft("Album", "Artist"),
            Some(&release_pick("rel-1")),
        )
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        db.save_import_candidate_edit_field(&hash, CandidateEditField::Year, "1991")
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        db.save_import_candidate_cover(&hash, &CoverSelection::Local("cover.jpg".to_string()))
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        db.replace_import_candidate_album_artists(
            &hash,
            &[new_artist("Different Artist")],
        )
        .await
        .unwrap(),
        4
    );
    assert_eq!(
        db.save_import_candidate_track_edit(
            &hash,
            &edited_row("candidate-track-0", "Changed title", None),
        )
        .await
        .unwrap(),
        5
    );
}

/// A verdict never revises a person's pick, so it never takes their edits
/// either — however different the release it would have picked.
#[tokio::test]
async fn a_verdict_leaves_a_person_s_pick_and_their_edits_alone() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    db.replace_candidate_metadata(
        &hash,
        "/music/Album",
        &metadata_draft("Album", "Artist"),
        Some(&release_pick("rel-chosen")),
    )
        .await
        .unwrap();
    db.save_import_candidate_edit_field(&hash, CandidateEditField::Year, "1991")
        .await
        .unwrap();

    assert!(
        db.save_import_candidate_verdict(&NewImportCandidateVerdict {
            content_hash: hash.clone(),
            folder_path: "/music/Album".to_string(),
            verdict: sample_verdict(),
            signals: signals_with(SourceDurations::default()),
            expected_edit_revision: 0,
            expected_metadata_revision: 2,
            metadata: crate::import::CandidateMetadataDraft {
                edit: metadata_draft("Different album", "Different Artist"),
                provenance: Some(release_pick("rel-1")),
                cover: None,
            },
        })
        .await
        .unwrap()
    );

    let state = db.load_import_candidate_state(&hash).await.unwrap().unwrap();
    assert_eq!(state.metadata_provenance, Some(release_pick("rel-chosen")));
    assert_eq!(
        db.load_import_candidate_pane_rows(&hash)
            .await
            .unwrap()
            .metadata_draft
            .pressing
            .year,
        "1991"
    );
}

/// A field edit made while identification is running wins even when the
/// current source was chosen by identification. The result was derived from
/// the older draft revision and therefore cannot replace the newer text.
#[tokio::test]
async fn a_stale_verdict_cannot_overwrite_a_newer_metadata_edit() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let first_pick = release_pick("rel-first");
    assert!(db
        .save_import_candidate_verdict(&NewImportCandidateVerdict {
            content_hash: hash.clone(),
            folder_path: "/music/Album".to_string(),
            verdict: sample_verdict(),
            signals: signals_with(SourceDurations::default()),
            expected_edit_revision: 0,
            expected_metadata_revision: 0,
            metadata: crate::import::CandidateMetadataDraft {
                edit: metadata_draft("First album", "Artist"),
                provenance: Some(first_pick.clone()),
                cover: None,
            },
        })
        .await
        .unwrap());
    db.save_import_candidate_edit_field(
        &hash,
        CandidateEditField::AlbumTitle,
        "Person's title",
    )
    .await
    .unwrap();

    assert!(!db
        .save_import_candidate_verdict(&NewImportCandidateVerdict {
            content_hash: hash.clone(),
            folder_path: "/music/Album".to_string(),
            verdict: sample_verdict(),
            signals: signals_with(SourceDurations::default()),
            expected_edit_revision: 0,
            expected_metadata_revision: 1,
            metadata: crate::import::CandidateMetadataDraft {
                edit: metadata_draft("Second album", "Different Artist"),
                provenance: Some(release_pick("rel-second")),
                cover: None,
            },
        })
        .await
        .unwrap());

    let state = db.load_import_candidate_state(&hash).await.unwrap().unwrap();
    assert_eq!(state.metadata_revision, 2);
    assert_eq!(state.metadata_provenance, Some(first_pick));
    assert_eq!(
        db.load_import_candidate_pane_rows(&hash)
            .await
            .unwrap()
            .metadata_draft
            .album_title,
        "Person's title"
    );
}

// What the pane stores under a candidate: measured durations, the settled
// signals, the failure an import left, the cover, and the metadata and track
// rows the user typed.

use crate::import::probe::{ProbedDurations, ProbedUnit};
use crate::import::folder_scanner::{CandidateFileEdits, FileRoleChoice};
use crate::import::{
    ArtistAssignment, AudioFile, CandidateEditField, CandidateEditOverlay, CandidateTrackEdit,
    CoverSelection, ExistingArtist, NewArtistSeed, RawTrackEdit, TrackArtistAssignments,
    TrackEditState,
};
use crate::signals::{
    BarcodeSignal, DiscIdSignal, LookupFailure, SignalOrigin, Signals, SourcedValue, TextSignal,
};

fn pane_candidate() -> CategorizedFiles {
    track_files_candidate(&[("01 Track.flac", 111), ("CDImage.flac", 222)])
}

fn file_unit(file_id: &str, duration_ms: Option<u64>) -> ProbedUnit {
    ProbedUnit {
        audio: AudioFile::Standalone {
            file_id: file_id.to_string(),
        },
        duration_ms,
    }
}

fn slice_unit(index: u32, duration_ms: Option<u64>) -> ProbedUnit {
    ProbedUnit {
        audio: AudioFile::SheetSlice {
            file_id: "CDImage.flac".to_string(),
            sheet_id: "CDImage.cue".to_string(),
            index,
        },
        duration_ms,
    }
}

fn signals_with(durations: ProbedDurations) -> Signals {
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
        metadata_seed: None,
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

/// The measurements a verdict carries come back exactly, kinds and absences
/// included, and the total the column holds is derived from them.
#[tokio::test]
async fn measured_durations_round_trip_with_the_verdict() {
    let (db, _tmp) = empty_db().await;
    let candidate = pane_candidate();
    let hash = candidate.content_hash();
    let durations = ProbedDurations::new(vec![
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
    assert_eq!(
        state.durations.units.len(),
        durations.units.len(),
        "{:?}",
        state.durations
    );
    for unit in &durations.units {
        assert_eq!(
            state.durations.duration_of(&unit.audio),
            Some(unit.duration_ms),
            "{:?} came back different",
            unit.audio
        );
    }
    assert_eq!(
        state.durations.total_ms(),
        780_000,
        "the total sums the files, and a slice with no timing is not one"
    );
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
    let hash = pane_candidate().content_hash();
    let durations = ProbedDurations::new(vec![
        file_unit("01 Track.flac", Some(180_000)),
        file_unit("CDImage.flac", None),
    ]);

    assert!(store_verdict(&db, &hash, signals_with(durations)).await);

    let state = db.load_import_candidate_state(&hash).await.unwrap().unwrap();
    assert_eq!(state.durations.total_ms(), 0);
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
            durations: ProbedDurations::new(vec![file_unit("01 Track.flac", Some(1_000))]),
        };

        assert!(store_verdict(&db, &hash, signals.clone()).await, "{what}");

        let stored = db
            .load_import_candidate_state(&hash)
            .await
            .unwrap()
            .unwrap()
            .signals
            .unwrap_or_else(|| panic!("{what}: the signals read back"));
        assert_eq!(stored, signals, "{what}");
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
            durations: ProbedDurations::default(),
        },
        Signals {
            disc_id: DiscIdSignal::Absent { track_count: 0 },
            barcode: BarcodeSignal::Absent,
            text: TextSignal::Scanning {
                catalogs: Vec::new(),
                free_text: Vec::new(),
            },
            durations: ProbedDurations::default(),
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
                metadata_seed: None,
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

/// A failed import is recorded even for a candidate nothing has identified or
/// picked — an import driven straight from a command still failed on those
/// bytes. Queueing the next attempt clears it.
#[tokio::test]
async fn a_failure_creates_its_own_row_and_is_replaced_then_cleared() {
    let (db, _tmp) = empty_db().await;
    let hash = pane_candidate().content_hash();

    db.save_import_candidate_failure(&hash, "/music/Album", 0, "the folder vanished")
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
    assert!(
        db.load_import_candidate_state(&hash)
            .await
            .unwrap()
            .is_some(),
        "the write created the row everything else hangs off"
    );

    db.save_import_candidate_failure(&hash, "/music/Album", 0, "the disc would not read")
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
        let hash = pane_candidate().content_hash();
        db.save_candidate_metadata_seed(&hash, "/music/Album", &release_pick("rel-1"))
            .await
            .unwrap();

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

/// The overlay is per field: writing two leaves the other six untouched, and
/// applying it replaces exactly those two.
#[tokio::test]
async fn the_edit_overlay_holds_only_the_fields_that_were_typed() {
    let (db, _tmp) = empty_db().await;
    let hash = pane_candidate().content_hash();
    db.save_candidate_metadata_seed(&hash, "/music/Album", &release_pick("rel-1"))
        .await
        .unwrap();

    db.save_import_candidate_edit_field(&hash, CandidateEditField::Year, "1991")
        .await
        .unwrap();
    db.save_import_candidate_edit_field(&hash, CandidateEditField::AlbumTitle, "Album Title")
        .await
        .unwrap();

    let overlay = db.load_import_candidate_pane_rows(&hash).await.unwrap().edit;
    assert_eq!(
        overlay,
        CandidateEditOverlay {
            album_title: Some("Album Title".to_string()),
            year: Some("1991".to_string()),
            ..Default::default()
        }
    );

    let seed = crate::import::RawReleaseEdit {
        album_title: "Seeded Title".to_string(),
        album_artist_assignments: vec![new_artist("Artist Name")],
        pressing: crate::import::RawPressingEdit {
            year: "1990".to_string(),
            format: "CD".to_string(),
            label: "Label Name".to_string(),
            catalog_number: "CAT-1".to_string(),
            country: "XE".to_string(),
            barcode: "0123456789012".to_string(),
        },
        tracks: Vec::new(),
    };
    let applied = overlay.apply(seed.clone());
    assert_eq!(applied.album_title, "Album Title");
    assert_eq!(applied.pressing.year, "1991");
    assert_eq!(
        applied.album_artist_assignments,
        seed.album_artist_assignments
    );
    assert_eq!(applied.pressing.format, seed.pressing.format);
    assert_eq!(applied.pressing.label, seed.pressing.label);
    assert_eq!(applied.pressing.catalog_number, seed.pressing.catalog_number);
    assert_eq!(applied.pressing.country, seed.pressing.country);
    assert_eq!(applied.pressing.barcode, seed.pressing.barcode);
}

#[tokio::test]
async fn existing_artist_assignments_resolve_the_canonical_artist_row() {
    let (db, _tmp) = empty_db().await;
    let hash = pane_candidate().content_hash();
    let existing = existing_artist();
    db.insert_artist(&existing).await.unwrap();
    db.save_candidate_metadata_seed(&hash, "/music/Album", &release_pick("rel-1"))
        .await
        .unwrap();

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
    assert_eq!(stored.edit.album_artist_assignments, Some(assignments));

    let explicit_empty = CandidateTrackEdit::edited(RawTrackEdit {
        id: "import-track-empty".to_string(),
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
            .track_edits,
        vec![explicit_empty]
    );
}

#[tokio::test]
async fn an_existing_artist_assignment_to_a_missing_row_is_rejected() {
    let (db, _tmp) = empty_db().await;
    let hash = pane_candidate().content_hash();
    db.save_candidate_metadata_seed(&hash, "/music/Album", &release_pick("rel-1"))
        .await
        .unwrap();

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

/// A row edit is stored whole, in each of the three shapes its audio can
/// take, and a dropped row keeps nothing else.
#[tokio::test]
async fn a_track_row_round_trips_in_every_shape() {
    let (db, _tmp) = empty_db().await;
    let hash = pane_candidate().content_hash();
    db.save_candidate_metadata_seed(&hash, "/music/Album", &release_pick("rel-1"))
        .await
        .unwrap();

    let edits = vec![
        edited_row(
            "import-track-0",
            "Track Title",
            Some(AudioFile::Standalone {
                file_id: "01 Track.flac".to_string(),
            }),
        ),
        edited_row(
            "import-track-1",
            "Second Track Title",
            Some(AudioFile::SheetSlice {
                file_id: "CDImage.flac".to_string(),
                sheet_id: "CDImage.cue".to_string(),
                index: 4,
            }),
        ),
        edited_row("import-track-2", "Third Track Title", None),
        CandidateTrackEdit::dropped("import-track-3"),
    ];
    for edit in &edits {
        db.save_import_candidate_track_edit(&hash, edit)
            .await
            .unwrap();
    }

    let stored = db
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .track_edits;
    assert_eq!(stored, edits);
    assert!(matches!(stored[3].state, TrackEditState::Dropped));
}

/// A file decision reshapes the folder, so the slice measurements, the
/// extracted signals and the row edits go with it. The whole files' own
/// lengths stay — those are facts about bytes the hash still covers — and so
/// do the metadata and cover, which belong to the pick.
#[tokio::test]
async fn a_file_decision_clears_what_the_reshaped_folder_invalidates() {
    let (db, _tmp) = empty_db().await;
    let candidate = pane_candidate();
    let hash = candidate.content_hash();
    let durations = ProbedDurations::new(vec![
        file_unit("01 Track.flac", Some(180_000)),
        file_unit("CDImage.flac", Some(600_000)),
        slice_unit(0, Some(200_000)),
    ]);
    assert!(store_verdict(&db, &hash, signals_with(durations)).await);
    db.save_candidate_metadata_seed(&hash, "/music/Album", &release_pick("rel-1"))
        .await
        .unwrap();
    db.save_import_candidate_edit_field(&hash, CandidateEditField::Year, "1991")
        .await
        .unwrap();
    db.save_import_candidate_cover(&hash, &CoverSelection::Local("cover.jpg".to_string()))
        .await
        .unwrap();
    db.save_import_candidate_track_edit(&hash, &edited_row("import-track-0", "Track Title", None))
        .await
        .unwrap();

    let mut edits = CandidateFileEdits::default();
    edits.file_roles.set(
        "CDImage.flac".to_string(),
        FileRoleChoice::NotATrack,
    );
    db.save_import_candidate_file_edits(&hash, "/music/Album", 0, &edits, &[])
        .await
        .unwrap();

    let state = db.load_import_candidate_state(&hash).await.unwrap().unwrap();
    assert_eq!(
        state.durations.units,
        vec![
            file_unit("01 Track.flac", Some(180_000)),
            file_unit("CDImage.flac", Some(600_000)),
        ],
        "the slices go and the files stay"
    );
    assert!(state.signals.is_none(), "the disc ID is recomputed");

    let pane = db.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert!(pane.track_edits.is_empty(), "the table's rows are a new set");
    assert_eq!(pane.edit.year, Some("1991".to_string()), "the pick survives");
    assert_eq!(
        pane.cover,
        Some(CoverSelection::Local("cover.jpg".to_string())),
        "and so does its cover"
    );
}

/// Picking a different release takes the metadata typed over the old one, the
/// rows addressed by its track identities, and remote art belonging to it. A
/// cover chosen from the candidate's own files remains valid. Re-picking the
/// same release keeps every choice.
#[tokio::test]
async fn picking_a_different_release_clears_only_what_belonged_to_the_old_one() {
    let (db, _tmp) = empty_db().await;
    let hash = pane_candidate().content_hash();
    db.save_candidate_metadata_seed(&hash, "/music/Album", &release_pick("rel-1"))
        .await
        .unwrap();
    db.save_import_candidate_edit_field(&hash, CandidateEditField::Year, "1991")
        .await
        .unwrap();
    db.save_import_candidate_cover(&hash, &CoverSelection::Local("cover.jpg".to_string()))
        .await
        .unwrap();
    db.save_import_candidate_track_edit(&hash, &edited_row("import-track-0", "Track Title", None))
        .await
        .unwrap();

    db.save_candidate_metadata_seed(&hash, "/music/Album", &release_pick("rel-1"))
        .await
        .unwrap();
    let kept = db.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert_eq!(kept.edit.year, Some("1991".to_string()));
    assert!(kept.cover.is_some());
    assert_eq!(kept.track_edits.len(), 1);

    db.save_candidate_metadata_seed(&hash, "/music/Album", &release_pick("rel-2"))
        .await
        .unwrap();
    let cleared = db.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert_eq!(cleared.edit, CandidateEditOverlay::default());
    assert_eq!(
        cleared.cover,
        Some(CoverSelection::Local("cover.jpg".to_string()))
    );
    assert!(cleared.track_edits.is_empty());

    db.save_import_candidate_cover(
        &hash,
        &CoverSelection::Remote(
            "https://example.invalid/front".to_string(),
            MetadataSource::MusicBrainz,
        ),
    )
    .await
    .unwrap();
    db.save_candidate_metadata_seed(&hash, "/music/Album", &release_pick("rel-3"))
        .await
        .unwrap();
    assert_eq!(
        db.load_import_candidate_pane_rows(&hash)
            .await
            .unwrap()
            .cover,
        None
    );
}

/// A verdict never revises a person's pick, so it never takes their edits
/// either — however different the release it would have picked.
#[tokio::test]
async fn a_verdict_leaves_a_person_s_pick_and_their_edits_alone() {
    let (db, _tmp) = empty_db().await;
    let hash = pane_candidate().content_hash();
    db.save_candidate_metadata_seed(&hash, "/music/Album", &release_pick("rel-chosen"))
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
            signals: signals_with(ProbedDurations::default()),
            expected_edit_revision: 0,
            metadata_seed: Some(release_pick("rel-1")),
        })
        .await
        .unwrap()
    );

    let state = db.load_import_candidate_state(&hash).await.unwrap().unwrap();
    assert_eq!(state.metadata_seed, Some(release_pick("rel-chosen")));
    assert_eq!(
        db.load_import_candidate_pane_rows(&hash)
            .await
            .unwrap()
            .edit
            .year,
        Some("1991".to_string())
    );
}

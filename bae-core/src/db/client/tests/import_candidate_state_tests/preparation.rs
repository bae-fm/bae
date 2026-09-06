use super::*;

#[tokio::test]
async fn stale_file_revision_cannot_replace_prepared_metadata() {
    let (db, _tmp) = empty_db().await;
    let (files, hash) = stored_pane_candidate(&db).await;
    let stale = db
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the scanned candidate is prepared");
    let mapping_preparation = crate::import::CandidateMappingPreparation {
        edit: stale.metadata_draft.clone(),
        track_mappings: stale.track_mappings.clone(),
        source_discogs_artist_ids: stale.source_discogs_artist_ids.clone(),
        artist_images: stale.assets.artist_images.clone(),
    };
    db.save_import_candidate_file_edits(
        &hash,
        &pane_candidate_path(),
        stale.file_edit_revision,
        stale.metadata_revision,
        &CandidateFileEdits::default(),
        &[(pane_candidate_path(), files)],
        &mapping_preparation,
    )
    .await
    .unwrap();

    let error = db
        .replace_candidate_metadata_prepared(
            &host_root("/music"),
            &hash,
            &pane_candidate_path(),
            stale.file_edit_revision,
            stale.metadata_revision,
            &crate::import::CandidateMetadataDraft {
                edit: stale.metadata_draft,
                track_mappings: stale.track_mappings,
                source_discogs_artist_ids: stale.source_discogs_artist_ids,
                provenance: stale.metadata_provenance,
                cover: stale.cover,
                assets: stale.assets,
            },
        )
        .await
        .expect_err("metadata prepared for the prior files is stale");
    assert!(error.to_string().contains("candidate changed"), "{error}");
}

#[tokio::test]
async fn metadata_replacement_refuses_a_candidate_key_that_now_names_other_files() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let stale = db
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the scanned candidate is prepared");
    let root = host_root("/music");
    let mut replacement = scanned_candidate(&root, "Album");
    let crate::import::folder_scanner::ScanItem::Valid(candidate) = &mut replacement else {
        unreachable!("the fixture creates a valid candidate");
    };
    candidate.files.files[0].file.size += 1;
    let generation = db.begin_folder_scan(&root).await.unwrap();
    db.save_folder_scan_item(&root, generation, &replacement)
        .await
        .unwrap()
        .expect("the current scan accepts the replacement");
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();

    let error = db
        .replace_candidate_metadata_prepared(
            &root,
            &hash,
            &pane_candidate_path(),
            stale.file_edit_revision,
            stale.metadata_revision,
            &crate::import::CandidateMetadataDraft {
                edit: stale.metadata_draft,
                track_mappings: stale.track_mappings,
                source_discogs_artist_ids: stale.source_discogs_artist_ids,
                provenance: stale.metadata_provenance,
                cover: stale.cover,
                assets: stale.assets,
            },
        )
        .await
        .expect_err("the candidate key no longer names the prepared files");
    assert!(error.to_string().contains("candidate changed"), "{error}");
}

#[tokio::test]
async fn cover_write_refuses_a_candidate_key_that_now_names_other_files() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let root = host_root("/music");
    let mut replacement = scanned_candidate(&root, "Album");
    let crate::import::folder_scanner::ScanItem::Valid(candidate) = &mut replacement else {
        unreachable!("the fixture creates a valid candidate");
    };
    candidate.files.files[0].file.size += 1;
    let generation = db.begin_folder_scan(&root).await.unwrap();
    db.save_folder_scan_item(&root, generation, &replacement)
        .await
        .unwrap()
        .expect("the current scan accepts the replacement");
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();

    let error = db
        .save_import_candidate_prepared_cover(
            &root,
            &pane_candidate_path(),
            &hash,
            0,
            0,
            &CoverSelection::Local("cover.jpg".to_string()),
            None,
        )
        .await
        .expect_err("the candidate key no longer names the prepared files");
    assert!(error.to_string().contains("candidate changed"), "{error}");
}

#[tokio::test]
async fn stale_metadata_revision_cannot_replace_prepared_file_mappings() {
    let (db, _tmp) = empty_db().await;
    let (files, hash) = stored_pane_candidate(&db).await;
    assert!(store_verdict(
        &db,
        &hash,
        signals_with(SourceDurations::new(vec![file_unit("01 Track.flac", 180_000)])),
    )
    .await);
    let stale = db
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the candidate is prepared");
    db.save_import_candidate_edit_field(&hash, CandidateEditField::PressingYear, "1991")
        .await
        .unwrap();

    let mut edits = CandidateFileEdits::default();
    edits
        .file_roles
        .set("CDImage.flac".to_string(), FileRoleChoice::NotATrack);
    let mut settled = files;
    settled.apply_candidate_file_edits(&edits).unwrap();
    let error = db
        .save_import_candidate_file_edits(
            &hash,
            &pane_candidate_path(),
            stale.file_edit_revision,
            stale.metadata_revision,
            &edits,
            &[(pane_candidate_path(), settled)],
            &crate::import::CandidateMappingPreparation {
                edit: stale.metadata_draft.clone(),
                track_mappings: stale.track_mappings.clone(),
                source_discogs_artist_ids: stale.source_discogs_artist_ids,
                artist_images: stale.assets.artist_images,
            },
        )
        .await
        .expect_err("file mappings prepared for the prior metadata are stale");
    assert!(error.to_string().contains("metadata changed"), "{error}");

    let state = db
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .expect("the candidate state remains");
    assert_eq!(state.file_edits, CandidateFileEdits::default());
    assert!(state.identify.is_some(), "the rejected write keeps its verdict");
    let pane = db.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert_eq!(pane.metadata_draft.pressing.year, "1991");
    assert_eq!(pane.track_mappings, stale.track_mappings);
}

#[tokio::test]
async fn an_existing_library_artist_needs_no_candidate_image_answer() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let artist = existing_artist();
    db.insert_artist(&artist).await.unwrap();
    let mut draft = metadata_draft("Release Title", "Artist Name");
    draft.album_artist_assignments = vec![ArtistAssignment::Existing {
        artist: ExistingArtist {
            artist_id: artist.id,
            name: artist.name,
            sort_name: artist.sort_name,
            musicbrainz_artist_id: artist.musicbrainz_artist_id,
            discogs_artist_id: artist.discogs_artist_id,
        },
    }];

    db.replace_candidate_metadata_prepared(
        &host_root("/music"),
        &hash,
        &pane_candidate_path(),
        0,
        0,
        &crate::import::CandidateMetadataDraft {
            edit: draft,
            track_mappings: Default::default(),
            source_discogs_artist_ids: Default::default(),
            provenance: None,
            cover: None,
            assets: crate::import::CandidatePreparedAssets::default(),
        },
    )
    .await
    .expect("an existing artist is not waiting to be inserted");
}

#[tokio::test]
async fn preparation_without_its_completeness_marker_is_refused() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let deleted_hash = hash.clone();
    db.call(move |sql| {
        sql.execute(
            "DELETE FROM import_candidate_asset_preparation WHERE content_hash = ?",
            [&deleted_hash],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let error = db
        .load_import_candidate_preparation(&hash)
        .await
        .expect_err("a migrated or partial candidate is not prepared");
    assert!(error.to_string().contains("no complete prepared asset set"));
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
        db.save_import_candidate_edit_field(&hash, CandidateEditField::PressingYear, "1991")
            .await
            .expect_err("a field with nothing picked"),
        db.save_import_candidate_track_edit(
            &hash,
            &edited_row("import-track-0", "Track Title", None),
        )
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
async fn draft_field_writes_keep_album_and_pressing_years_distinct() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let seed = metadata_draft("Seeded Title", "Artist Name");
    db.replace_candidate_metadata(
        &hash,
        &pane_candidate_path(),
        &seed,
        Some(&release_pick("rel-1")),
    )
    .await
    .unwrap();

    db.save_import_candidate_edit_field(&hash, CandidateEditField::AlbumYear, "1987")
        .await
        .unwrap();
    db.save_import_candidate_edit_field(&hash, CandidateEditField::PressingYear, "1991")
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
    assert_eq!(stored.album_year, "1987");
    assert_eq!(stored.pressing.year, "1991");
    assert_eq!(
        stored.album_artist_assignments,
        seed.album_artist_assignments
    );
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
        &pane_candidate_path(),
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
    db.save_import_candidate_track_edit(&hash, &edit)
        .await
        .unwrap();

    let stored = db.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert_eq!(stored.metadata_draft.tracks[0].title, "Edited title");
    assert_eq!(stored.track_mappings.len(), 1);
    assert_eq!(
        stored.track_mappings[0].file.audio().cloned(),
        edit.file().cloned()
    );
}

/// A file decision reshapes the folder, so the slice measurements, the
/// extracted signals go with it. The caller's replacement mappings, metadata,
/// and cover land with the file decision as one candidate state.
#[tokio::test]
async fn a_file_decision_clears_what_the_reshaped_folder_invalidates() {
    let (db, _tmp) = empty_db().await;
    let (files, hash) = stored_pane_candidate(&db).await;
    let durations = SourceDurations::new(vec![
        file_unit("01 Track.flac", 180_000),
        file_unit("CDImage.flac", 600_000),
        slice_unit(0, 200_000),
    ]);
    assert!(store_verdict(&db, &hash, signals_with(durations)).await);
    db.replace_candidate_metadata(
        &hash,
        &pane_candidate_path(),
        &metadata_draft("Album", "Artist"),
        Some(&release_pick("rel-1")),
    )
    .await
    .unwrap();
    db.save_import_candidate_edit_field(&hash, CandidateEditField::PressingYear, "1991")
        .await
        .unwrap();
    db.save_import_candidate_cover(&hash, &CoverSelection::Local("cover.jpg".to_string()))
        .await
        .unwrap();
    let preparation = db
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the candidate has a stored preparation");
    db.save_import_candidate_track_edits_prepared(
        &host_root("/music"),
        &pane_candidate_path(),
        &hash,
        preparation.file_edit_revision,
        preparation.metadata_revision,
        &[edited_row("candidate-track-0", "Track Title", None)],
        &preparation.source_discogs_artist_ids,
        &preparation.assets.artist_images,
    )
    .await
    .unwrap();
    let mut edits = CandidateFileEdits::default();
    edits
        .file_roles
        .set("CDImage.flac".to_string(), FileRoleChoice::NotATrack);
    let mut settled = files;
    settled.apply_candidate_file_edits(&edits).unwrap();
    let (metadata_revision, mapping_preparation) = current_mapping_preparation(&db, &hash).await;
    db.save_import_candidate_file_edits(
        &hash,
        &pane_candidate_path(),
        0,
        metadata_revision,
        &edits,
        &[(pane_candidate_path(), settled)],
        &mapping_preparation,
    )
    .await
    .unwrap();

    let state = db
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .unwrap();
    assert!(state.signals.is_none(), "the disc ID is recomputed");

    let pane = db.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert_eq!(
        pane.track_mappings, mapping_preparation.track_mappings,
        "the prepared replacement mappings land atomically"
    );
    assert_eq!(
        pane.metadata_draft.pressing.year, "1991",
        "the draft survives"
    );
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
        &pane_candidate_path(),
        &old_draft,
        Some(&release_pick("rel-1")),
    )
    .await
    .unwrap();
    let mut file_edits = CandidateFileEdits::default();
    file_edits
        .file_roles
        .set("CDImage.flac".to_string(), FileRoleChoice::NotATrack);
    let mut settled = files;
    settled.apply_candidate_file_edits(&file_edits).unwrap();
    let (metadata_revision, mapping_preparation) = current_mapping_preparation(&db, &hash).await;
    db.save_import_candidate_file_edits(
        &hash,
        &pane_candidate_path(),
        0,
        metadata_revision,
        &file_edits,
        &[(pane_candidate_path(), settled)],
        &mapping_preparation,
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
    let applied_revision = db
        .replace_candidate_metadata(
            &hash,
            &pane_candidate_path(),
            &new_draft,
            Some(&release_pick("rel-2")),
        )
        .await
        .unwrap();
    let applied = db.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert_eq!(applied.metadata_draft, new_draft);
    assert_eq!(
        applied.track_mappings[0].file.audio().cloned(),
        mapping.file().cloned()
    );
    assert_eq!(applied_revision, 4);
    assert_eq!(
        applied.cover, None,
        "applying a source clears a local cover"
    );
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
    let cleared_revision = db
        .replace_candidate_metadata(
            &hash,
            &pane_candidate_path(),
            &blank,
            None,
        )
        .await
        .unwrap();
    let cleared = db.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert!(cleared.metadata_draft.is_blank());
    assert_eq!(
        cleared.track_mappings[0].file.audio().cloned(),
        mapping.file().cloned()
    );
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
            &pane_candidate_path(),
            &metadata_draft("Album", "Artist"),
            Some(&release_pick("rel-1")),
        )
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        db.save_import_candidate_edit_field(&hash, CandidateEditField::PressingYear, "1991")
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
        db.replace_import_candidate_album_artists(&hash, &[new_artist("Different Artist")],)
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
        &pane_candidate_path(),
        &metadata_draft("Album", "Artist"),
        Some(&release_pick("rel-chosen")),
    )
    .await
    .unwrap();
    db.save_import_candidate_edit_field(&hash, CandidateEditField::PressingYear, "1991")
        .await
        .unwrap();

    assert!(db
        .save_import_candidate_verdict(&NewImportCandidateVerdict {
            content_hash: hash.clone(),
            folder_path: pane_candidate_path(),
            verdict: sample_verdict(),
            signals: signals_with(SourceDurations::default()),
            expected_edit_revision: 0,
            expected_metadata_revision: 2,
            metadata: crate::import::CandidateMetadataDraft {
                edit: metadata_draft("Different album", "Different Artist"),
                track_mappings: Default::default(),
                source_discogs_artist_ids: Default::default(),
                provenance: Some(release_pick("rel-1")),
                cover: None,
                assets: crate::import::CandidatePreparedAssets::default(),
            },
        })
        .await
        .unwrap());

    let state = db
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .unwrap();
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
            folder_path: pane_candidate_path(),
            verdict: sample_verdict(),
            signals: signals_with(SourceDurations::default()),
            expected_edit_revision: 0,
            expected_metadata_revision: 0,
            metadata: crate::import::CandidateMetadataDraft {
                edit: metadata_draft("First album", "Artist"),
                track_mappings: Default::default(),
                source_discogs_artist_ids: Default::default(),
                provenance: Some(first_pick.clone()),
                cover: None,
                assets: crate::import::CandidatePreparedAssets::default(),
            },
        })
        .await
        .unwrap());
    db.save_import_candidate_edit_field(&hash, CandidateEditField::AlbumTitle, "Person's title")
        .await
        .unwrap();

    assert!(!db
        .save_import_candidate_verdict(&NewImportCandidateVerdict {
            content_hash: hash.clone(),
            folder_path: pane_candidate_path(),
            verdict: sample_verdict(),
            signals: signals_with(SourceDurations::default()),
            expected_edit_revision: 0,
            expected_metadata_revision: 1,
            metadata: crate::import::CandidateMetadataDraft {
                edit: metadata_draft("Second album", "Different Artist"),
                track_mappings: Default::default(),
                source_discogs_artist_ids: Default::default(),
                provenance: Some(release_pick("rel-second")),
                cover: None,
                assets: crate::import::CandidatePreparedAssets::default(),
            },
        })
        .await
        .unwrap());

    let state = db
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .unwrap();
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

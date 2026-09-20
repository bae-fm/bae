/// The import reads the folder's own names off the signals identification
/// settled on, so a commit keeps them whatever the draft was read from — a
/// folder identified from its tags still states its barcode. A catalog number
/// out of extraction's pool is one of them only once somebody chose it.
#[tokio::test]
async fn the_preparation_carries_the_names_the_folder_states() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    assert!(
        db.load_import_candidate_preparation(&hash)
            .await
            .unwrap()
            .expect("the scanned candidate is prepared")
            .marks
            .is_empty(),
        "nothing has read the folder yet"
    );

    assert!(
        store_verdict(
            &db,
            &hash,
            Signals {
                barcode: BarcodeSignal::Settled {
                    codes: vec![SourcedValue::in_file(
                        "0075678164521".to_string(),
                        SignalOrigin::Artwork,
                        "back.jpg".to_string(),
                    )],
                },
                text: TextSignal::Settled {
                    catalogs: vec![SourcedValue::new(
                        "7559-60691-2".to_string(),
                        SignalOrigin::FolderName,
                    )],
                    free_text: Vec::new(),
                },
                ..signals_with(SourceDurations::default())
            },
        )
        .await
    );

    let barcode = crate::import::ReleaseMark {
        corroborated: false,
        kind: crate::import::MarkKind::Barcode,
        sighting: SourcedValue::in_file(
            "0075678164521".to_string(),
            SignalOrigin::Artwork,
            "back.jpg".to_string(),
        ),
    };
    let marks = db
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the scanned candidate is prepared")
        .marks;
    assert_eq!(
        marks,
        vec![barcode.clone()],
        "nobody has chosen the folder's catalog number, so the commit keeps none"
    );

    db.save_import_candidate_lookup_choices(
        &hash,
        &crate::import::LookupChoices {
            chosen_catalogs: vec!["7559-60691-2".to_string()],
            ..crate::import::LookupChoices::default()
        },
    )
    .await
    .unwrap();

    let marks = db
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the scanned candidate is prepared")
        .marks;
    assert_eq!(
        marks,
        vec![
            barcode,
            crate::import::ReleaseMark {
                corroborated: false,
                kind: crate::import::MarkKind::CatalogNumber,
                sighting: SourcedValue::new("7559-60691-2".to_string(), SignalOrigin::FolderName,),
            },
        ],
    );
}

/// What the rip databases said about the folder's audio is stored with the
/// candidate's signals and handed to the commit, track by track — the counts
/// and the CRC of the bits they are about.
#[tokio::test]
async fn the_preparation_carries_what_the_rip_databases_said() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    assert!(
        db.load_import_candidate_preparation(&hash)
            .await
            .unwrap()
            .expect("the scanned candidate is prepared")
            .verification
            .is_none(),
        "nothing has read the folder's log yet"
    );

    let verification = crate::import::Verification {
        source: crate::import::VerificationSource::Log,
        tracks: vec![
            crate::import::TrackVerification {
                number: 1,
                accuraterip_confidence: Some(37),
                ctdb_confidence: Some(12),
                crc: Some(0xE94F_69D5),
            },
            crate::import::TrackVerification {
                number: 2,
                accuraterip_confidence: None,
                ctdb_confidence: None,
                crc: None,
            },
        ],
    };
    assert!(
        store_verdict(
            &db,
            &hash,
            Signals {
                verification: Some(verification.clone()),
                ..signals_with(SourceDurations::default())
            },
        )
        .await
    );

    assert_eq!(
        db.load_import_candidate_preparation(&hash)
            .await
            .unwrap()
            .expect("the scanned candidate is prepared")
            .verification,
        Some(verification),
        "every track's counts survive the store, the unverified one included"
    );
}

/// Which name read off the folder tied its files to the record the draft
/// reads is asked of the stored verdict's own match rows, and handed to the
/// commit. A record the run never named — one somebody found by searching —
/// was tied to the folder by nothing.
#[tokio::test]
async fn the_preparation_carries_what_tied_the_files_to_the_record() {
    for (pick, expected) in [
        (release_pick("rel-1"), Some(crate::import::MarkKind::DiscId)),
        (release_pick("rel-searched"), None),
    ] {
        let (db, _tmp) = empty_db().await;
        let (_, hash) = stored_pane_candidate(&db).await;
        assert!(crate::import::CandidatePreparations::new(db.clone())
            .store_verdict(&NewImportCandidateVerdict {
                candidate: as_read(&hash, 0),
                folder_path: pane_candidate_path(),
                verdict: sample_verdict(),
                signals: signals_with(SourceDurations::default()),
                metadata: Some(crate::import::CandidateMetadataDraft {
                    draft: candidate_draft("Album Title", "Artist Name"),
                    source_discogs_artist_ids: Default::default(),
                    provenance: Some(pick),
                    cover: None,
                    assets: crate::import::CandidatePreparedAssets::default(),
                }),
            })
            .await
            .unwrap());

        assert_eq!(
            db.load_import_candidate_preparation(&hash)
                .await
                .unwrap()
                .expect("the scanned candidate is prepared")
                .identified_by,
            expected,
        );
    }
}

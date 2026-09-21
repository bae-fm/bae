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

// Whether the person has seen a candidate's identification result: unread
// when it lands on a candidate nobody has open, read once opened, and read on
// arrival while open.

/// The question the candidate's row flags as unread, as the pane's own read
/// of it places the row.
async fn attention(db: &Database, key: &str) -> Option<crate::identify::NeedsYou> {
    db.load_import_candidate(key)
        .await
        .unwrap()
        .expect("the candidate reads back")
        .resolve(&crate::import::TriageRuntimeFacts::default())
        .row
        .attention
}

/// A candidate with no pick, stored where a scan found it.
async fn unpicked_candidate(db: &Database) -> (String, String) {
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let key = host_root("/music/Album");
    let hash = store_candidate_state(db, &candidate, &key).await;
    (key, hash)
}

/// A run's result asking a question: the lone match lists no tracks.
fn asking(hash: &str, key: &str) -> NewImportCandidateVerdict {
    new_candidate_row(hash, key, &sample_verdict())
}

/// A result stored while nobody has the candidate open is unread, and the
/// row flags its question; opening the candidate reads it.
#[tokio::test]
async fn a_result_nobody_has_open_is_unread_until_the_candidate_is_opened() {
    let (db, _tmp) = empty_db().await;
    let (key, hash) = unpicked_candidate(&db).await;
    let preparations = crate::import::CandidatePreparations::new(db.clone());

    assert!(preparations.store_verdict(&asking(&hash, &key)).await.unwrap());
    assert_eq!(
        attention(&db, &key).await,
        Some(crate::identify::NeedsYou::SourceTracksUnknown)
    );

    let opened = preparations.open_candidate(&key).await.unwrap();
    assert_eq!(attention(&db, &key).await, None);

    drop(opened);
    assert_eq!(
        attention(&db, &key).await,
        None,
        "closing the candidate does not make what it read unread again"
    );
}

/// A result landing on the candidate the person has open is one they are
/// looking at: it arrives read, so the row never flags it.
#[tokio::test]
async fn a_result_landing_on_the_open_candidate_arrives_read() {
    let (db, _tmp) = empty_db().await;
    let (key, hash) = unpicked_candidate(&db).await;
    let preparations = crate::import::CandidatePreparations::new(db.clone());

    let opened = preparations.open_candidate(&key).await.unwrap();
    assert!(preparations.store_verdict(&asking(&hash, &key)).await.unwrap());
    assert_eq!(attention(&db, &key).await, None);

    drop(opened);
    assert!(preparations.store_verdict(&asking(&hash, &key)).await.unwrap());
    assert_eq!(
        attention(&db, &key).await,
        Some(crate::identify::NeedsYou::SourceTracksUnknown),
        "the next run's result, landing with the candidate closed, is unread"
    );
}

/// A write that keeps the stored result — here a typed field — keeps it as
/// read or unread as it stood: it does not carry back the read-ness it
/// loaded.
#[tokio::test]
async fn a_draft_edit_leaves_the_result_as_read_as_it_stood() {
    let (db, _tmp) = empty_db().await;
    let (key, hash) = unpicked_candidate(&db).await;
    let preparations = crate::import::CandidatePreparations::new(db.clone());
    assert!(preparations.store_verdict(&asking(&hash, &key)).await.unwrap());

    preparations
        .set_field(&hash, crate::import::CandidateEditField::AlbumTitle, "Album")
        .await
        .unwrap();
    assert_eq!(
        attention(&db, &key).await,
        Some(crate::identify::NeedsYou::SourceTracksUnknown),
        "still unread"
    );

    drop(preparations.open_candidate(&key).await.unwrap());
    preparations
        .set_field(&hash, crate::import::CandidateEditField::Label, "Label")
        .await
        .unwrap();
    assert_eq!(attention(&db, &key).await, None, "still read");
}

/// A release the person picked, standing as the result of a candidate that
/// had none, is one they reached themselves: nothing in it is unread.
#[tokio::test]
async fn a_pick_standing_as_the_result_is_read() {
    let (db, _tmp) = empty_db().await;
    let (key, hash) = unpicked_candidate(&db).await;
    let preparations = crate::import::CandidatePreparations::new(db.clone());

    preparations
        .apply_source_as_result(
            &host_root("/music"),
            &as_read(&hash, 0),
            &key,
            &crate::import::CandidateMetadataDraft {
                draft: candidate_draft("Album", "Artist"),
                source_discogs_artist_ids: Default::default(),
                provenance: Some(release_pick("rel-1")),
                cover: None,
                assets: crate::import::CandidatePreparedAssets::default(),
            },
            sample_verdict(),
        )
        .await
        .unwrap();

    // The pane cannot be read without the pick's archived documents, which
    // this store never fetched; the stored row says what it says directly.
    let unread = db
        .read(move |sql| {
            Ok(sql.query_row(
                "SELECT unread FROM import_candidate_verdict WHERE content_hash = ?",
                [hash],
                |row| row.get::<_, bool>(0),
            )?)
        })
        .await
        .unwrap();
    assert!(!unread, "the pick is stored as a read result");
}

//! What a row states about its release's identity: the catalogs its
//! documents describe it in, read once so the pane's row and the queue's
//! agree.

use super::*;

/// Which catalogs describe a pick is read off its stored release once:
/// the queue's row and the pane's detail name the same records, in the order
/// surfaces list catalogs — the pick's own, the release its document links,
/// and the two its release group links.
#[tokio::test]
async fn the_row_and_the_pane_name_the_same_records() {
    let (db, _tmp, root) = watched_root().await;
    let candidate = scanned(&db, &root, "Album").await;
    save_verdict(&db, &candidate, "mb-verdict").await;
    db.save_source_release(
        &crate::import::payloads::ReleasePayloads::for_test(
            crate::import::MetadataRef::new(Catalog::MusicBrainz, "mb-linked"),
            musicbrainz_release_linked_out("mb-linked", "mb-group").to_string(),
            vec![crate::import::SourcePayload::new(
                PayloadSource::MusicBrainzReleaseGroup,
                "mb-group",
                release_group_linked_out().to_string(),
            )],
        )
        .extract()
        .unwrap(),
    )
    .await
    .unwrap();
    let draft = db
        .load_import_candidate_pane_rows(&candidate.files.content_hash())
        .await
        .unwrap()
        .draft
        .release_edit();
    crate::import::CandidatePreparations::new(db.clone())
        .replace_metadata(
            &candidate.files.content_hash(),
            &candidate.path.to_string_lossy(),
            &draft,
            Some(&MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(
                    Catalog::MusicBrainz,
                    "mb-linked".to_string(),
                ),
                partners: vec![],
            }),
        )
        .await
        .unwrap();

    let projection = db
        .load_import_list(request(TriageTab::Pending).await)
        .await
        .unwrap();
    let row = rows(&projection).remove(0);
    let crate::import::triage::TriageReading::Identified { records } = &row.reading else {
        panic!("a picked row reads as identified, got {:?}", row.reading);
    };
    assert_eq!(
        records
            .iter()
            .map(|record| (record.catalog(), record.key()))
            .collect::<Vec<_>>(),
        vec![
            (Catalog::MusicBrainz, "mb-linked"),
            (Catalog::Discogs, "4242"),
            (Catalog::AllMusic, "mw0000424242"),
            (Catalog::Wikidata, "Q424242"),
        ]
    );

    let detail = db
        .load_import_candidate(&candidate.path.to_string_lossy())
        .await
        .unwrap()
        .expect("the scanned candidate has a pane")
        .resolve(&crate::import::TriageRuntimeFacts::default());
    assert_eq!(
        detail.row.reading, row.reading,
        "the pane's row names the records the queue's does"
    );
}

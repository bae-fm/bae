//! A save's draft written over the stored one.

use crate::tables::import_state::tests::*;
use crate::CandidateEditField;
use bae_store::test_fixtures::empty_db;
use bae_store::Store;
use rusqlite::params;

/// The list places a row by whether its draft is blank and valid, read off two
/// columns rather than the draft whole; every draft write keeps them current.
#[tokio::test]
async fn every_draft_write_keeps_the_lists_draft_columns_current() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    fetched(&db, "rel-1").await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let columns = |db: &Store, hash: &str| {
        let db = db.clone();
        let hash = hash.to_string();
        async move {
            db.read(move |sql| {
                Ok(sql.query_row(
                    "SELECT draft_blank, draft_valid FROM import_candidate_edit \
                     WHERE content_hash = ?1",
                    params![hash],
                    |row| Ok((row.get::<_, bool>(0)?, row.get::<_, bool>(1)?)),
                )?)
            })
            .await
            .unwrap()
        }
    };
    let preparations = crate::tables::ImportTables::new(db.clone());
    preparations
        .replace_metadata(
            &hash,
            &pane_candidate_key(),
            &metadata_draft("Album Title", "Artist Name"),
            Some(&release_pick("rel-1")),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(columns(&db, &hash).await, (false, true));

    let pane = PaneControls::of_pane_candidate(&db);
    pane.type_field(&hash, CandidateEditField::AlbumYear, "not a year").await.unwrap();
    assert_eq!(
        columns(&db, &hash).await,
        (false, false),
        "a year that does not parse leaves the draft invalid"
    );

    pane.type_field(&hash, CandidateEditField::AlbumYear, "1987").await.unwrap();
    assert_eq!(columns(&db, &hash).await, (false, true));
}

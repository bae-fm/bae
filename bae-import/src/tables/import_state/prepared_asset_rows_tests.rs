//! The bytes prepared for a remote cover, stored with it.

use crate::cover_art::PreparedCover;
use crate::tables::import_state::tests::*;
use bae_catalogs::catalog::Catalog;
use bae_db::test_fixtures::empty_db;
use bae_db::watched_path::host_root;

fn remote_cover() -> PreparedCover {
    PreparedCover::Remote {
        image: bae_catalogs::remote_image::RemoteImageSet::original(
            "https://example.invalid/image".to_string(),
        ),
        source: Catalog::Discogs,
        fetched: bae_catalogs::remote_image::RemoteImage {
            bytes: vec![1, 2, 3, 4],
            content_type: bae_util::content_type::ContentType::Jpeg,
        },
    }
}

#[tokio::test]
async fn a_remote_cover_round_trips_the_exact_prepared_bytes() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;

    crate::CandidatePreparations::new(db.clone())
        .set_prepared_cover(
            &host_root("/music"),
            &pane_candidate_key(),
            &as_read(&hash, 0),
            &remote_cover(),
        )
        .await
        .unwrap();

    assert_eq!(
        crate::preparation::stored_preparation(&tables, &hash)
            .await
            .unwrap()
            .expect("the candidate is prepared")
            .metadata
            .cover,
        Some(remote_cover())
    );
}

/// The cover's save writes its row and its bytes together, so a remote
/// cover whose bytes are gone is a damaged row: reading the candidate fails
/// naming it, and nothing reads the cover as one the import could fetch.
#[tokio::test]
async fn a_remote_cover_stored_without_its_bytes_fails_its_read() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    crate::CandidatePreparations::new(db.clone())
        .set_prepared_cover(
            &host_root("/music"),
            &pane_candidate_key(),
            &as_read(&hash, 0),
            &remote_cover(),
        )
        .await
        .unwrap();
    lose_remote_cover_bytes(&db, &hash).await;

    let error = crate::preparation::stored_preparation(&tables, &hash)
        .await
        .expect_err("a remote cover without its bytes is a damaged row");
    assert!(error.to_string().contains("remote cover has no stored bytes"), "{error}");
}

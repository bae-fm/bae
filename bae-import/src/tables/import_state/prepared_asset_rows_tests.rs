//! The bytes prepared for a remote cover, stored with it.

use crate::tables::import_state::tests::*;
use crate::CoverSelection;
use bae_catalogs::catalog::Catalog;
use bae_db::test_fixtures::empty_db;
use bae_db::watched_path::host_root;

#[tokio::test]
async fn a_remote_cover_round_trips_the_exact_prepared_bytes() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    let cover = CoverSelection::Remote(
        bae_catalogs::remote_image::RemoteImageSet::original(
            "https://example.invalid/image".to_string(),
        ),
        Catalog::Discogs,
    );
    let image = bae_catalogs::remote_image::RemoteImage {
        bytes: vec![1, 2, 3, 4],
        content_type: bae_util::content_type::ContentType::Jpeg,
    };

    crate::CandidatePreparations::new(db.clone())
        .set_prepared_cover(
            &host_root("/music"),
            &pane_candidate_key(),
            &as_read(&hash, 0),
            &cover,
            Some(&image),
        )
        .await
        .unwrap();

    assert_eq!(
        tables.load_import_candidate_prepared_assets(&hash).await.unwrap().remote_cover,
        Some(image)
    );
}

/// A remote cover is stored with the bytes fetched for it, so one whose bytes
/// are missing is a broken row, refused when an import reads it rather than
/// imported without its cover.
#[tokio::test]
async fn a_remote_cover_stored_without_its_bytes_is_refused_on_read() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    crate::CandidatePreparations::new(db.clone())
        .set_prepared_cover(
            &host_root("/music"),
            &pane_candidate_key(),
            &as_read(&hash, 0),
            &CoverSelection::Remote(
                bae_catalogs::remote_image::RemoteImageSet::original(
                    "https://example.invalid/cover".to_string(),
                ),
                Catalog::Discogs,
            ),
            Some(&bae_catalogs::remote_image::RemoteImage {
                bytes: vec![1, 2, 3],
                content_type: bae_util::content_type::ContentType::Jpeg,
            }),
        )
        .await
        .unwrap();
    let deleted_hash = hash.clone();
    db.call(move |sql| {
        sql.execute(
            "DELETE FROM import_candidate_remote_cover_asset WHERE content_hash = ?",
            [&deleted_hash],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let error = tables
        .load_import_candidate_preparation(&hash)
        .await
        .expect_err("a remote cover without its bytes is not ready to import");
    assert!(error.to_string().contains("a remote candidate cover has no prepared bytes"));
}

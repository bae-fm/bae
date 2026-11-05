//! The candidate's cover choice as stored and read back, a remote one with
//! its copies and the bytes prepared for it.

use crate::cover_art::PreparedCover;
use crate::tables::import_state::tests::*;
use crate::tables::watched_path::host_root;
use bae_catalogs::catalog::Catalog;
use bae_store::test_fixtures::empty_db;

fn remote_with_copies(url: &str) -> PreparedCover {
    use bae_catalogs::remote_image::{DownscaledCopy, RemoteImageSet};
    PreparedCover::Remote {
        image: RemoteImageSet::with_copies(
            url.to_string(),
            [250, 500]
                .map(|max_edge| DownscaledCopy { url: format!("{url}-{max_edge}"), max_edge })
                .to_vec(),
        ),
        source: Catalog::MusicBrainz,
        fetched: fetched_bytes(),
    }
}

/// The bytes a remote cover is stored with, fetched with it; none for a cover
/// the folder holds.
fn fetched_bytes() -> bae_catalogs::remote_image::RemoteImage {
    bae_catalogs::remote_image::RemoteImage {
        bytes: vec![1, 2, 3],
        content_type: bae_util::content_type::ContentType::Jpeg,
    }
}

/// Every kind of cover choice comes back as it went in, a remote one with its
/// downscaled copies.
#[tokio::test]
async fn a_cover_choice_round_trips_in_every_shape() {
    for cover in [
        PreparedCover::Local("cover.jpg".to_string()),
        PreparedCover::Remote {
            image: bae_catalogs::remote_image::RemoteImageSet::original(
                "https://example.invalid/front".to_string(),
            ),
            source: Catalog::Discogs,
            fetched: fetched_bytes(),
        },
        remote_with_copies("https://example.invalid/front"),
    ] {
        let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
        let tables = crate::tables::ImportTables::new(db.clone());
        let (_, hash) = stored_pane_candidate(&db).await;

        PaneControls::of_pane_candidate(&db).choose_cover(&hash, &cover).await.unwrap();

        assert_eq!(
            tables.load_import_candidate_pane_rows(&hash).await.unwrap().cover,
            Some(cover.selection())
        );
    }
}

/// Choosing again replaces the previous choice's copies rather than adding the
/// new ones beside them.
#[tokio::test]
async fn a_new_cover_choice_leaves_none_of_the_old_copies() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    let pane = PaneControls::of_pane_candidate(&db);

    for cover in [
        remote_with_copies("https://example.invalid/first"),
        PreparedCover::Local("cover.jpg".to_string()),
        remote_with_copies("https://example.invalid/second"),
    ] {
        pane.choose_cover(&hash, &cover).await.unwrap();
        assert_eq!(
            tables.load_import_candidate_pane_rows(&hash).await.unwrap().cover,
            Some(cover.selection())
        );
    }
}

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
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
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
        crate::preparations::stored_preparation(&tables, &hash)
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
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
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

    let error = crate::preparations::stored_preparation(&tables, &hash)
        .await
        .expect_err("a remote cover without its bytes is a damaged row");
    assert!(error.to_string().contains("remote cover has no stored bytes"), "{error}");
}

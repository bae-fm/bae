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

/// A cover row holds a file, or a catalog's address, the catalog and the
/// bytes it served, as its kind says, and never a part of the other: a
/// folder's or a tag's cover with any of a catalog's is refused, as is a
/// catalog's cover missing one, its bytes included.
#[tokio::test]
async fn a_cover_row_holds_only_what_its_kind_names() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let url = Some("https://example.invalid/front");
    let source = Some("discogs");
    let content_type = Some("image/jpeg");
    let bytes = Some(vec![1_u8, 2, 3]);
    for (kind, file_id, url, source, content_type, bytes) in [
        ("local", Some("cover.jpg"), url, None, None, None),
        ("local", Some("cover.jpg"), None, source, None, None),
        ("local", Some("cover.jpg"), None, None, content_type, bytes.clone()),
        ("embedded", Some("01.flac"), url, None, None, None),
        ("embedded", Some("01.flac"), None, source, None, None),
        ("embedded", Some("01.flac"), None, None, content_type, bytes.clone()),
        ("remote", None, url, None, content_type, bytes.clone()),
        ("remote", None, None, source, content_type, bytes.clone()),
        ("remote", Some("cover.jpg"), url, source, content_type, bytes.clone()),
        ("remote", None, url, source, None, None),
        ("remote", None, url, source, content_type, None),
    ] {
        let hash = hash.clone();
        let refused = db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO import_candidate_cover \
                         (content_hash, kind, file_id, url, source, content_type, bytes) \
                     VALUES (?, ?, ?, ?, ?, ?, ?)",
                    rusqlite::params![hash, kind, file_id, url, source, content_type, bytes],
                )
                .map_err(bae_store::DbError::from)
            })
            .await
            .expect_err("the row states what its kind does not hold");
        assert!(
            refused.to_string().contains("CHECK constraint failed"),
            "{kind} {file_id:?} {url:?} {source:?} {content_type:?}: {refused}"
        );
    }
}

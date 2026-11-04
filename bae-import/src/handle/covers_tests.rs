//! The covers the picker offers: those the catalogs show for the candidate's
//! picked releases or album as stored, none for a draft read from the tags or
//! an unlinked candidate, and a library release's from its records.

use super::*;
use crate::handle::tests::*;
use bae_catalogs::fake_catalogs::FakeCatalogs;
use bae_catalogs::test_releases::seed_mb_release_with_front;
use bae_catalogs::test_releases::seed_two_track_discogs_release;
use tempfile::TempDir;

/// The fixture candidate, picked as `release_id`, whose release the Cover
/// Art Archive states a front image for. The archive holds no image there,
/// so the pick fetches none and the folder's own cover stands.
async fn picked_with_front(
    release_id: &str,
) -> (ImportServiceHandle, TempDir, crate::CandidateKey, String) {
    let archive = bae_util::http::serve_not_found().await;
    let fakes =
        FakeCatalogs::over(bae_util::http::Http::for_test().serve("coverartarchive.org", &archive));
    let (handle, tmp, key, hash) = pane_fixture_with(fakes.providers()).await;
    seed_mb_release_with_front(&fakes, release_id, "CAT-1");
    pick(&handle, &key, release_id).await;
    (handle, tmp, key, hash)
}

/// The addresses of the covers the pane offers the picker, in order.
async fn offered(handle: &ImportServiceHandle, key: &crate::CandidateKey) -> Vec<String> {
    pane(handle, key)
        .await
        .draft()
        .unwrap()
        .remote_covers
        .clone()
        .into_iter()
        .map(|cover| cover.image.url)
        .collect()
}

fn front_of(release_id: &str) -> String {
    format!("{}/release/{release_id}/front", bae_catalogs::remote_image::ARCHIVE)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pick_offers_its_releases_covers() {
    let (handle, _tmp, key, _hash) = picked_with_front("offering-release").await;
    assert_eq!(offered(&handle, &key).await, [front_of("offering-release")]);
    shut_down(handle).await;
}

/// Reading the draft from the files' tags keeps the link, and the draft is no
/// longer the pressing's, so it is offered none of its covers.
#[tokio::test(flavor = "multi_thread")]
async fn a_draft_read_from_the_tags_offers_no_catalog_cover() {
    let (handle, _tmp, key, _hash) = picked_with_front("tags-release").await;
    handle.select_candidate_file_tags(key.clone()).await.unwrap();
    assert!(pane(&handle, &key).await.draft().unwrap().release_link.is_some());
    assert_eq!(offered(&handle, &key).await, Vec::<String>::new());
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn unlinking_offers_no_catalog_cover() {
    let (handle, _tmp, key, _hash) = picked_with_front("unlinked-release").await;
    handle.unlink_candidate_release(key.clone()).await.unwrap();
    assert_eq!(offered(&handle, &key).await, Vec::<String>::new());
    shut_down(handle).await;
}

/// Linking the album its pressings are of keeps the draft read from the
/// pressing picked before, and an album offers no pressing's covers.
#[tokio::test(flavor = "multi_thread")]
async fn linking_the_album_offers_no_catalog_cover() {
    let archive = bae_util::http::serve_not_found().await;
    let fakes =
        FakeCatalogs::over(bae_util::http::Http::for_test().serve("coverartarchive.org", &archive));
    let (handle, _tmp, key, hash) = pane_fixture_with(fakes.providers()).await;
    store_two_pressings(&handle, &fakes, &hash).await;
    seed_mb_release_with_front(&fakes, "mb-pressing-1", "CAT-100");
    pick(&handle, &key, "mb-pressing-1").await;
    assert_eq!(offered(&handle, &key).await, [front_of("mb-pressing-1")]);

    handle.link_candidate_shared_album(key.clone()).await.unwrap();

    assert_eq!(offered(&handle, &key).await, Vec::<String>::new());
    shut_down(handle).await;
}

/// The offer is read from the stored release as it stands, so a later
/// fetch that stored another image shows it.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_stored_again_offers_what_it_holds_now() {
    let (handle, _tmp, key, _hash) = picked_with_front("refetched-release").await;
    let tables = handle.tables.clone();
    let record = bae_catalogs::records::LookupRef::new(
        bae_catalogs::catalog::LookupCatalog::MusicBrainz,
        "refetched-release".to_string(),
    );
    let mut stored = handle
        .tables
        .load_source_release(&record)
        .await
        .unwrap()
        .expect("the pick stored its release");
    stored.covers.release.push(bae_catalogs::remote_image::RemoteCover {
        image: bae_catalogs::remote_image::RemoteImageSet::original(
            "https://images.example/back.jpg".to_string(),
        ),
        label: "Back".to_string(),
        source: bae_catalogs::catalog::Catalog::MusicBrainz,
        standing: bae_catalogs::remote_image::CoverStanding::Stated,
    });
    tables.save_source_release(&stored).await.unwrap();
    assert_eq!(
        offered(&handle, &key).await,
        [front_of("refetched-release"), "https://images.example/back.jpg".to_string()]
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn unlinked_cover_gallery_for_candidate_without_metadata() {
    let StoredCandidate { handle, key, tmp: _tmp, .. } = stored_candidate().await;
    let gallery = handle.fetch_remote_covers(CoverTarget::Candidate(key)).await.unwrap();
    shut_down(handle).await;
    assert_eq!(gallery, RemoteCoverGallery::Unlinked);
}

#[tokio::test(flavor = "multi_thread")]
async fn unlinked_cover_gallery_for_file_tags() {
    let (handle, _tmp, key, _) = pane_fixture().await;
    let gallery = handle.fetch_remote_covers(CoverTarget::Candidate(key)).await.unwrap();
    shut_down(handle).await;
    assert_eq!(gallery, RemoteCoverGallery::Unlinked);
}

#[tokio::test(flavor = "multi_thread")]
async fn unlinked_cover_gallery_for_library_release() {
    let (parts, database, _tmp) = setup_test_import().await;
    let artist = make_artist("Artist Name", None, None);
    database.insert_artist(&artist).await.unwrap();
    let (_album, release) = insert_library_release(&database, &artist.id, "Album Title").await;
    let handle = parts.start().await.unwrap();
    let gallery = handle.fetch_remote_covers(CoverTarget::Release(release.id)).await.unwrap();
    shut_down(handle).await;
    assert_eq!(gallery, RemoteCoverGallery::Unlinked);
}

#[tokio::test(flavor = "multi_thread")]
async fn cover_gallery_reports_unconfigured_discogs_as_failure() {
    let (parts, database, _tmp) = setup_test_import().await;
    let artist = make_artist("Artist Name", None, None);
    database.insert_artist(&artist).await.unwrap();
    let (_album, release) = insert_library_release(&database, &artist.id, "Album Title").await;
    database
        .insert_release_records(
            &release.id,
            &[bae_catalogs::records::ReleaseRecord::new(
                &bae_catalogs::records::MetadataRef::new(
                    bae_catalogs::catalog::Catalog::Discogs,
                    "22",
                ),
                Some("11".to_string()),
            )],
        )
        .await
        .unwrap();
    let handle = parts.start().await.unwrap();
    let result = handle.fetch_remote_covers(CoverTarget::Release(release.id)).await;
    shut_down(handle).await;
    assert!(matches!(
        result,
        Err(crate::ImportError::Catalog(bae_catalogs::CatalogError::DiscogsNotConfigured))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn linked_cover_gallery_can_be_empty() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, _) = pane_fixture_with(fakes.providers()).await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    let release_id = "70000003";
    seed_two_track_discogs_release(&fakes, release_id);
    handle
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    release_id.to_string(),
                ),
                partners: vec![],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    let gallery = handle.fetch_remote_covers(CoverTarget::Candidate(key)).await.unwrap();
    shut_down(handle).await;
    assert_eq!(gallery, RemoteCoverGallery::Linked(vec![]));
}

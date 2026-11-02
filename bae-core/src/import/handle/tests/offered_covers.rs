//! The covers the picker offers are the linked pressing's, read from its
//! stored releases, while the draft is the one read from that pressing.

use super::*;

/// The fixture candidate, picked as `release_id`, whose release the Cover
/// Art Archive states a front image for. The archive holds no image there,
/// so the pick fetches none and the folder's own cover stands.
async fn picked_with_front(
    release_id: &str,
) -> (ImportServiceHandle, TempDir, crate::import::CandidateKey, String) {
    let archive = bae_util::http::serve_not_found().await;
    let (handle, tmp, key, hash) =
        pane_fixture_with(bae_util::http::Http::for_test().serve("coverartarchive.org", &archive))
            .await;
    super::lookup_choices::seed_mb_release_with_front(
        handle.catalogs.providers(),
        release_id,
        "CAT-1",
    );
    super::lookup_choices::pick(&handle, &key, release_id).await;
    (handle, tmp, key, hash)
}

/// The addresses of the covers the pane offers the picker, in order.
async fn offered(handle: &ImportServiceHandle, key: &crate::import::CandidateKey) -> Vec<String> {
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
    let (handle, _tmp, key, hash) =
        pane_fixture_with(bae_util::http::Http::for_test().serve("coverartarchive.org", &archive))
            .await;
    super::shared_album::store_two_pressings(&handle, &hash).await;
    super::lookup_choices::seed_mb_release_with_front(
        handle.catalogs.providers(),
        "mb-pressing-1",
        "CAT-100",
    );
    super::lookup_choices::pick(&handle, &key, "mb-pressing-1").await;
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
    let record = crate::catalog_model::records::LookupRef::new(
        bae_catalogs::catalog::LookupCatalog::MusicBrainz,
        "refetched-release".to_string(),
    );
    let mut stored = handle
        .database
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
    handle.database.save_source_release(&stored).await.unwrap();
    assert_eq!(
        offered(&handle, &key).await,
        [front_of("refetched-release"), "https://images.example/back.jpg".to_string()]
    );
    shut_down(handle).await;
}

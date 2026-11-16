//! The covers the picker offers: those the catalogs show for the candidate's
//! picked releases or album as stored, none for a draft read from the tags or
//! an unlinked candidate, what a fetch stored for the candidate's link, the
//! folder's images, and a library release's from its records.

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
    let http = bae_util::http::Http::for_test().serve("coverartarchive.org", &archive);
    let fakes = FakeCatalogs::over(http.clone());
    let (handle, tmp, key, hash) = pane_fixture_with(fakes.providers(), http).await;
    seed_mb_release_with_front(&fakes, release_id, "CAT-1");
    pick(&handle, &key, release_id).await;
    (handle, tmp, key, hash)
}

/// The gallery the picker of the candidate at `key` shows under `filter` now.
async fn candidate_gallery(
    handle: &ImportServiceHandle,
    key: &crate::CandidateKey,
    filter: crate::CoverFilter,
) -> crate::CoverGallery {
    focused_gallery(handle, key, filter, None).await
}

/// The gallery the picker of the candidate at `key` shows under `filter`
/// now, asked to focus the cover `focus` names.
async fn focused_gallery(
    handle: &ImportServiceHandle,
    key: &crate::CandidateKey,
    filter: crate::CoverFilter,
    focus: Option<crate::CoverKey>,
) -> crate::CoverGallery {
    let asked = crate::CoverGalleryRequest {
        owner: crate::CoverGalleryOwner::Candidate(key.clone()),
        filter,
        focus,
    };
    let mut live = handle.subscribe_cover_gallery(Some(asked.clone()));
    loop {
        let event = live.next_inline().await.expect("the read runs");
        if event.request.as_ref() == Some(&asked) {
            break event.result.unwrap().expect("the live query answers the request").gallery;
        }
    }
}

/// Each section the gallery shows: its source, its count and its covers'
/// names, a catalog's cover by its address.
fn sections(gallery: &crate::CoverGallery) -> Vec<(crate::CoverSectionSource, u32, Vec<String>)> {
    gallery
        .sections
        .iter()
        .map(|section| {
            let names = section
                .items
                .iter()
                .map(|item| match (&item.label, &item.image) {
                    (crate::GalleryLabel::File { name, .. }, _) => name.clone(),
                    (_, crate::CoverImageSource::Remote { image }) => image.url.clone(),
                    other => panic!("a candidate's gallery holds {other:?}"),
                })
                .collect();
            (section.source, section.count, names)
        })
        .collect()
}

/// The addresses of the catalogs' covers the candidate's picker offers, in
/// order.
async fn offered(handle: &ImportServiceHandle, key: &crate::CandidateKey) -> Vec<String> {
    let gallery = candidate_gallery(
        handle,
        key,
        crate::CoverFilter::Catalog(bae_catalogs::catalog::Catalog::MusicBrainz),
    )
    .await;
    match sections(&gallery).as_slice() {
        [(crate::CoverSectionSource::Remote, _, urls)] => urls.clone(),
        other => panic!("a catalog's filter shows the remote section alone, not {other:?}"),
    }
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
    let http = bae_util::http::Http::for_test().serve("coverartarchive.org", &archive);
    let fakes = FakeCatalogs::over(http.clone());
    let (handle, _tmp, key, hash) = pane_fixture_with(fakes.providers(), http).await;
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
    let tables = handle.services.tables.clone();
    let record = bae_catalogs::records::LookupRef::new(
        bae_catalogs::catalog::LookupCatalog::MusicBrainz,
        "refetched-release".to_string(),
    );
    let mut stored = handle
        .services
        .tables
        .load_source_release(&record)
        .await
        .unwrap()
        .expect("the pick stored its release");
    stored.covers.release.push(bae_catalogs::remote_image::RemoteCover {
        image: bae_catalogs::remote_image::RemoteImageSet::original(
            "https://images.example/back.jpg".to_string(),
        ),
        label: bae_catalogs::remote_image::CoverLabel::Lead { release_group: false },
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
    handle.refresh_candidate_covers(&key).await.unwrap();
    let gallery = candidate_gallery(&handle, &key, crate::CoverFilter::All).await;
    shut_down(handle).await;
    assert_eq!(gallery.fetch, crate::CoverFetch::Unlinked);
}

#[tokio::test(flavor = "multi_thread")]
async fn unlinked_cover_gallery_for_file_tags() {
    let (handle, _tmp, key, _) = pane_fixture().await;
    handle.refresh_candidate_covers(&key).await.unwrap();
    let gallery = candidate_gallery(&handle, &key, crate::CoverFilter::All).await;
    shut_down(handle).await;
    assert_eq!(gallery.fetch, crate::CoverFetch::Unlinked);
}

/// The candidate's picker counts what each source shows: the folder's image
/// under all and the release's files, and no catalog cover for a candidate
/// linked to nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_candidates_picker_counts_what_each_source_shows() {
    use crate::CoverSectionSource::{ReleaseFiles, Remote};
    let (handle, _tmp, key, _) = pane_fixture().await;
    let all = candidate_gallery(&handle, &key, crate::CoverFilter::All).await;
    let files = candidate_gallery(&handle, &key, crate::CoverFilter::ReleaseFiles).await;
    let discogs = candidate_gallery(
        &handle,
        &key,
        crate::CoverFilter::Catalog(bae_catalogs::catalog::Catalog::Discogs),
    )
    .await;
    shut_down(handle).await;
    assert_eq!(all.fetch, crate::CoverFetch::NotFetched);
    assert_eq!(all.current, None);
    assert_eq!(
        (sections(&all), all.total),
        (vec![(Remote, 0, vec![]), (ReleaseFiles, 1, vec!["cover.jpg".to_string()])], 1)
    );
    assert_eq!(
        (sections(&files), files.total),
        (vec![(ReleaseFiles, 1, vec!["cover.jpg".to_string()])], 1)
    );
    assert_eq!((sections(&discogs), discogs.total), (vec![(Remote, 0, vec![])], 0));
}

/// While the candidate keeps the link bae fetched the covers for, the picker
/// offers those covers in place of the pressing's own; once the link changes,
/// the picker shows no fetch until the next one.
#[tokio::test(flavor = "multi_thread")]
async fn a_fetch_shows_while_the_link_it_was_fetched_for_stands() {
    let (handle, _tmp, key, hash) = picked_with_front("fetched-release").await;
    let tables = handle.services.tables.clone();
    let link = tables.candidate_cover_link(&hash).await.unwrap();
    assert!(link.is_some(), "the pick linked the candidate");
    let fetched = bae_catalogs::remote_image::RemoteCover {
        image: bae_catalogs::remote_image::RemoteImageSet::original(
            "https://images.example/fetched.jpg".to_string(),
        ),
        label: bae_catalogs::remote_image::CoverLabel::Lead { release_group: false },
        source: bae_catalogs::catalog::Catalog::MusicBrainz,
        standing: bae_catalogs::remote_image::CoverStanding::Stated,
    };
    tables.replace_candidate_covers(&hash, link, Some(vec![fetched])).await.unwrap();
    assert_eq!(offered(&handle, &key).await, ["https://images.example/fetched.jpg".to_string()]);
    assert_eq!(
        candidate_gallery(&handle, &key, crate::CoverFilter::All).await.fetch,
        crate::CoverFetch::Linked
    );

    handle.unlink_candidate_release(key.clone()).await.unwrap();

    let gallery = candidate_gallery(&handle, &key, crate::CoverFilter::All).await;
    shut_down(handle).await;
    assert_eq!(gallery.fetch, crate::CoverFetch::NotFetched);
    assert_eq!(sections(&gallery)[0], (crate::CoverSectionSource::Remote, 0, vec![]));
}

/// A candidate's picker focuses the cover its draft chose, after the
/// catalogs' covers, unless the request names another cover it shows.
#[tokio::test(flavor = "multi_thread")]
async fn a_candidates_picker_focuses_its_chosen_cover_unless_asked() {
    use crate::CoverKey::{File, Remote};
    let (handle, _tmp, key, hash) = picked_with_front("focused-release").await;
    let tables = handle.services.tables.clone();
    let link = tables.candidate_cover_link(&hash).await.unwrap();
    let cover = |url: &str| bae_catalogs::remote_image::RemoteCover {
        image: bae_catalogs::remote_image::RemoteImageSet::original(url.to_string()),
        label: bae_catalogs::remote_image::CoverLabel::Lead { release_group: false },
        source: bae_catalogs::catalog::Catalog::MusicBrainz,
        standing: bae_catalogs::remote_image::CoverStanding::Stated,
    };
    let (front, back) = ("https://images.example/front.jpg", "https://images.example/back.jpg");
    tables
        .replace_candidate_covers(&hash, link, Some(vec![cover(front), cover(back)]))
        .await
        .unwrap();

    let own = focused_gallery(&handle, &key, crate::CoverFilter::All, None).await;
    let asked =
        focused_gallery(&handle, &key, crate::CoverFilter::All, Some(Remote(back.to_string())))
            .await;
    shut_down(handle).await;

    let own = own.focus.expect("the picker shows covers");
    assert_eq!(
        (own.cover.key, own.position, own.neighbors),
        (
            File("cover.jpg".to_string()),
            2,
            Some(crate::Neighbors {
                previous: Remote(back.to_string()),
                next: Remote(front.to_string())
            })
        )
    );
    let asked = asked.focus.expect("the picker shows covers");
    assert_eq!((asked.cover.key, asked.position), (Remote(back.to_string()), 1));
}

#[tokio::test(flavor = "multi_thread")]
async fn unlinked_cover_gallery_for_library_release() {
    let (parts, records, _tmp) = setup_test_import().await;
    let artist = make_artist("Artist Name", None, None);
    bae_records::test_fixtures::test_records(&records).insert_artist(&artist).await.unwrap();
    let (_album, release) = insert_library_release(&records, &artist.id, "Album Title").await;
    let handle = parts.start().await.unwrap();
    handle.refresh_release_covers(&release.id).await.unwrap();
    let asked = crate::CoverGalleryRequest {
        owner: crate::CoverGalleryOwner::Release(release.id.clone()),
        filter: crate::CoverFilter::All,
        focus: None,
    };
    let mut live = handle.subscribe_cover_gallery(Some(asked.clone()));
    let gallery = loop {
        let event = live.next_inline().await.expect("the read runs");
        if event.request.as_ref() == Some(&asked) {
            break event.result.unwrap().expect("the live query answers the request").gallery;
        }
    };
    shut_down(handle).await;
    assert_eq!(gallery.fetch, crate::CoverFetch::Unlinked);
    assert_eq!(gallery.total, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn cover_gallery_reports_unconfigured_discogs_as_failure() {
    let (parts, records, _tmp) = setup_test_import().await;
    let artist = make_artist("Artist Name", None, None);
    bae_records::test_fixtures::test_records(&records).insert_artist(&artist).await.unwrap();
    let (_album, release) = insert_library_release(&records, &artist.id, "Album Title").await;
    bae_records::test_fixtures::test_records(&records)
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
    let result = handle.refresh_release_covers(&release.id).await;
    let asked = crate::CoverGalleryRequest {
        owner: crate::CoverGalleryOwner::Release(release.id.clone()),
        filter: crate::CoverFilter::All,
        focus: None,
    };
    let mut live = handle.subscribe_cover_gallery(Some(asked.clone()));
    let gallery = loop {
        let event = live.next_inline().await.expect("the read runs");
        if event.request.as_ref() == Some(&asked) {
            break event.result.unwrap().expect("the live query answers the request").gallery;
        }
    };
    shut_down(handle).await;
    assert!(matches!(
        result,
        Err(crate::ImportError::Catalog(bae_catalogs::CatalogError::DiscogsNotConfigured))
    ));
    // The failed fetch stored no answer.
    assert_eq!(gallery.fetch, crate::CoverFetch::NotFetched);
}

#[tokio::test(flavor = "multi_thread")]
async fn linked_cover_gallery_can_be_empty() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, _) =
        pane_fixture_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    handle
        .services
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
    handle.refresh_candidate_covers(&key).await.unwrap();
    let gallery = candidate_gallery(&handle, &key, crate::CoverFilter::All).await;
    shut_down(handle).await;
    assert_eq!(gallery.fetch, crate::CoverFetch::Linked);
    assert_eq!(sections(&gallery)[0], (crate::CoverSectionSource::Remote, 0, vec![]));
}

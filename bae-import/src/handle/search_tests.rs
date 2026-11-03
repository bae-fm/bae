//! Picking a release: what a pick of a paired pressing stores and reads, the
//! catalog reads it reports, and a pick running to its end once asked for.

use super::*;
use crate::handle::tests::*;
use bae_catalogs::fake_catalogs::FakeCatalogs;
use bae_catalogs::test_releases::seed_two_track_discogs_release;
use bae_catalogs::test_releases::seed_two_track_discogs_release_in;
use bae_catalogs::test_releases::seed_two_track_mb_release;
use bae_util::test_channels::HANG_GUARD;

/// A pick is a person's decision about the folder, so it runs to its end once
/// asked for: the bridge drops the call's future when the person looks at
/// another candidate, and coven would commit a write that was already
/// dispatched without anyone left to announce it. Both the write and its
/// announcement have to land whatever happens to the caller.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_lands_and_is_announced_when_its_caller_is_torn_down() {
    use std::future::Future;

    let StoredCandidate { handle, key, tmp: _tmp, .. } = stored_candidate().await;
    let mut events = handle.every_event();

    // One poll asks for the pick; dropping the future at the end of the block
    // is the caller being torn down.
    {
        let mut pick = std::pin::pin!(handle.select_candidate_file_tags(key.clone()));
        let first_poll =
            std::future::poll_fn(|cx| std::task::Poll::Ready(pick.as_mut().poll(cx))).await;
        assert!(first_poll.is_pending(), "the first poll asks for the pick and waits on it");
    }

    tokio::time::timeout(HANG_GUARD, async {
        loop {
            match events.recv().await {
                Some(ImportEvent::Scan(ScanEvent::CandidateMetadataChanged { candidate_key }))
                    if candidate_key == key =>
                {
                    break;
                }
                Some(_) => continue,
                None => panic!("the import bus closed"),
            }
        }
    })
    .await
    .expect("the pick is announced");
    assert_eq!(
        pane(&handle, &key).await.draft().unwrap().metadata_provenance,
        Some(crate::MetadataProvenance::FileMetadata),
        "the pick landed"
    );
    shut_down(handle).await;
}

/// Picking a paired pressing claims both sources. The partner is stored beside
/// the primary, and its own documents are archived in the same apply — so
/// opening the candidate, importing it, or re-reading its identity later needs
/// no network.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_with_a_partner_stores_it_and_archives_its_documents() {
    // The pick offers the partner's album address, which the archive holds
    // no image at.
    let archive = bae_util::http::serve_not_found().await;
    let fakes =
        FakeCatalogs::over(bae_util::http::Http::for_test().serve("coverartarchive.org", &archive));
    let (handle, _tmp, key, hash) = pane_fixture_with(fakes.providers()).await;
    let tables = handle.tables.clone();
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();

    let discogs_release_id = "70000001";
    seed_two_track_discogs_release(&fakes, discogs_release_id);
    let mb_release_id = "partner-mb-rel-1";
    seed_two_track_mb_release(&fakes, mb_release_id, "partner-mb-group-1");
    let partner = bae_catalogs::records::LookupRef::new(
        bae_catalogs::catalog::LookupCatalog::MusicBrainz,
        mb_release_id.to_string(),
    );

    handle
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    discogs_release_id.to_string(),
                ),
                partners: vec![partner.clone()],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();

    let stored = tables
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .expect("the candidate row reads back");
    assert_eq!(
        stored.release_link,
        Some(crate::ReleaseLink::Pressing(bae_identify::PressingLink {
            record: bae_catalogs::records::LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::Discogs,
                discogs_release_id.to_string()
            ),
            partners: vec![partner.clone()],
        })),
        "the partner reads back with the pick that claimed it"
    );

    assert!(
        tables.load_source_release(&partner).await.unwrap().is_some(),
        "the partner's own release is stored by the apply"
    );
    assert_eq!(
        crate::preparation::import_preparation(&handle.tables, &hash)
            .await
            .unwrap()
            .expect("the picked candidate is prepared")
            .cover,
        Some(crate::CoverSelection::Local("cover.jpg".to_string())),
        "neither claimed release holds an image, so the folder's cover stands"
    );
    shut_down(handle).await;
}

/// A pick naming a partner that will not prepare stores nothing: the
/// provenance the candidate had stands, rather than a pick naming a source
/// with no documents behind it. Here Discogs has no key, so the partner is
/// unreachable while the primary reads fine.
#[tokio::test(flavor = "multi_thread")]
async fn a_partner_that_will_not_prepare_fails_the_apply() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, hash) = pane_fixture_with(fakes.providers()).await;
    let tables = handle.tables.clone();

    let mb_release_id = "unpaired-mb-rel-1";
    seed_two_track_mb_release(&fakes, mb_release_id, "unpaired-mb-group-1");

    let before = tables
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .expect("the candidate row reads back")
        .metadata_provenance;

    handle
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    mb_release_id.to_string(),
                ),
                partners: vec![bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    "70000002",
                )],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .expect_err("a partner that will not prepare fails the apply");

    assert_eq!(
        tables
            .load_import_candidate_state(&hash)
            .await
            .unwrap()
            .expect("the candidate row reads back")
            .metadata_provenance,
        before,
        "the failed apply stored nothing"
    );
    shut_down(handle).await;
}

/// A pick names one release per source. A second claim about a source the pick
/// already names is two answers to one question, and it would silently replace
/// the identity the primary document states, so it is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_partner_repeating_the_primary_source_is_refused() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, hash) = pane_fixture_with(fakes.providers()).await;
    let tables = handle.tables.clone();

    let mb_release_id = "repeat-mb-rel-1";
    seed_two_track_mb_release(&fakes, mb_release_id, "repeat-mb-group-1");

    let before = tables
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .expect("the candidate row reads back")
        .metadata_provenance;

    handle
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    mb_release_id.to_string(),
                ),
                partners: vec![bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    "repeat-mb-rel-2",
                )],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .expect_err("two MusicBrainz releases for one pressing is refused");

    assert_eq!(
        tables
            .load_import_candidate_state(&hash)
            .await
            .unwrap()
            .expect("the candidate row reads back")
            .metadata_provenance,
        before,
        "the refused apply stored nothing"
    );
    shut_down(handle).await;
}

/// A Discogs release under a master, which MusicBrainz's lookup by its
/// address links to a MusicBrainz release, seeded into the caches a read asks.
fn seed_linked_discogs_release(fakes: &FakeCatalogs) -> bae_catalogs::records::LookupRef {
    seed_two_track_discogs_release_in(fakes, "70000007", Some("70000008"), Some("linked-mb-rel-1"));
    seed_two_track_mb_release(fakes, "linked-mb-rel-1", "linked-mb-group-1");
    bae_catalogs::records::LookupRef::new(bae_catalogs::catalog::LookupCatalog::Discogs, "70000007")
}

/// An identify run reads a release only as far as ranking it needs. Picking
/// that release reads the rest before the pick applies it, so the release the
/// pick stores is the one a complete read from scratch stores.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_of_a_release_a_run_read_reads_the_rest() {
    let archive = bae_util::http::serve_not_found().await;
    let fakes =
        FakeCatalogs::over(bae_util::http::Http::for_test().serve("coverartarchive.org", &archive));
    let (handle, _tmp, key, _) = pane_fixture_with(fakes.providers()).await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    let release = seed_linked_discogs_release(&fakes);

    let read_by_the_run = handle
        .release_reads
        .release(
            &release,
            bae_catalogs::payloads::ReadDepth::Identification,
            bae_catalogs::rate_limiter::CallPriority::Interactive,
        )
        .await
        .unwrap();
    assert!(
        !read_by_the_run
            .release
            .records()
            .iter()
            .any(|record| record.catalog() == bae_catalogs::catalog::Catalog::MusicBrainz),
        "the run's read asks MusicBrainz nothing: {:?}",
        read_by_the_run.release.records()
    );

    handle
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink { record: release.clone(), partners: vec![] },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    let picked = handle
        .tables
        .load_source_release(&release)
        .await
        .unwrap()
        .expect("the pick stores its release");
    shut_down(handle).await;

    let scratch_fakes = FakeCatalogs::offline();
    let (scratch, _scratch_tmp, _, _) = pane_fixture_with(scratch_fakes.providers()).await;
    scratch
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    seed_linked_discogs_release(&scratch_fakes);
    let complete = scratch
        .release_reads
        .release(
            &release,
            bae_catalogs::payloads::ReadDepth::Complete,
            bae_catalogs::rate_limiter::CallPriority::Interactive,
        )
        .await
        .unwrap();
    shut_down(scratch).await;

    assert!(picked.records().iter().any(|record| record.release_ref()
        == Some(&bae_catalogs::records::MetadataRef::new(
            bae_catalogs::catalog::Catalog::MusicBrainz,
            "linked-mb-rel-1"
        ))));
    assert_eq!(picked, complete.release);
}

/// The case a pick has to carry a join its own documents never reach: a run
/// read that a MusicBrainz release group's album is a Discogs master only
/// through one of its pressings — its release links a Discogs release filed
/// under the master — and kept it. A pick of another release of that master
/// names the release group among its records, from what the run kept.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_carries_the_album_a_reading_found_through_another_pressing() {
    // The pick offers the joined group's album address, which the archive
    // holds no image at.
    let archive = bae_util::http::serve_not_found().await;
    let fakes =
        FakeCatalogs::over(bae_util::http::Http::for_test().serve("coverartarchive.org", &archive));
    let (handle, _tmp, key, _hash) = pane_fixture_with(fakes.providers()).await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    let group = "chain-mb-group";
    seed_two_track_discogs_release_in(&fakes, "70000012", Some("909"), None);
    crate::identify_service::keep_album_links(
        &handle.tables,
        vec![(
            group.to_string(),
            vec![bae_catalogs::records::MetadataRef::new(
                bae_catalogs::catalog::Catalog::Discogs,
                "909",
            )],
        )],
    )
    .await;

    let picked = bae_catalogs::records::LookupRef::new(
        bae_catalogs::catalog::LookupCatalog::Discogs,
        "70000012",
    );
    handle
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink { record: picked.clone(), partners: vec![] },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    let records = handle
        .tables
        .load_source_release(&picked)
        .await
        .unwrap()
        .expect("the pick stores its release")
        .records();
    shut_down(handle).await;
    assert!(records.contains(&bae_catalogs::records::ReleaseRecord::album(
        &bae_catalogs::records::MetadataRef::new(
            bae_catalogs::catalog::Catalog::MusicBrainz,
            group
        )
    )));
}

/// Steps that keep what a pick told them, `None` where each step ended.
fn recorded() -> (
    crate::CatalogApplySteps,
    std::sync::Arc<std::sync::Mutex<Vec<Option<crate::CatalogApplyStep>>>>,
) {
    let told = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let keep = told.clone();
    (crate::CatalogApplySteps::new(move |step| keep.lock().unwrap().push(step)), told)
}

/// A pick of a paired pressing tells each release it reads from its catalog,
/// the partner after the primary, and the cover it fetches, each ending
/// before the next starts.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_tells_each_catalog_read_it_waits_on() {
    let archive = bae_util::http::serve_not_found().await;
    let fakes =
        FakeCatalogs::over(bae_util::http::Http::for_test().serve("coverartarchive.org", &archive));
    let (handle, _tmp, key, _hash) = pane_fixture_with(fakes.providers()).await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    let discogs_release_id = "70000021";
    seed_two_track_discogs_release(&fakes, discogs_release_id);
    let mb_release_id = "steps-mb-rel-1";
    seed_two_track_mb_release(&fakes, mb_release_id, "steps-mb-group-1");
    let (steps, told) = recorded();

    handle
        .select_candidate_release(
            key,
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    discogs_release_id.to_string(),
                ),
                partners: vec![bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    mb_release_id.to_string(),
                )],
            },
            steps,
        )
        .await
        .unwrap();
    shut_down(handle).await;

    assert_eq!(
        *told.lock().unwrap(),
        vec![
            Some(crate::CatalogApplyStep::ReadingRelease(bae_catalogs::catalog::Catalog::Discogs)),
            None,
            Some(crate::CatalogApplyStep::ReadingRelease(
                bae_catalogs::catalog::Catalog::MusicBrainz
            )),
            None,
            Some(crate::CatalogApplyStep::FetchingCover(
                bae_catalogs::catalog::Catalog::MusicBrainz
            )),
            None,
        ]
    );
}

/// A release the library already holds whole is read without a step: the
/// second pick of one release tells only the cover it fetches.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_of_a_held_release_tells_no_read() {
    let archive = bae_util::http::serve_not_found().await;
    let fakes =
        FakeCatalogs::over(bae_util::http::Http::for_test().serve("coverartarchive.org", &archive));
    let (handle, _tmp, key, _hash) = pane_fixture_with(fakes.providers()).await;
    let mb_release_id = "steps-held-mb-rel-1";
    seed_two_track_mb_release(&fakes, mb_release_id, "steps-held-mb-group-1");
    let link = bae_identify::PressingLink {
        record: bae_catalogs::records::LookupRef::new(
            bae_catalogs::catalog::LookupCatalog::MusicBrainz,
            mb_release_id.to_string(),
        ),
        partners: vec![],
    };
    let (first, first_told) = recorded();
    handle.select_candidate_release(key.clone(), link.clone(), first).await.unwrap();
    let (again, again_told) = recorded();

    handle.select_candidate_release(key, link, again).await.unwrap();
    shut_down(handle).await;

    assert_eq!(
        *first_told.lock().unwrap(),
        vec![
            Some(crate::CatalogApplyStep::ReadingRelease(
                bae_catalogs::catalog::Catalog::MusicBrainz
            )),
            None,
            Some(crate::CatalogApplyStep::FetchingCover(
                bae_catalogs::catalog::Catalog::MusicBrainz
            )),
            None,
        ]
    );
    assert_eq!(
        *again_told.lock().unwrap(),
        vec![
            Some(crate::CatalogApplyStep::FetchingCover(
                bae_catalogs::catalog::Catalog::MusicBrainz
            )),
            None,
        ]
    );
}

//! Picking a release: the release a pick reads (a settled lead's stored
//! release offline, a release a Disc ID answer named read in full, one outside
//! the verdict archived), what a pick of a paired pressing stores and reads,
//! the catalog reads it reports, a pick running to its end once asked for, and
//! a picked release left for the person to import, what a pick writes
//! over the draft, and who the draft's author is after a pick.

use super::*;
use crate::handle::tests::*;
use crate::identification::tests::*;
use crate::{MetadataAuthor, PendingStanding};
use bae_catalogs::fake_catalogs::FakeCatalogs;
use bae_catalogs::rate_limiter::CallClass;
use bae_catalogs::test_releases::seed_discogs_pressing;
use bae_catalogs::test_releases::seed_discogs_pressing_printed_with;
use bae_catalogs::test_releases::seed_mb_release_with_catalog;
use bae_catalogs::test_releases::seed_two_track_discogs_release;
use bae_catalogs::test_releases::seed_two_track_discogs_release_in;
use bae_catalogs::test_releases::seed_two_track_mb_release;
use bae_catalogs::test_releases::{mb_release_json, mb_search_json};
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
    let http = bae_util::http::Http::for_test().serve("coverartarchive.org", &archive);
    let fakes = FakeCatalogs::over(http.clone());
    let (handle, _tmp, key, hash) = pane_fixture_with(fakes.providers(), http).await;
    let tables = handle.services.tables.clone();
    handle
        .services
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
        crate::preparations::stored_preparation(&handle.services.tables, &hash)
            .await
            .unwrap()
            .expect("the picked candidate is prepared")
            .metadata
            .cover,
        Some(crate::cover_art::PreparedCover::Local("cover.jpg".to_string())),
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
    let (handle, _tmp, key, hash) =
        pane_fixture_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    let tables = handle.services.tables.clone();

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
    let (handle, _tmp, key, hash) =
        pane_fixture_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    let tables = handle.services.tables.clone();

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
    let http = bae_util::http::Http::for_test().serve("coverartarchive.org", &archive);
    let fakes = FakeCatalogs::over(http.clone());
    let (handle, _tmp, key, _) = pane_fixture_with(fakes.providers(), http).await;
    handle
        .services
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
            bae_catalogs::rate_limiter::CallClass::REQUESTED,
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
        .services
        .tables
        .load_source_release(&release)
        .await
        .unwrap()
        .expect("the pick stores its release");
    shut_down(handle).await;

    let scratch_fakes = FakeCatalogs::offline();
    let (scratch, _scratch_tmp, _, _) =
        pane_fixture_with(scratch_fakes.providers(), bae_util::http::Http::for_test()).await;
    scratch
        .services
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
            bae_catalogs::rate_limiter::CallClass::REQUESTED,
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
    let http = bae_util::http::Http::for_test().serve("coverartarchive.org", &archive);
    let fakes = FakeCatalogs::over(http.clone());
    let (handle, _tmp, key, _hash) = pane_fixture_with(fakes.providers(), http).await;
    handle
        .services
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    let group = "chain-mb-group";
    seed_two_track_discogs_release_in(&fakes, "70000012", Some("909"), None);
    handle
        .services
        .tables
        .replace_group_album_links(vec![(
            group.to_string(),
            vec![bae_catalogs::records::MetadataRef::new(
                bae_catalogs::catalog::Catalog::Discogs,
                "909",
            )],
        )])
        .await
        .unwrap();

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
        .services
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
    let http = bae_util::http::Http::for_test().serve("coverartarchive.org", &archive);
    let fakes = FakeCatalogs::over(http.clone());
    let (handle, _tmp, key, _hash) = pane_fixture_with(fakes.providers(), http).await;
    handle
        .services
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
    let http = bae_util::http::Http::for_test().serve("coverartarchive.org", &archive);
    let fakes = FakeCatalogs::over(http.clone());
    let (handle, _tmp, key, _hash) = pane_fixture_with(fakes.providers(), http).await;
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

/// Explicitly applying a settled release whose group failed to fetch fetches
/// the release again, parent and all. Opening the resulting candidate reads
/// the stored release offline.
#[tokio::test(flavor = "multi_thread")]
async fn applying_a_settled_candidate_refetches_a_missing_parent_then_reads_offline() {
    let fixture = Fixture::new("offline-open").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;

    // The stored pressing names its group as missing; both can be fetched.
    fixture.provider.route(
        "/release/mb-offline-1?",
        200,
        mb_release_json("mb-offline-1", "rg-offline-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release-group/rg-offline-1?",
        200,
        serde_json::json!({
            "id": "rg-offline-1", "title": "Album", "relations": [],
            "first-release-date": "1981"
        })
        .to_string(),
    );
    fixture.archive_missing_its_group("mb-offline-1", "rg-offline-1", &[probed, 0]).await;
    fixture.store_settled_verdict(&dir, "mb-offline-1", "rg-offline-1").await;
    let before = fixture.provider.requests().len();

    fixture
        .import
        .select_candidate_release(
            crate::CandidateKey::folder(&dir),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    "mb-offline-1".to_string(),
                ),
                partners: vec![],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .expect("a settled candidate opens once its release is fetched again");

    let after_apply = fixture.provider.requests().len();
    let stored = fixture.stored_release("mb-offline-1").await.expect("the release stays stored");
    assert!(stored.unfetched.is_empty(), "the group fetched this time: {:?}", stored.unfetched);
    let detail = fixture.pane(&dir).await.expect("the picked candidate reads back");
    assert_eq!(
        fixture.provider.requests().len(),
        after_apply,
        "reading the candidate does not fetch metadata"
    );
    let link = detail
        .draft()
        .unwrap()
        .release_link
        .as_ref()
        .and_then(crate::ReleaseLink::pressing)
        .expect("the pick names a release");
    assert_eq!(link.record.key, "mb-offline-1");
    let edit = detail.draft().unwrap().metadata_draft.clone();
    assert_eq!(edit.tracks.len(), 2);
    assert_eq!(edit.album_title, "Album");
    assert_eq!(
        detail
            .draft()
            .unwrap()
            .remote_covers
            .iter()
            .map(|cover| cover.image.url.as_str())
            .collect::<Vec<_>>(),
        vec![format!("{}/release-group/rg-offline-1/front", bae_catalogs::remote_image::ARCHIVE)],
        "the pressing states no front image of its own, so the album's is the \
         only option — stored with the draft when it was applied, not asked for"
    );
    let requested = &fixture.provider.requests()[before..];
    assert_eq!(requested.len(), 3, "{requested:?}");
    assert!(requested[0].starts_with("/ws/2/release/mb-offline-1?"), "{requested:?}");
    assert_eq!(
        &requested[1..],
        &[
            "/ws/2/release-group/rg-offline-1?inc=artist-credits+url-rels&fmt=json".to_string(),
            "/release-group/rg-offline-1/front".to_string(),
        ],
        "selection fetches the release again, parent and all, then the cover"
    );
}

/// A settled lead whose release is not stored is a broken invariant, not a
/// cold cache. Picking it fails loudly, and stores no pick — so nothing is
/// left naming a release the pane could not draw.
#[tokio::test(flavor = "multi_thread")]
async fn a_settled_lead_with_no_stored_release_fails_loud() {
    let fixture = Fixture::new("offline-miss").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.scan(1).await;
    fixture.store_settled_lead_without_its_pick(&dir, "mb-missing-1", "rg-missing-1").await;

    let error = fixture
        .import
        .select_candidate_release(
            crate::CandidateKey::folder(&dir),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    "mb-missing-1".to_string(),
                ),
                partners: vec![],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .expect_err("a settled lead with nothing archived must not silently re-fetch");

    assert!(
        matches!(&error, crate::ImportError::Internal { detail }
            if detail.contains("mb-missing-1")),
        "unexpected error: {error}"
    );
}

/// A pick identification never made — another pressing on the list, a manual
/// search hit — fetches, and archives what it fetched, so opening it again is
/// local too.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_outside_the_verdict_archives_what_it_fetched() {
    let fixture = Fixture::new("manual-pick").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    fixture.provider.route(
        "/release/mb-manual-1?",
        200,
        mb_release_json("mb-manual-1", "rg-manual-1", &[probed, 0]),
    );

    assert!(
        fixture.stored_release("mb-manual-1").await.is_none(),
        "nothing has fetched this release yet"
    );

    let pick = || bae_identify::PressingLink {
        record: bae_catalogs::records::LookupRef::new(
            bae_catalogs::catalog::LookupCatalog::MusicBrainz,
            "mb-manual-1".to_string(),
        ),
        partners: vec![],
    };
    fixture
        .import
        .select_candidate_release(
            crate::CandidateKey::folder(&dir),
            pick(),
            crate::CatalogApplySteps::unread(),
        )
        .await
        .expect("a manual pick fetches");

    assert!(
        fixture.stored_release("mb-manual-1").await.is_some(),
        "and archives the release it fetched"
    );

    // Re-picking it costs nothing.
    let before = fixture.provider.requests().len();
    fixture
        .import
        .select_candidate_release(
            crate::CandidateKey::folder(&dir),
            pick(),
            crate::CatalogApplySteps::unread(),
        )
        .await
        .expect("re-picking reads what the first pick archived");
    assert_eq!(
        fixture.provider.requests().len(),
        before,
        "the second pick reached the wire: {:?}",
        fixture.provider.requests()
    );
}

/// Picking a release the run read from its Disc ID answer asks for the
/// release itself before anything applies it, and stores the release a
/// complete read from scratch stores: its annotation among its notes.
#[tokio::test(flavor = "multi_thread")]
async fn picking_a_release_read_from_its_disc_id_answer_reads_it_in_full() {
    let (fixture, dir) = two_pressings_fixture("disc-id-pick").await;
    fixture.scan(1).await;
    fixture.drain_automatic().await;
    assert_eq!(fixture.count_release_lookups("mb-disc-de"), 0);
    let release = bae_catalogs::records::LookupRef::new(
        bae_catalogs::catalog::LookupCatalog::MusicBrainz,
        "mb-disc-de",
    );

    fixture
        .import
        .select_candidate_release(
            crate::CandidateKey::folder(&dir),
            bae_identify::PressingLink { record: release.clone(), partners: vec![] },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .expect("the pick applies the release");

    assert_eq!(fixture.count_release_lookups("mb-disc-de"), 1, "{:?}", fixture.provider.requests());
    let picked = fixture.stored_release("mb-disc-de").await.expect("the pick stores its release");
    assert_eq!(picked.notes, vec!["club edition".to_string(), "Matrix: A1".to_string()]);

    let scratch = Fixture::new("disc-id-complete").await;
    route_two_pressings(&scratch, fixture.probed_total_ms(&dir));
    let (complete, _) =
        scratch.import.read_pick(&release, &[], CallClass::REQUESTED).await.unwrap();
    assert_eq!(picked, complete.release);
}

/// A release a person picks from their typed search is their choice, and
/// theirs to import, whatever the setting says.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_picked_from_a_typed_search_is_not_imported() {
    let fixture = Fixture::new("auto-import-typed-search").await;
    fixture.set_identify_automatically(false).await;
    fixture.set_import_when_identified(true).await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route("/release?", 200, mb_search_json("mb-typed", "rg-typed"));
    fixture.provider.route(
        "/release/mb-typed?",
        200,
        mb_release_json("mb-typed", "rg-typed", &[probed, 0]),
    );
    fixture.scan(1).await;
    // The queue is running while the person searches and picks.
    fixture.identification();
    let mut events = fixture.import.every_event();

    fixture
        .import
        .start_candidate_search(
            key.clone(),
            bae_catalogs::search::SearchQuery::General {
                artist: "Artist".to_string(),
                album: "Album".to_string(),
            },
        )
        .await
        .unwrap();
    fixture
        .await_runtime(&key, |runtime| {
            runtime.search.as_ref().is_some_and(|search| {
                matches!(
                    search.source(bae_catalogs::catalog::LookupCatalog::MusicBrainz),
                    Some(crate::SourceSearch::Done { .. })
                )
            })
        })
        .await;
    fixture
        .import
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    "mb-typed".to_string(),
                ),
                partners: vec![],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .expect("the person's pick applies");
    tokio::time::timeout(HANG_GUARD, fixture.identification().drained_for_test())
        .await
        .expect("the queue has nothing left to do");

    assert_no_import(&fixture, &mut events, &key, "the person picked from their typed search")
        .await;
}

impl Fixture {
    /// Store a release whose group failed to fetch.
    async fn archive_missing_its_group(
        &self,
        release_id: &str,
        group_id: &str,
        track_lengths: &[u64],
    ) {
        let mut release = stored_pressing(release_id, group_id, track_lengths);
        release.unfetched.push(bae_catalogs::source_release::UnfetchedDocument {
            document: bae_catalogs::records::PayloadSource::MusicBrainzReleaseGroup,
            key: group_id.to_string(),
            reason: bae_catalogs::source_release::UnfetchedReason::Failed,
        });
        self.tables().save_source_release(&release).await.unwrap();
    }
}

/// Picking a release after keeping the draft answers the folder with it:
/// Identified, and the kept draft stands no more.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_after_keeping_the_draft_is_identified() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, hash) =
        pane_fixture_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    let tables = handle.services.tables.clone();
    store_not_found(&handle, &hash).await;
    handle.keep_candidate_draft(key.clone()).await.unwrap();
    seed_mb_release_with_catalog(&fakes, "kept-then-picked", "CAT-1");

    pick(&handle, &key, "kept-then-picked").await;

    assert_eq!(standing(&handle, &key).await, PendingStanding::Identified);
    let kept = tables
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .and_then(|state| state.identify)
        .expect("the verdict stands")
        .kept_own_draft;
    assert!(!kept, "the pick answers the folder over the kept draft");
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pick_replaces_typed_fields_with_the_catalog_metadata() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, _hash) =
        pane_fixture_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    handle
        .services
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    handle
        .set_candidate_edit_field(
            &key,
            crate::DraftFieldEdit::Barcodes(vec!["5099749".to_string()]),
        )
        .await
        .unwrap();

    let release_id = "70000101";
    seed_discogs_pressing(&fakes, release_id, "Label Name", "CAT-1");
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
    let pane = pane(&handle, &key).await;
    shut_down(handle).await;

    assert_eq!(
        pane.draft().unwrap().metadata_draft.pressing.labels,
        vec![crate::RawLabelEdit {
            name: "Label Name".to_string(),
            catalog_number: "CAT-1".to_string(),
            ..Default::default()
        }]
    );
    assert_eq!(
        pane.draft().unwrap().metadata_draft.pressing.barcodes,
        Vec::<String>::new(),
        "explicit application replaces typed values, including with absent metadata"
    );
}

/// A Discogs release printed with an EAN and a UPC keeps both: applying it
/// writes both barcode rows into the draft, and the edit the import writes
/// states both.
#[tokio::test(flavor = "multi_thread")]
async fn applying_a_release_with_two_barcodes_keeps_both() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, _hash) =
        pane_fixture_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    handle
        .services
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    let release_id = "70000102";
    seed_discogs_pressing_printed_with(
        &fakes,
        release_id,
        "Label Name",
        "CAT-1",
        &["5 012345 678900", "0 12345 67890 5"],
    );
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
    let pane = pane(&handle, &key).await;
    shut_down(handle).await;

    let both = ["5 012345 678900", "0 12345 67890 5"];
    assert_eq!(pane.draft().unwrap().metadata_draft.pressing.barcodes, both);
    let shaped = pane.draft().unwrap().metadata_draft.shape().expect("the applied draft shapes");
    assert_eq!(shaped.pressing.barcodes, both);
    assert_eq!(
        crate::RawPressingEdit::from_pressing(&shaped.pressing).barcodes,
        both,
        "the editor's rows read both back"
    );
}

/// Who wrote the draft travels on the pane's own value, so a surface reading
/// the detail knows whether the pick in front of it is the person's or
/// identification's without inferring it from anything else.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_the_person_made_names_them_as_the_author() {
    let StoredCandidate { handle, key, tmp: _tmp, .. } = stored_candidate().await;
    assert_eq!(
        pane(&handle, &key).await.draft().unwrap().metadata_author,
        MetadataAuthor::Prefill,
        "a candidate nobody has picked for carries the scan's draft"
    );

    handle.select_candidate_file_tags(key.clone()).await.unwrap();

    assert_eq!(pane(&handle, &key).await.draft().unwrap().metadata_author, MetadataAuthor::Person);
    shut_down(handle).await;
}

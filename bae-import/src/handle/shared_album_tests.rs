//! Linking a folder to the album its lookup's pressings share, when the
//! person cannot tell which of them the folder is.

use super::*;
use crate::handle::tests::*;
use crate::{PendingStanding, ReleaseLink};
use bae_catalogs::fake_catalogs::FakeCatalogs;
use bae_catalogs::records::LookupRef;

async fn link(handle: &ImportServiceHandle, hash: &str) -> Option<ReleaseLink> {
    let tables = handle.services.tables.clone();
    tables
        .load_import_candidate_state(hash)
        .await
        .unwrap()
        .expect("the candidate is stored")
        .release_link
}

/// The pane offers the album only while the rows are several pressings of
/// it; linking it identifies the folder as the album, takes what the
/// pressings share into the draft, and keeps the draft's own value where they
/// differ.
#[tokio::test(flavor = "multi_thread")]
async fn linking_the_album_takes_what_its_pressings_share() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, hash) =
        pane_fixture_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    store_two_pressings(&handle, &fakes, &hash).await;
    let before = pane(&handle, &key).await;
    let bae_identify::IdentifyStateView::Found { offers_shared_album, .. } =
        before.resumed_identify_state.clone()
    else {
        panic!("the stored verdict reads back as found");
    };
    assert!(offers_shared_album, "two pressings of one album are offered");
    let own_numbers: Vec<String> = before
        .draft()
        .unwrap()
        .metadata_draft
        .pressing
        .labels
        .iter()
        .map(|label| label.catalog_number.clone())
        .collect();

    handle.link_candidate_shared_album(key.clone()).await.unwrap();

    assert_eq!(link(&handle, &hash).await, Some(ReleaseLink::Album(album())));
    let after = pane(&handle, &key).await;
    assert_eq!(after.live.standing, Some(PendingStanding::Identified));
    assert_eq!(after.draft().unwrap().metadata_draft.album_title, "Album Title");
    assert_eq!(after.draft().unwrap().metadata_draft.pressing.year, "1996");
    let label = after
        .draft()
        .unwrap()
        .metadata_draft
        .pressing
        .labels
        .first()
        .expect("the pressings share their label");
    assert_eq!(label.name, "Label Name");
    assert_eq!(
        label.catalog_number,
        own_numbers.first().cloned().unwrap_or_default(),
        "the pressings' numbers differ, so the draft's own stands"
    );
    assert_eq!(
        after.draft().unwrap().metadata_provenance,
        before.draft().unwrap().metadata_provenance,
        "the fields they differ on are still read from where they were"
    );
    assert_eq!(
        after.placement,
        crate::CandidatePanePlacement::Pending { folder_check: None, records: album().records() }
    );
    shut_down(handle).await;
}

/// Importing a folder linked to an album stores the album's record in each
/// catalog, and no pressing's.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_linked_to_an_album_stores_the_album() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, hash) =
        pane_fixture_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    store_two_pressings(&handle, &fakes, &hash).await;
    handle.link_candidate_shared_album(key.clone()).await.unwrap();

    let mut events = handle.every_event();
    let import_id = handle.start_import(&key).await.unwrap().expect_queued();
    let (release_id, _) = await_import_outcome(&mut events, &import_id).await.unwrap();
    let records = handle.services.records.get_release_records(&release_id).await.unwrap();
    shut_down(handle).await;
    assert_eq!(records, album().records());
    assert_eq!(records[0].url(), format!("https://musicbrainz.org/release-group/{ALBUM}"));
}

/// Picking a pressing after all replaces the album link with the pressing.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_replaces_the_album_link() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, hash) =
        pane_fixture_with(fakes.providers(), bae_util::http::Http::for_test()).await;
    store_two_pressings(&handle, &fakes, &hash).await;
    handle.link_candidate_shared_album(key.clone()).await.unwrap();

    pick(&handle, &key, "mb-pressing-2").await;

    assert_eq!(
        link(&handle, &hash).await,
        Some(ReleaseLink::Pressing(bae_identify::PressingLink {
            record: LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                "mb-pressing-2"
            ),
            partners: Vec::new(),
        }))
    );
    shut_down(handle).await;
}

/// With one pressing offered there is no album to be unsure about, and the
/// command is refused with nothing written.
#[tokio::test(flavor = "multi_thread")]
async fn linking_the_album_without_several_pressings_is_refused() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    assert!(handle.link_candidate_shared_album(key.clone()).await.is_err());
    assert_eq!(link(&handle, &hash).await, None);
    shut_down(handle).await;
}

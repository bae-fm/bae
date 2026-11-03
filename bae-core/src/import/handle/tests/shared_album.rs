//! Linking a folder to the album its lookup's pressings are of, when the
//! person cannot tell which of them their copy is.

use bae_catalogs::records::LookupRef;
use super::*;
use crate::import::{PendingStanding, ReleaseLink};
use bae_identify::AlbumLink;
use bae_catalogs::records::{MetadataRef};

const ALBUM: &str = "album-group-1";

/// One MusicBrainz pressing of the album, as the lookup offered it.
fn offered(release_id: &str, year: i32) -> bae_catalogs::search::MetadataResult {
    bae_catalogs::search::MetadataResult {
        source: bae_catalogs::catalog::LookupCatalog::MusicBrainz,
        release_id: release_id.to_string(),
        title: "Album Title".to_string(),
        artist: Some("Artist Name".to_string()),
        year: Some(year),
        labels: Vec::new(),
        area: None,
        status: None,
        packaging: None,
        discogs_details: Vec::new(),
        barcodes: Vec::new(),
        media: bae_types::pressing::StatedMedia::Undescribed,
        links: Vec::new(),
        cover_art: None,
        source_group_id: Some(ALBUM.to_string()),
        album_links: bae_catalogs::album_links::AlbumLinks::NotAsked,
        source_tracks: Some(bae_catalogs::search::SourceTracks::Listed { count: 2 }),
        document_failure: None,
        album_first_year: None,
        track_titles: Vec::new(),
        notes: Vec::new(),
    }
}

/// Store a verdict offering two pressings of one album for the fixture's
/// candidate, each seeded where a read of it finds it, under its own
/// catalog number.
pub(super) async fn store_two_pressings(
    handle: &ImportServiceHandle,
    fakes: &FakeCatalogs,
    hash: &str,
) {
    super::lookup_choices::seed_mb_release_with_catalog(fakes, "mb-pressing-1", "CAT-100");
    super::lookup_choices::seed_mb_release_with_catalog(fakes, "mb-pressing-2", "CAT-200");
    let prep = crate::import::preparation::import_preparation(&handle.tables, hash)
        .await
        .unwrap()
        .expect("the fixture candidate is prepared");
    let stored = handle
        .preparations
        .store_verdict(&crate::import::tables::NewImportCandidateVerdict {
            content_hash: hash.to_string(),
            file_edit_revision: prep.file_edit_revision,
            candidate_key: crate::import::CandidateKey::from_bytes(Vec::new()),
            verdict: bae_identify::TerminalVerdict::Found {
                findings: bae_identify::Findings {
                    matches: vec![offered("mb-pressing-1", 1996), offered("mb-pressing-2", 2004)],
                    pressings: vec![0, 1],
                    narrowed_out: Default::default(),
                    medium_conflict: None,
                    named_notes: Vec::new(),
                },
                track_count: 2,
                ledger: None,
            },
            signals: bae_identify::signals::Signals {
                origin: bae_catalogs::AudioOrigin::default(),
                disc_id: bae_identify::signals::DiscIdSignal::Absent,
                barcode: bae_identify::signals::BarcodeSignal::Absent,
                text: bae_identify::signals::TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            pick: None,
            releases: Vec::new(),
        })
        .await
        .unwrap();
    assert!(stored, "the verdict lands on the fixture candidate");
}

fn album() -> AlbumLink {
    AlbumLink::new([MetadataRef::new(bae_catalogs::catalog::Catalog::MusicBrainz, ALBUM)])
        .expect("one album")
}

async fn link(handle: &ImportServiceHandle, hash: &str) -> Option<ReleaseLink> {
    let tables = handle.tables.clone();
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
    let (handle, _tmp, key, hash) = pane_fixture_with(fakes.providers()).await;
    store_two_pressings(&handle, &fakes, &hash).await;
    let before = pane(&handle, &key).await;
    let bae_identify::IdentifyStateView::Found { offers_shared_album, .. } =
        bae_identify::IdentifyStateView::from(before.resumed_identify_state.clone())
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
        crate::import::CandidatePanePlacement::Pending {
            folder_check: None,
            records: album().records(),
        }
    );
    shut_down(handle).await;
}

/// Importing a folder linked to an album stores the album's record in each
/// catalog, and no pressing's.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_linked_to_an_album_stores_the_album() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, hash) = pane_fixture_with(fakes.providers()).await;
    store_two_pressings(&handle, &fakes, &hash).await;
    handle.link_candidate_shared_album(key.clone()).await.unwrap();

    let mut events = handle.every_event();
    let import_id = handle.start_import(&key).await.unwrap().expect_queued();
    let (release_id, _) = await_import_outcome(&mut events, &import_id).await.unwrap();
    let records = handle.database.get_release_records(&release_id).await.unwrap();
    shut_down(handle).await;
    assert_eq!(records, album().records());
    assert_eq!(records[0].url(), format!("https://musicbrainz.org/release-group/{ALBUM}"));
}

/// Picking a pressing after all replaces the album link with the pressing.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_replaces_the_album_link() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, hash) = pane_fixture_with(fakes.providers()).await;
    store_two_pressings(&handle, &fakes, &hash).await;
    handle.link_candidate_shared_album(key.clone()).await.unwrap();

    super::lookup_choices::pick(&handle, &key, "mb-pressing-2").await;

    assert_eq!(
        link(&handle, &hash).await,
        Some(ReleaseLink::Pressing(bae_identify::PressingLink {
            record: LookupRef::new(bae_catalogs::catalog::LookupCatalog::MusicBrainz, "mb-pressing-2"),
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

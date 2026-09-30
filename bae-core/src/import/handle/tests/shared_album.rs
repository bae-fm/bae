//! Linking a folder to the album its lookup's pressings are of, when the
//! person cannot tell which of them their copy is.

use super::*;
use crate::import::{AlbumLink, MetadataRef, PendingStanding, ReleaseLink};

const ALBUM: &str = "album-group-1";

/// One MusicBrainz pressing of the album, as the lookup offered it.
fn offered(release_id: &str, year: i32) -> crate::import::search::MetadataResult {
    crate::import::search::MetadataResult {
        source: crate::import::Catalog::MusicBrainz,
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
        media: crate::pressing::StatedMedia::Undescribed,
        links: Vec::new(),
        cover_art: None,
        source_group_id: Some(ALBUM.to_string()),
        album_links: crate::import::album_links::AlbumLinks::NotAsked,
        source_tracks: Some(crate::import::search::SourceTracks::Listed { count: 2 }),
        document_failure: None,
        album_first_year: None,
        track_titles: Vec::new(),
        notes: Vec::new(),
    }
}

/// Store a verdict offering two pressings of one album for the fixture's
/// candidate, each seeded where a read of it finds it, under its own
/// catalog number.
async fn store_two_pressings(handle: &ImportServiceHandle, hash: &str) {
    let providers = handle.library_manager.providers();
    super::lookup_choices::seed_mb_release_with_catalog(providers, "mb-pressing-1", "CAT-100");
    super::lookup_choices::seed_mb_release_with_catalog(providers, "mb-pressing-2", "CAT-200");
    let prep = handle
        .library_manager
        .load_import_candidate_preparation(hash)
        .await
        .unwrap()
        .expect("the fixture candidate is prepared");
    let lookup = crate::identify::LookupProvenance {
        by_disc_id: false,
        by_barcode: false,
        by_catalog: false,
        by_isrc: false,
        by_search: true,
        by_pressing: false,
    };
    let stored = handle
        .preparations
        .store_verdict(&crate::db::NewImportCandidateVerdict {
            content_hash: hash.to_string(),
            file_edit_revision: prep.file_edit_revision,
            folder_path: String::new(),
            verdict: crate::identify::TerminalVerdict::Found {
                findings: crate::identify::Findings {
                    matches: vec![offered("mb-pressing-1", 1996), offered("mb-pressing-2", 2004)],
                    provenance: vec![lookup.clone(), lookup],
                    pressings: vec![0, 1],
                    narrowed_out: Default::default(),
                    medium_conflict: None,
                    named_notes: Vec::new(),
                },
                track_count: 2,
                ledger: None,
            },
            signals: crate::signals::Signals {
                origin: crate::signals::AudioOrigin::default(),
                disc_id: crate::signals::DiscIdSignal::Absent,
                barcode: crate::signals::BarcodeSignal::Absent,
                text: crate::signals::TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            pick: None,
        })
        .await
        .unwrap();
    assert!(stored, "the verdict lands on the fixture candidate");
}

fn album() -> AlbumLink {
    AlbumLink::new([MetadataRef::new(crate::import::Catalog::MusicBrainz, ALBUM)])
        .expect("one album")
}

async fn link(handle: &ImportServiceHandle, hash: &str) -> Option<ReleaseLink> {
    handle
        .library_manager
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
    let (handle, _tmp, key, hash) = pane_fixture().await;
    store_two_pressings(&handle, &hash).await;
    let before = pane(&handle, &key).await;
    let crate::identify::IdentifyStateView::Found {
        offers_shared_album, ..
    } = crate::identify::IdentifyStateView::from(before.resumed_identify_state.clone())
    else {
        panic!("the stored verdict reads back as found");
    };
    assert!(offers_shared_album, "two pressings of one album are offered");
    let own_numbers: Vec<String> = before
        .metadata_draft
        .pressing
        .labels
        .iter()
        .map(|label| label.catalog_number.clone())
        .collect();

    handle
        .link_candidate_shared_album(key.clone())
        .await
        .unwrap();

    assert_eq!(link(&handle, &hash).await, Some(ReleaseLink::Album(album())));
    let after = pane(&handle, &key).await;
    assert_eq!(after.live.standing, Some(PendingStanding::Identified));
    assert_eq!(after.metadata_draft.album_title, "Album Title");
    assert_eq!(after.metadata_draft.pressing.year, "1996");
    let label = after
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
        after.metadata_provenance, before.metadata_provenance,
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
    let (handle, _tmp, key, hash) = pane_fixture().await;
    store_two_pressings(&handle, &hash).await;
    handle
        .link_candidate_shared_album(key.clone())
        .await
        .unwrap();

    let mut events = handle.every_event();
    let import_id = handle.start_import(&key).await.unwrap();
    let (release_id, _) = await_import_outcome(&mut events, &import_id).await.unwrap();
    let records = handle
        .library_manager
        .get_release_records(&release_id)
        .await
        .unwrap();
    shut_down(handle).await;
    assert_eq!(records, album().records());
    assert_eq!(
        records[0].url(),
        format!("https://musicbrainz.org/release-group/{ALBUM}")
    );
}

/// Picking a pressing after all replaces the album link with the pressing.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_replaces_the_album_link() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    store_two_pressings(&handle, &hash).await;
    handle
        .link_candidate_shared_album(key.clone())
        .await
        .unwrap();

    super::lookup_choices::pick(&handle, &key, "mb-pressing-2").await;

    assert_eq!(
        link(&handle, &hash).await,
        Some(ReleaseLink::Pressing(crate::import::PressingLink {
            record: MetadataRef::new(crate::import::Catalog::MusicBrainz, "mb-pressing-2"),
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

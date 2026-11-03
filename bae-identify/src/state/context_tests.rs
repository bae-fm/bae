//! What a run keeps of the albums its documents state.

use super::*;
use crate::documents::ReleaseDocument;
use crate::state::tests::*;
use crate::state::*;
use bae_catalogs::album_links::AlbumLinks;
/// A catalog number only the fetched documents state joins the albums no
/// document links, and what the group was then read to be is kept.
#[test]
fn a_catalog_number_only_a_document_states_joins_the_albums() {
    let (state, _) = update(
        started_with(vec![MB, DG]),
        signals(DiscIdSignal::Absent, BarcodeSignal::Settled { codes: artwork_codes(&["A"]) }, &[]),
    );
    let (state, _) =
        crate::state::step(state, barcode_matched(MB, "A", vec![pair("mb-1", Some("g-1"))]));
    let reading =
        crate::state::step(state, barcode_matched(DG, "A", vec![discogs_pair("dg-1", Some("7"))]));
    let (state, effects, _) = answer_rounds(
        reading,
        |release| ReleaseDocument {
            labels: vec![bae_types::pressing::ReleaseLabel::of(Some("Imprint"), Some("LB-100"))],
            album_links: match release.catalog {
                LookupCatalog::MusicBrainz => AlbumLinks::Read(Vec::new()),
                _ => AlbumLinks::NotAsked,
            },
            ..plain_document(&[])
        },
        |_, _| Vec::new(),
    );
    let joined = MetadataRef::new(DG.catalog(), "7");
    assert_eq!(
        effects,
        vec![Effect::KeepAlbumLinks { kept: vec![("g-1".to_string(), vec![joined.clone()])] }]
    );
    let mb = rows_of(&state)
        .into_iter()
        .find(|(result, _, _)| result.release_id == "mb-1")
        .expect("mb-1 is on the list")
        .0;
    assert_eq!(mb.album_links, AlbumLinks::Read(vec![joined]));
}

/// A disc ID alone names only MusicBrainz releases: where their documents
/// name no album there is no other catalog's album to compare them against,
/// so nothing is kept, and what an earlier run kept for the group stands.
#[test]
fn a_disc_id_alone_whose_documents_name_nothing_keeps_nothing() {
    let reading = disc_run_reading(vec![pair("mb-1", Some("g-1"))], Vec::new());
    let (state, effects, _) = answer_rounds(
        reading,
        |_| ReleaseDocument { album_links: AlbumLinks::Read(Vec::new()), ..plain_document(&[]) },
        |_, _| Vec::new(),
    );
    assert!(effects.is_empty(), "nothing is kept: {effects:?}");
    assert!(matches!(state, IdentifyState::Found { .. }));
}

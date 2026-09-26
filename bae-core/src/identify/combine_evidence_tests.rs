//! How the facts that speak to the object on the desk rank the rows: what
//! names one pressing against what names every pressing cut from one master.

use super::*;
use crate::pressing::{Medium, StatedMedia};
use crate::signals::{TextLine, TextOrigin};

type Outcome = (Findings, LibraryStatuses);
type Found = (MetadataResult, LibraryStatus);

/// A folder named after the album, with the catalog number every pressing
/// below carries.
fn folder() -> CandidateText {
    CandidateText::of(
        &[TextLine {
            text: "Artist One - Album One [L1-100]".to_string(),
            origin: TextOrigin::FolderName,
            file: None,
            region: None,
        }],
        &[],
    )
}

/// One pressing of the album, made of `media`, carrying the folder's catalog
/// number.
fn pressing(release_id: &str, media: StatedMedia) -> Found {
    (
        MetadataResult {
            title: "Album One".to_string(),
            artist: Some("Artist One".to_string()),
            catalog_number: Some("L1-100".to_string()),
            media,
            ..MetadataResult::for_test(Catalog::MusicBrainz, release_id, Some("rg-one"))
        },
        LibraryStatus::absent(release_id),
    )
}

fn made_of(media: &[Medium]) -> StatedMedia {
    StatedMedia::PerMedium(media.iter().copied().map(Some).collect())
}

fn offered(outcome: &Outcome) -> Vec<&str> {
    outcome
        .0
        .matches
        .iter()
        .map(|result| result.release_id.as_str())
        .collect()
}

fn set_aside(outcome: &Outcome) -> Vec<&str> {
    outcome
        .0
        .narrowed_out
        .matches
        .iter()
        .map(|result| result.release_id.as_str())
        .collect()
}

/// The folder's barcode and its catalog number both name one pressing; the
/// disc ID names another with the same table of contents. A barcode and a
/// catalog number are each printed on one pressing, where every pressing cut
/// from one master shares a disc ID, so the one they name is offered and the
/// disc ID's goes behind the disclosure.
#[test]
fn the_pressing_the_barcode_and_catalog_number_name_outranks_the_disc_id_s() {
    let named = pressing("rel-named", made_of(&[Medium::Cd]));
    let mut other = pressing("rel-other", made_of(&[Medium::Cd]));
    other.0.catalog_number = Some("L1-999".to_string());
    let outcome = combine_results(
        vec![other],
        vec![named],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        &folder(),
    );
    assert_eq!(offered(&outcome), vec!["rel-named"]);
    assert_eq!(set_aside(&outcome), vec!["rel-other"]);
}

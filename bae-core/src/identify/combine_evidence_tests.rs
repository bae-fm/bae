//! How the facts that speak to the object on the desk rank the rows: the
//! medium the folder's files prove or rule out, and what names one pressing
//! against what names every pressing cut from one master.

use super::*;
use crate::identify::MediumConflict;
use crate::pressing::{Medium, StatedMedia};
use crate::signals::{CdProof, TextLine, TextOrigin};

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

const CD_RIP: RipEvidence = RipEvidence::Cd {
    proof: CdProof::RipLog,
    file: None,
};

const NOT_CD: RipEvidence = RipEvidence::NotCd {
    sample_rate_hz: 96_000,
};

/// The catalog lookup returned `rows`, and the folder's files say `rip`.
fn by_catalog(rows: Vec<Found>, rip: &RipEvidence) -> Outcome {
    combine_results(
        Vec::new(),
        Vec::new(),
        rows,
        Vec::new(),
        Vec::new(),
        &folder(),
        rip,
    )
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

/// A folder its rip log proves is a CD rip was not copied from a record, so
/// the vinyl pressing goes behind the disclosure and the CD stays.
#[test]
fn a_cd_rip_sets_aside_a_vinyl_pressing() {
    let outcome = by_catalog(
        vec![
            pressing("rel-vinyl", made_of(&[Medium::Vinyl])),
            pressing("rel-cd", made_of(&[Medium::Cd])),
        ],
        &CD_RIP,
    );
    assert_eq!(offered(&outcome), vec!["rel-cd"]);
    assert_eq!(set_aside(&outcome), vec!["rel-vinyl"]);
}

/// A pressing with a CD among its media, and one whose record says nothing
/// about its media, could each be what a CD rip was read off.
#[test]
fn a_cd_rip_keeps_a_cd_and_dvd_pressing_and_one_stating_nothing() {
    let outcome = by_catalog(
        vec![
            pressing("rel-cd", made_of(&[Medium::Cd])),
            pressing("rel-cd-dvd", made_of(&[Medium::Cd, Medium::Dvd])),
            pressing("rel-undescribed", StatedMedia::Undescribed),
        ],
        &CD_RIP,
    );
    assert_eq!(
        offered(&outcome),
        vec!["rel-cd", "rel-cd-dvd", "rel-undescribed"]
    );
    assert!(set_aside(&outcome).is_empty());
}

/// A folder whose files prove nothing about its medium sets nothing aside
/// for it.
#[test]
fn a_folder_that_proves_nothing_sets_nothing_aside() {
    let outcome = by_catalog(
        vec![
            pressing("rel-vinyl", made_of(&[Medium::Vinyl])),
            pressing("rel-cd", made_of(&[Medium::Cd])),
        ],
        &RipEvidence::Unproven,
    );
    assert_eq!(offered(&outcome), vec!["rel-vinyl", "rel-cd"]);
    assert!(set_aside(&outcome).is_empty());
}

/// Audio at a rate no CD plays at was not read off a CD, so a pressing made
/// only of CDs goes behind the disclosure, while a record, a CD beside a DVD,
/// and a pressing stating nothing stay.
#[test]
fn audio_no_cd_holds_sets_aside_a_cd_pressing() {
    let outcome = by_catalog(
        vec![
            pressing("rel-cd", made_of(&[Medium::Cd])),
            pressing("rel-vinyl", made_of(&[Medium::Vinyl])),
            pressing("rel-cd-dvd", made_of(&[Medium::Cd, Medium::Dvd])),
            pressing("rel-undescribed", StatedMedia::Undescribed),
        ],
        &NOT_CD,
    );
    assert_eq!(
        offered(&outcome),
        vec!["rel-vinyl", "rel-cd-dvd", "rel-undescribed"]
    );
    assert_eq!(set_aside(&outcome), vec!["rel-cd"]);
}

/// A disc ID a catalog knows matched the folder's layout to the frame, which
/// proves a CD as surely as a rip log: the vinyl pressing the barcode and the
/// catalog number both name goes behind the disclosure.
#[test]
fn a_matched_disc_id_proves_a_cd() {
    let vinyl = pressing("rel-vinyl", made_of(&[Medium::Vinyl]));
    let outcome = combine_results(
        vec![pressing("rel-cd", made_of(&[Medium::Cd]))],
        vec![vinyl.clone()],
        vec![vinyl],
        Vec::new(),
        Vec::new(),
        &folder(),
        &RipEvidence::Unproven,
    );
    assert_eq!(offered(&outcome), vec!["rel-cd"]);
    assert_eq!(set_aside(&outcome), vec!["rel-vinyl"]);
}

/// Every row contradicted is still every row the run found: the list is
/// shortened, never emptied.
#[test]
fn rows_all_contradicted_all_stay() {
    let outcome = by_catalog(
        vec![
            pressing("rel-vinyl", made_of(&[Medium::Vinyl])),
            pressing("rel-cassette", made_of(&[Medium::Cassette])),
        ],
        &CD_RIP,
    );
    assert_eq!(offered(&outcome), vec!["rel-vinyl", "rel-cassette"]);
    assert!(set_aside(&outcome).is_empty());
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
        &CD_RIP,
    );
    assert_eq!(offered(&outcome), vec!["rel-named"]);
    assert_eq!(set_aside(&outcome), vec!["rel-other"]);
}

/// Two pressings of one year the barcode names alike, one released in the US
/// and one in Europe, and a sleeve that says where it was made — beside the label's
/// address, which says nothing of it. The statement agrees with the European
/// pressing's country, which leads; the other stays on the list, since a
/// pressing released in one market is often made in another.
#[test]
fn a_sleeve_saying_where_it_was_made_puts_that_pressing_first() {
    let line = |text: &str, origin| TextLine {
        text: text.to_string(),
        origin,
        file: None,
        region: None,
    };
    let text = CandidateText::of(
        &[
            line("2010. Artist One - Album One", TextOrigin::FolderName),
            line(
                "Placeholder Records, 100 Placeholder Drive, Beverly Hills, CA 90210.",
                TextOrigin::Artwork,
            ),
            line(
                "Unauthorised copying prohibited. Made in the EU. LC 00000. BIEM/SABAM.",
                TextOrigin::Artwork,
            ),
        ],
        &[],
    );
    let released_in = |release_id: &str, area: &str| {
        let (result, status) = pressing(release_id, made_of(&[Medium::Cd]));
        (
            MetadataResult {
                area: Some(crate::pressing::area(area)),
                year: Some(2010),
                catalog_number: None,
                ..result
            },
            status,
        )
    };
    let outcome = combine_results(
        Vec::new(),
        vec![released_in("rel-us", "US"), released_in("rel-europe", "XE")],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        &text,
        &RipEvidence::Unproven,
    );
    assert_eq!(offered(&outcome), vec!["rel-europe", "rel-us"]);
    assert!(set_aside(&outcome).is_empty());
}

/// Every row ruled out by the folder is still every row the run found, and
/// the verdict says what the folder proves, so nothing picks one unattended.
#[test]
fn rows_the_folder_rules_out_carry_the_conflict() {
    let every_cd = by_catalog(
        vec![
            pressing("rel-cd-1", made_of(&[Medium::Cd])),
            pressing("rel-cd-2", made_of(&[Medium::Cd, Medium::Cd])),
        ],
        &NOT_CD,
    );
    assert_eq!(offered(&every_cd), vec!["rel-cd-1", "rel-cd-2"]);
    assert_eq!(
        every_cd.0.medium_conflict,
        Some(MediumConflict::NotCdAudio {
            sample_rate_hz: 96_000
        })
    );
    let every_vinyl = by_catalog(
        vec![pressing("rel-vinyl", made_of(&[Medium::Vinyl]))],
        &CD_RIP,
    );
    assert_eq!(every_vinyl.0.medium_conflict, Some(MediumConflict::CdRip));
}

/// A row the folder admits wins as it always has, and the verdict carries no
/// conflict: what leads it is not ruled out.
#[test]
fn an_admitted_row_leads_with_no_conflict() {
    let mixed = by_catalog(
        vec![
            pressing("rel-cd", made_of(&[Medium::Cd])),
            pressing("rel-vinyl", made_of(&[Medium::Vinyl])),
        ],
        &NOT_CD,
    );
    assert_eq!(offered(&mixed), vec!["rel-vinyl"]);
    assert_eq!(mixed.0.medium_conflict, None);
}

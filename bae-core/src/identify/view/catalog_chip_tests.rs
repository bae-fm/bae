//! The Catalog # row's chips: which numbers they are, and what striking one
//! out does to the ranking.

use super::tests::*;
use super::*;
use crate::identify::{Findings, LookupProvenance, NarrowedOut, TerminalVerdict};
use crate::import::search::MetadataResult;
use crate::signals::TextLine;
use crate::signals::TextOrigin;

fn folder(lines: &[&str], struck_out: &[&str]) -> CandidateText {
    let pool: Vec<TextLine> = lines
        .iter()
        .map(|text| TextLine {
            text: (*text).to_string(),
            origin: TextOrigin::FolderName,
            file: None,
            region: None,
        })
        .collect();
    let struck_out: Vec<String> = struck_out
        .iter()
        .map(|value| (*value).to_string())
        .collect();
    CandidateText::of(&pool, &struck_out)
}

/// One pressing with the fields the folder's text may agree with.
fn pressing(
    release_id: &str,
    group_id: &str,
    catalog: Option<&str>,
    label: Option<&str>,
    year: Option<i32>,
    country: Option<&str>,
) -> MetadataResult {
    MetadataResult {
        labels: (catalog.is_some() || label.is_some())
            .then(|| crate::pressing::ReleaseLabel::of(label, catalog))
            .into_iter()
            .collect(),
        year,
        area: country.map(crate::pressing::area),
        status: None,
        packaging: None,
        discogs_details: Vec::new(),
        ..MetadataResult::for_test(MB, release_id, Some(group_id))
    }
}

/// A stored verdict read back against the candidate's text.
fn resumed(
    matches: Vec<MetadataResult>,
    narrowed_out: Vec<MetadataResult>,
    ledger: Option<IdentifyRunView>,
    text: CandidateText,
) -> IdentifyStateView {
    let by_disc_id = |count: usize| {
        vec![
            LookupProvenance {
                by_disc_id: true,
                by_barcode: false,
                by_catalog: false,
                by_search: false,
                named_by: None,
            };
            count
        ]
    };
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            provenance: by_disc_id(matches.len()),
            // Rows formed the way a run forms them.
            pressings: crate::import::release_group::form_rows(&matches),
            matches,
            narrowed_out: NarrowedOut {
                provenance: by_disc_id(narrowed_out.len()),
                pressings: crate::import::release_group::form_rows(&narrowed_out),
                matches: narrowed_out,
            },
            medium_conflict: None,
        },
        track_count: 9,
        ledger,
    };
    IdentifyStateView::from(verdict.resume_state(&not_in_library, text))
}

fn chips(view: &IdentifyStateView) -> Vec<(&str, bool)> {
    let IdentifyStateView::Found {
        catalog_agreements, ..
    } = view
    else {
        panic!("a settled verdict, got {view:?}");
    };
    catalog_agreements
        .iter()
        .map(|chip| (chip.value.as_str(), chip.discounted))
        .collect()
}

fn card_ids(view: &IdentifyStateView) -> Vec<&str> {
    let IdentifyStateView::Found { groups, .. } = view else {
        panic!("a settled verdict, got {view:?}");
    };
    groups
        .iter()
        .map(|group| {
            group.sections[0].pressings[0].releases[0]
                .release_id
                .as_str()
        })
        .collect()
}

/// The chips are the numbers the folder states about offered releases, not
/// set-aside ones.
#[test]
fn the_chips_are_the_numbers_the_offered_releases_carry() {
    let view = resumed(
        vec![pressing("rel-a", "rg-a", Some("16033-2"), None, None, None)],
        vec![pressing("rel-b", "rg-b", Some("SD-19"), None, None, None)],
        None,
        folder(&["Dirty Deeds [16033-2]", "Atlantic SD-19"], &[]),
    );
    assert_eq!(chips(&view), vec![("16033-2", false)]);
}

/// A release on two labels carries both numbers, so the folder printing the
/// second label's number is a chip for it.
#[test]
fn a_second_labels_number_is_a_chip() {
    let release = MetadataResult {
        labels: vec![
            crate::pressing::ReleaseLabel::of(Some("Label A"), Some("AB 100")),
            crate::pressing::ReleaseLabel::of(Some("Label B"), Some("CL 719")),
        ],
        ..pressing("rel-a", "rg-a", None, None, None, None)
    };
    let view = resumed(
        vec![release],
        Vec::new(),
        None,
        folder(&["Artist - Album [CL 719]"], &[]),
    );
    assert_eq!(chips(&view), vec![("CL 719", false)]);
}

/// A number the folder never states is no chip, however many releases carry
/// it: the chip is what the folder says, not what a provider does.
#[test]
fn a_number_the_folder_never_states_is_no_chip() {
    let view = resumed(
        vec![pressing("rel-a", "rg-a", Some("16033-2"), None, None, None)],
        Vec::new(),
        None,
        folder(&["Dirty Deeds"], &[]),
    );
    assert!(chips(&view).is_empty());
}

/// Striking a number out drops its agreement and re-ranks the list, and the
/// chip stays, struck out.
#[test]
fn striking_a_number_out_drops_its_agreement_and_re_ranks() {
    let lines = &["Dirty Deeds [16033-2]", "Atlantic 1976 US"];
    let matches = || {
        vec![
            pressing(
                "rel-a",
                "rg-a",
                Some("16033-2"),
                Some("Atlantic"),
                Some(1976),
                None,
            ),
            pressing(
                "rel-b",
                "rg-b",
                None,
                Some("Atlantic"),
                Some(1976),
                Some("US"),
            ),
        ]
    };
    let counted = resumed(matches(), Vec::new(), None, folder(lines, &[]));
    let IdentifyStateView::Found { agreements, .. } = &counted else {
        panic!("a settled verdict");
    };
    assert!(agreements.iter().any(|(id, a)| id == "rel-a" && a.catalog));
    assert_eq!(card_ids(&counted), vec!["rel-a", "rel-b"]);

    let struck = resumed(matches(), Vec::new(), None, folder(lines, &["16033-2"]));
    let IdentifyStateView::Found { agreements, .. } = &struck else {
        panic!("a settled verdict");
    };
    assert!(agreements.iter().any(|(id, a)| id == "rel-a" && !a.catalog));
    assert_eq!(card_ids(&struck), vec!["rel-b", "rel-a"]);
    assert_eq!(chips(&struck), vec![("16033-2", true)]);
}

/// A number an offered release carries is a chip, not also a tile.
#[test]
fn a_number_an_offered_release_carries_is_a_chip_rather_than_a_tile() {
    let ledger = IdentifyRunView {
        providers: vec![MB],
        disc_id: DiscIdStepView::Absent,
        barcode: BarcodeStepView::Absent,
        catalog: CatalogStepView::Numbers {
            scanning: false,
            rows: Vec::new(),
            candidates: vec![
                CatalogCandidateView {
                    value: "16033 2".to_string(),
                    sources: Vec::new(),
                },
                CatalogCandidateView {
                    value: "SD-19".to_string(),
                    sources: Vec::new(),
                },
            ],
        },
        search: SearchStepView::NotNeeded,
    };
    let view = resumed(
        vec![pressing("rel-a", "rg-a", Some("16033-2"), None, None, None)],
        Vec::new(),
        Some(ledger),
        folder(&["Dirty Deeds [16033-2]", "SD-19"], &[]),
    );
    assert_eq!(chips(&view), vec![("16033-2", false)]);
    let IdentifyStateView::Found { run: Some(run), .. } = &view else {
        panic!("a resumed ledger");
    };
    let CatalogStepView::Numbers { candidates, .. } = &run.catalog else {
        panic!("numbers, got {:?}", run.catalog);
    };
    assert_eq!(
        candidates
            .iter()
            .map(|tile| tile.value.as_str())
            .collect::<Vec<_>>(),
        vec!["SD-19"]
    );
}

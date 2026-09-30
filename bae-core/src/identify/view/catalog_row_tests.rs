//! The Catalog # row: which numbers are in effect, which are struck out and
//! which are only offered, and what striking one out does to the ranking.

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
        })
        .collect();
    let struck_out: Vec<String> = struck_out
        .iter()
        .map(|value| (*value).to_string())
        .collect();
    CandidateText::of(&pool, &struck_out, &[])
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
                by_isrc: false,
                by_search: false,
                by_pressing: false,
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
            named_notes: Vec::new(),
        },
        track_count: 9,
        ledger,
    };
    IdentifyStateView::from(verdict.resume_state(&not_in_library, text, Default::default()))
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

/// Striking a number out drops its agreement and re-ranks the list.
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
    assert!(agreements
        .iter()
        .any(|(id, a)| id == "rel-a" && a.fields.catalog));
    assert_eq!(card_ids(&counted), vec!["rel-a", "rel-b"]);

    let struck = resumed(matches(), Vec::new(), None, folder(lines, &["16033-2"]));
    let IdentifyStateView::Found { agreements, .. } = &struck else {
        panic!("a settled verdict");
    };
    assert!(agreements
        .iter()
        .any(|(id, a)| id == "rel-a" && !a.fields.catalog));
    assert_eq!(card_ids(&struck), vec!["rel-b", "rel-a"]);
}

/// The row lists the picked numbers and the confirmed ones with their
/// searches, then the struck-out ones, left out; every other number the text
/// offers is a candidate, once however it is spelled.
#[test]
fn the_row_lists_the_numbers_in_effect_then_the_struck_ones() {
    use crate::identify::state::{CatalogProgress, LookupState, ProviderLookup, ValueLookup};
    let mut context = crate::identify::state::SignalsContext {
        providers: vec![MB],
        text: folder(&["Album LBL-1 AB 12345-2 ZZ-9 QQ-5"], &["ZZ-9"]),
        ..Default::default()
    };
    context.catalog.numbers = ["LBL-1", "AB12345-2", "ZZ-9", "QQ-5"]
        .map(String::from)
        .to_vec();
    context.catalog.chosen = vec!["LBL-1".to_string(), "ZZ-9".to_string()];
    context.catalog.confirmed = vec!["AB 12345-2".to_string()];
    context.catalog.struck_out = vec!["ZZ-9".to_string()];
    let searched = |value: &str| ValueLookup {
        value: value.to_string(),
        providers: vec![ProviderLookup {
            source: MB,
            state: LookupState::Done {
                results: Vec::new(),
            },
        }],
    };
    let step = catalog_step(
        &CatalogProgress::Lookups {
            values: vec![searched("LBL-1"), searched("AB 12345-2")],
        },
        &context,
        false,
    );
    let CatalogStepView::Numbers {
        rows, candidates, ..
    } = step
    else {
        panic!("numbers, got {step:?}");
    };
    assert_eq!(
        rows.iter()
            .map(|row| (row.value.as_str(), row.excluded))
            .collect::<Vec<_>>(),
        vec![("LBL-1", false), ("AB 12345-2", false), ("ZZ-9", true)]
    );
    assert_eq!(rows[0].cells[0].lookup, LookupView::NoMatch);
    assert_eq!(
        rows[2].cells[0].lookup,
        LookupView::NotAsked {
            reason: NotAskedReason::LeftOut
        }
    );
    assert_eq!(
        candidates
            .iter()
            .map(|tile| tile.value.as_str())
            .collect::<Vec<_>>(),
        vec!["QQ-5"]
    );
}

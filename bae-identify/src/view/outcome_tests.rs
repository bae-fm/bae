//! Which of a failed run's failures a surface says in lines of their own,
//! beneath the ledger: those no cell of it shows.

use crate::view::tests::*;
use crate::view::*;
use crate::{Findings, IdentifyFailure, TerminalVerdict};
use bae_catalogs::search::SourceFailure;

const DG: LookupCatalog = LookupCatalog::Discogs;

/// The failures a surface says in lines of their own under the ledger: a
/// lookup's failure its cell shows, with the cell's retry, is not said again
/// beneath it; a failure the ledger has no cell for — the cover of the
/// release the run picked — is.
#[test]
fn a_failure_its_cell_shows_has_no_line_of_its_own() {
    let catalog = IdentifyFailure::Catalog(SourceFailure {
        source: DG.catalog(),
        failure: LookupFailure::Timeout,
    });
    let cover = IdentifyFailure::Cover(SourceFailure {
        source: MB.catalog(),
        failure: LookupFailure::Network,
    });
    let ledger = IdentifyRunView {
        providers: vec![MB, DG],
        disc_id: DiscIdStepView::Absent,
        barcode: BarcodeStepView::Absent,
        catalog: CatalogStepView::Numbers {
            scanning: false,
            rows: vec![SignalValueRow {
                value: "AB-1".to_string(),
                excluded: false,
                cells: vec![
                    ProviderCell { source: MB, lookup: LookupView::NoMatch },
                    ProviderCell {
                        source: DG,
                        lookup: LookupView::Failed { failure: LookupFailure::Timeout },
                    },
                ],
            }],
            candidates: Vec::new(),
        },
        isrc: IsrcStepView::Absent,
        search: SearchStepView::NotNeeded,
    };
    let verdict = TerminalVerdict::Failed {
        failures: vec![catalog, cover.clone()],
        findings: Findings::default(),
        track_count: 9,
        ledger: Some(ledger),
    };
    let IdentifyStateView::Failed { failure_lines, .. } =
        crate::view::reference_resume(verdict, None, &crate::LookupChoices::default())
    else {
        panic!("a failed run");
    };
    assert_eq!(failure_lines, vec![cover]);
}

/// With no ledger, no cell shows any failure: each is a line of its own.
#[test]
fn every_failure_of_a_run_with_no_ledger_has_a_line() {
    let failures = vec![IdentifyFailure::Search(SourceFailure {
        source: DG.catalog(),
        failure: LookupFailure::Network,
    })];
    let verdict = TerminalVerdict::Failed {
        failures: failures.clone(),
        findings: Findings::default(),
        track_count: 9,
        ledger: None,
    };
    let IdentifyStateView::Failed { failure_lines, .. } =
        crate::view::reference_resume(verdict, None, &crate::LookupChoices::default())
    else {
        panic!("a failed run");
    };
    assert_eq!(failure_lines, failures);
}

/// The view states the check and the pick its cards stored; it does not
/// judge the findings again. A found verdict whose one release fits the
/// folder shows no pick when its cards state none.
#[test]
fn a_found_view_states_what_its_cards_stored_without_judging_its_findings() {
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            matches: vec![MetadataResult {
                source_tracks: Some(bae_catalogs::search::SourceTracks::Listed { count: 13 }),
                ..MetadataResult::for_test(DG, "dg-1", Some("g"))
            }],
            pressings: vec![0],
            ..Findings::default()
        },
        track_count: 13,
        ledger: None,
    };
    let stored = VerdictCards {
        folder_check: Some(crate::FolderCheck::SourceTracksUnknown),
        unattended_pick: Some("stored-pick".to_string()),
        ..VerdictCards::default()
    };
    let IdentifyStateView::Found { folder_check, unattended_pick, .. } =
        IdentifyStateView::of(RunOutcome::Answered(verdict.clone()), stored)
    else {
        panic!("a found verdict");
    };
    assert_eq!(folder_check, Some(crate::FolderCheck::SourceTracksUnknown));
    assert_eq!(unattended_pick.as_deref(), Some("stored-pick"));

    let IdentifyStateView::Found { folder_check, unattended_pick, .. } =
        IdentifyStateView::of(RunOutcome::Answered(verdict), VerdictCards::default())
    else {
        panic!("a found verdict");
    };
    assert_eq!((folder_check, unattended_pick), (None, None));
}

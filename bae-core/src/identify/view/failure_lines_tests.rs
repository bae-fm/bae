//! Which of a failed run's failures a surface says in lines of their own,
//! beneath the ledger: those no cell of it shows.

use super::tests::*;
use super::*;
use crate::identify::{Findings, IdentifyFailure, TerminalVerdict};
use crate::import::search::SourceFailure;

const DG: Catalog = Catalog::Discogs;

/// The failures a surface says in lines of their own under the ledger: a
/// lookup's failure its cell shows, with the cell's retry, is not said again
/// beneath it; a failure the ledger has no cell for — the cover of the
/// release the run picked — is.
#[test]
fn a_failure_its_cell_shows_has_no_line_of_its_own() {
    let catalog = IdentifyFailure::Catalog(SourceFailure {
        source: DG,
        failure: LookupFailure::Timeout,
    });
    let cover = IdentifyFailure::Cover(SourceFailure {
        source: MB,
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
                    ProviderCell {
                        source: MB,
                        lookup: LookupView::NoMatch,
                    },
                    ProviderCell {
                        source: DG,
                        lookup: LookupView::Failed {
                            failure: LookupFailure::Timeout,
                        },
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
    let IdentifyStateView::Failed { failure_lines, .. } = IdentifyStateView::from(
        verdict.resume_state(&not_in_library, Default::default(), Default::default()),
    ) else {
        panic!("a failed run");
    };
    assert_eq!(failure_lines, vec![cover]);
}

/// With no ledger, no cell shows any failure: each is a line of its own.
#[test]
fn every_failure_of_a_run_with_no_ledger_has_a_line() {
    let failures = vec![IdentifyFailure::Search(SourceFailure {
        source: DG,
        failure: LookupFailure::Network,
    })];
    let verdict = TerminalVerdict::Failed {
        failures: failures.clone(),
        findings: Findings::default(),
        track_count: 9,
        ledger: None,
    };
    let IdentifyStateView::Failed { failure_lines, .. } = IdentifyStateView::from(
        verdict.resume_state(&not_in_library, Default::default(), Default::default()),
    ) else {
        panic!("a failed run");
    };
    assert_eq!(failure_lines, failures);
}

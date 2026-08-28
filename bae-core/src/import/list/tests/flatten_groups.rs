//! Group headers derived from scan-owned folder ancestry.

use super::*;

/// A folder nothing has settled either way, holding several releases, is
/// combinable too — the scan names it on every row below it, and the header
/// for it is where the choice belongs.
#[test]
fn a_folder_the_scan_named_as_an_ancestor_offers_to_combine() {
    let mut rows = queue();
    rows.candidates = vec![
        ScanCandidateListRow {
            combine_ancestor_relative_path: Some("Wrapper".to_string()),
            ..candidate("Wrapper/Box/Disc 1")
        },
        ScanCandidateListRow {
            combine_ancestor_relative_path: Some("Wrapper".to_string()),
            ..candidate("Wrapper/Box/Disc 2")
        },
    ];

    let flat = flattened(&rows, &view(TriageTab::Pending));
    let header = flat.headers.first().expect("a header for Wrapper");
    assert_eq!(header.group.name, "Wrapper");
    assert!(header.group.combinable);
}

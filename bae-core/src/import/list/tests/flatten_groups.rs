//! Group headers derived from scan-owned folder ancestry.

use super::*;

/// A folder nothing has settled either way, holding several releases below a
/// folder that keeps them apart, is combinable too — it is the nearest folder
/// above them to hold them all, and the header for it is where the choice
/// belongs.
#[test]
fn a_folder_nothing_settled_over_several_releases_offers_to_combine() {
    let mut rows = queue();
    rows.candidates = vec![
        candidate("Wrapper/Box/Disc 1"),
        candidate("Wrapper/Box/Disc 2"),
    ];
    rows.folder_readings
        .insert((root(), "Wrapper/Box".to_string()), false);

    let flat = flattened(&rows, &view(TriageTab::Pending));
    let header = flat.headers.first().expect("a header for Wrapper");
    assert_eq!(header.group.name, "Wrapper");
    assert!(header.group.combinable);
}

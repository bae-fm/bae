//! Which item references a window asks for.

use super::*;

fn window(offset: u64, limit: u64) -> LibraryPageWindow {
    LibraryPageWindow { offset, limit }
}

fn five_releases() -> ImportQueueRows {
    let mut rows = queue();
    rows.candidates = (1..=5)
        .map(|index| candidate(&format!("Release {index}")))
        .collect();
    rows
}

#[test]
fn a_window_past_the_end_is_empty() {
    let rows = five_releases();
    let flat = flattened(&rows, &view(TriageTab::Pending));

    assert!(window_refs(&flat.items, &window(5, 10)).is_empty());
    assert_eq!(
        window_refs(&flat.items, &window(3, 10)).len(),
        2,
        "a window that overruns the end stops at it"
    );
}

#[test]
fn two_windows_deliver_disjoint_items() {
    let rows = five_releases();
    let flat = flattened(&rows, &view(TriageTab::Pending));

    let first: Vec<_> = window_refs(&flat.items, &window(0, 2)).to_vec();
    let second: Vec<_> = window_refs(&flat.items, &window(2, 2)).to_vec();

    assert_eq!(first.len(), 2);
    assert_eq!(second.len(), 2);
    assert!(
        first.iter().all(|item| !second.contains(item)),
        "the two windows name different items"
    );
}

/// A group header sits at its own offset, so a window boundary falling inside
/// a group leaves the header in the earlier window and the rest of the entries
/// in the later one.
#[test]
fn a_window_boundary_inside_a_group_keeps_the_header_in_the_earlier_window() {
    let mut rows = queue();
    rows.candidates = vec![
        candidate("Group/Release 1"),
        candidate("Group/Release 2"),
        candidate("Group/Release 3"),
    ];
    let flat = flattened(&rows, &view(TriageTab::Pending));
    assert_eq!(flat.items.len(), 4, "one header and three entries");

    let first = window_refs(&flat.items, &window(0, 2));
    let second = window_refs(&flat.items, &window(2, 2));

    assert!(matches!(first[0], ItemRef::Header(_)));
    assert!(matches!(first[1], ItemRef::Candidate(_)));
    assert!(second
        .iter()
        .all(|item| matches!(item, ItemRef::Candidate(_))));
}

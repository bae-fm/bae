//! The import list's selection through the bridge object the macOS app holds.

use crate::types::{
    BridgeImportListItem, BridgeImportListOrder, BridgeImportListSnapshot, BridgeImportListView,
    BridgeLibraryPageWindow, BridgePendingFilter, BridgeSelectionChange, BridgeTriageTab,
};
use std::sync::Arc;

/// The candidate keys the snapshot's page shows, and those it shows selected,
/// top to bottom.
fn rows(snapshot: &BridgeImportListSnapshot) -> (Vec<String>, Vec<String>) {
    let shown: Vec<_> = snapshot
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            BridgeImportListItem::Candidate { row, .. } => Some(row),
            _ => None,
        })
        .collect();
    (
        shown.iter().map(|row| row.candidate_key.clone()).collect(),
        shown
            .iter()
            .filter(|row| row.selected)
            .map(|row| row.candidate_key.clone())
            .collect(),
    )
}

/// The list's values until one `accept` admits.
fn until(
    handle: &super::super::AppHandle,
    list: &Arc<super::ImportListSubscription>,
    accept: impl Fn(&BridgeImportListSnapshot) -> bool,
) -> BridgeImportListSnapshot {
    handle.runtime.block_on(async {
        tokio::time::timeout(bae_test_support::HANG_GUARD, async {
            loop {
                let snapshot = list.clone().next().await.expect("the list answers");
                if accept(&snapshot) {
                    return snapshot;
                }
            }
        })
        .await
        .expect("the list gets there")
    })
}

/// Keeping a combined release as separate releases selects the releases it
/// returns, as combining selects the release it makes.
#[test]
fn separating_the_selection_selects_the_releases_it_returns() {
    let (handle, root) = super::super::tests::fresh_bridge_handle("separate-selection");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../bae-core/tests/fixtures/flac/01 Test Track 1.flac");
    let music = root.join("music");
    for part in ["Part A", "Part B"] {
        let folder = music.join("Collection").join(part);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::copy(&fixture, folder.join("01.flac")).unwrap();
    }
    let music = std::fs::canonicalize(&music).unwrap();
    let parts: Vec<String> = ["Part A", "Part B"]
        .iter()
        .map(|part| {
            music
                .join("Collection")
                .join(part)
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    handle.runtime.block_on(async {
        // Nothing but the test acts on the candidates: a run going would hold
        // back combining them.
        handle
            .services
            .set_identify_automatically(false)
            .await
            .unwrap();
        handle
            .services
            .import_add_watched_folder(music.to_string_lossy().into_owned())
            .await
            .unwrap();
    });
    let list = handle.subscribe_import_list(BridgeImportListView {
        tab: BridgeTriageTab::Pending,
        filter_text: String::new(),
        pending_filter: BridgePendingFilter::All,
        collapsed_groups: Vec::new(),
        order: BridgeImportListOrder::PathAscending,
    });
    list.set_windows(vec![BridgeLibraryPageWindow {
        offset: 0,
        limit: 50,
    }])
    .unwrap();
    until(&handle, &list, |snapshot| rows(snapshot).0 == parts);

    let revision = list
        .change_selection(BridgeSelectionChange::Replace {
            keys: parts.clone(),
        })
        .unwrap();
    until(&handle, &list, |snapshot| {
        snapshot.selection_revision >= revision && snapshot.selection.count == 2
    });
    let combined = handle
        .runtime
        .block_on(list.clone().combine_selection())
        .unwrap();
    until(&handle, &list, |snapshot| {
        rows(snapshot) == (vec![combined.clone()], vec![combined.clone()])
    });

    let returned = handle
        .runtime
        .block_on(list.clone().separate_selection())
        .unwrap();
    assert_eq!(returned, parts);
    let separated = until(&handle, &list, |snapshot| {
        rows(snapshot) == (parts.clone(), parts.clone())
    });
    assert_eq!(separated.selection.count, 2);
    handle
        .runtime
        .block_on(list.clone().cancel())
        .expect("cancel the list");
}

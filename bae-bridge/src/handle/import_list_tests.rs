//! The import list's selection through the bridge object the macOS app holds.

use crate::types::{
    BridgeImportListItem, BridgeImportListOrder, BridgeImportListSnapshot, BridgeImportListView,
    BridgeLibraryPageWindow, BridgePendingFilter, BridgeSelectionChange, BridgeTriageTab,
};
use std::sync::Arc;

/// The candidate keys the snapshot's page shows, and those it shows selected,
/// top to bottom.
fn rows(snapshot: &BridgeImportListSnapshot) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
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
    let parts: Vec<Vec<u8>> = ["Part A", "Part B"]
        .iter()
        .map(|part| coven::path_bytes(&music.join("Collection").join(part)))
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
            .import_add_watched_folder(music.clone())
            .await
            .unwrap();
    });
    let list = handle
        .runtime
        .block_on(handle.subscribe_import_list(BridgeImportListView {
            tab: BridgeTriageTab::Pending,
            filter_text: String::new(),
            pending_filter: BridgePendingFilter::All,
            order: BridgeImportListOrder::PathAscending,
        }))
        .expect("the list opens");
    list.set_windows(vec![BridgeLibraryPageWindow {
        offset: 0,
        limit: 50,
    }])
    .unwrap();
    until(&handle, &list, |snapshot| rows(snapshot).0 == parts);

    let revision = handle
        .runtime
        .block_on(
            list.clone()
                .change_selection(BridgeSelectionChange::Replace {
                    keys: parts.clone(),
                }),
        )
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

/// Every batch of keys a stream says started or stopped.
#[derive(Default)]
struct HeardKeys(std::sync::Mutex<Vec<Vec<Vec<u8>>>>);

struct Activity(Arc<HeardKeys>);

impl crate::types::KeyActivityCallback for Activity {
    fn on_keys_changed(&self, keys: Vec<Vec<u8>>) {
        self.0 .0.lock().unwrap().push(keys);
    }
}

/// A bulk action writes its progress into the stream the app reads while it
/// runs, and the stream empties as it ends; once the app cancels that
/// stream, an action run after writes nothing into it.
#[test]
fn a_cancelled_progress_stream_hears_no_later_action() {
    let (handle, root) = super::super::tests::fresh_bridge_handle("cancelled-selection-progress");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../bae-core/tests/fixtures/flac/01 Test Track 1.flac");
    let music = root.join("music");
    let folder = music.join("Album");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::copy(&fixture, folder.join("01.flac")).unwrap();
    let music = std::fs::canonicalize(&music).unwrap();
    let key = coven::path_bytes(&music.join("Album"));
    handle.runtime.block_on(async {
        handle
            .services
            .set_identify_automatically(false)
            .await
            .unwrap();
        handle
            .services
            .import_add_watched_folder(music.clone())
            .await
            .unwrap();
    });
    let list = handle
        .runtime
        .block_on(handle.subscribe_import_list(BridgeImportListView {
            tab: BridgeTriageTab::Pending,
            filter_text: String::new(),
            pending_filter: BridgePendingFilter::All,
            order: BridgeImportListOrder::PathAscending,
        }))
        .expect("the list opens");
    list.set_windows(vec![BridgeLibraryPageWindow {
        offset: 0,
        limit: 50,
    }])
    .unwrap();
    until(&handle, &list, |snapshot| rows(snapshot).0 == [key.clone()]);
    let revision = handle
        .runtime
        .block_on(
            list.clone()
                .change_selection(BridgeSelectionChange::Replace {
                    keys: vec![key.clone()],
                }),
        )
        .unwrap();
    until(&handle, &list, |snapshot| {
        snapshot.selection_revision >= revision && snapshot.selection.count == 1
    });

    let read = Arc::new(HeardKeys::default());
    let progress = list.selection_action_progress(Box::new(Activity(read.clone())));
    handle
        .runtime
        .block_on(
            list.clone()
                .run_selection_action(crate::types::BridgeCandidateAction::Skip),
        )
        .unwrap();
    assert_eq!(
        std::mem::take(&mut *read.0.lock().unwrap()),
        vec![vec![Vec::<u8>::new()], vec![Vec::new()]],
        "the action's progress came and went"
    );
    assert_eq!(progress.latest(Vec::new()), None);

    progress.cancel();
    handle
        .runtime
        .block_on(
            list.clone()
                .run_selection_action(crate::types::BridgeCandidateAction::Skip),
        )
        .unwrap();
    assert!(
        read.0.lock().unwrap().is_empty(),
        "a cancelled stream hears no later action"
    );
    assert_eq!(progress.latest(Vec::new()), None);
    handle
        .runtime
        .block_on(list.clone().cancel())
        .expect("cancel the list");
}

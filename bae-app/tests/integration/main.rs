//! bae-app's integration tests, one module per area, built as one test
//! binary: the app's bootstrap opening a library, and the services it starts
//! over one.
//!
//! Run one area with a filter on its module: `scripts/cargo.sh test -p
//! bae-app --features test-utils --test integration -- test_startup_sync::`.

mod test_bootstrap_fixture;
mod test_bootstrap_pointer;
mod test_close_open_files;
mod test_import_live_query_reruns;
mod test_import_selection;
mod test_live_reads;
mod test_open_progress;
mod test_scan_cost;
mod test_startup_sync;
mod test_storage_state_machine;
mod test_transfer;

/// Where this release's files are, and whether this device pins them.
pub(crate) async fn release_storage(
    db: &bae_store::Store,
    release_id: &str,
) -> bae_views::album_detail::ReleaseStorageState {
    let summary = bae_views::LibraryViews::new(db.clone())
        .find_release_detail(release_id)
        .await
        .unwrap()
        .unwrap()
        .summary;
    summary.storage_state
}

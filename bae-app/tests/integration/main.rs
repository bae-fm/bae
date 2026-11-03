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
mod test_open_progress;
mod test_scan_cost;
mod test_startup_sync;

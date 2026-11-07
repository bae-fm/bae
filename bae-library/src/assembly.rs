//! The library manager's parts, each built once from the library's opened
//! store. The app's bootstrap and every test that opens a library call this,
//! so tests open a library through the app's wiring. The caller opens the
//! store and builds what the manager shares with the library's services —
//! the import's release reads and the closing token — and passes them by name.

use crate::{CancellationToken, LibraryManager};
use bae_diagnostics::Diagnostics;
use bae_store::{DbError, Store};
use bae_sync_status::SyncStatus;
use coven::{ClockRef, IdSourceRef};

/// Open the library manager over the library's store, with the services it
/// drives built from that store. `closing` ends its tasks when it closes.
pub async fn open_library_manager(
    store: Store,
    preferences: bae_preferences::PreferencesRow,
    records: bae_records::Records,
    people: bae_people::People,
    membership: bae_people::Membership,
    sync_status: SyncStatus,
    #[cfg(desktop)] release_reads: bae_import::ReleaseReads,
    closing: CancellationToken,
    app_dir: bae_app_dir::AppDir,
    clock: ClockRef,
    ids: IdSourceRef,
    diagnostics: Diagnostics,
    runtime_handle: tokio::runtime::Handle,
    #[cfg(desktop)] remote_images: bae_catalogs::remote_image::RemoteImageCache,
) -> Result<LibraryManager, DbError> {
    let downloads = bae_transfers::Downloads::new(store.clone());
    #[cfg(desktop)]
    let outputs = bae_transfers::Outputs::new(store.clone());
    LibraryManager::open(
        store,
        preferences,
        records,
        people,
        membership,
        sync_status,
        app_dir,
        clock,
        ids,
        diagnostics,
        runtime_handle,
        #[cfg(desktop)]
        remote_images,
        #[cfg(desktop)]
        release_reads,
        downloads,
        #[cfg(desktop)]
        outputs,
        closing,
    )
    .await
}

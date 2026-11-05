//! The library manager's parts, each built once from the library's opened
//! store. The app's bootstrap and every test that opens a library call this,
//! so tests open a library through the app's wiring. The caller opens the
//! store and builds what the manager shares with the library's services —
//! the catalog sessions and the closing token — and passes them by name.

use crate::{CancellationToken, LibraryManager};
use bae_db::{Database, DbError};
use bae_diagnostics::Diagnostics;
use coven::{ClockRef, IdSourceRef};

/// Open the library manager over the library's store, with the services it
/// drives built from that store. `closing` ends its tasks when it closes.
pub async fn open_library_manager(
    database: Database,
    #[cfg(desktop)] catalogs: bae_import::CatalogSessions,
    closing: CancellationToken,
    app_dir: bae_app_dir::AppDir,
    clock: ClockRef,
    ids: IdSourceRef,
    diagnostics: Diagnostics,
    runtime_handle: tokio::runtime::Handle,
    #[cfg(desktop)] remote_images: bae_catalogs::remote_image::RemoteImageCache,
) -> Result<LibraryManager, DbError> {
    let downloads = bae_transfers::Downloads::new(database.clone());
    #[cfg(desktop)]
    let outputs = bae_transfers::Outputs::new(database.clone());
    LibraryManager::open(
        database,
        app_dir,
        clock,
        ids,
        diagnostics,
        runtime_handle,
        #[cfg(desktop)]
        remote_images,
        #[cfg(desktop)]
        catalogs,
        downloads,
        #[cfg(desktop)]
        outputs,
        closing,
    )
    .await
}

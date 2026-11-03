//! The import service a unit test starts over a bare library store, from the
//! parts the app's composition root would hand it: the catalog sessions, the
//! remote image cache, the clock, ids and diagnostics, and the token that
//! ends it with the library.

use crate::import::{CatalogSessions, ImportServiceHandle};
use bae_catalogs::remote_image::RemoteImageCache;
use bae_db::Database;
use bae_diagnostics::Diagnostics;
use coven::{ClockRef, IdSourceRef};
use tokio_util::sync::CancellationToken;

/// What an import service over one test store is started from. Every service
/// started from it shares its catalog sessions, so a Discogs key one stores
/// is the next one's too.
pub(crate) struct TestImport {
    database: Database,
    catalogs: CatalogSessions,
    remote_images: RemoteImageCache,
    clock: ClockRef,
    ids: IdSourceRef,
    diagnostics: Diagnostics,
    closing: CancellationToken,
}

impl TestImport {
    /// Over `database` on `clock` and `ids`, asking `providers`, reporting
    /// to `diagnostics`.
    pub(crate) fn new(
        database: Database,
        providers: bae_catalogs::providers::Providers,
        clock: ClockRef,
        ids: IdSourceRef,
        diagnostics: Diagnostics,
    ) -> Self {
        let remote_images = RemoteImageCache::for_test(providers.http().clone());
        let catalogs = CatalogSessions::new(
            providers,
            database.clone(),
            diagnostics.clone(),
            tokio::runtime::Handle::current(),
        );
        Self {
            database,
            catalogs,
            remote_images,
            clock,
            ids,
            diagnostics,
            closing: CancellationToken::new(),
        }
    }

    /// The catalog sessions the services ask through.
    pub(crate) fn catalogs(&self) -> &CatalogSessions {
        &self.catalogs
    }

    /// Start an import service, as opening the library does.
    pub(crate) async fn start(&self) -> Result<ImportServiceHandle, crate::import::ImportError> {
        crate::import::ImportService::start(
            tokio::runtime::Handle::current(),
            self.database.clone(),
            self.catalogs.clone(),
            self.remote_images.clone(),
            self.diagnostics.clone(),
            crate::import::CandidatePreparations::new(self.database.clone()),
            self.clock.clone(),
            self.ids.clone(),
            self.closing.child_token(),
        )
        .await
    }

    /// Edit this library's preferences in one write of their row, as the
    /// settings screens do.
    pub(crate) async fn edit_preferences(
        &self,
        edit: impl FnOnce(&mut bae_preferences::Preferences) + Send + 'static,
    ) {
        bae_preferences::edit_preferences(&self.database, move |prefs| {
            edit(prefs);
            Ok(())
        })
        .await
        .expect("the test's preferences are stored");
    }
}

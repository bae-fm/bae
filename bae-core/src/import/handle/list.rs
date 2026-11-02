//! The import lists a surface opens over the store.

use super::*;

impl ImportServiceHandle {
    /// Open an import list showing `view` through `windows`, with nothing
    /// folded and nothing selected. Closing the library ends it.
    pub async fn open_list(
        &self,
        view: crate::import::ImportListView,
        windows: crate::library::LibraryPageWindows,
    ) -> Result<crate::import::ImportListSubscription, crate::import::ImportError> {
        Ok(crate::import::ImportListSubscription::open(
            self.database.clone(),
            self.closing.child_token(),
            view,
            windows,
        )
        .await?)
    }

    /// One read of a list showing `view` through `windows`, for a caller
    /// with no list open: a list opened for the read and closed after it.
    pub async fn load_list(
        &self,
        view: crate::import::ImportListView,
        windows: crate::library::LibraryPageWindows,
    ) -> Result<crate::import::ImportListProjection, crate::import::ImportError> {
        let list = self.database.open_import_list(view).await?;
        let projection = self
            .database
            .load_import_list(crate::import::ImportListRequest { list, windows })
            .await;
        self.database.close_import_list(list).await?;
        Ok(projection?)
    }
}

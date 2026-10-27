//! One read of a list for a test: a list opened for it, selected as asked,
//! and closed after it.

use super::super::super::*;
use crate::import::list::ImportListView;
use crate::import::TriageTab;

/// One read of a list: its view, its windows, and the rows it has selected.
pub(super) struct ListRead {
    pub(super) view: ImportListView,
    pub(super) windows: crate::library::LibraryPageWindows,
    pub(super) selected: Vec<String>,
}

/// The first fifty items of `tab`, nothing selected.
pub(super) async fn request(tab: TriageTab) -> ListRead {
    ListRead {
        view: ImportListView {
            tab,
            ..ImportListView::default()
        },
        windows: std::iter::once(crate::library::LibraryPageWindow {
            offset: 0,
            limit: 50,
        })
        .collect(),
        selected: Vec::new(),
    }
}

impl Database {
    /// `read` as a list opened for it shows it.
    pub(super) async fn load_import_list_read(
        &self,
        read: ListRead,
    ) -> Result<crate::import::ImportListProjection, DbError> {
        let list = self.open_import_list(read.view).await?;
        if !read.selected.is_empty() {
            self.change_import_list_selection(
                list,
                crate::import::selection::SelectionChange::Replace {
                    keys: read.selected,
                },
            )
            .await?;
        }
        let projection = self
            .load_import_list(crate::import::ImportListRequest {
                list,
                windows: read.windows,
            })
            .await;
        self.close_import_list(list).await?;
        projection
    }
}

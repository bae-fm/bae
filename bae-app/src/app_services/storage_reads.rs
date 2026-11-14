//! The Storage Manager's live reads on [`AppServices`]: each window of its
//! list, its count, the position of a key in it, its selection and the
//! transfer queues are reads of their own.

use super::*;

impl AppServices {
    /// One window of the Storage Manager list under `sort`, `filter` and the
    /// search text `search`.
    pub fn subscribe_storage_window(
        &self,
        sort: &bae_views::StorageSortCriterion,
        filter: bae_views::StorageFilter,
        search: &str,
        window: bae_store::ListWindow,
    ) -> bae_views::LiveListWindow {
        super::list_reads::list_window(
            &self.inner.manager,
            self.inner.views.subscribe_storage_window(sort, filter, search, window),
            bae_views::storage::resolve_storage_window,
        )
    }

    /// How many releases the Storage Manager list holds and their files' total
    /// size, and whether the search narrows it.
    pub fn subscribe_storage_count(
        &self,
        filter: bae_views::StorageFilter,
        search: &str,
    ) -> bae_store::live_query::LiveValues<bae_views::ListCount> {
        self.inner.manager.values_until_closed(
            self.inner.views.subscribe_storage_count(filter, search).process(|counts| {
                Ok(bae_views::ListCount {
                    rows: counts.rows,
                    extra: bae_views::ListCountExtra::Storage {
                        total_size: counts.total_size,
                        searching: counts.searching,
                    },
                })
            }),
        )
    }

    /// How many releases of the Storage Manager list come before the row at
    /// `key`.
    pub fn subscribe_storage_position(
        &self,
        sort: &bae_views::StorageSortCriterion,
        filter: bae_views::StorageFilter,
        search: &str,
        key: bae_store::list_windows::OrderKey,
    ) -> bae_views::LiveListPosition {
        super::list_reads::list_position(
            &self.inner.manager,
            self.inner.views.subscribe_storage_position(sort, filter, search, key),
        )
    }

    /// The rows of the releases selected in the Storage Manager list that it
    /// holds, with the storage actions every one allows; the selection starts
    /// empty and moves in place.
    pub fn subscribe_storage_selection(
        &self,
        filter: bae_views::StorageFilter,
        search: &str,
    ) -> crate::live_reads::LiveStorageSelection {
        crate::live_reads::LiveStorageSelection::new(
            self.inner.manager.until_closed(self.inner.views.subscribe_storage_selection(
                filter,
                search,
                Default::default(),
            )),
            |read, projection| {
                Some(bae_views::storage::resolve_storage_selection(read.request, projection))
            },
        )
    }

    /// The transfer queues holding work, as the Storage Manager shows them.
    pub fn subscribe_storage_queues(
        &self,
    ) -> bae_store::live_query::LiveValues<bae_views::StorageQueues> {
        self.inner.manager.values_until_closed(self.inner.views.subscribe_storage_queues())
    }

    delegate_view!(storage_key_at => storage_key_at(sort: &bae_views::StorageSortCriterion, filter: bae_views::StorageFilter, search: &str, position: u64) -> Option<bae_store::list_windows::OrderKey>);
}

//! The storage manager's reads over releases: one release's storage summary,
//! the remote releases' representative files, and the paged listing with its
//! counts and totals.
//!
//! Kept apart from the release writes beside them: this is one surface's
//! questions about where a release's bytes live, not the rows an import or an
//! edit puts down.

use super::*;

impl Database {
    /// The SELECT for one release's storage summary (`DbReleaseStorageSummary`).
    fn release_storage_summary_query() -> String {
        format!(
            "SELECT \
            r.id AS release_id, \
            r.album_id, \
            a.title AS album_title, \
            r.media, \
            listing.stored AS remote, \
            {RELEASE_LISTING_PINNED} AS pinned, \
            {upload_columns}, \
            {artist_names} AS artist_names, \
            listing.file_count, \
            listing.total_size \
        FROM releases r \
        JOIN albums a ON a.id = r.album_id \
        JOIN release_listing listing ON listing.release_id = r.id \
        LEFT JOIN ({releases_sending}) sending ON sending.release_id = listing.release_id \
        WHERE r.id = ?1",
            artist_names = album_credits_json_sql(),
            upload_columns = super::queued_uploads::RELEASE_UPLOAD_COLUMNS,
            releases_sending = super::queued_uploads::RELEASES_SENDING,
        )
    }

    /// The storage summary for one release, or `None` if it doesn't exist. The
    /// download queue reads it at enqueue time to skip a local or pinned
    /// release.
    pub async fn find_release_storage_summary(
        &self,
        release_id: &str,
    ) -> Result<Option<DbReleaseStorageSummary>, DbError> {
        let release_id = release_id.to_string();
        let query = Self::release_storage_summary_query();
        self.read(move |sql| {
            sql.query_row(&query, [release_id], |row| {
                row_to_release_storage_summary(&sql, row)
            })
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// How many uploaded releases this device doesn't keep pinned: those with
    /// a file coven's `coven_pins` doesn't list as kept whole here, as the
    /// listing's pinned count keeps them. Each is reachable only through the
    /// cloud, so disconnecting would leave it unplayable here.
    pub async fn cloud_only_release_count(&self) -> Result<u64, DbError> {
        self.read(|sql| {
            sql.query_row(CLOUD_ONLY_RELEASE_COUNT, [], |row| row.get::<_, u64>(0))
                .map_err(DbError::from)
        })
        .await
    }

    /// Count storage rows matching `filter`: the set `get_storage_page` pages.
    pub async fn get_storage_count(&self, filter: StorageFilter) -> Result<u64, DbError> {
        self.read(move |sql| storage_count_on(&sql, filter)).await
    }

    /// The size of every file of the storage rows matching `filter` — the
    /// storage-manager footer's "Total:", independent of how many pages have
    /// loaded.
    pub async fn get_storage_total_size(&self, filter: StorageFilter) -> Result<u64, DbError> {
        self.read(move |sql| storage_total_size_on(&sql, filter))
            .await
    }

    pub async fn get_storage_page(
        &self,
        sort: &StorageSortCriterion,
        filter: StorageFilter,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbStorageRow>, DbError> {
        let order = storage_order(sort);
        self.read(move |sql| {
            Ok(storage_rows_on(
                &sql,
                &order,
                filter,
                None,
                &order.everything(),
                limit,
                offset,
            )?
            .into_iter()
            .map(|(row, _)| row)
            .collect::<Vec<_>>())
        })
        .process(super::release_projection::process_storage_rows)
        .await
    }

    /// Follow the Storage Manager list: every window the request names under
    /// its sort, filter and search, each read from the row beside it when one
    /// is known (`list_windows`), with the filtered set's count and total
    /// size and the rows of the selected releases it holds read once per run.
    /// A new sort, filter, search, window set, selection, or upload queue
    /// points the same query at it through its request handle.
    ///
    /// A search's count is read first, from what it finds: it decides
    /// whether each window is read from those releases or through the
    /// order's index (`StorageSearch`). Its rows' places hold for the next
    /// read only while what library search finds has not moved either.
    pub(crate) fn subscribe_storage_browse(
        &self,
        initial: StorageBrowseRequest,
    ) -> coven::ReconfigurableLiveQuery<StorageBrowseRequest, StorageBrowseProjection> {
        let places = KnownPlaces::new();
        self.inner
            .handle
            .subscribe_reconfigurable(initial, move |request, sql| {
                let order = storage_order(&request.sort);
                let query = crate::library::LibrarySearchQuery::parse(&request.search);
                let (total_count, total_size, search) =
                    storage_totals_on(&sql, request.filter, query.as_ref())
                        .map_err(CovenError::from)?;
                // The places a read found hold for the next only under the
                // same sort, filter and search.
                let sources: &[&str] = match query {
                    Some(_) => &["release_listing", "library_search"],
                    None => &["release_listing"],
                };
                let standing = (
                    request.sort,
                    request.filter,
                    query,
                    list_order_versions_on(&sql, sources)?,
                );
                let WindowsRead { windows, found } = read_windows(
                    &order,
                    &request.windows,
                    places.under(&standing),
                    |seek, limit, offset| {
                        storage_rows_on(
                            &sql,
                            &order,
                            request.filter,
                            search.as_ref(),
                            seek,
                            limit,
                            offset,
                        )
                    },
                )
                .map_err(CovenError::from)?;
                places.keep(standing, found);
                let selected =
                    storage_selected_on(&sql, request.filter, search.as_ref(), &request.selected)
                        .map_err(CovenError::from)?;
                let queues = storage_queues_on(&sql).map_err(CovenError::from)?;
                let searching = search.is_some();
                Ok((
                    request.clone(),
                    searching,
                    windows,
                    total_count,
                    total_size,
                    selected,
                    queues,
                ))
            })
            .process(
                |(request, searching, windows, total_count, total_size, selected, queues)| {
                    let windows = windows
                        .into_iter()
                        .map(|(window, rows)| {
                            Ok(crate::library::LibraryBrowseWindow {
                                window,
                                rows: super::release_projection::process_storage_rows(rows)?,
                            })
                        })
                        .collect::<Result<Vec<_>, CovenError>>()?;
                    Ok(StorageBrowseProjection {
                        sort: request.sort,
                        filter: request.filter,
                        search: request.search,
                        searching,
                        windows,
                        total_count,
                        total_size,
                        selected: super::release_projection::process_storage_rows(selected)?,
                        queues,
                    })
                },
            )
    }
}

/// The transfer queues holding files waiting or on the move, each with
/// whether it is paused, and for the pin and output queues the release
/// running. A queue holding only failed entries holds no work.
pub(super) fn storage_queues_on(
    sql: &SqlReadContext<'_>,
) -> Result<crate::library::StorageQueues, DbError> {
    let uploads = sql.query_row(
        "SELECT EXISTS (SELECT 1 FROM coven_upload_queue), \
             (SELECT paused FROM coven_upload_pause)",
        [],
        |row| {
            Ok(row
                .get::<_, bool>(0)?
                .then_some(crate::library::UploadQueueRun {
                    paused: row.get(1)?,
                }))
        },
    )?;
    let release_queue = |table: &str, queue: &str| {
        sql.query_row(
            &format!(
                "SELECT EXISTS (SELECT 1 FROM {table} WHERE state IN ('queued', 'active')), \
                     (SELECT paused FROM release_queues WHERE queue = ?1), \
                     (SELECT release_id FROM {table} WHERE state = 'active')"
            ),
            [queue],
            |row| {
                Ok(if row.get::<_, bool>(0)? {
                    Some(crate::library::ReleaseQueueRun {
                        paused: row.get(1)?,
                        active: row.get(2)?,
                    })
                } else {
                    None
                })
            },
        )
    };
    Ok(crate::library::StorageQueues {
        uploads,
        pins: release_queue("download_queue", "download")?,
        outputs: release_queue("output_queue", "output")?,
    })
}

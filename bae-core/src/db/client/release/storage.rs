//! The storage manager's reads over releases: one release's storage summary,
//! the remote releases' representative blobs, and the paged listing with its
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
            r.format, \
            r.remote, \
            (SELECT rf.id FROM release_files rf WHERE rf.release_id = r.id LIMIT 1) AS any_file_id, \
            {artist_names} AS artist_names, \
            COALESCE(( \
                SELECT COUNT(*) FROM release_files rf WHERE rf.release_id = r.id \
            ), 0) AS file_count, \
            COALESCE(( \
                SELECT SUM(rf.file_size) FROM release_files rf WHERE rf.release_id = r.id \
            ), 0) AS total_size \
        FROM releases r \
        JOIN albums a ON a.id = r.album_id \
        WHERE r.id = ?1",
            artist_names = album_artist_names_sql()
        )
    }

    /// The storage summary for one release, or `None` if it doesn't exist. The
    /// download queue reads a release's title / file count / total size from it at
    /// enqueue time, for the Downloads-pane row.
    pub async fn find_release_storage_summary(
        &self,
        release_id: &str,
    ) -> Result<Option<DbReleaseStorageSummary>, DbError> {
        let release_id = release_id.to_string();
        let query = Self::release_storage_summary_query();
        self.read(move |sql| {
            sql.query_row(&query, [release_id], row_to_release_storage_summary)
                .optional()
                .map_err(DbError::from)
        })
        .await
    }

    /// A representative file id for every remote release — one per release, `None`
    /// for a remote release with no files. Pin/unpin act on all a release's blobs
    /// together, so one file stands for the release. The disconnect flow asks
    /// coven's cache whether each is pinned, to count the releases that become
    /// unreachable when the cloud provider is removed: an unpinned remote release
    /// is reachable only through the cloud.
    pub async fn get_remote_release_file_ids(&self) -> Result<Vec<Option<String>>, DbError> {
        self.read(move |sql| {
            sql.query(
                "SELECT (SELECT rf.id FROM release_files rf WHERE rf.release_id = r.id LIMIT 1) \
                     FROM releases r WHERE r.remote = 1",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .map_err(DbError::from)
        })
        .await
    }

    /// The releases coven currently has blobs queued to upload for, or empty for
    /// any filter that does not ask about uploading.
    ///
    /// Which releases are mid-upload is coven's queue state, so the storage page
    /// reads it from there and binds the ids into its own query rather than
    /// joining across into coven's tables. Every upload carries the gated root it
    /// belongs to, which for bae is always the release.
    async fn uploading_release_ids(&self, filter: StorageFilter) -> Result<Vec<String>, DbError> {
        if filter != StorageFilter::Uploading {
            return Ok(Vec::new());
        }
        let mut seen = HashSet::new();
        let ids = self
            .inner
            .handle
            .queued_uploads()
            .await?
            .into_iter()
            .filter(|upload| upload.root_table == "releases")
            .map(|upload| upload.root_id)
            .filter(|release_id| seen.insert(release_id.clone()))
            .collect();
        Ok(ids)
    }

    /// Count storage rows matching `filter`, using the same filter logic as
    /// `get_storage_page` so `total_count` describes the same set the page pages.
    pub async fn get_storage_count(&self, filter: StorageFilter) -> Result<u64, DbError> {
        let uploading = self.uploading_release_ids(filter).await?;
        let where_clause = storage_filter_where(filter, uploading.len());
        self.read(move |sql| storage_count_on(&sql, &where_clause, &uploading))
            .await
    }

    /// Sum of `total_size` over every storage row matching `filter` — the
    /// storage-manager footer's "Total:", independent of how many pages have
    /// loaded. Same filter logic as `get_storage_page` / `get_storage_count`. A
    /// release with no files contributes nothing through the inner join, matching a
    /// page row's own `COALESCE(SUM(...), 0)`.
    pub async fn get_storage_total_size(&self, filter: StorageFilter) -> Result<u64, DbError> {
        let uploading = self.uploading_release_ids(filter).await?;
        let where_clause = storage_filter_where(filter, uploading.len());
        self.read(move |sql| storage_total_size_on(&sql, &where_clause, &uploading))
            .await
    }

    pub async fn get_storage_page(
        &self,
        sort: &StorageSortCriterion,
        filter: StorageFilter,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbStorageRow>, DbError> {
        let uploading = self.uploading_release_ids(filter).await?;
        let queue_ordered = filter == StorageFilter::Uploading && !uploading.is_empty();
        let (order_by, needs_artist_sort_join) = if queue_ordered {
            ("upload_queue.position".to_string(), false)
        } else {
            storage_order_by(sort)
        };
        let artist_sort_join = album_summary_artist_join(needs_artist_sort_join);
        let where_clause = storage_filter_where(filter, uploading.len());
        let page_where = if queue_ordered { "" } else { &where_clause };

        let query = storage_page_query(
            &order_by,
            &artist_sort_join,
            page_where,
            usize::from(queue_ordered) * uploading.len(),
        );

        self.read(move |sql| storage_page_on(&sql, &query, &uploading, offset, limit))
            .process(super::release_projection::process_storage_rows)
            .await
    }

    /// Follow the Storage Manager list: every window the request names under
    /// its sort and filter, with the filtered set's count and total size read
    /// once per run. A new sort, filter, window set, or upload queue points the
    /// same query at it through its request handle.
    pub(crate) fn subscribe_storage_browse(
        &self,
        initial: StorageBrowseRequest,
    ) -> coven::ReconfigurableLiveQuery<StorageBrowseRequest, StorageBrowseProjection> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, move |request, sql| {
                let uploading = request.uploading.as_slice();
                let queue_ordered =
                    request.filter == StorageFilter::Uploading && !uploading.is_empty();
                let (order_by, needs_artist_sort_join) = if queue_ordered {
                    ("upload_queue.position".to_string(), false)
                } else {
                    storage_order_by(&request.sort)
                };
                let artist_sort_join = album_summary_artist_join(needs_artist_sort_join);
                let where_clause = storage_filter_where(request.filter, uploading.len());
                let page_where = if queue_ordered { "" } else { &where_clause };
                let query = storage_page_query(
                    &order_by,
                    &artist_sort_join,
                    page_where,
                    usize::from(queue_ordered) * uploading.len(),
                );
                let windows = request
                    .windows
                    .iter()
                    .map(|window| {
                        Ok((
                            window.clone(),
                            storage_page_on(&sql, &query, uploading, window.offset, window.limit)
                                .map_err(CovenError::from)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, CovenError>>()?;
                let total_count =
                    storage_count_on(&sql, &where_clause, uploading).map_err(CovenError::from)?;
                let total_size = storage_total_size_on(&sql, &where_clause, uploading)
                    .map_err(CovenError::from)?;
                let album_ids = windows
                    .iter()
                    .flat_map(|(_, rows)| rows.iter().map(|(_, album)| album.id.clone()))
                    .collect::<Vec<_>>();
                let cover_versions = album_cover_versions_on(&sql, &album_ids)?;
                Ok((windows, total_count, total_size, cover_versions))
            })
            .process(
                |request, (windows, total_count, total_size, mut cover_versions)| {
                    let windows = windows
                        .into_iter()
                        .map(|(window, rows)| {
                            Ok(crate::library::LibraryBrowseWindow {
                                window,
                                rows: super::release_projection::process_storage_rows(rows)?,
                            })
                        })
                        .collect::<Result<Vec<_>, CovenError>>()?;
                    let cover_ids = windows
                        .iter()
                        .flat_map(|window| &window.rows)
                        .flat_map(|row| {
                            [row.release.id.clone()]
                                .into_iter()
                                .chain(resolve_primary_release_id(
                                    row.album.primary_release_id.as_deref(),
                                    row.album.release_ids.iter().map(String::as_str),
                                ))
                        })
                        .collect::<HashSet<_>>();
                    cover_versions.retain(|id, _| cover_ids.contains(id));
                    Ok(StorageBrowseProjection {
                        sort: request.sort,
                        filter: request.filter,
                        windows,
                        total_count,
                        total_size,
                        cover_versions,
                    })
                },
            )
    }
}

//! The Storage Manager's view over releases: the paged list with its
//! counts and totals.
//!
//! Its tables and the triggers keeping them are `storage.sql`.

use super::*;
use crate::album_detail::{StoragePage, StorageRow};
use bae_records::records::queued_uploads::{
    upload_queue_wait_in_row, RELEASES_SENDING, RELEASE_UPLOAD_COLUMNS, UPLOAD_QUEUE_WAIT_COLUMNS,
};

/// The SQL creating this part's tables and triggers.
pub(super) const SQL: &str = include_str!("storage.sql");

impl LibraryViews {
    /// Count storage rows matching `filter`: the set `get_storage_page` pages.
    pub async fn get_storage_count(&self, filter: StorageFilter) -> Result<u64, DbError> {
        self.store.read(move |sql| Ok(storage_counts_on(&sql, filter, None)?.0)).await
    }

    /// The size of every file of the storage rows matching `filter` — the
    /// storage-manager footer's "Total:", independent of how many pages have
    /// loaded.
    pub async fn get_storage_total_size(&self, filter: StorageFilter) -> Result<u64, DbError> {
        self.store.read(move |sql| Ok(storage_counts_on(&sql, filter, None)?.1)).await
    }

    /// One page of the Storage Manager list, pre-sorted and pre-filtered.
    /// `total_count` counts the filtered subset, not the whole library.
    pub async fn get_storage_page(
        &self,
        sort: &StorageSortCriterion,
        filter: StorageFilter,
        offset: u64,
        limit: u64,
    ) -> Result<StoragePage, DbError> {
        let raw_rows = self.get_db_storage_page(sort, filter, offset, limit).await?;
        let total_count = self.get_storage_count(filter).await?;
        let syncs = self.store.syncs().await?;
        let rows = raw_rows.into_iter().map(|raw| StorageRow::from_raw(raw, syncs)).collect();
        Ok(StoragePage { rows, total_count })
    }

    /// One page of the Storage Manager list as its rows read it,
    /// unresolved.
    pub(crate) async fn get_db_storage_page(
        &self,
        sort: &StorageSortCriterion,
        filter: StorageFilter,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbStorageRow>, DbError> {
        let order = storage_order(sort);
        self.store
            .read(move |sql| {
                Ok(storage_rows_on(&sql, &order, filter, None, &order.everything(), limit, offset)?
                    .into_iter()
                    .map(|(row, _)| row)
                    .collect::<Vec<_>>())
            })
            .process(process_storage_rows)
            .await
    }

    /// Follow one window of the Storage Manager list under `sort`, `filter`
    /// and the search text `search`: a range of its order (`list_windows`),
    /// each row beside whether storage is set up, which resolves it. Coven
    /// reruns it for a write inside its range alone.
    ///
    pub fn subscribe_storage_window(
        &self,
        sort: &StorageSortCriterion,
        filter: StorageFilter,
        search: &str,
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<
        ListWindow,
        WindowRead<(DbStorageRow, bool)>,
        bae_store::DbError,
    > {
        let order = storage_order(sort);
        let read_order = order.clone();
        let pattern =
            crate::LibrarySearchQuery::parse(search).map(|query| search_pattern(query.as_str()));
        self.store
            .subscribe_list_window(initial, order, move |sql, seek, limit, offset| {
                let syncs = syncs_on(sql)?;
                Ok(storage_rows_on(
                    sql,
                    &read_order,
                    filter,
                    pattern.as_deref(),
                    seek,
                    limit,
                    offset,
                )?
                .into_iter()
                .map(|(parts, key)| ((parts, syncs), key))
                .collect())
            })
            .process(|window| {
                window.try_map_rows(|(parts, syncs)| Ok((process_storage_row(parts)?, syncs)))
            })
    }

    /// Follow how many releases the Storage Manager list holds under `filter`
    /// and `search`, and their files' total size. Its own query, never beside
    /// a window's rows: every release added or removed moves it.
    pub fn subscribe_storage_count(
        &self,
        filter: StorageFilter,
        search: &str,
    ) -> coven::LiveQuery<StorageCounts, bae_store::DbError> {
        let pattern =
            crate::LibrarySearchQuery::parse(search).map(|query| search_pattern(query.as_str()));
        self.store.subscribe(move |sql| {
            let (rows, total_size) = storage_counts_on(&sql, filter, pattern.as_deref())?;
            Ok(StorageCounts { rows, total_size, searching: pattern.is_some() })
        })
    }

    /// Follow how many releases of the Storage Manager list come before the
    /// row at a key, a count over index ranges that moves in place with the
    /// key.
    pub fn subscribe_storage_position(
        &self,
        sort: &StorageSortCriterion,
        filter: StorageFilter,
        search: &str,
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
        let order = storage_order(sort);
        let pattern =
            crate::LibrarySearchQuery::parse(search).map(|query| search_pattern(query.as_str()));
        self.store.subscribe_reconfigurable(initial, move |key, sql| {
            rows_before(&order, key, |seek| {
                storage_seek_count_on(&sql, filter, pattern.as_deref(), &order, seek)
            })
        })
    }

    /// The key of the release at `position` of the Storage Manager list, or
    /// of its last row when it ends before it; `None` for an empty list.
    pub async fn storage_key_at(
        &self,
        sort: &StorageSortCriterion,
        filter: StorageFilter,
        search: &str,
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        let order = storage_order(sort);
        let pattern =
            crate::LibrarySearchQuery::parse(search).map(|query| search_pattern(query.as_str()));
        self.store
            .read(move |sql| {
                key_at(&order, position, &mut |seek, limit, offset| {
                    storage_rows_on(&sql, &order, filter, pattern.as_deref(), seek, limit, offset)
                })
            })
            .await
    }

    /// Follow the rows of the releases `initial` names that the Storage
    /// Manager list under `filter` and `search` holds, wherever in it they
    /// sit, with the storage actions every one of them allows. A selected
    /// release the list no longer holds is absent, so the inspector never
    /// shows, and the storage actions never act on, a release the list
    /// doesn't. The selection moves in place through the request handle.
    pub fn subscribe_storage_selection(
        &self,
        filter: StorageFilter,
        search: &str,
        initial: BTreeSet<String>,
    ) -> coven::ReconfigurableLiveQuery<
        BTreeSet<String>,
        (Vec<DbStorageRow>, bool),
        bae_store::DbError,
    > {
        let pattern =
            crate::LibrarySearchQuery::parse(search).map(|query| search_pattern(query.as_str()));
        self.store
            .subscribe_reconfigurable(initial, move |selected, sql| {
                let rows = storage_selected_on(&sql, filter, pattern.as_deref(), selected)?;
                Ok((rows, syncs_on(&sql)?))
            })
            .process(|(rows, syncs)| Ok((process_storage_rows(rows)?, syncs)))
    }

    /// Follow the transfer queues holding work, as the Storage Manager shows
    /// them.
    pub fn subscribe_storage_queues(
        &self,
    ) -> coven::LiveQuery<crate::StorageQueues, bae_store::DbError> {
        self.store.subscribe(|sql| storage_queues_on(&sql))
    }
}

/// How many releases the Storage Manager list holds, their files' total size,
/// and whether a search narrows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageCounts {
    pub rows: u64,
    pub total_size: u64,
    /// Whether the search text narrows the list: it is a library search, not
    /// blank.
    pub searching: bool,
}

/// One delivery of a Storage Manager window as the list shows it.
pub fn resolve_storage_window(read: WindowRead<(DbStorageRow, bool)>) -> crate::ListWindowValue {
    crate::ListWindowValue::of(read, |rows| {
        crate::ListRows::Storage(
            rows.into_iter().map(|(raw, syncs)| StorageRow::from_raw(raw, syncs)).collect(),
        )
    })
}

/// The rows of the selected releases the Storage Manager list holds, and the
/// storage actions every one of them allows.
pub fn resolve_storage_selection(
    (rows, syncs): (Vec<DbStorageRow>, bool),
) -> crate::StorageSelection {
    let selected: Vec<StorageRow> =
        rows.into_iter().map(|raw| StorageRow::from_raw(raw, syncs)).collect();
    crate::StorageSelection {
        selection_actions: crate::album_detail::common_storage_actions(&selected),
        selected,
    }
}

/// The transfer queues holding files waiting or on the move, each with what
/// holds it back, and for the pin and output queues the release running. A
/// queue holding only failed entries holds no work.
fn storage_queues_on(sql: &SqlReadContext<'_>) -> Result<crate::StorageQueues, DbError> {
    let uploads = sql.query_row(
        &format!("SELECT EXISTS (SELECT 1 FROM coven_upload_queue), {UPLOAD_QUEUE_WAIT_COLUMNS}"),
        [],
        |row| {
            Ok(if row.get::<_, bool>(0)? {
                Some(crate::UploadQueueRun { wait: upload_queue_wait_in_row(row)? })
            } else {
                None
            })
        },
    )?;
    let release_queue = |table: &str, queue: &str, watches_network: bool| {
        sql.query_row(
            &format!(
                "SELECT EXISTS (SELECT 1 FROM {table} WHERE state IN ('queued', 'active')), \
                     (SELECT paused FROM release_queues WHERE queue = ?1), \
                     (SELECT release_id FROM {table} WHERE state = 'active'), \
                     (SELECT usable FROM coven_network)"
            ),
            [queue],
            |row| {
                Ok(if row.get::<_, bool>(0)? {
                    let network_usable: Option<bool> = row.get(3)?;
                    Some(crate::ReleaseQueueRun {
                        wait: bae_transfers::QueueWait::of(
                            row.get(1)?,
                            network_usable.filter(|_| watches_network),
                        ),
                        active: row.get(2)?,
                    })
                } else {
                    None
                })
            },
        )
    };
    Ok(crate::StorageQueues {
        uploads,
        downloads: release_queue("download_queue", "download", true)?,
        outputs: release_queue("output_queue", "output", false)?,
    })
}

/// One Storage Manager row as read: the release's half, the album's half,
/// what its storage state shows, and when the release was added.
pub(crate) type StorageRowParts = (
    DbReleaseSummary,
    Option<String>,
    AlbumSummaryRow,
    crate::album_detail::StorageRowStatus,
    DateTime<Utc>,
);

fn storage_row_parts(row: &Row<'_>) -> rusqlite::Result<StorageRowParts> {
    Ok((
        row_to_release_summary(row)?,
        row.get("release_cover_version")?,
        AlbumSummaryRow::read(row)?,
        storage_status(row)?,
        rfc3339_column(row, "date_added")?,
    ))
}

fn row_to_release_summary(row: &Row<'_>) -> rusqlite::Result<DbReleaseSummary> {
    let id: String = row.get("release_id")?;
    Ok(DbReleaseSummary {
        unstored_audio: unstored_audio_column(row, "unstored_audio")?,
        id,
        album_id: row.get("album_id")?,
        media: read_media(row, "release_media")?,
        remote: row.get("remote")?,
        pinned: row.get("pinned")?,
        upload: bae_records::records::queued_uploads::release_upload_in_row(row)?,
        download: bae_transfers::download_entry_column(row)?,
        file_count: row.get("file_count")?,
        total_size: bae_store::read::count_column(row, "total_size")?,
    })
}

/// What the row's storage state shows: where its release's files are, from the
/// listing's `storage_order`, with the transition, the download queue entry
/// and the failed move's file that go with it.
fn storage_status(row: &Row<'_>) -> rusqlite::Result<crate::album_detail::StorageRowStatus> {
    use crate::album_detail::{StoragePlace, StorageRowStatus};
    let column = row.as_ref().column_index("storage_order")?;
    let place = match row.get::<_, i64>(column)? {
        0 => StoragePlace::Failed,
        1 => StoragePlace::Uploading,
        2 => StoragePlace::Pinning,
        3 => StoragePlace::OnOneDevice,
        4 => StoragePlace::Pinned,
        5 => StoragePlace::InStorage,
        order => {
            return Err(bae_store::read::column_conversion_error(
                row,
                column,
                format!("storage order {order} names no place"),
            ))
        }
    };
    let download = bae_transfers::download_entry_column(row)?;
    let failed_move_file = bae_records::records::queued_uploads::changed_original_in_row(row)?;
    StorageRowStatus::of(place, download, failed_move_file)
        .map_err(|reason| bae_store::read::column_conversion_error(row, column, reason))
}

/// What a Storage Manager filter keeps of `release_listing listing`, as a
/// condition.
fn storage_filter(filter: StorageFilter) -> String {
    stored_value(filter)
        .map_or_else(|| "1".to_string(), |stored| format!("listing.stored = {stored}"))
}

/// The value of `stored` a filter keeps (1 for remote, 0 for local), or
/// `None` when it keeps both.
fn stored_value(filter: StorageFilter) -> Option<u8> {
    match filter {
        StorageFilter::All => None,
        StorageFilter::Remote => Some(1),
        StorageFilter::Local => Some(0),
    }
}

/// Which releases a Storage Manager statement reads: every release the
/// filter keeps, or those whose album the search pattern `:search` finds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StorageRows {
    Listed,
    Searched,
}

/// The condition keeping the releases `rows` reads whose album is `album_id`
/// (an SQL expression): the albums the search finds, or all.
fn storage_search_condition(rows: StorageRows, album_id: &str) -> String {
    match rows {
        StorageRows::Listed => "1".to_string(),
        StorageRows::Searched => album_matches_sql(album_id, ":search"),
    }
}

/// The Storage Manager row's columns: the release's half and its album's,
/// the raw aggregates the resolver maps to `ReleaseSummary` / `AlbumSummary`,
/// whether it is pinned, where it sorts by storage, when it was added, and
/// `extra` columns.
fn storage_row_columns(extra: &str) -> String {
    let album_columns = album_summary_columns();
    format!(
        "r.id AS release_id, \
         r.album_id, \
         {release_media} AS release_media, \
         storage.stored AS remote, \
         storage.file_count, \
         storage.total_size, \
         {RELEASE_STORAGE_PINNED} AS pinned, \
         listing.storage_order, \
         listing.created_at AS date_added, \
         {release_cover} AS release_cover_version, \
         {RELEASE_UPLOAD_COLUMNS}, \
         {unstored_audio} AS unstored_audio, \
         {download_entry}, \
         {album_columns}{extra}",
        release_cover = cover_version_sql("r.id"),
        release_media = release_media_sql("r.id"),
        unstored_audio = unstored_audio_json_sql("r.id"),
        download_entry = download_entry_sql("r.id"),
    )
}

/// What a row's columns read besides `release_listing listing`, which each
/// statement names itself.
fn storage_row_joins() -> String {
    format!(
        "CROSS JOIN release_storage storage ON storage.release_id = listing.release_id \
         JOIN releases r ON r.id = listing.release_id \
         JOIN albums a ON a.id = listing.album_id \
         LEFT JOIN ({RELEASES_SENDING}) sending ON sending.release_id = listing.release_id"
    )
}

/// One Storage Manager row per release `rows` reads that the filter keeps
/// and `condition` passes, with the columns [`storage_row_columns`] names.
fn storage_row_select(
    filter: StorageFilter,
    rows: StorageRows,
    extra: &str,
    condition: &str,
) -> String {
    format!(
        "SELECT {columns} \
        FROM release_listing listing {joins} \
        WHERE {filter} AND {searched} AND ({condition})",
        columns = storage_row_columns(extra),
        joins = storage_row_joins(),
        filter = storage_filter(filter),
        searched = storage_search_condition(rows, "listing.album_id"),
    )
}

/// The rows of the Storage Manager list under `filter` that `rows` reads and
/// `seek` passes, in its order, each with its key in `order`, from `:offset`
/// for at most `:limit` rows.
///
/// An inner statement picks the window's releases by id alone, so a release
/// `:offset` skips costs its place in the order and nothing else; the outer
/// statement reads the columns of just the picked releases, in the same
/// order.
pub(crate) fn storage_window_query(
    order: &ListOrder,
    filter: StorageFilter,
    rows: StorageRows,
    seek: &Seek,
) -> String {
    format!(
        "SELECT {columns} \
         FROM (SELECT listing.release_id AS picked_release_id FROM release_listing listing \
               WHERE {filter} AND {searched} AND ({condition}) \
               ORDER BY {ordering} LIMIT :limit OFFSET :offset) picked \
         CROSS JOIN release_listing listing ON listing.release_id = picked.picked_release_id \
         {joins} \
         ORDER BY {ordering}",
        columns = storage_row_columns(&format!(", {}", order.key_columns())),
        joins = storage_row_joins(),
        filter = storage_filter(filter),
        searched = storage_search_condition(rows, "listing.album_id"),
        condition = order.condition(seek),
        ordering = order.ordering(seek),
    )
}

/// The rows of the release ids the JSON array `:selected` names that the
/// list under `filter` holds, and with `searching` the search pattern
/// `:search` finds, by release id. SQLite folds the outer id filter into the
/// list's rows, so each selected release is looked up by its key rather than
/// the list read whole.
pub(crate) fn storage_selected_query(filter: StorageFilter, searching: bool) -> String {
    let rows = match searching {
        true => StorageRows::Searched,
        false => StorageRows::Listed,
    };
    format!(
        "SELECT * FROM ({}) \
         WHERE release_id IN (SELECT value FROM json_each(:selected)) ORDER BY release_id",
        storage_row_select(filter, rows, "", "1"),
    )
}

/// How many releases the Storage Manager list under `filter` holds and their
/// files' total size, with `pattern` (a search pattern) narrowed to the
/// releases whose album it finds.
pub(crate) fn storage_counts_on(
    sql: &SqlReadContext<'_>,
    filter: StorageFilter,
    pattern: Option<&str>,
) -> Result<(u64, u64), DbError> {
    let rows = pattern.map_or(StorageRows::Listed, |_| StorageRows::Searched);
    let statement = format!(
        "SELECT COUNT(*), COALESCE(SUM(st.total_size), 0) \
         FROM release_storage st JOIN releases r ON r.id = st.release_id \
         WHERE {stored} AND {searched}",
        stored = stored_value(filter)
            .map_or_else(|| "1".to_string(), |stored| format!("st.stored = {stored}")),
        searched = storage_search_condition(rows, "r.album_id"),
    );
    let params: Vec<(&str, &dyn rusqlite::ToSql)> =
        pattern.iter().map(|pattern| (":search", pattern as &dyn rusqlite::ToSql)).collect();
    Ok(sql.query_row(&statement, params.as_slice(), |row| {
        Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64))
    })?)
}

/// The Storage Manager list's rows under `filter` and the search `pattern`
/// that `seek` reads in `order`, at most `limit` from `offset`, each with its key.
pub(crate) fn storage_rows_on(
    sql: &SqlReadContext<'_>,
    order: &ListOrder,
    filter: StorageFilter,
    pattern: Option<&str>,
    seek: &Seek,
    limit: u64,
    offset: u64,
) -> Result<Vec<Keyed<StorageRowParts>>, DbError> {
    let rows = pattern.map_or(StorageRows::Listed, |_| StorageRows::Searched);
    let statement = storage_window_query(order, filter, rows, seek);
    Ok(seek.bound(limit, offset, |params| {
        let mut params = params.to_vec();
        if let Some(pattern) = &pattern {
            params.push((":search", pattern));
        }
        sql.query(&statement, params.as_slice(), |row| {
            Ok((storage_row_parts(row)?, order.key_of(row)?))
        })
    })?)
}

pub(crate) fn process_storage_rows(
    rows: Vec<StorageRowParts>,
) -> Result<Vec<DbStorageRow>, DbError> {
    rows.into_iter().map(process_storage_row).collect()
}

fn process_storage_row(
    (release, release_cover_version, album, status, date_added): StorageRowParts,
) -> Result<DbStorageRow, DbError> {
    Ok(DbStorageRow { release, date_added, release_cover_version, album: album.process()?, status })
}

/// How many releases of the Storage Manager list under `filter` (narrowed to
/// the albums the search `pattern` finds, with one) come before the rows `seek`
/// reads.
pub(crate) fn storage_seek_count_on(
    sql: &SqlReadContext<'_>,
    filter: StorageFilter,
    pattern: Option<&str>,
    order: &ListOrder,
    seek: &Seek,
) -> Result<u64, DbError> {
    let rows = pattern.map_or(StorageRows::Listed, |_| StorageRows::Searched);
    let statement = format!(
        "SELECT COUNT(*) FROM release_listing listing \
         WHERE {filter} AND {searched} AND ({condition})",
        filter = storage_filter(filter),
        searched = storage_search_condition(rows, "listing.album_id"),
        condition = order.condition(seek),
    );
    let mut params = seek.params();
    if let Some(pattern) = &pattern {
        params.push((":search", pattern));
    }
    Ok(sql
        .query_row(&statement, params.as_slice(), |row| row.get::<_, i64>(0))
        .map(|count| count as u64)?)
}

/// The rows of the `release_ids` the list under `filter` and the search
/// `pattern` (see [`search_pattern`]) holds, wherever in it they sit, by
/// release id.
pub(crate) fn storage_selected_on(
    sql: &SqlReadContext<'_>,
    filter: StorageFilter,
    pattern: Option<&str>,
    release_ids: &BTreeSet<String>,
) -> Result<Vec<StorageRowParts>, DbError> {
    if release_ids.is_empty() {
        return Ok(Vec::new());
    }
    let selected = serde_json::to_string(release_ids)
        .map_err(|error| DbError::encode("selected release ids", error))?;
    let query = storage_selected_query(filter, pattern.is_some());
    let mut params: Vec<(&str, &dyn rusqlite::ToSql)> = vec![(":selected", &selected)];
    if let Some(pattern) = &pattern {
        params.push((":search", pattern));
    }
    Ok(sql.query(&query, params.as_slice(), storage_row_parts)?)
}

/// The Storage Manager list's order over `release_listing listing`. Every
/// order ends with the album title, then `created_at` unless it sorts by
/// that, and the release id, which tell apart two releases sharing a title
/// and a timestamp (bulk imports, tests). An index of `release_listing`
/// serves each, alone and after `stored`.
pub(crate) fn storage_order(sort: &StorageSortCriterion) -> ListOrder {
    let dir = sort.direction;
    let mut terms = match sort.field {
        StorageSortField::AlbumTitle => {
            vec![SortTerm::running("listing.album_title_sort_key", dir)]
        }
        StorageSortField::ArtistNames => vec![
            SortTerm::running("listing.credit_sort_key", dir).nullable(),
            SortTerm::running("listing.credit_artist_ids", dir).nullable(),
            SortTerm::ascending("listing.album_title_sort_key"),
        ],
        // A release stating no medium goes last either way.
        StorageSortField::Media => vec![
            SortTerm::ascending("listing.media_empty"),
            SortTerm::running("listing.media_key", dir).nullable(),
            SortTerm::ascending("listing.album_title_sort_key"),
        ],
        StorageSortField::FileCount => vec![
            SortTerm::running("listing.file_count", dir),
            SortTerm::ascending("listing.album_title_sort_key"),
        ],
        StorageSortField::TotalSize => vec![
            SortTerm::running("listing.total_size", dir),
            SortTerm::ascending("listing.album_title_sort_key"),
        ],
        StorageSortField::Storage => vec![
            SortTerm::running("listing.storage_order", dir),
            SortTerm::ascending("listing.album_title_sort_key"),
        ],
        StorageSortField::DateAdded => vec![
            SortTerm::running("listing.created_at", dir),
            SortTerm::ascending("listing.album_title_sort_key"),
        ],
    };
    if sort.field != StorageSortField::DateAdded {
        terms.push(SortTerm::ascending("listing.created_at"));
    }
    terms.push(SortTerm::ascending("listing.release_id"));
    ListOrder::new(terms)
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "storage_audio_tests.rs"]
mod audio_tests;

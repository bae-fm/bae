//! The Storage Manager's view over releases: the paged list with its
//! counts and totals.
//!
//! Its tables and the triggers keeping them are `storage.sql`.

use super::*;
use crate::album_detail::{StoragePage, StorageRow};
use bae_records::records::queued_uploads::{RELEASES_SENDING, RELEASE_UPLOAD_COLUMNS};

/// The SQL creating this part's tables and triggers.
pub(super) const SQL: &str = include_str!("storage.sql");

impl LibraryViews {
    /// Count storage rows matching `filter`: the set `get_storage_page` pages.
    pub async fn get_storage_count(&self, filter: StorageFilter) -> Result<u64, DbError> {
        self.store.read(move |sql| storage_count_on(&sql, filter)).await
    }

    /// The size of every file of the storage rows matching `filter` — the
    /// storage-manager footer's "Total:", independent of how many pages have
    /// loaded.
    pub async fn get_storage_total_size(&self, filter: StorageFilter) -> Result<u64, DbError> {
        self.store.read(move |sql| storage_total_size_on(&sql, filter)).await
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
    pub fn subscribe_storage_browse(
        &self,
        initial: StorageBrowseRequest,
    ) -> coven::ReconfigurableLiveQuery<
        StorageBrowseRequest,
        StorageBrowseProjection,
        bae_store::DbError,
    > {
        let places = KnownPlaces::new();
        self.store
            .subscribe_reconfigurable(initial, move |request, sql| {
                let order = storage_order(&request.sort);
                let query = crate::LibrarySearchQuery::parse(&request.search);
                let (total_count, total_size, search) =
                    storage_totals_on(&sql, request.filter, query.as_ref())?;
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
                    list_versions_on(&sql, crate::versions::TABLE, sources)?,
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
                )?;
                places.keep(standing, found);
                let selected =
                    storage_selected_on(&sql, request.filter, search.as_ref(), &request.selected)?;
                let queues = storage_queues_on(&sql)?;
                let syncs = syncs_on(&sql)?;
                let searching = search.is_some();
                Ok((
                    request.clone(),
                    searching,
                    windows,
                    total_count,
                    total_size,
                    selected,
                    queues,
                    syncs,
                ))
            })
            .process(
                |(
                    request,
                    searching,
                    windows,
                    total_count,
                    total_size,
                    selected,
                    queues,
                    syncs,
                )| {
                    let windows = windows
                        .into_iter()
                        .map(|(window, rows)| {
                            Ok(bae_store::LibraryBrowseWindow {
                                window,
                                rows: process_storage_rows(rows)?,
                            })
                        })
                        .collect::<Result<Vec<_>, DbError>>()?;
                    Ok(StorageBrowseProjection {
                        sort: request.sort,
                        filter: request.filter,
                        search: request.search,
                        searching,
                        windows,
                        total_count,
                        total_size,
                        selected: process_storage_rows(selected)?,
                        queues,
                        syncs,
                    })
                },
            )
    }
}

/// One delivery of the Storage Manager list as it shows it.
pub fn resolve_storage_browse(projection: StorageBrowseProjection) -> crate::StorageBrowseSnapshot {
    let syncs = projection.syncs;
    let resolve = |raw: DbStorageRow| StorageRow::from_raw(raw, syncs);
    let windows = projection
        .windows
        .into_iter()
        .map(|window| bae_store::LibraryBrowseWindow {
            window: window.window,
            rows: window.rows.into_iter().map(&resolve).collect(),
        })
        .collect();
    let selected: Vec<StorageRow> = projection.selected.into_iter().map(&resolve).collect();
    crate::StorageBrowseSnapshot {
        sort: projection.sort,
        filter: projection.filter,
        search: projection.search,
        searching: projection.searching,
        windows,
        total_count: projection.total_count,
        total_size: projection.total_size,
        selection_actions: crate::album_detail::common_storage_actions(&selected),
        selected,
        queues: projection.queues,
    }
}

/// The transfer queues holding files waiting or on the move, each with
/// whether it is paused, and for the pin and output queues the release
/// running. A queue holding only failed entries holds no work.
fn storage_queues_on(sql: &SqlReadContext<'_>) -> Result<crate::StorageQueues, DbError> {
    let uploads = sql.query_row(
        "SELECT EXISTS (SELECT 1 FROM coven_upload_queue), \
             (SELECT paused FROM coven_upload_pause)",
        [],
        |row| Ok(row.get::<_, bool>(0)?.then_some(crate::UploadQueueRun { paused: row.get(1)? })),
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
                    Some(crate::ReleaseQueueRun { paused: row.get(1)?, active: row.get(2)? })
                } else {
                    None
                })
            },
        )
    };
    Ok(crate::StorageQueues {
        uploads,
        pins: release_queue("download_queue", "download")?,
        outputs: release_queue("output_queue", "output")?,
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
        // Read once the query has returned: the row mapper reads columns only.
        unstored_audio: Vec::new(),
        id,
        album_id: row.get("album_id")?,
        media: read_media(row, "release_media")?,
        remote: row.get("remote")?,
        pinned: row.get("pinned")?,
        keeping_here: row.get("keeping_here")?,
        upload: bae_records::records::queued_uploads::release_upload_in_row(row)?,
        download: bae_transfers::download_entry_column(row)?,
        file_count: row.get("file_count")?,
        total_size: row.get("total_size")?,
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
    let keeping: bool = row.get("keeping_here")?;
    StorageRowStatus::of(place, download, failed_move_file, keeping)
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

/// `SUM(column)` over `release_listing_totals`, of the rows `filter` keeps.
fn listing_totals_sum_query(column: &str, filter: StorageFilter) -> String {
    let kept =
        stored_value(filter).map_or_else(String::new, |stored| format!(" WHERE stored = {stored}"));
    format!("SELECT COALESCE(SUM({column}), 0) FROM release_listing_totals{kept}")
}

/// `release_listing listing` reached by album alone: a statement reading
/// what a search finds looks up each album it finds there, and never walks
/// an order's index past the releases it doesn't find.
const FOUND_LISTING: &str = "release_listing listing INDEXED BY idx_release_listing_album";

/// Which releases a Storage Manager statement reads, and how it reaches
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StorageRows {
    /// Every release the filter keeps, through the order's index.
    Listed,
    /// The releases whose album the search pattern `:search` finds, each
    /// album tested alone as the order's index reaches its release.
    SearchedInOrder,
    /// The releases whose album the search pattern `:search` finds, read
    /// from what it finds and sorted.
    Found,
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
         {RELEASE_STORAGE_KEEPING} AS keeping_here, \
         listing.storage_order, \
         listing.created_at AS date_added, \
         {release_cover} AS release_cover_version, \
         {RELEASE_UPLOAD_COLUMNS}, \
         {download_entry}, \
         {album_columns}{extra}",
        release_cover = cover_version_sql("r.id"),
        release_media = release_media_sql("r.id"),
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

/// The `listing` a statement reads `rows` from, and the condition that keeps
/// the releases the search finds.
fn storage_listing(rows: StorageRows) -> (&'static str, String) {
    match rows {
        StorageRows::Listed => ("release_listing listing", "1".to_string()),
        StorageRows::SearchedInOrder => {
            ("release_listing listing", album_holds_sql("listing.album_id", ":search"))
        }
        StorageRows::Found => (FOUND_LISTING, album_matches_sql("listing.album_id", ":search")),
    }
}

/// One Storage Manager row per release `rows` reads that the filter keeps
/// and `condition` passes, with the columns [`storage_row_columns`] names.
fn storage_row_select(
    filter: StorageFilter,
    rows: StorageRows,
    extra: &str,
    condition: &str,
) -> String {
    let (listing, searched) = storage_listing(rows);
    format!(
        "SELECT {columns} \
        FROM {listing} {joins} \
        WHERE {filter} AND {searched} AND ({condition})",
        columns = storage_row_columns(extra),
        joins = storage_row_joins(),
        filter = storage_filter(filter),
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
    let (listing, searched) = storage_listing(rows);
    format!(
        "SELECT {columns} \
         FROM (SELECT listing.release_id AS picked_release_id FROM {listing} \
               WHERE {filter} AND {searched} AND ({condition}) \
               ORDER BY {ordering} LIMIT :limit OFFSET :offset) picked \
         CROSS JOIN release_listing listing ON listing.release_id = picked.picked_release_id \
         {joins} \
         ORDER BY {ordering}",
        columns = storage_row_columns(&format!(", {}", order.key_columns())),
        joins = storage_row_joins(),
        filter = storage_filter(filter),
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
        true => StorageRows::SearchedInOrder,
        false => StorageRows::Listed,
    };
    format!(
        "SELECT * FROM ({}) \
         WHERE release_id IN (SELECT value FROM json_each(:selected)) ORDER BY release_id",
        storage_row_select(filter, rows, "", "1"),
    )
}

/// How many releases the list under `filter` holds whose album the search
/// pattern `:search` finds, and their files' total size, read from what the
/// search tables' indexes find, beside whether the pattern is in the titles
/// of fewer than `:titles` albums. Only then is the count read
/// ([`storage_found_titles`]): the title search stops at `:titles` albums
/// and the match list reads nothing more.
pub(crate) fn storage_found_totals_query(filter: StorageFilter) -> String {
    format!(
        "WITH titled (album_id) AS MATERIALIZED ( \
             SELECT album_id FROM album_search WHERE title GLOB :search LIMIT :titles), \
         few (counted) AS (SELECT COUNT(*) < :titles FROM titled) \
         SELECT (SELECT counted FROM few), COUNT(*), COALESCE(SUM(listing.total_size), 0) \
         FROM {FOUND_LISTING} \
         WHERE {filter} AND listing.album_id IN ( \
             SELECT album_id FROM titled WHERE (SELECT counted FROM few) \
             UNION ALL \
             SELECT album_id FROM ({credits}) WHERE (SELECT counted FROM few))",
        filter = storage_filter(filter),
        credits = albums_credits_find_sql(":search"),
    )
}

/// How many releases the list under `filter` holds whose album the search
/// pattern `:search` doesn't find, and their files' total size: what a
/// search in many titles is counted from, taken off the counts kept beside
/// the listing. Every album's title, as its search table keeps it, is tested
/// in one pass over that table, which costs less than its index finding
/// each album; only the albums whose title doesn't hold the pattern are
/// looked up in the listing, and of those the ones a credit's name finds,
/// through the credits' indexes, are left out.
pub(crate) fn storage_unfound_totals_query(filter: StorageFilter) -> String {
    format!(
        "SELECT COUNT(*), COALESCE(SUM(listing.total_size), 0) \
         FROM album_search untitled \
         CROSS JOIN {FOUND_LISTING} ON listing.album_id = untitled.album_id \
         WHERE NOT +untitled.title GLOB :search AND {filter} AND NOT {credited}",
        filter = storage_filter(filter),
        credited = album_credits_found_sql("listing.album_id", ":search"),
    )
}

/// What counting one album costs each way of counting a search, measured
/// against testing one album's title in a pass over every title: an album
/// whose title holds the pattern found through the title search's index and
/// looked up in the listing ([`storage_found_totals_query`]), and an album
/// whose title doesn't looked up beside the pass
/// ([`storage_unfound_totals_query`]).
const FOUND_ALBUM_COST: u64 = 16;
const UNTITLED_LOOKUP_COST: u64 = 3;

/// The most titles of `albums` albums a search's pattern is counted through
/// the indexes for. Past this many, testing every title and taking the
/// albums whose title doesn't hold it off the listing's counts costs less:
/// counted through the indexes, `titled` albums cost
/// `FOUND_ALBUM_COST * titled`; counted off, `albums` passed plus
/// `UNTITLED_LOOKUP_COST * (albums - titled)` looked up.
pub(crate) fn storage_found_titles(albums: u64) -> u64 {
    let albums = u128::from(albums);
    (albums * u128::from(1 + UNTITLED_LOOKUP_COST)
        / u128::from(FOUND_ALBUM_COST + UNTITLED_LOOKUP_COST)) as u64
}

/// What a row read from a search's matches costs against a row the order's
/// index passes and tests: it is reached through the search tables and
/// joined to its listing row, and the sort picks the window's rows from the
/// rest, about twenty-five times the few lookups testing its album's text.
const FOUND_ROW_COST: u128 = 25;

/// A Storage Manager search as one read of the list runs it: the pattern its
/// text matches, how many releases the filter keeps, and how many of those
/// the search finds, which decide where each statement reads its rows from.
pub(crate) struct StorageSearch {
    pattern: String,
    kept: u64,
    found: u64,
}

impl StorageSearch {
    /// Where a statement reading at most `limit` rows from `offset` reads
    /// them from. Read in the order's index, it passes about its end times
    /// the releases the filter keeps over those the search finds before it
    /// holds its rows; read from what the search finds, it reads each of
    /// those, at [`FOUND_ROW_COST`]. A search finding few releases is read
    /// from them, so it never walks the whole index for a handful of rows.
    pub(crate) fn rows(&self, limit: u64, offset: u64) -> StorageRows {
        let found = u128::from(self.found);
        let end = u128::from(offset.saturating_add(limit));
        match FOUND_ROW_COST * found * found < end * u128::from(self.kept) {
            true => StorageRows::Found,
            false => StorageRows::SearchedInOrder,
        }
    }
}

/// How many releases the Storage Manager list holds and their files' total
/// size, under `filter` and, with one, the search `query`: the counts kept
/// beside the listing, or those of what the search finds. A search comes
/// back as the read runs it.
pub(crate) fn storage_totals_on(
    sql: &SqlReadContext<'_>,
    filter: StorageFilter,
    query: Option<&crate::LibrarySearchQuery>,
) -> Result<(u64, u64, Option<StorageSearch>), DbError> {
    let Some(query) = query else {
        return Ok((storage_count_on(sql, filter)?, storage_total_size_on(sql, filter)?, None));
    };
    let pattern = search_pattern(query.as_str());
    let kept = storage_count_on(sql, filter)?;
    // The listing's release count stands in for how many albums there are.
    let titles = storage_found_titles(storage_count_on(sql, StorageFilter::All)?);
    let (counted, found, size) = sql.query_row(
        &storage_found_totals_query(filter),
        rusqlite::named_params! {
            ":search": pattern,
            ":titles": bae_store::list_windows::sql_bound(titles),
        },
        |row| {
            Ok((row.get::<_, bool>(0)?, row.get::<_, i64>(1)? as u64, row.get::<_, i64>(2)? as u64))
        },
    )?;
    let (found, size) = match counted {
        true => (found, size),
        false => {
            let (unfound, unfound_size) = sql.query_row(
                &storage_unfound_totals_query(filter),
                rusqlite::named_params! { ":search": pattern },
                |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64)),
            )?;
            let kept_size = storage_total_size_on(sql, filter)?;
            // The releases a search leaves out are among those the list
            // holds, so leaving out more is a bug of bae's: the counts kept
            // beside the listing or this count disagree with the listing.
            // The panic comes back from the store's work as a bug.
            match (kept.checked_sub(unfound), kept_size.checked_sub(unfound_size)) {
                (Some(found), Some(size)) => (found, size),
                _ => panic!(
                    "the search leaves out {unfound} releases of {unfound_size} bytes, \
                     more than the {kept} releases of {kept_size} bytes the list holds"
                ),
            }
        }
    };
    let search = StorageSearch { pattern, kept, found };
    Ok((found, size, Some(search)))
}

/// The Storage Manager list's rows under `filter` and `search` that `seek`
/// reads in `order`, at most `limit` from `offset`, each with its key.
pub(crate) fn storage_rows_on(
    sql: &SqlReadContext<'_>,
    order: &ListOrder,
    filter: StorageFilter,
    search: Option<&StorageSearch>,
    seek: &Seek,
    limit: u64,
    offset: u64,
) -> Result<Vec<Keyed<StorageRowParts>>, DbError> {
    let rows = search.map_or(StorageRows::Listed, |search| search.rows(limit, offset));
    let statement = storage_window_query(order, filter, rows, seek);
    let mut rows = seek.bound(limit, offset, |params| {
        let mut params = params.to_vec();
        if let Some(search) = search {
            params.push((":search", &search.pattern));
        }
        sql.query(&statement, params.as_slice(), |row| {
            Ok((storage_row_parts(row)?, order.key_of(row)?))
        })
    })?;
    for ((release, ..), _key) in &mut rows {
        release.unstored_audio = unstored_audio_on(sql, &release.id)?;
    }
    Ok(rows)
}

pub(crate) fn process_storage_rows(
    rows: Vec<StorageRowParts>,
) -> Result<Vec<DbStorageRow>, DbError> {
    rows.into_iter()
        .map(|(release, release_cover_version, album, status, date_added)| {
            Ok(DbStorageRow {
                release,
                date_added,
                release_cover_version,
                album: album.process()?,
                status,
            })
        })
        .collect()
}

/// How many releases the Storage Manager list holds under `filter`.
pub(crate) fn storage_count_on(
    sql: &SqlReadContext<'_>,
    filter: StorageFilter,
) -> Result<u64, DbError> {
    sql.query_row(&storage_count_query(filter), [], |row| row.get::<_, i64>(0))
        .map(|count| count as u64)
        .map_err(DbError::from)
}

/// The statement [`storage_count_on`] reads.
pub(crate) fn storage_count_query(filter: StorageFilter) -> String {
    listing_totals_sum_query("release_count", filter)
}

/// The total size of the files of the releases the Storage Manager list
/// holds under `filter`.
pub(crate) fn storage_total_size_on(
    sql: &SqlReadContext<'_>,
    filter: StorageFilter,
) -> Result<u64, DbError> {
    sql.query_row(&storage_total_size_query(filter), [], |row| row.get::<_, i64>(0))
        .map(|size| size as u64)
        .map_err(DbError::from)
}

/// The statement [`storage_total_size_on`] reads.
pub(crate) fn storage_total_size_query(filter: StorageFilter) -> String {
    listing_totals_sum_query("total_size", filter)
}

/// The rows of the `release_ids` the list under `filter` and `search`
/// holds, wherever in it they sit, by release id.
pub(crate) fn storage_selected_on(
    sql: &SqlReadContext<'_>,
    filter: StorageFilter,
    search: Option<&StorageSearch>,
    release_ids: &BTreeSet<String>,
) -> Result<Vec<StorageRowParts>, DbError> {
    if release_ids.is_empty() {
        return Ok(Vec::new());
    }
    let selected = serde_json::to_string(release_ids)
        .map_err(|error| DbError::encode("selected release ids", error))?;
    let query = storage_selected_query(filter, search.is_some());
    let mut params: Vec<(&str, &dyn rusqlite::ToSql)> = vec![(":selected", &selected)];
    if let Some(search) = search {
        params.push((":search", &search.pattern));
    }
    let mut rows = sql.query(&query, params.as_slice(), storage_row_parts)?;
    for (release, ..) in &mut rows {
        release.unstored_audio = unstored_audio_on(sql, &release.id)?;
    }
    Ok(rows)
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
            vec![SortTerm::running("listing.album_title COLLATE NOCASE", dir)]
        }
        StorageSortField::ArtistNames => vec![
            SortTerm::running("listing.credit_sort_names COLLATE NOCASE", dir).nullable(),
            SortTerm::running("listing.credit_artist_ids", dir).nullable(),
            SortTerm::ascending("listing.album_title COLLATE NOCASE"),
        ],
        // A release stating no medium goes last either way.
        StorageSortField::Media => vec![
            SortTerm::ascending("listing.media_empty"),
            SortTerm::running("listing.media_key", dir).nullable(),
            SortTerm::ascending("listing.album_title COLLATE NOCASE"),
        ],
        StorageSortField::FileCount => vec![
            SortTerm::running("listing.file_count", dir),
            SortTerm::ascending("listing.album_title COLLATE NOCASE"),
        ],
        StorageSortField::TotalSize => vec![
            SortTerm::running("listing.total_size", dir),
            SortTerm::ascending("listing.album_title COLLATE NOCASE"),
        ],
        StorageSortField::Storage => vec![
            SortTerm::running("listing.storage_order", dir),
            SortTerm::ascending("listing.album_title COLLATE NOCASE"),
        ],
        StorageSortField::DateAdded => vec![
            SortTerm::running("listing.created_at", dir),
            SortTerm::ascending("listing.album_title COLLATE NOCASE"),
        ],
    };
    if sort.field != StorageSortField::DateAdded {
        terms.push(SortTerm::ascending("listing.created_at"));
    }
    terms.push(SortTerm::ascending("listing.release_id"));
    ListOrder::new(terms)
}

/// What the Storage Manager shows, and its list reads: the list under one
/// sort, filter and search text, the windows of it on screen, and the
/// releases selected in it. The search narrows the list to the releases whose
/// album library search finds (§2.3); text that is no search (blank) narrows
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageBrowseRequest {
    pub sort: StorageSortCriterion,
    pub filter: StorageFilter,
    pub search: String,
    pub windows: bae_store::LibraryPageWindows,
    pub selected: BTreeSet<String>,
}

/// Each window a read names holds that window's rows of the list it was read
/// under, so a read for other windows still answers; one under another
/// sort, filter, search or selection reads another list.
impl bae_store::live_query::LiveRequest for StorageBrowseRequest {
    fn still_answers(&self, current: &Self) -> bool {
        self.sort == current.sort
            && self.filter == current.filter
            && self.search == current.search
            && self.selected == current.selected
    }
}

/// Every requested window of the Storage Manager list under the sort, filter
/// and search text it was read for, the filtered set's count and total size,
/// and the rows of the selected releases the filtered set holds, by release
/// id.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageBrowseProjection {
    pub sort: StorageSortCriterion,
    pub filter: StorageFilter,
    pub search: String,
    /// Whether the search text narrows the list: it is a library search,
    /// not blank.
    pub searching: bool,
    pub windows: Vec<bae_store::LibraryBrowseWindow<DbStorageRow>>,
    pub total_count: u64,
    pub total_size: u64,
    pub selected: Vec<DbStorageRow>,
    pub queues: crate::StorageQueues,
    /// Whether storage is set up on this device, as coven's sync status
    /// says.
    pub syncs: bool,
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "storage_audio_tests.rs"]
mod audio_tests;

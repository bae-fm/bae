use super::*;

/// One Storage Manager row as read: the release's half, the album's half,
/// where the release's files are, and when the release was added.
pub(super) type StorageRowParts = (
    DbReleaseSummary,
    Option<String>,
    AlbumSummaryRow,
    crate::album_detail::StoragePlace,
    DateTime<Utc>,
);

fn storage_row_parts(sql: &SqlReadContext<'_>, row: &Row<'_>) -> rusqlite::Result<StorageRowParts> {
    Ok((
        row_to_release_summary(sql, row)?,
        row.get("release_cover_version")?,
        AlbumSummaryRow::read(row)?,
        storage_place(row)?,
        rfc3339_column(row, "date_added")?,
    ))
}

/// Where the row's release's files are, from the listing's `storage_order`.
fn storage_place(row: &Row<'_>) -> rusqlite::Result<crate::album_detail::StoragePlace> {
    let column = row.as_ref().column_index("storage_order")?;
    match row.get::<_, i64>(column)? {
        0 => Ok(crate::album_detail::StoragePlace::Uploading),
        1 => Ok(crate::album_detail::StoragePlace::Pinning),
        2 => Ok(crate::album_detail::StoragePlace::Local),
        3 => Ok(crate::album_detail::StoragePlace::Pinned),
        4 => Ok(crate::album_detail::StoragePlace::Cloud),
        order => Err(rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Integer,
            format!("storage order {order} names no place").into(),
        )),
    }
}

/// What a Storage Manager filter keeps of `release_listing listing`, as a
/// condition.
fn storage_filter(filter: StorageFilter) -> &'static str {
    match filter {
        StorageFilter::All => "1",
        StorageFilter::Remote => "listing.stored = 1",
        StorageFilter::Local => "listing.stored = 0",
    }
}

/// `release_listing listing` reached by album alone: a statement reading
/// what a search finds looks up each album it finds there, and never walks
/// an order's index past the releases it doesn't find.
const FOUND_LISTING: &str = "release_listing listing INDEXED BY idx_release_listing_album";

/// Which releases a Storage Manager statement reads, and how it reaches
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StorageRows {
    /// Every release the filter keeps, through the order's index.
    Listed,
    /// The releases whose album the search pattern `:search` finds, each
    /// tested as the order's index reaches it.
    SearchedInOrder,
    /// The releases whose album the search pattern `:search` finds, read
    /// from what it finds and sorted.
    Found,
}

/// One Storage Manager row per release `rows` reads that the filter keeps
/// and `condition` passes: the release's half and its album's, the raw
/// aggregates the resolver maps to `ReleaseSummary` / `AlbumSummary`, whether
/// it is pinned, where it sorts by storage, when it was added, and `extra`
/// columns.
fn storage_row_select(
    filter: StorageFilter,
    rows: StorageRows,
    extra: &str,
    condition: &str,
) -> String {
    let album_columns = album_summary_columns();
    let filter = storage_filter(filter);
    let (listing, searched) = match rows {
        StorageRows::Listed => ("release_listing listing", "1".to_string()),
        // `+` keeps SQLite from reaching the listing by its album: the
        // order's index reaches it, so a window reads its rows in order.
        StorageRows::SearchedInOrder => (
            "release_listing listing",
            album_matches_sql("+listing.album_id", ":search"),
        ),
        StorageRows::Found => (
            FOUND_LISTING,
            album_matches_sql("listing.album_id", ":search"),
        ),
    };
    format!(
        "SELECT \
            r.id AS release_id, \
            r.album_id, \
            r.media AS release_media, \
            listing.stored AS remote, \
            listing.file_count, \
            listing.total_size, \
            {RELEASE_LISTING_PINNED} AS pinned, \
            listing.storage_order, \
            listing.created_at AS date_added, \
            {release_cover} AS release_cover_version, \
            {upload_columns}, \
            {album_columns}{extra} \
        FROM {listing} \
        JOIN releases r ON r.id = listing.release_id \
        JOIN albums a ON a.id = listing.album_id \
        LEFT JOIN ({releases_sending}) sending ON sending.release_id = listing.release_id \
        WHERE {filter} AND {searched} AND ({condition})",
        release_cover = super::files::cover_version_sql("r.id"),
        upload_columns = super::queued_uploads::RELEASE_UPLOAD_COLUMNS,
        releases_sending = super::queued_uploads::RELEASES_SENDING,
    )
}

/// The rows of the Storage Manager list under `filter` that `rows` reads and
/// `seek` passes, in its order, each with its key in `order`, from `:offset`
/// for at most `:limit` rows.
pub(super) fn storage_window_query(
    order: &ListOrder,
    filter: StorageFilter,
    rows: StorageRows,
    seek: &Seek,
) -> String {
    format!(
        "{} ORDER BY {} LIMIT :limit OFFSET :offset",
        storage_row_select(
            filter,
            rows,
            &format!(", {}", order.key_columns()),
            &order.condition(seek)
        ),
        order.ordering(seek)
    )
}

/// The rows of the release ids the JSON array `:selected` names that the
/// list under `filter` holds, and with `searching` the search pattern
/// `:search` finds, by release id. SQLite folds the outer id filter into the
/// list's rows, so each selected release is looked up by its key rather than
/// the list read whole.
pub(super) fn storage_selected_query(filter: StorageFilter, searching: bool) -> String {
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
/// pattern `:search` finds, and their files' total size, read from what it
/// finds.
pub(super) fn storage_found_totals_query(filter: StorageFilter) -> String {
    format!(
        "SELECT COUNT(*), COALESCE(SUM(listing.total_size), 0) \
         FROM {FOUND_LISTING} \
         WHERE {} AND {}",
        storage_filter(filter),
        album_matches_sql("listing.album_id", ":search"),
    )
}

/// What a row read from a search's matches costs against a row the order's
/// index passes and tests: it is reached through the search tables, joined
/// to its listing row and sorted with the rest, about three times as much.
const FOUND_ROW_COST: u128 = 3;

/// A Storage Manager search as one read of the list runs it: the pattern its
/// text matches, how many releases the filter keeps, and how many of those
/// the search finds, which decide where each statement reads its rows from.
pub(super) struct StorageSearch {
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
    pub(super) fn rows(&self, limit: u64, offset: u64) -> StorageRows {
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
pub(super) fn storage_totals_on(
    sql: &SqlReadContext<'_>,
    filter: StorageFilter,
    query: Option<&crate::library::LibrarySearchQuery>,
) -> Result<(u64, u64, Option<StorageSearch>), DbError> {
    let Some(query) = query else {
        return Ok((
            storage_count_on(sql, filter)?,
            storage_total_size_on(sql, filter)?,
            None,
        ));
    };
    let pattern = crate::library::search::search_pattern(query.as_str());
    let (found, size) = sql.query_row(
        &storage_found_totals_query(filter),
        rusqlite::named_params! { ":search": pattern },
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    )?;
    let found = found as u64;
    let search = StorageSearch {
        pattern,
        kept: storage_count_on(sql, filter)?,
        found,
    };
    Ok((found, size as u64, Some(search)))
}

/// The Storage Manager list's rows under `filter` and `search` that `seek`
/// reads in `order`, at most `limit` from `offset`, each with its key.
pub(super) fn storage_rows_on(
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
    Ok(seek.bound(limit, offset, |params| {
        let mut params = params.to_vec();
        if let Some(search) = search {
            params.push((":search", &search.pattern));
        }
        sql.query(&statement, params.as_slice(), |row| {
            Ok((storage_row_parts(sql, row)?, order.key_of(row)?))
        })
    })?)
}

/// How many releases of its album come before the release `?1` in the
/// album's order (`album_order`, then id): its index among them, counted on
/// the album's releases index.
pub(super) const RELEASE_INDEX_IN_ALBUM: &str = "SELECT COUNT(*) FROM releases AS shown \
     JOIN releases AS earlier ON earlier.album_id = shown.album_id \
      AND (earlier.album_order < shown.album_order \
           OR (earlier.album_order = shown.album_order AND earlier.id < shown.id)) \
     WHERE shown.id = ?1";

pub(super) fn find_release_detail_context_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<ReleaseContextRows>, DbError> {
    let Some(release) = find_release_by_id_on(sql, release_id)? else {
        return Ok(None);
    };
    let album_artists = get_artists_for_album_on(sql, &release.album_id)?;
    let release_index: i64 = sql.query_row(RELEASE_INDEX_IN_ALBUM, params![release.id], |row| {
        row.get(0)
    })?;
    let release_index = usize::try_from(release_index)
        .map_err(|error| DbError::Message(format!("release index {release_index}: {error}")))?;
    let is_compilation =
        find_album_by_id_on(sql, &release.album_id)?.is_some_and(|album| album.is_compilation);
    Ok(Some(ReleaseContextRows {
        detail: build_release_detail_on(sql, release)?,
        album_artists,
        release_index,
        is_compilation,
    }))
}

#[derive(Clone, PartialEq)]
pub(super) struct ReleaseContextRows {
    detail: ReleaseDetailRows,
    album_artists: Vec<LibraryCredit>,
    release_index: usize,
    is_compilation: bool,
}

impl ReleaseContextRows {
    pub(super) fn process(self) -> ReleaseDetailContext {
        ReleaseDetailContext {
            detail: self.detail.process(),
            album_artists: self.album_artists,
            release_index: self.release_index,
            is_compilation: self.is_compilation,
        }
    }
}

pub(super) fn process_storage_rows(
    rows: Vec<StorageRowParts>,
) -> Result<Vec<DbStorageRow>, DbError> {
    rows.into_iter()
        .map(
            |(release, release_cover_version, album, place, date_added)| {
                Ok(DbStorageRow {
                    release,
                    date_added,
                    release_cover_version,
                    album: album.process()?,
                    place,
                })
            },
        )
        .collect()
}

/// How many releases the Storage Manager list holds under `filter`.
pub(super) fn storage_count_on(
    sql: &SqlReadContext<'_>,
    filter: StorageFilter,
) -> Result<u64, DbError> {
    sql.query_row(&storage_count_query(filter), [], |row| row.get::<_, i64>(0))
        .map(|count| count as u64)
        .map_err(DbError::from)
}

/// The statement [`storage_count_on`] reads.
pub(super) fn storage_count_query(filter: StorageFilter) -> String {
    match filter {
        StorageFilter::All => {
            "SELECT COALESCE(SUM(release_count), 0) FROM release_listing_totals".to_string()
        }
        StorageFilter::Remote => {
            "SELECT COALESCE(SUM(release_count), 0) FROM release_listing_totals WHERE stored = 1"
                .to_string()
        }
        StorageFilter::Local => {
            "SELECT COALESCE(SUM(release_count), 0) FROM release_listing_totals WHERE stored = 0"
                .to_string()
        }
    }
}

/// The total size of the files of the releases the Storage Manager list
/// holds under `filter`.
pub(super) fn storage_total_size_on(
    sql: &SqlReadContext<'_>,
    filter: StorageFilter,
) -> Result<u64, DbError> {
    sql.query_row(&storage_total_size_query(filter), [], |row| {
        row.get::<_, i64>(0)
    })
    .map(|size| size as u64)
    .map_err(DbError::from)
}

/// The statement [`storage_total_size_on`] reads.
pub(super) fn storage_total_size_query(filter: StorageFilter) -> String {
    match filter {
        StorageFilter::All => {
            "SELECT COALESCE(SUM(total_size), 0) FROM release_listing_totals".to_string()
        }
        StorageFilter::Remote => {
            "SELECT COALESCE(SUM(total_size), 0) FROM release_listing_totals WHERE stored = 1"
                .to_string()
        }
        StorageFilter::Local => {
            "SELECT COALESCE(SUM(total_size), 0) FROM release_listing_totals WHERE stored = 0"
                .to_string()
        }
    }
}

/// The rows of the `release_ids` the list under `filter` and `search`
/// holds, wherever in it they sit, by release id.
pub(super) fn storage_selected_on(
    sql: &SqlReadContext<'_>,
    filter: StorageFilter,
    search: Option<&StorageSearch>,
    release_ids: &BTreeSet<String>,
) -> Result<Vec<StorageRowParts>, DbError> {
    if release_ids.is_empty() {
        return Ok(Vec::new());
    }
    let selected = serde_json::to_string(release_ids)
        .map_err(|error| DbError::Message(format!("selected release ids: {error}")))?;
    let query = storage_selected_query(filter, search.is_some());
    let mut params: Vec<(&str, &dyn rusqlite::ToSql)> = vec![(":selected", &selected)];
    if let Some(search) = search {
        params.push((":search", &search.pattern));
    }
    Ok(sql.query(&query, params.as_slice(), |row| storage_row_parts(sql, row))?)
}

/// What the Storage Manager list reads: the sort, filter and search text it
/// shows, the windows of that list it holds, and the releases selected in it.
/// Text that parses as no library search (blank) narrows nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageBrowseRequest {
    pub sort: StorageSortCriterion,
    pub filter: StorageFilter,
    pub search: String,
    pub windows: crate::library::LibraryPageWindows,
    pub selected: BTreeSet<String>,
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
    pub windows: Vec<crate::library::LibraryBrowseWindow<DbStorageRow>>,
    pub total_count: u64,
    pub total_size: u64,
    pub selected: Vec<DbStorageRow>,
    pub queues: crate::library::StorageQueues,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseDetailProjection {
    pub context: Option<ReleaseDetailContext>,
}

use super::*;

/// One Storage Manager row as read: the release's half and the album's half.
pub(super) type StorageRowParts = (DbReleaseSummary, AlbumSummaryRow);

fn storage_row_parts(sql: &SqlReadContext<'_>, row: &Row<'_>) -> rusqlite::Result<StorageRowParts> {
    Ok((
        row_to_release_summary(sql, row)?,
        AlbumSummaryRow::read(row)?,
    ))
}

pub(super) fn storage_page_on(
    sql: &SqlReadContext<'_>,
    query: &str,
    offset: u64,
    limit: u64,
) -> Result<Vec<StorageRowParts>, DbError> {
    sql.query(query, params![limit as i64, offset as i64], |row| {
        storage_row_parts(sql, row)
    })
    .map_err(DbError::from)
}

pub(super) fn find_release_detail_context_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<ReleaseContextRows>, DbError> {
    let Some(release) = find_release_by_id_on(sql, release_id)? else {
        return Ok(None);
    };
    let album_artists = get_artists_for_album_on(sql, &release.album_id)?;
    let releases = get_releases_for_album_on(sql, &release.album_id)?;
    let Some(release_index) = releases.iter().position(|row| row.id == release_id) else {
        return Ok(None);
    };
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
        .map(|(release, album)| {
            Ok(DbStorageRow {
                release,
                album: album.process()?,
            })
        })
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
        StorageFilter::Uploading => {
            format!("SELECT COUNT(*) FROM release_listing listing {UPLOADING_WHERE}")
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
        StorageFilter::Uploading => format!(
            "SELECT COALESCE(SUM(listing.total_size), 0) FROM release_listing listing \
             {UPLOADING_WHERE}"
        ),
    }
}

/// The rows of the `release_ids` the list under `filter` holds, wherever in
/// it they sit, by release id.
pub(super) fn storage_selected_on(
    sql: &SqlReadContext<'_>,
    filter: StorageFilter,
    release_ids: &BTreeSet<String>,
) -> Result<Vec<StorageRowParts>, DbError> {
    let release_ids = release_ids.iter().collect::<Vec<_>>();
    let mut rows = Vec::new();
    for chunk in release_ids.chunks(SQL_MAX_IN_VARS) {
        let query = storage_selected_query(filter, chunk.len());
        rows.extend(
            sql.query(&query, rusqlite::params_from_iter(chunk.iter()), |row| {
                storage_row_parts(sql, row)
            })
            .map_err(DbError::from)?,
        );
    }
    Ok(rows)
}

/// What the Storage Manager list reads: the sort and filter it shows, the
/// windows of that list it holds, and the releases selected in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageBrowseRequest {
    pub sort: StorageSortCriterion,
    pub filter: StorageFilter,
    pub windows: crate::library::LibraryPageWindows,
    pub selected: BTreeSet<String>,
}

/// Every requested window of the Storage Manager list under the sort and
/// filter it was read for, the filtered set's count and total size, and the
/// rows of the selected releases the filtered set holds, by release id.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageBrowseProjection {
    pub sort: StorageSortCriterion,
    pub filter: StorageFilter,
    pub windows: Vec<crate::library::LibraryBrowseWindow<DbStorageRow>>,
    pub total_count: u64,
    pub total_size: u64,
    pub selected: Vec<DbStorageRow>,
    pub cover_versions: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseDetailProjection {
    pub context: Option<ReleaseDetailContext>,
    pub cover_versions: HashMap<String, String>,
}

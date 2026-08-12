use super::*;

pub(super) fn storage_page_on(
    sql: &SqlReadContext<'_>,
    query: &str,
    uploading: &[String],
    offset: u64,
    limit: u64,
) -> Result<Vec<DbStorageRow>, DbError> {
    let mut binds = uploading
        .iter()
        .map(|id| Box::new(id.clone()) as Box<dyn coven::rusqlite::ToSql>)
        .collect::<Vec<_>>();
    binds.push(Box::new(limit as i64));
    binds.push(Box::new(offset as i64));
    sql.query(
        query,
        coven::rusqlite::params_from_iter(binds.iter()),
        |row| {
            let release = row_to_release_summary(row)?;
            Ok(parse_album_summary_row(row).map(|album| DbStorageRow { release, album }))
        },
    )?
    .into_iter()
    .collect()
}

pub(super) fn find_release_detail_context_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<ReleaseDetailContext>, DbError> {
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
    Ok(Some(ReleaseDetailContext {
        detail: build_release_detail_on(sql, release)?,
        album_artists,
        release_index,
        is_compilation,
    }))
}

pub(super) fn storage_count_on(
    sql: &SqlReadContext<'_>,
    where_clause: &str,
    uploading: &[String],
) -> Result<u64, DbError> {
    let query = format!("SELECT COUNT(*) FROM releases r {where_clause}");
    sql.query_row(
        &query,
        coven::rusqlite::params_from_iter(uploading.iter()),
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count as u64)
    .map_err(DbError::from)
}

pub(super) fn storage_total_size_on(
    sql: &SqlReadContext<'_>,
    where_clause: &str,
    uploading: &[String],
) -> Result<u64, DbError> {
    let query = format!(
        "SELECT COALESCE(SUM(rf.file_size), 0) \
         FROM releases r JOIN release_files rf ON rf.release_id = r.id \
         {where_clause}"
    );
    sql.query_row(
        &query,
        coven::rusqlite::params_from_iter(uploading.iter()),
        |row| row.get::<_, i64>(0),
    )
    .map(|size| size as u64)
    .map_err(DbError::from)
}

#[derive(Debug, Clone)]
pub struct StoragePageProjection {
    pub rows: Vec<DbStorageRow>,
    pub total_count: u64,
    pub total_size: u64,
    pub cover_versions: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ReleaseDetailProjection {
    pub context: Option<ReleaseDetailContext>,
    pub cover_versions: HashMap<String, String>,
}

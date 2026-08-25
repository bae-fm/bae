use super::*;

pub(crate) const SQL_MAX_IN_VARS: usize = 900;

pub(super) fn in_clause_placeholders(count: usize) -> String {
    (0..count).map(|_| "?").collect::<Vec<_>>().join(",")
}

/// One-row SELECT shared by coven's write [`SqlContext`] and its read
/// [`SqlReadContext`]. The release-path resolvers run their lookup under a write
/// transaction in production and on the read connection elsewhere; both expose
/// the same `query_row`, so the resolvers stay generic over this.
pub(super) trait QueryOne {
    fn query_row<T, P: Params, F: FnOnce(&Row<'_>) -> coven::rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> coven::rusqlite::Result<T>;
}

/// Multi-row SELECT shared by coven's read and write SQL contexts. Kept
/// separate from [`QueryOne`] because the release-path resolvers need only a
/// single-row lookup, while deletion planning needs both forms.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) trait QueryRows {
    fn query<T, P: Params, F: FnMut(&Row<'_>) -> coven::rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> coven::rusqlite::Result<Vec<T>>;
}

/// The resolvers' unit tests run them against a bare seeded connection, which is
/// the only place bae still holds one — coven owns every production connection.
#[cfg(test)]
impl QueryOne for Connection {
    fn query_row<T, P: Params, F: FnOnce(&Row<'_>) -> coven::rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> coven::rusqlite::Result<T> {
        Connection::query_row(self, sql, params, f)
    }
}

#[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
impl QueryRows for Connection {
    fn query<T, P: Params, F: FnMut(&Row<'_>) -> coven::rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> coven::rusqlite::Result<Vec<T>> {
        let mut statement = self.prepare(sql)?;
        let values = statement.query_map(params, f)?.collect();
        values
    }
}

impl QueryOne for SqlReadContext<'_> {
    fn query_row<T, P: Params, F: FnOnce(&Row<'_>) -> coven::rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> coven::rusqlite::Result<T> {
        SqlReadContext::query_row(self, sql, params, f)
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl QueryRows for SqlReadContext<'_> {
    fn query<T, P: Params, F: FnMut(&Row<'_>) -> coven::rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> coven::rusqlite::Result<Vec<T>> {
        SqlReadContext::query(self, sql, params, f)
    }
}

impl QueryOne for SqlContext<'_, '_> {
    fn query_row<T, P: Params, F: FnOnce(&Row<'_>) -> coven::rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> coven::rusqlite::Result<T> {
        SqlContext::query_row(self, sql, params, f)
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl QueryRows for SqlContext<'_, '_> {
    fn query<T, P: Params, F: FnMut(&Row<'_>) -> coven::rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> coven::rusqlite::Result<Vec<T>> {
        SqlContext::query(self, sql, params, f)
    }
}

/// The table a host-provided image blob's row lives in. The image type IS the
/// table (`covers` / `artist_images`), so there is no `type` column. A fixed
/// match over the enum, so the interpolated name is always a trusted literal.
pub(super) fn image_table(image_type: &LibraryImageType) -> &'static str {
    match image_type {
        LibraryImageType::Cover => "covers",
        LibraryImageType::Artist => "artist_images",
    }
}

pub(super) fn artist_image_cloud_path_for_storage(
    storage: crate::config::HomeStorage,
    artist_id: &str,
    blob_id: &str,
    content_type: &ContentType,
) -> Option<String> {
    storage
        .is_browsable()
        .then(|| resolve_artist_cloud_path(artist_id, blob_id, content_type))
}

/// What a delete owes coven's blob engine once its rows are gone.
///
/// Both halves are captured *before* the delete runs, because both name rows
/// that will not exist afterwards: a cloud tombstone is bound to the exact row
/// blob it removes, and an external-file registration is keyed by its row. The
/// transaction that drops the rows hands them over through
/// `apply_delete_cleanup_on`.
///
/// An in-flight make-remote is not represented here: cancelling one is
/// [`CovenHandle::cancel_make_remote`](coven::CovenHandle::cancel_make_remote),
/// which clears the intent, drops the pending uploads, and tombstones whatever
/// already reached the cloud — all of it coven's bookkeeping, none of it bae's.
#[derive(Clone, Debug, Default)]
pub struct DeleteCleanupPlan {
    /// Remote blobs whose cloud objects this delete must tombstone, as the exact
    /// row references captured while their rows still existed.
    pub blobs_to_tombstone: Vec<coven::RowBlobRef>,
    /// `(table, row_id)` pairs whose external-file registration this delete must
    /// drop — the user's own in-place files, which are never themselves deleted.
    pub external_refs_to_clear: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct ImportReplacementDelete {
    pub release_id: String,
    pub album_id: String,
    pub cleanup: DeleteCleanupPlan,
}

#[derive(Clone, Debug)]
pub struct ImportReplacementOutcome {
    pub release_id: String,
    pub album_id: String,
    pub album_deleted: bool,
}

/// Hand a delete's captured blob cleanup to coven, inside the same transaction
/// that removes the rows. See [`DeleteCleanupPlan`].
pub(super) fn apply_delete_cleanup_on(
    conn: &SqlContext<'_, '_>,
    cleanup: &DeleteCleanupPlan,
) -> Result<(), DbError> {
    for blob in &cleanup.blobs_to_tombstone {
        conn.enqueue_blob_delete(blob)?;
    }
    for (table, row_id) in &cleanup.external_refs_to_clear {
        conn.clear_external_blob(table, row_id)?;
    }
    Ok(())
}

/// After `removed_release_id` has left `album_id` inside the current transaction:
/// delete the album when no releases remain, otherwise NULL a
/// `primary_release_id` that pointed at the removed release (the user's
/// cover-release choice is gone with it, and read paths fall back to the album's
/// first release). Does not touch `imports` — delete flows clear
/// `imports.release_id` before the release row goes, and a moved release keeps
/// its import row.
///
/// Precondition: the release row must already be deleted or repointed away from
/// `album_id`, since this counts what remains. Returns true if the album was
/// deleted.
pub(super) fn cleanup_album_after_release_removal_on(
    conn: &SqlContext<'_, '_>,
    album_id: &str,
    removed_release_id: &str,
    reg: &str,
) -> Result<bool, DbError> {
    let remaining: i64 = conn.query_row(
        "SELECT COUNT(*) FROM releases WHERE album_id = ?",
        params![album_id],
        |row| row.get(0),
    )?;
    if remaining == 0 {
        conn.execute("DELETE FROM albums WHERE id = ?", params![album_id])?;
        return Ok(true);
    }
    conn.execute(
        "UPDATE albums SET primary_release_id = NULL, _updated_at = ? \
         WHERE id = ? AND primary_release_id = ?",
        params![reg, album_id, removed_release_id],
    )?;
    Ok(false)
}

pub(super) fn composer_summary_query(filter: Option<&str>, tail: Option<&str>) -> String {
    let release_unlinked = unlinked_release_composer_role_predicate("rar");
    let track_unlinked = unlinked_track_composer_role_predicate("tar");
    let mut query = format!(
        "SELECT composer.id AS artist_id,
                composer.name AS artist_name,
                composer.sort_name AS artist_sort_name,
                composer.discogs_artist_id AS artist_discogs_artist_id,
                composer.musicbrainz_artist_id AS artist_musicbrainz_artist_id,
                composer.created_at AS artist_created_at,
                COUNT(DISTINCT wa.work_id) AS work_count,
                COUNT(DISTINCT linked.id) AS linked_release_count,
                (
                    SELECT COUNT(*) FROM release_artist_roles rar
                    WHERE rar.artist_id = composer.id
                      AND {release_unlinked}
                ) + (
                    SELECT COUNT(*) FROM track_artist_roles tar
                    WHERE tar.artist_id = composer.id
                      AND {track_unlinked}
                ) AS unlinked_credit_count
         FROM artists composer
         LEFT JOIN work_artists wa ON wa.artist_id = composer.id
         LEFT JOIN track_works tw ON tw.work_id = wa.work_id
         LEFT JOIN tracks linked_track ON linked_track.id = tw.track_id
         LEFT JOIN releases linked ON linked.id = linked_track.release_id
         ",
    );
    if let Some(filter) = filter {
        query.push_str(filter);
    }
    query.push_str(
        "
         GROUP BY composer.id, composer.name, composer.sort_name, composer.discogs_artist_id, composer.musicbrainz_artist_id, composer.created_at
         HAVING work_count > 0 OR unlinked_credit_count > 0",
    );
    if let Some(tail) = tail {
        query.push('\n');
        query.push_str(tail);
    }
    query
}

/// The artist-browser summary query: every artist with at least one album link,
/// plus its distinct album count. Membership comes from album-artist links only —
/// the primary `albums.artist_id` FK unioned with `album_artists` rows;
/// track-level credits (`track_artists`) don't confer it. Various Artists gets no
/// special case: it is a real `artists` row and lists like any other artist.
pub(super) fn artist_summary_query(filter: Option<&str>, tail: Option<&str>) -> String {
    let mut query = String::from(
        "SELECT ar.id AS artist_id,
                ar.name AS artist_name,
                ar.sort_name AS artist_sort_name,
                ar.discogs_artist_id AS artist_discogs_artist_id,
                ar.musicbrainz_artist_id AS artist_musicbrainz_artist_id,
                ar.created_at AS artist_created_at,
                COUNT(DISTINCT link.album_id) AS album_count
         FROM artists ar
         JOIN (
             SELECT artist_id, id AS album_id FROM albums
             UNION
             SELECT artist_id, album_id FROM album_artists
         ) link ON link.artist_id = ar.id
         ",
    );
    if let Some(filter) = filter {
        query.push_str(filter);
    }
    query.push_str(
        "
         GROUP BY ar.id, ar.name, ar.sort_name, ar.discogs_artist_id, ar.musicbrainz_artist_id, ar.created_at",
    );
    if let Some(tail) = tail {
        query.push('\n');
        query.push_str(tail);
    }
    query
}

pub(super) fn unlinked_release_composer_role_predicate(role_alias: &str) -> String {
    unlinked_composer_role_predicate(
        role_alias,
        "release",
        "JOIN tracks t_unlinked_release ON t_unlinked_release.id = tw_unlinked_release.track_id",
        "t_unlinked_release.release_id",
        "release_id",
    )
}

pub(super) fn unlinked_track_composer_role_predicate(role_alias: &str) -> String {
    unlinked_composer_role_predicate(
        role_alias,
        "track",
        "",
        "tw_unlinked_track.track_id",
        "track_id",
    )
}

pub(super) fn unlinked_composer_role_predicate(
    role_alias: &str,
    scope: &str,
    track_join: &str,
    linked_target_column: &str,
    role_target_column: &str,
) -> String {
    format!(
        "NOT EXISTS (
            SELECT 1 FROM work_artists wa_unlinked_{scope}
            JOIN track_works tw_unlinked_{scope} ON tw_unlinked_{scope}.work_id = wa_unlinked_{scope}.work_id
            {track_join}
            WHERE wa_unlinked_{scope}.artist_id = {role_alias}.artist_id
              AND {linked_target_column} = {role_alias}.{role_target_column}
        )"
    )
}

pub(super) fn work_summary_query(filter: Option<&str>, tail: Option<&str>) -> String {
    let mut query = String::from(
        "SELECT w.id AS work_id,
                w.title AS work_title,
                w.disambiguation AS work_disambiguation,
                w.work_type AS work_type,
                w.musicbrainz_work_id AS work_musicbrainz_id,
                w.created_at AS work_created_at,
                (
                    SELECT wp.parent_work_id
                    FROM work_parts wp
                    WHERE wp.child_work_id = w.id
                    ORDER BY wp.position, wp.parent_work_id
                    LIMIT 1
                ) AS parent_work_id,
                (
                    SELECT tr.release_id
                    FROM track_works tw_cover
                    JOIN tracks tr ON tr.id = tw_cover.track_id
                    WHERE tw_cover.work_id = w.id
                    ORDER BY tr.side, tr.track_number, tr.release_id
                    LIMIT 1
                ) AS representative_release_id,
                (
                    SELECT GROUP_CONCAT(composer.name, ', ' ORDER BY wa.position)
                    FROM work_artists wa
                    JOIN artists composer ON composer.id = wa.artist_id
                    WHERE wa.work_id = w.id
                ) AS composer_names,
                COUNT(DISTINCT tr.release_id) AS linked_release_count
         FROM works w
         LEFT JOIN track_works tw ON tw.work_id = w.id
         LEFT JOIN tracks tr ON tr.id = tw.track_id
         ",
    );
    if let Some(filter) = filter {
        query.push_str(filter);
    }
    query.push_str(
        "
         GROUP BY w.id, w.title, w.disambiguation, w.work_type, w.musicbrainz_work_id, \
                  w.created_at",
    );
    if let Some(tail) = tail {
        query.push('\n');
        query.push_str(tail);
    }
    query
}

pub(super) fn row_to_work_summary(row: &Row<'_>) -> coven::rusqlite::Result<DbWorkSummary> {
    Ok(DbWorkSummary {
        work: DbWork {
            id: row.get("work_id")?,
            title: row.get("work_title")?,
            disambiguation: row.get("work_disambiguation")?,
            work_type: row.get("work_type")?,
            musicbrainz_work_id: row.get("work_musicbrainz_id")?,
            created_at: rfc3339_column(row, "work_created_at")?,
        },
        parent_work_id: row.get("parent_work_id")?,
        representative_release_id: row.get("representative_release_id")?,
        composer_names: row.get("composer_names")?,
        linked_release_count: row.get("linked_release_count")?,
    })
}

pub(super) fn row_to_composer_summary(row: &Row<'_>) -> coven::rusqlite::Result<DbComposerSummary> {
    Ok(DbComposerSummary {
        artist: row_to_joined_artist(row)?,
        work_count: row.get("work_count")?,
        linked_release_count: row.get("linked_release_count")?,
        unlinked_credit_count: row.get("unlinked_credit_count")?,
    })
}

pub(super) fn row_to_artist_summary(row: &Row<'_>) -> coven::rusqlite::Result<DbArtistSummary> {
    Ok(DbArtistSummary {
        artist: row_to_joined_artist(row)?,
        album_count: row.get("album_count")?,
    })
}

pub(super) fn work_summary_sort_key(work: &DbWorkSummary) -> String {
    work.work.title.to_lowercase()
}

pub(super) fn row_to_track_role_summary(
    row: &Row<'_>,
) -> coven::rusqlite::Result<DbTrackRoleSummary> {
    Ok(DbTrackRoleSummary {
        role: DbTrackArtistRole {
            id: row.get("track_artist_role_id")?,
            track_id: row.get("track_id")?,
            artist_id: row.get("artist_id")?,
            position: row.get("position")?,
            source: metadata_source_column(row, "role_source")?,
            source_credit: row.get("source_credit")?,
            created_at: rfc3339_column(row, "created_at")?,
        },
        track: row_to_joined_track(row)?,
        album: row_to_joined_album(row)?,
        artist: row_to_joined_artist(row)?,
    })
}

pub(super) fn work_release_rows(
    sql: &SqlReadContext<'_>,
    work_id: &str,
) -> Result<Vec<DbWorkReleaseSummary>, DbError> {
    sql.query(
        "SELECT DISTINCT
            r.id AS release_id,
            r.album_id,
            a.title AS album_title,
            r.release_name,
            r.year AS release_year,
            r.format AS release_format,
            (
                SELECT COUNT(*)
                FROM releases indexed_release
                WHERE indexed_release.album_id = r.album_id
                  AND (
                      indexed_release.created_at < r.created_at
                      OR (
                          indexed_release.created_at = r.created_at
                          AND indexed_release.id <= r.id
                      )
                  )
            ) AS release_index
         FROM track_works tw
         JOIN tracks t ON t.id = tw.track_id
         JOIN releases r ON r.id = t.release_id
         JOIN albums a ON a.id = r.album_id
         WHERE tw.work_id = ?
         ORDER BY a.title, r.created_at, r.id",
        params![work_id],
        row_to_work_release_summary,
    )
    .map_err(DbError::from)
}

pub(super) fn row_to_work_release_summary(
    row: &Row<'_>,
) -> coven::rusqlite::Result<DbWorkReleaseSummary> {
    Ok(DbWorkReleaseSummary {
        release_id: row.get("release_id")?,
        album_id: row.get("album_id")?,
        album_title: row.get("album_title")?,
        release_name: row.get("release_name")?,
        year: row.get("release_year")?,
        format: row.get("release_format")?,
        release_index: row.get("release_index")?,
    })
}

pub(super) fn row_to_release_summary(row: &Row<'_>) -> coven::rusqlite::Result<DbReleaseSummary> {
    Ok(DbReleaseSummary {
        id: row.get("release_id")?,
        album_id: row.get("album_id")?,
        format: row.get("release_format")?,
        remote: row.get("remote")?,
        any_file_id: row.get("any_file_id")?,
        file_count: row.get("file_count")?,
        total_size: row.get("total_size")?,
    })
}

/// ORDER BY for composer-page sorts. `composer.name ASC, composer.id ASC` is
/// always appended — pagination needs a total order and `composer.name` is not
/// unique — and is the whole clause when `sort` is empty. Mirrors
/// `build_order_by`'s `a.id` album tie-break.
pub(super) fn composer_order_by(sort: &[ComposerSortCriterion]) -> String {
    if sort.is_empty() {
        return "composer.name ASC, composer.id ASC".to_string();
    }
    let clause = sort
        .iter()
        .map(|c| {
            let field = match c.field {
                ComposerSortField::Name => "composer.name",
                ComposerSortField::WorkCount => "work_count",
                ComposerSortField::LinkedReleaseCount => "linked_release_count",
            };
            let direction = match c.direction {
                SortDirection::Ascending => "ASC",
                SortDirection::Descending => "DESC",
            };
            format!("{field} {direction}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{clause}, composer.name ASC, composer.id ASC")
}

/// ORDER BY for artist-page sorts. Same shape as `composer_order_by`, with
/// `ar.name ASC, ar.id ASC` as the always-appended pagination tie-break.
pub(super) fn artist_order_by(sort: &[ArtistSortCriterion]) -> String {
    if sort.is_empty() {
        return "ar.name ASC, ar.id ASC".to_string();
    }
    let clause = sort
        .iter()
        .map(|c| {
            let field = match c.field {
                ArtistSortField::Name => "ar.name",
                ArtistSortField::AlbumCount => "album_count",
            };
            let direction = match c.direction {
                SortDirection::Ascending => "ASC",
                SortDirection::Descending => "DESC",
            };
            format!("{field} {direction}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{clause}, ar.name ASC, ar.id ASC")
}

/// Build a `DbLibraryImage` from a `covers`/`artist_images` row. The table is the
/// type, so `image_type` is supplied by the caller rather than read from a column.
pub(super) fn row_to_library_image(
    row: &Row,
    image_type: LibraryImageType,
) -> coven::rusqlite::Result<DbLibraryImage> {
    Ok(DbLibraryImage {
        id: row.get("id")?,
        blob_id: row.get("blob_id")?,
        image_type,
        content_type: ContentType::from_mime(&row.get::<_, String>("content_type")?),
        file_size: row.get("file_size")?,
        width: row.get("width")?,
        height: row.get("height")?,
        source: row.get("source")?,
        source_url: row.get("source_url")?,
        cloud_path: row.get("cloud_path")?,
        content_hash: row.get("hash")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

/// Returns `(order_by_clause, needs_artist_join)`.
pub(super) fn build_order_by(sort: &[AlbumSortCriterion], default: &str) -> (String, bool) {
    // Every clause ends with `a.id`, a total order over the album PK. Without it
    // rows sharing a sort value order arbitrarily — and `get_album_page`
    // (LIMIT/OFFSET) and `get_album_index` (a ROW_NUMBER window) are different
    // query shapes, so SQLite could tie-break them differently and a reveal would
    // scroll to the wrong album.
    if sort.is_empty() {
        return (format!("{default}, a.id"), false);
    }
    let needs_artist_join = sort.iter().any(|c| c.field == AlbumSortField::Artist);
    let clause = sort
        .iter()
        .flat_map(|c| {
            let dir = match c.direction {
                SortDirection::Ascending => "ASC",
                SortDirection::Descending => "DESC",
            };
            match c.field {
                AlbumSortField::Title => {
                    vec![format!("a.title COLLATE NOCASE {dir}")]
                }
                AlbumSortField::Artist => {
                    vec![format!(
                        "COALESCE(art_sort.sort_name, art_sort.name) COLLATE NOCASE {dir}"
                    )]
                }
                AlbumSortField::Year => {
                    let nulls_order = match c.direction {
                        SortDirection::Ascending => "ASC",
                        SortDirection::Descending => "DESC",
                    };
                    vec![
                        format!("CASE WHEN a.year IS NULL THEN 1 ELSE 0 END {nulls_order}"),
                        format!("a.year {dir}"),
                    ]
                }
                AlbumSortField::DateAdded => {
                    vec![format!("a.created_at {dir}")]
                }
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    (format!("{clause}, a.id"), needs_artist_join)
}

/// ORDER BY for `get_storage_page`. Returns `(order_by_clause,
/// needs_artist_join)` — the artist join is needed only for artist-derived sorts.
pub(super) fn storage_order_by(sort: &StorageSortCriterion) -> (String, bool) {
    let dir = match sort.direction {
        SortDirection::Ascending => "ASC",
        SortDirection::Descending => "DESC",
    };
    // Every clause ends with `r.created_at, r.id`; the id keeps pagination
    // deterministic when two releases share a timestamp (bulk imports, tests).
    match sort.field {
        StorageSortField::AlbumTitle => (
            format!("a.title COLLATE NOCASE {dir}, r.created_at, r.id"),
            false,
        ),
        StorageSortField::ArtistNames => (
            format!(
                "COALESCE(art_sort.sort_name, art_sort.name) COLLATE NOCASE {dir}, \
                 a.title COLLATE NOCASE, r.created_at, r.id"
            ),
            true,
        ),
        StorageSortField::Format => (
            format!(
                "CASE WHEN r.format IS NULL THEN 1 ELSE 0 END {dir}, \
                 r.format COLLATE NOCASE {dir}, \
                 a.title COLLATE NOCASE, r.created_at, r.id"
            ),
            false,
        ),
        StorageSortField::FileCount => (
            format!("file_count {dir}, a.title COLLATE NOCASE, r.created_at, r.id"),
            false,
        ),
        StorageSortField::TotalSize => (
            format!("total_size {dir}, a.title COLLATE NOCASE, r.created_at, r.id"),
            false,
        ),
    }
}

/// Paginated storage-page query. Joins releases × albums × (optional)
/// primary-artist sort table; both halves of the returned row are the
/// raw aggregates the resolver maps to `ReleaseSummary` / `AlbumSummary`.
pub(super) fn storage_page_query(
    order_by: &str,
    artist_sort_join: &str,
    where_clause: &str,
    uploading_count: usize,
) -> String {
    let album_columns = album_summary_columns();
    let (queue_cte, queue_join) = if uploading_count == 0 {
        (String::new(), String::new())
    } else {
        let values = (0..uploading_count)
            .map(|position| format!("(?, {position})"))
            .collect::<Vec<_>>()
            .join(", ");
        (
            format!("WITH upload_queue(release_id, position) AS (VALUES {values}) "),
            "JOIN upload_queue ON upload_queue.release_id = r.id".to_string(),
        )
    };
    format!(
        "{queue_cte}SELECT \
            r.id AS release_id, \
            r.album_id, \
            r.format AS release_format, \
            r.remote, \
            (SELECT rf.id FROM release_files rf WHERE rf.release_id = r.id LIMIT 1) AS any_file_id, \
            COALESCE(( \
                SELECT COUNT(*) FROM release_files rf WHERE rf.release_id = r.id \
            ), 0) AS file_count, \
            COALESCE(( \
                SELECT SUM(rf.file_size) FROM release_files rf WHERE rf.release_id = r.id \
            ), 0) AS total_size, \
            {album_columns} \
        FROM releases r \
        JOIN albums a ON a.id = r.album_id \
        {artist_sort_join} \
        {queue_join} \
        {where_clause} \
        ORDER BY {order_by} \
        LIMIT ? OFFSET ?"
    )
}

/// Build a WHERE clause fragment for storage-page filtering. Returns the
/// empty string for `StorageFilter::All`.
/// The WHERE clause for a storage-page filter.
///
/// `Uploading` is the odd one: which releases are mid-upload is coven's queue
/// state, not a bae column, so the caller asks coven for those release ids first
/// and this renders a bound `IN` list of `uploading_count` placeholders. An empty
/// set is `WHERE 0` — nothing is uploading, so the page is empty — rather than an
/// `IN ()`, which SQLite rejects.
pub(super) fn storage_filter_where(filter: StorageFilter, uploading_count: usize) -> String {
    match filter {
        StorageFilter::All => String::new(),
        StorageFilter::Remote => "WHERE r.remote = 1".to_string(),
        StorageFilter::Local => "WHERE r.remote = 0".to_string(),
        StorageFilter::Uploading if uploading_count == 0 => "WHERE 0".to_string(),
        StorageFilter::Uploading => {
            format!(
                "WHERE r.id IN ({})",
                in_clause_placeholders(uploading_count)
            )
        }
    }
}

pub(super) fn album_artist_names_sql() -> &'static str {
    "(SELECT CASE \
        WHEN primary_name = '' THEN extra_names \
        WHEN extra_names = '' THEN primary_name \
        ELSE primary_name || ', ' || extra_names \
    END \
    FROM ( \
        SELECT \
            COALESCE(( \
                SELECT art_primary.name \
                FROM artists art_primary \
                WHERE art_primary.id = a.artist_id \
            ), '') AS primary_name, \
            COALESCE(( \
                SELECT GROUP_CONCAT(ar.name, ', ' ORDER BY aa.position) \
                FROM album_artists aa \
                JOIN artists ar ON ar.id = aa.artist_id \
                WHERE aa.album_id = a.id \
            ), '') AS extra_names \
    ))"
}

pub(super) fn album_release_ids_json_sql() -> &'static str {
    "COALESCE(( \
        SELECT json_group_array(album_release.id ORDER BY album_release.created_at, album_release.id) \
        FROM releases album_release \
        WHERE album_release.album_id = a.id \
    ), '[]')"
}

/// Shared column list for album-summary queries. Emits `artist_names`
/// (primary artist + album_artists, comma-joined) and `release_ids_json`
/// (releases in created_at order).
pub(super) fn album_summary_columns() -> String {
    format!(
        "a.id, a.title, a.year, a.is_compilation, a.primary_release_id, \
            {artist_names} AS artist_names, \
            {release_ids} AS release_ids_json",
        artist_names = album_artist_names_sql(),
        release_ids = album_release_ids_json_sql()
    )
}

/// Shared SELECT list for album-summary queries. Callers append
/// `FROM albums a`, any `art_sort` join (see `album_summary_artist_join`),
/// and their own `ORDER BY` / `WHERE` / `LIMIT`.
pub(super) fn album_summary_select() -> String {
    format!("SELECT {}", album_summary_columns())
}

/// The `art_sort` join clause for album-summary queries that sort by an
/// artist-derived column; empty otherwise.
pub(super) fn album_summary_artist_join(needs_artist_join: bool) -> &'static str {
    if needs_artist_join {
        "JOIN artists art_sort ON a.artist_id = art_sort.id"
    } else {
        ""
    }
}

/// Parse one album summary row (page queries and per-album lookups alike).
/// Requires a `release_ids_json` column on the row.
pub(super) fn parse_album_summary_row(row: &Row) -> Result<DbAlbumSummary, DbError> {
    let release_ids_json: String = row.get("release_ids_json")?;
    let release_ids: Vec<String> =
        serde_json::from_str(&release_ids_json).map_err(|e| DbError::Message(e.to_string()))?;

    Ok(DbAlbumSummary {
        id: row.get("id")?,
        title: row.get("title")?,
        year: row.get("year")?,
        is_compilation: row.get("is_compilation")?,
        artist_names: row.get("artist_names")?,
        release_ids,
        primary_release_id: row.get("primary_release_id")?,
    })
}

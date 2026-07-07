use crate::db::models::*;
use crate::import::MetadataSource;
use crate::playback::QueueEntry;
use crate::queue::QueueItem;
use crate::util::content_type::ContentType;
use chrono::{DateTime, Utc};
use coven::rusqlite::{params, Connection, OptionalExtension, Params, Row};
use coven::{ClockRef, Coven, CovenError, CovenHandle, DbError, SqlContext};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tracing::warn;

mod album;
mod artist;
mod blobs;
mod identity;
mod playback;
mod release;
mod track;

#[cfg(test)]
mod tests;

pub(crate) const SQL_MAX_IN_VARS: usize = 900;

fn in_clause_placeholders(count: usize) -> String {
    (0..count).map(|_| "?").collect::<Vec<_>>().join(",")
}

/// The table a host-provided image blob's row lives in. The image type IS the
/// table (`covers` / `artist_images`), so there is no `type` column. A fixed
/// match over the enum, so the interpolated name is always a trusted literal.
fn image_table(image_type: &LibraryImageType) -> &'static str {
    match image_type {
        LibraryImageType::Cover => "covers",
        LibraryImageType::Artist => "artist_images",
    }
}

fn artist_image_cloud_path_for_storage(
    storage: crate::config::HomeStorage,
    artist_id: &str,
    content_type: &ContentType,
) -> Option<String> {
    storage
        .is_browsable()
        .then(|| resolve_artist_cloud_path(artist_id, content_type))
}

fn register_external_blob_on(
    conn: &Connection,
    blob_id: &str,
    namespace: &str,
    path: &Path,
    size: u64,
) -> Result<(), DbError> {
    let path = path.to_str().ok_or_else(|| {
        DbError(format!(
            "external blob path for {blob_id} is not valid UTF-8: {path:?}"
        ))
    })?;
    conn.execute(
        "INSERT INTO local_blob_refs (blob_id, namespace, path, size) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(blob_id) DO UPDATE SET \
             namespace = excluded.namespace, \
             path = excluded.path, \
             size = excluded.size",
        (blob_id, namespace, path, size as i64),
    )
    .map(|_| ())
    .map_err(DbError::from)
}

fn clear_external_blob_on(conn: &Connection, blob_id: &str) -> Result<(), DbError> {
    conn.execute("DELETE FROM local_blob_refs WHERE blob_id = ?1", [blob_id])
        .map(|_| ())
        .map_err(DbError::from)
}

#[derive(Clone, Debug)]
pub struct InFlightMakeRemoteBlobCleanup {
    pub blob_id: String,
    pub cloud_key: String,
}

#[derive(Clone, Debug)]
pub struct DeleteCleanupPlan {
    pub cloud_delete_keys: Vec<String>,
    pub in_flight_make_remote_blobs: Vec<InFlightMakeRemoteBlobCleanup>,
    pub external_blob_ids_to_clear: Vec<String>,
    pub make_remote_release_ids_to_clear: Vec<String>,
}

/// A host-provided image blob (a cover keyed by release id, or an artist image
/// keyed by artist id) that a failed-import rollback orphaned by deleting its
/// row. The DB transaction drops the row but cannot reach coven's on-device
/// blob store; the manager evicts these after the transaction commits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrphanedImageBlob {
    pub namespace: &'static str,
    pub id: String,
    pub cloud_path: Option<String>,
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

fn add_cloud_outbox_delete_on(
    conn: &Connection,
    cloud_key: &str,
    created_at: &str,
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM cloud_outbox \
         WHERE operation IN ('upload', 'cancel') AND cloud_key = ?1",
        [cloud_key],
    )
    .map_err(DbError::from)?;
    conn.execute(
        "INSERT OR IGNORE INTO cloud_outbox \
         (operation, cloud_key, scope, created_at) \
         VALUES ('delete', ?1, NULL, ?2)",
        (cloud_key, created_at),
    )
    .map(|_| ())
    .map_err(DbError::from)
}

fn apply_delete_cleanup_on(
    conn: &Connection,
    cleanup: &DeleteCleanupPlan,
    created_at: &str,
) -> Result<(), DbError> {
    for cloud_key in &cleanup.cloud_delete_keys {
        add_cloud_outbox_delete_on(conn, cloud_key, created_at)?;
    }
    for blob in &cleanup.in_flight_make_remote_blobs {
        let still_pending = conn
            .query_row(
                "SELECT 1 FROM cloud_outbox WHERE operation = 'upload' AND file_id = ?1",
                [&blob.blob_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(DbError::from)?
            .is_some();
        if still_pending {
            conn.execute(
                "DELETE FROM cloud_outbox WHERE operation = 'upload' AND file_id = ?1",
                [&blob.blob_id],
            )
            .map_err(DbError::from)?;
        } else {
            add_cloud_outbox_delete_on(conn, &blob.cloud_key, created_at)?;
        }
    }
    for blob_id in &cleanup.external_blob_ids_to_clear {
        clear_external_blob_on(conn, blob_id)?;
    }
    for release_id in &cleanup.make_remote_release_ids_to_clear {
        conn.execute(
            "DELETE FROM blob_make_remote_intents WHERE root_table = 'releases' AND root_id = ?1",
            [release_id],
        )
        .map_err(DbError::from)?;
    }
    Ok(())
}

/// After `removed_release_id` has left `album_id` inside the current
/// transaction (its row deleted, or its `album_id` repointed): delete the
/// album when it has no releases left, otherwise clear a
/// `primary_release_id` that pointed at the removed release.
/// `primary_release_id` is the user's cover-release choice; when the chosen
/// release leaves, the choice is gone (NULL) and read paths fall back to the
/// album's first release. Does not touch `imports` — delete flows clear
/// `imports.release_id` before the release row goes, and a moved release
/// keeps its import row.
/// Precondition: the release row must already be gone from / repointed away
/// from `album_id` when this runs — it counts what remains.
/// Returns true when the album row was deleted.
fn cleanup_album_after_release_removal_on(
    conn: &Connection,
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

fn composer_summary_query(filter: Option<&str>, tail: Option<&str>) -> String {
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

fn unlinked_release_composer_role_predicate(role_alias: &str) -> String {
    unlinked_composer_role_predicate(
        role_alias,
        "release",
        "JOIN tracks t_unlinked_release ON t_unlinked_release.id = tw_unlinked_release.track_id",
        "t_unlinked_release.release_id",
        "release_id",
    )
}

fn unlinked_track_composer_role_predicate(role_alias: &str) -> String {
    unlinked_composer_role_predicate(
        role_alias,
        "track",
        "",
        "tw_unlinked_track.track_id",
        "track_id",
    )
}

fn unlinked_composer_role_predicate(
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

fn work_summary_query(filter: Option<&str>, tail: Option<&str>) -> String {
    let mut query = String::from(
        "SELECT w.id AS work_id,
                w.title AS work_title,
                w.disambiguation AS work_disambiguation,
                w.work_type AS work_type,
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
         GROUP BY w.id, w.title, w.disambiguation, w.work_type, w.created_at",
    );
    if let Some(tail) = tail {
        query.push('\n');
        query.push_str(tail);
    }
    query
}

fn row_to_work_summary(row: &Row<'_>) -> coven::rusqlite::Result<DbWorkSummary> {
    Ok(DbWorkSummary {
        work: DbWork {
            id: row.get("work_id")?,
            title: row.get("work_title")?,
            disambiguation: row.get("work_disambiguation")?,
            work_type: row.get("work_type")?,
            created_at: rfc3339_column(row, "work_created_at")?,
        },
        parent_work_id: row.get("parent_work_id")?,
        representative_release_id: row.get("representative_release_id")?,
        composer_names: row.get("composer_names")?,
        linked_release_count: row.get("linked_release_count")?,
    })
}

fn row_to_composer_summary(row: &Row<'_>) -> coven::rusqlite::Result<DbComposerSummary> {
    Ok(DbComposerSummary {
        artist: row_to_joined_artist(row)?,
        work_count: row.get("work_count")?,
        linked_release_count: row.get("linked_release_count")?,
        unlinked_credit_count: row.get("unlinked_credit_count")?,
    })
}

fn work_summary_sort_key(work: &DbWorkSummary) -> String {
    work.work.title.to_lowercase()
}

fn row_to_track_role_summary(row: &Row<'_>) -> coven::rusqlite::Result<DbTrackRoleSummary> {
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

fn work_release_rows(
    conn: &Connection,
    work_id: &str,
) -> Result<Vec<DbWorkReleaseSummary>, DbError> {
    let mut stmt = conn.prepare(
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
    )?;
    let rows = stmt.query_map(params![work_id], row_to_work_release_summary)?;
    rows.collect::<coven::rusqlite::Result<Vec<_>>>()
        .map_err(DbError::from)
}

fn row_to_work_release_summary(row: &Row<'_>) -> coven::rusqlite::Result<DbWorkReleaseSummary> {
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

fn row_to_release_summary(row: &Row<'_>) -> coven::rusqlite::Result<DbReleaseSummary> {
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

fn composer_order_by(sort: ComposerSortCriterion) -> String {
    let field = match sort.field {
        ComposerSortField::Name => "composer.name",
        ComposerSortField::WorkCount => "work_count",
        ComposerSortField::LinkedReleaseCount => "linked_release_count",
    };
    let direction = match sort.direction {
        SortDirection::Ascending => "ASC",
        SortDirection::Descending => "DESC",
    };
    // Pagination needs a total order; composer.name is not unique.
    format!("{field} {direction}, composer.name ASC, composer.id ASC")
}

#[cfg(any(test, feature = "test-utils"))]
fn row_to_outbox_entry(row: &Row<'_>) -> coven::rusqlite::Result<coven::OutboxEntry> {
    let op_tag: String = row.get(1)?;
    let operation = match op_tag.as_str() {
        "upload" => {
            let scope_str: String = row.get(5)?;
            let scope = coven::BlobScope::from_outbox_str(&scope_str).ok_or_else(|| {
                coven::rusqlite::Error::FromSqlConversionFailure(
                    5,
                    coven::rusqlite::types::Type::Text,
                    format!("invalid cloud_outbox.scope: {scope_str:?}").into(),
                )
            })?;
            coven::OutboxOperation::Upload {
                file_id: row.get(2)?,
                source_path: row.get(4)?,
                scope,
                retain_pinned: row.get(6)?,
            }
        }
        "delete" => coven::OutboxOperation::Delete,
        "cancel" => coven::OutboxOperation::Cancel,
        other => {
            return Err(coven::rusqlite::Error::FromSqlConversionFailure(
                1,
                coven::rusqlite::types::Type::Text,
                format!("invalid cloud_outbox.operation: {other:?}").into(),
            ));
        }
    };
    Ok(coven::OutboxEntry {
        id: row.get(0)?,
        cloud_key: row.get(3)?,
        attempt_count: row.get(7)?,
        last_attempt_at: row.get(8)?,
        operation,
    })
}

/// Build a `DbLibraryImage` from a `covers`/`artist_images` row. The table is the
/// type, so `image_type` is supplied by the caller rather than read from a column.
fn row_to_library_image(
    row: &Row,
    image_type: LibraryImageType,
) -> coven::rusqlite::Result<DbLibraryImage> {
    Ok(DbLibraryImage {
        id: row.get("id")?,
        image_type,
        content_type: ContentType::from_mime(&row.get::<_, String>("content_type")?),
        file_size: row.get("file_size")?,
        width: row.get("width")?,
        height: row.get("height")?,
        source: row.get("source")?,
        source_url: row.get("source_url")?,
        cloud_path: row.get("cloud_path")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

/// Build an ORDER BY clause from sort criteria.
/// Returns `(order_by_clause, needs_artist_join)`.
fn build_order_by(sort: &[AlbumSortCriterion], default: &str) -> (String, bool) {
    // Every clause ends with `a.id` — a total-order tiebreaker over the album
    // primary key. Without it, rows sharing a sort value (same title, same
    // year, same created_at from a bulk import) order arbitrarily, and the
    // ambiguity is worse than cosmetic: `get_album_page` (LIMIT/OFFSET) and
    // `get_album_index` (a ROW_NUMBER window) are different query shapes, so
    // SQLite could tie-break them differently and a reveal would scroll to the
    // wrong album. The id tiebreaker makes both deterministic and mutually
    // consistent.
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

/// Build an ORDER BY clause for `get_storage_page`. Returns
/// `(order_by_clause, needs_artist_join)` — the artist join is needed
/// only when sorting by artist-derived columns.
fn storage_order_by(sort: &StorageSortCriterion) -> (String, bool) {
    let dir = match sort.direction {
        SortDirection::Ascending => "ASC",
        SortDirection::Descending => "DESC",
    };
    // Every ORDER BY ends with `r.created_at, r.id` — the id tiebreaker
    // keeps pagination deterministic when two releases share a timestamp
    // (bulk imports, test fixtures, same-millisecond inserts).
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

/// Build a WHERE clause fragment for storage-page filtering. Returns the
/// empty string for `StorageFilter::All`.
fn storage_filter_where(filter: StorageFilter) -> &'static str {
    match filter {
        StorageFilter::All => "",
        StorageFilter::Remote => "WHERE r.remote = 1",
        StorageFilter::Local => "WHERE r.remote = 0",
        StorageFilter::Uploading => {
            "WHERE EXISTS ( \
            SELECT 1 FROM cloud_outbox co \
            JOIN release_files rf ON rf.id = co.file_id \
            WHERE rf.release_id = r.id AND co.operation = 'upload' \
        )"
        }
    }
}

fn album_artist_names_sql() -> &'static str {
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

fn album_release_ids_json_sql() -> &'static str {
    "COALESCE(( \
        SELECT json_group_array(album_release.id ORDER BY album_release.created_at, album_release.id) \
        FROM releases album_release \
        WHERE album_release.album_id = a.id \
    ), '[]')"
}

/// Shared column list for album-summary queries. Emits `artist_names`
/// (primary artist + album_artists, comma-joined) and `release_ids_json`
/// (releases in created_at order).
fn album_summary_columns() -> String {
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
fn album_summary_select() -> String {
    format!("SELECT {}", album_summary_columns())
}

/// The `art_sort` join clause for album-summary queries that sort by an
/// artist-derived column; empty otherwise.
fn album_summary_artist_join(needs_artist_join: bool) -> &'static str {
    if needs_artist_join {
        "JOIN artists art_sort ON a.artist_id = art_sort.id"
    } else {
        ""
    }
}

/// Parse a single album summary row. Shared between page queries and
/// per-album lookups (e.g. `find_album_summary`).
/// Requires a `release_ids_json` column on the row.
fn parse_album_summary_row(row: &Row) -> Result<DbAlbumSummary, DbError> {
    let release_ids_json: String = row.get("release_ids_json")?;
    let release_ids: Vec<String> =
        serde_json::from_str(&release_ids_json).map_err(|e| DbError(e.to_string()))?;

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

struct DatabaseInner {
    /// The top-level coven handle owns the connection and exposes the host SQL
    /// path. Writes to synced tables are captured by coven's attached session.
    handle: CovenHandle,
    /// Wall clock for `created_at` and status timestamps bound into write SQL.
    /// Synced-table `_updated_at` is stamped from coven's SQL context.
    clock: ClockRef,
}

/// An external user-owned file a blob id resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalBlob {
    pub path: PathBuf,
    pub size: u64,
}

/// Database client over coven's owned connection.
///
/// All reads and writes run through [`CovenHandle::sql`]. Writes to synced
/// tables are captured by coven's attached session for changeset sync; reads
/// share the same serialized connection.
///
/// coven also owns connection pragmas such as `foreign_keys` and `journal_mode`.
/// bae never opens a production SQLite connection or sets a production
/// connection pragma; it inherits those guarantees from the coven handle.
#[derive(Clone)]
pub struct Database {
    inner: Arc<DatabaseInner>,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
    }
}

/// Outcome of `set_identity_atomic`. Tells the caller which event to
/// emit for the source album: `AlbumRemoved` when `source_album_deleted`
/// is true, `AlbumUpdated` otherwise (when the release moved but other
/// releases stayed). Same-album updates leave the field meaningless and
/// the caller should skip the source-album event.
#[derive(Debug, Clone, Copy)]
pub struct SetIdentityOutcome {
    pub source_album_deleted: bool,
}

impl Database {
    // ── Database Method Conventions ───────────────────────────────────────
    //
    // Lookup methods follow two patterns. Using the wrong one is a bug.
    // The deciding question: **where did the ID come from?**
    //
    // 1. `find_*` — The ID came from the caller (a UI event, an API
    //    parameter, a user-provided string). The row may not exist — the
    //    user could be looking at something that was since deleted.
    //    Returns `Result<Option<T>>`.
    //
    // 2. `get_*_for_*` — You're following a foreign key from a record you
    //    already have from the DB. The row MUST exist — if it doesn't,
    //    our data integrity is broken. Returns `Result<T>` (NOT Option).
    //    Takes the parent record as its argument, not a raw ID string,
    //    so you can't accidentally call it with a caller-provided ID.
    //    A missing row surfaces as `QueryReturnedNoRows`.
    //
    // When adding a new method:
    // - ID from a function parameter, URL, UI event → find_*
    // - ID from a field on a DB record you already hold → get_*_for_*

    /// The one coven handle backing bae's SQL, blob, and sync operations.
    pub fn handle(&self) -> &CovenHandle {
        &self.inner.handle
    }

    fn coven_error(error: CovenError) -> DbError {
        match error {
            CovenError::Database(error) => error,
            other => DbError(other.to_string()),
        }
    }

    async fn call<R>(
        &self,
        f: impl FnOnce(&Connection) -> Result<R, DbError> + Send + 'static,
    ) -> Result<R, DbError>
    where
        R: Send + 'static,
    {
        self.call_sql(move |sql| f(sql.tx())).await
    }

    async fn call_sql<R>(
        &self,
        f: impl for<'ctx, 'conn> FnOnce(SqlContext<'ctx, 'conn>) -> Result<R, DbError> + Send + 'static,
    ) -> Result<R, DbError>
    where
        R: Send + 'static,
    {
        self.inner
            .handle
            .sql(move |sql| f(sql).map_err(CovenError::from))
            .await
            .map_err(Self::coven_error)
    }

    pub fn from_handle(handle: CovenHandle, clock: ClockRef) -> Self {
        Database {
            inner: Arc::new(DatabaseInner { handle, clock }),
        }
    }

    /// Open the database through coven's top-level builder, running coven's
    /// bookkeeping migration plus bae's schema.
    pub fn open(
        config: impl Into<coven::CovenConfig>,
        clock: ClockRef,
        key_service: coven::KeyService,
        synced_tables: Vec<coven::SyncedTable>,
        observer: Option<Arc<dyn coven::BlobTransitionObserver>>,
    ) -> Result<Self, DbError> {
        let mut builder = Coven::builder(config)
            .synced_tables(synced_tables)
            .clock(clock.clone())
            .key_service(key_service);
        if let Some(observer) = observer {
            builder = builder.observer(observer);
        }
        let handle = builder
            .migrations(crate::migrations::all())
            .open()
            .map_err(Self::coven_error)?;
        Ok(Self::from_handle(handle, clock))
    }

    /// Test convenience: open over `path` with a fresh device id and bae's real
    /// synced-table set, so unit/integration tests don't repeat the wiring.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_test(database_path: &str, clock: ClockRef) -> Result<Self, DbError> {
        tracing::info!("Opening database at {}", database_path);
        let path = Path::new(database_path);
        let library_root = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| DbError(format!("database path has no parent: {database_path}")))?;
        let library_dir = coven::LibraryDir::new(library_root);
        let config = coven::Config::with_defaults(
            "test-library".to_string(),
            "test-device".to_string(),
            library_dir,
            "Test Library".to_string(),
        );
        let key_service = coven::KeyService::new(config.library_id.clone());
        Self::open(
            config,
            clock,
            key_service,
            crate::sync::synced_tables(),
            None,
        )
    }
}

fn find_album_by_id_on(conn: &Connection, album_id: &str) -> Result<Option<DbAlbum>, DbError> {
    conn.query_row(
        r#"
            SELECT
                id, title, artist_id, year, primary_release_id,
                is_compilation,
                created_at
            FROM albums
            WHERE id = ?
            "#,
        params![album_id],
        row_to_album,
    )
    .optional()
    .map_err(DbError::from)
}

fn get_artists_for_album_on(conn: &Connection, album_id: &str) -> Result<Vec<DbArtist>, DbError> {
    // Primary artist from FK (sort_key = -1 so it's first), then additional
    // artists from the junction table ordered by position.
    let mut stmt = conn.prepare(
        r#"
            SELECT a.*, -1 AS sort_key FROM artists a
            JOIN albums alb ON alb.artist_id = a.id
            WHERE alb.id = ?
            UNION ALL
            SELECT a.*, aa.position AS sort_key FROM artists a
            JOIN album_artists aa ON a.id = aa.artist_id
            WHERE aa.album_id = ?
            ORDER BY sort_key
            "#,
    )?;
    let rows = stmt.query_map(params![album_id, album_id], row_to_artist)?;
    rows.collect::<coven::rusqlite::Result<Vec<_>>>()
        .map_err(DbError::from)
}

fn find_release_by_id_on(
    conn: &Connection,
    release_id: &str,
) -> Result<Option<DbRelease>, DbError> {
    conn.query_row(
        "SELECT * FROM releases WHERE id = ?",
        params![release_id],
        row_to_release,
    )
    .optional()
    .map_err(DbError::from)
}

fn get_releases_for_album_on(conn: &Connection, album_id: &str) -> Result<Vec<DbRelease>, DbError> {
    let mut stmt = conn.prepare("SELECT * FROM releases WHERE album_id = ? ORDER BY created_at")?;
    let rows = stmt.query_map(params![album_id], row_to_release)?;
    rows.collect::<coven::rusqlite::Result<Vec<_>>>()
        .map_err(DbError::from)
}

fn build_release_detail_on(
    conn: &Connection,
    release: DbRelease,
) -> Result<DbReleaseDetail, DbError> {
    let tracks = get_tracks_with_artists_for_release_on(conn, &release.id)?;
    let files = get_files_for_release_on(conn, &release.id)?;
    let audio_formats = get_audio_formats_for_release_on(conn, &release.id)?;
    let audio_segments = get_audio_segments_for_release_on(conn, &release.id)?;
    let identities = get_release_identities_on(conn, &release.id)?;

    Ok(DbReleaseDetail {
        release,
        tracks,
        files,
        audio_formats,
        audio_segments,
        identities,
    })
}

fn get_tracks_with_artists_for_release_on(
    conn: &Connection,
    release_id: &str,
) -> Result<Vec<DbTrackWithArtists>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT
            track.id AS track_id,
            track.release_id AS track_release_id,
            track.title AS track_title,
            track.side AS track_side,
            track.track_number AS track_track_number,
            track.duration_ms AS track_duration_ms,
            track.discogs_position AS track_discogs_position,
            track.created_at AS track_created_at,
            artist.id AS artist_id,
            artist.name AS artist_name,
            artist.sort_name AS artist_sort_name,
            artist.discogs_artist_id AS artist_discogs_artist_id,
            artist.musicbrainz_artist_id AS artist_musicbrainz_artist_id,
            artist.created_at AS artist_created_at
         FROM tracks track
         LEFT JOIN track_artists ta ON ta.track_id = track.id
         LEFT JOIN artists artist ON artist.id = ta.artist_id
         WHERE track.release_id = ?
         ORDER BY track.side, track.track_number, track.id, ta.position",
    )?;
    let mut rows = stmt.query(params![release_id])?;
    let mut tracks = Vec::new();
    let mut current_track: Option<DbTrackWithArtists> = None;
    let mut current_track_id: Option<String> = None;

    while let Some(row) = rows.next()? {
        let track = row_to_joined_track(row)?;
        if current_track_id.as_deref() != Some(track.id.as_str()) {
            if let Some(track) = current_track.take() {
                tracks.push(track);
            }
            current_track_id = Some(track.id.clone());
            current_track = Some(DbTrackWithArtists {
                track,
                artists: Vec::new(),
            });
        }

        let artist_id: Option<String> = row.get("artist_id")?;
        if artist_id.is_some() {
            current_track
                .as_mut()
                .expect("joined release row has a current track")
                .artists
                .push(row_to_joined_artist(row)?);
        }
    }

    if let Some(track) = current_track {
        tracks.push(track);
    }

    Ok(tracks)
}

fn get_files_for_release_on(conn: &Connection, release_id: &str) -> Result<Vec<DbFile>, DbError> {
    let mut stmt = conn.prepare("SELECT * FROM release_files WHERE release_id = ?")?;
    let rows = stmt.query_map(params![release_id], row_to_file)?;
    rows.collect::<coven::rusqlite::Result<Vec<_>>>()
        .map_err(DbError::from)
}

fn get_audio_formats_for_release_on(
    conn: &Connection,
    release_id: &str,
) -> Result<Vec<DbAudioFormat>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT af.* FROM audio_formats af \
             JOIN tracks t ON t.id = af.track_id \
             WHERE t.release_id = ?",
    )?;
    let rows = stmt.query_map(params![release_id], row_to_audio_format)?;
    rows.collect::<coven::rusqlite::Result<Vec<_>>>()
        .map_err(DbError::from)
}

fn get_audio_segments_for_release_on(
    conn: &Connection,
    release_id: &str,
) -> Result<Vec<DbAudioSegment>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT s.* FROM audio_format_segments s \
             JOIN audio_formats af ON af.id = s.audio_format_id \
             JOIN tracks t ON t.id = af.track_id \
             WHERE t.release_id = ? \
             ORDER BY af.track_id, s.segment_index",
    )?;
    let rows = stmt.query_map(params![release_id], row_to_audio_segment)?;
    rows.collect::<coven::rusqlite::Result<Vec<_>>>()
        .map_err(DbError::from)
}

fn get_release_identities_on(
    conn: &Connection,
    release_id: &str,
) -> Result<Vec<crate::import::ReleaseIdentity>, DbError> {
    let mut stmt = conn.prepare(
        r#"
            SELECT source, source_group_id, source_release_id
            FROM release_identities
            WHERE release_id = ?
            "#,
    )?;
    let raw = stmt
        .query_map(params![release_id], |row| {
            Ok((
                row.get::<_, String>("source")?,
                row.get::<_, String>("source_group_id")?,
                row.get::<_, Option<String>>("source_release_id")?,
            ))
        })?
        .collect::<coven::rusqlite::Result<Vec<_>>>()?;

    let mut identities = Vec::with_capacity(raw.len());
    for (source_str, source_group_id, source_release_id) in raw {
        let Ok(source) = crate::import::MetadataSource::from_str(&source_str) else {
            tracing::warn!(
                %release_id, source = %source_str,
                "skipping release_identities row with unknown source"
            );
            continue;
        };
        identities.push(crate::import::ReleaseIdentity {
            source,
            source_group_id,
            source_release_id,
        });
    }
    Ok(identities)
}

// ─── Row-map helpers (free functions; take `&Row`) ──────────────────────────

/// Build a column-conversion error for a named column whose stored text the
/// mapper could not turn into its typed value. A corrupt column then surfaces
/// like any other bad read instead of panicking or silently mis-defaulting.
fn column_conversion_error(row: &Row, column: &str, message: String) -> coven::rusqlite::Error {
    // The column was just read, so its index resolves; if it somehow doesn't,
    // that lookup error is itself a faithful failure to return.
    match row.as_ref().column_index(column) {
        Ok(idx) => coven::rusqlite::Error::FromSqlConversionFailure(
            idx,
            coven::rusqlite::types::Type::Text,
            message.into(),
        ),
        Err(e) => e,
    }
}

/// Read a named rfc3339 timestamp column, surfacing a malformed value as a
/// column-conversion error rather than panicking on the parse.
fn rfc3339_column(row: &Row, column: &str) -> coven::rusqlite::Result<DateTime<Utc>> {
    let raw: String = row.get(column)?;
    DateTime::parse_from_rfc3339(&raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            column_conversion_error(
                row,
                column,
                format!("{column} {raw:?} is not a valid rfc3339 timestamp: {e}"),
            )
        })
}

fn metadata_source_column(row: &Row, column: &str) -> coven::rusqlite::Result<MetadataSource> {
    let raw: String = row.get(column)?;
    raw.parse::<MetadataSource>()
        .map_err(|e| column_conversion_error(row, column, e))
}

fn row_to_joined_artist(row: &Row) -> coven::rusqlite::Result<DbArtist> {
    Ok(DbArtist {
        id: row.get("artist_id")?,
        name: row.get("artist_name")?,
        sort_name: row.get("artist_sort_name")?,
        discogs_artist_id: row.get("artist_discogs_artist_id")?,
        musicbrainz_artist_id: row.get("artist_musicbrainz_artist_id")?,
        created_at: rfc3339_column(row, "artist_created_at")?,
    })
}

fn row_to_joined_album(row: &Row) -> coven::rusqlite::Result<DbAlbum> {
    Ok(DbAlbum {
        id: row.get("album_id")?,
        title: row.get("album_title")?,
        artist_id: row.get("album_artist_id")?,
        year: row.get("album_year")?,
        primary_release_id: row.get("album_primary_release_id")?,
        is_compilation: row.get("album_is_compilation")?,
        created_at: rfc3339_column(row, "album_created_at")?,
    })
}

fn row_to_joined_track(row: &Row) -> coven::rusqlite::Result<DbTrack> {
    Ok(DbTrack {
        id: row.get("track_id")?,
        release_id: row.get("track_release_id")?,
        title: row.get("track_title")?,
        side: row.get("track_side")?,
        track_number: row.get("track_track_number")?,
        duration_ms: row.get("track_duration_ms")?,
        discogs_position: row.get("track_discogs_position")?,
        created_at: rfc3339_column(row, "track_created_at")?,
    })
}

fn row_to_release(row: &Row) -> coven::rusqlite::Result<DbRelease> {
    let metadata_source: String = row.get("metadata_source")?;
    let metadata_source = metadata_source
        .parse::<ReleaseMetadataSource>()
        .map_err(|e| column_conversion_error(row, "metadata_source", format!("releases.{e}")))?;
    Ok(DbRelease {
        id: row.get("id")?,
        album_id: row.get("album_id")?,
        release_name: row.get("release_name")?,
        pressing: Pressing {
            year: row.get("year")?,
            format: row.get("format")?,
            label: row.get("label")?,
            catalog_number: row.get("catalog_number")?,
            country: row.get("country")?,
            barcode: row.get("barcode")?,
        },
        disc_id: row.get("disc_id")?,
        metadata_source,
        metadata_source_release_id: row.get("metadata_source_release_id")?,
        remote: row.get("remote")?,
        source_folder_name: row.get("source_folder_name")?,
        content_hash: row.get("content_hash")?,
        album_loudness_lufs: row.get("album_loudness_lufs")?,
        album_peak_linear: row.get("album_peak_linear")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

fn row_to_file(row: &Row) -> coven::rusqlite::Result<DbFile> {
    Ok(DbFile {
        id: row.get("id")?,
        release_id: row.get("release_id")?,
        original_filename: row.get("original_filename")?,
        file_size: row.get("file_size")?,
        content_type: ContentType::from_mime(&row.get::<_, String>("content_type")?),
        cloud_path: row.get("cloud_path")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

fn row_to_artist(row: &Row) -> coven::rusqlite::Result<DbArtist> {
    Ok(DbArtist {
        id: row.get("id")?,
        name: row.get("name")?,
        sort_name: row.get("sort_name")?,
        discogs_artist_id: row.get("discogs_artist_id")?,
        musicbrainz_artist_id: row.get("musicbrainz_artist_id")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

/// Parse a DbAlbum from a SQL row.
fn row_to_album(row: &Row) -> coven::rusqlite::Result<DbAlbum> {
    Ok(DbAlbum {
        id: row.get("id")?,
        title: row.get("title")?,
        artist_id: row.get("artist_id")?,
        year: row.get("year")?,
        primary_release_id: row.get("primary_release_id")?,
        is_compilation: row.get("is_compilation")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

fn row_to_track(row: &Row) -> coven::rusqlite::Result<DbTrack> {
    Ok(DbTrack {
        id: row.get("id")?,
        release_id: row.get("release_id")?,
        title: row.get("title")?,
        side: row.get("side")?,
        track_number: row.get("track_number")?,
        duration_ms: row.get("duration_ms")?,
        discogs_position: row.get("discogs_position")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

fn row_to_audio_format(row: &Row) -> coven::rusqlite::Result<DbAudioFormat> {
    Ok(DbAudioFormat {
        id: row.get("id")?,
        track_id: row.get("track_id")?,
        content_type: ContentType::from_mime(&row.get::<_, String>("content_type")?),
        pregap_ms: row.get("pregap_ms")?,
        generated_pregap_ms: row.get("generated_pregap_ms")?,
        pregap_samples: row.get("pregap_samples")?,
        generated_pregap_samples: row.get("generated_pregap_samples")?,
        sample_rate: row.get("sample_rate")?,
        bits_per_sample: row.get("bits_per_sample")?,
        channels: row.get("channels")?,
        track_loudness_lufs: row.get("track_loudness_lufs")?,
        track_peak_linear: row.get("track_peak_linear")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

fn row_to_audio_segment(row: &Row) -> coven::rusqlite::Result<DbAudioSegment> {
    let role_text: String = row.get("role")?;
    let role = DbAudioSegmentRole::from_db_value(&role_text).ok_or_else(|| {
        coven::rusqlite::Error::FromSqlConversionFailure(
            0,
            coven::rusqlite::types::Type::Text,
            format!("unknown audio segment role: {role_text}").into(),
        )
    })?;
    Ok(DbAudioSegment {
        id: row.get("id")?,
        audio_format_id: row.get("audio_format_id")?,
        segment_index: row.get("segment_index")?,
        role,
        file_id: row.get("file_id")?,
        start_sample: row.get("start_sample")?,
        end_sample: row.get("end_sample")?,
        start_byte: row.get("start_byte")?,
        end_byte: row.get("end_byte")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

fn row_to_import(row: &Row) -> coven::rusqlite::Result<DbImport> {
    let status_str: String = row.get("status")?;
    let status = match status_str.as_str() {
        "importing" | "preparing" => ImportOperationStatus::Importing,
        "complete" => ImportOperationStatus::Complete,
        "failed" => ImportOperationStatus::Failed,
        other => {
            return Err(column_conversion_error(
                row,
                "status",
                format!("imports.status {other:?} is not a known import status"),
            ));
        }
    };
    Ok(DbImport {
        id: row.get("id")?,
        status,
        release_id: row.get("release_id")?,
        album_title: row.get("album_title")?,
        artist_name: row.get("artist_name")?,
        folder_path: row.get("folder_path")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        error_message: row.get("error_message")?,
    })
}

/// Map one row of the release-storage-summary query to a `DbReleaseStorageSummary`.
fn row_to_release_storage_summary(row: &Row) -> coven::rusqlite::Result<DbReleaseStorageSummary> {
    Ok(DbReleaseStorageSummary {
        release_id: row.get("release_id")?,
        album_id: row.get("album_id")?,
        album_title: row.get("album_title")?,
        artist_names: row.get("artist_names")?,
        format: row.get("format")?,
        primary_release_id: row.get("primary_release_id")?,
        remote: row.get("remote")?,
        any_file_id: row.get("any_file_id")?,
        file_count: row.get("file_count")?,
        total_size: row.get("total_size")?,
    })
}

// ─── Synced-row INSERT helpers (run inside `db.call`, against `&Connection`
// or a `&Transaction`, both of which deref to `&Connection`). ───────────────

fn insert_artist_row(conn: &Connection, artist: &DbArtist, reg: &str) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO artists (
            id, name, sort_name, discogs_artist_id,
            musicbrainz_artist_id, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            artist.id,
            artist.name,
            artist.sort_name,
            artist.discogs_artist_id,
            artist.musicbrainz_artist_id,
            reg,
            artist.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

fn upsert_library_image_row_with_cloud_path(
    conn: &Connection,
    image: &DbLibraryImage,
    cloud_path: Option<String>,
    reg: &str,
) -> Result<(), DbError> {
    let image = DbLibraryImage {
        cloud_path,
        ..image.clone()
    };
    upsert_library_image_row(conn, &image, reg)
}

fn update_artist_external_ids_row(
    conn: &Connection,
    id: &str,
    discogs_artist_id: Option<&str>,
    musicbrainz_artist_id: Option<&str>,
    sort_name: Option<&str>,
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        UPDATE artists SET
            discogs_artist_id = COALESCE(discogs_artist_id, ?),
            musicbrainz_artist_id = COALESCE(musicbrainz_artist_id, ?),
            sort_name = COALESCE(sort_name, ?),
            _updated_at = ?
        WHERE id = ?
        "#,
        params![discogs_artist_id, musicbrainz_artist_id, sort_name, reg, id,],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

fn insert_album_row(conn: &Connection, album: &DbAlbum, reg: &str) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO albums (
            id, title, artist_id, year, primary_release_id, is_compilation,
            _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            album.id,
            album.title,
            album.artist_id,
            album.year,
            album.primary_release_id,
            album.is_compilation,
            reg,
            album.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

fn insert_album_artist_row(
    conn: &Connection,
    aa: &DbAlbumArtist,
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
        params![
            aa.id,
            aa.album_id,
            aa.artist_id,
            aa.position,
            reg,
            aa.created_at.to_rfc3339()
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

fn insert_release_row(conn: &Connection, release: &DbRelease, reg: &str) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO releases (
            id, album_id, release_name, year,
            disc_id, metadata_source, metadata_source_release_id,
            format, label, catalog_number, country, barcode,
            remote,
            source_folder_name, content_hash,
            album_loudness_lufs, album_peak_linear,
            _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            release.id,
            release.album_id,
            release.release_name,
            release.pressing.year,
            release.disc_id,
            release.metadata_source.as_str(),
            release.metadata_source_release_id,
            release.pressing.format,
            release.pressing.label,
            release.pressing.catalog_number,
            release.pressing.country,
            release.pressing.barcode,
            release.remote,
            release.source_folder_name,
            release.content_hash,
            release.album_loudness_lufs,
            release.album_peak_linear,
            reg,
            release.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

fn insert_track_row(conn: &Connection, track: &DbTrack, reg: &str) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO tracks (
            id, release_id, title, side, track_number, duration_ms,
            discogs_position, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            track.id,
            track.release_id,
            track.title,
            track.side,
            track.track_number,
            track.duration_ms,
            track.discogs_position,
            reg,
            track.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

fn insert_track_artist_row(
    conn: &Connection,
    ta: &DbTrackArtist,
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO track_artists (id, track_id, artist_id, position, _updated_at, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
        params![
            ta.id,
            ta.track_id,
            ta.artist_id,
            ta.position,
            reg,
            ta.created_at.to_rfc3339()
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

fn insert_work_row(conn: &Connection, work: &DbWork, reg: &str) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT OR IGNORE INTO works (
            id, title, disambiguation, work_type, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        "#,
        params![
            work.id,
            work.title,
            work.disambiguation,
            work.work_type,
            reg,
            work.created_at.to_rfc3339()
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

fn insert_work_artist_row(
    conn: &Connection,
    link: &DbWorkArtist,
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT OR IGNORE INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            link.id,
            link.work_id,
            link.artist_id,
            link.position,
            link.source.as_str(),
            reg,
            link.created_at.to_rfc3339()
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

fn insert_work_part_row(conn: &Connection, part: &DbWorkPart, reg: &str) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT OR IGNORE INTO work_parts (
            id, parent_work_id, child_work_id, position, source, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            part.id,
            part.parent_work_id,
            part.child_work_id,
            part.position,
            part.source.as_str(),
            reg,
            part.created_at.to_rfc3339()
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

fn insert_track_work_row(conn: &Connection, link: &DbTrackWork, reg: &str) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO track_works (id, track_id, work_id, position, source, _updated_at, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            link.id,
            link.track_id,
            link.work_id,
            link.position,
            link.source.as_str(),
            reg,
            link.created_at.to_rfc3339()
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

fn insert_release_artist_role_row(
    conn: &Connection,
    role: &DbReleaseArtistRole,
    reg: &str,
) -> Result<(), DbError> {
    insert_artist_role_row(
        conn,
        "release_artist_roles",
        "release_id",
        params![
            role.id,
            role.release_id,
            role.artist_id,
            role.position,
            role.source.as_str(),
            role.source_credit,
            reg,
            role.created_at.to_rfc3339()
        ],
    )
}

fn insert_track_artist_role_row(
    conn: &Connection,
    role: &DbTrackArtistRole,
    reg: &str,
) -> Result<(), DbError> {
    insert_artist_role_row(
        conn,
        "track_artist_roles",
        "track_id",
        params![
            role.id,
            role.track_id,
            role.artist_id,
            role.position,
            role.source.as_str(),
            role.source_credit,
            reg,
            role.created_at.to_rfc3339()
        ],
    )
}

fn insert_artist_role_row(
    conn: &Connection,
    table: &'static str,
    target_column: &'static str,
    values: impl Params,
) -> Result<(), DbError> {
    let sql = format!(
        r#"
        INSERT INTO {table} (
            id, {target_column}, artist_id, position, source, source_credit, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#
    );
    conn.execute(&sql, values)
        .map(|_| ())
        .map_err(DbError::from)
}

fn insert_file_row(conn: &Connection, file: &DbFile, reg: &str) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO release_files (
            id, release_id, original_filename, file_size, content_type, cloud_path, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            file.id,
            file.release_id,
            file.original_filename,
            file.file_size,
            file.content_type.as_str(),
            file.cloud_path,
            reg,
            file.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

fn insert_audio_format_row(
    conn: &Connection,
    af: &DbAudioFormat,
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO audio_formats (
            id, track_id, content_type, pregap_ms, generated_pregap_ms, pregap_samples, generated_pregap_samples, sample_rate, bits_per_sample, channels, track_loudness_lufs, track_peak_linear, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            af.id,
            af.track_id,
            af.content_type.as_str(),
            af.pregap_ms,
            af.generated_pregap_ms,
            af.pregap_samples,
            af.generated_pregap_samples,
            af.sample_rate,
            af.bits_per_sample,
            af.channels,
            af.track_loudness_lufs,
            af.track_peak_linear,
            reg,
            af.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

fn insert_audio_segment_row(
    conn: &Connection,
    segment: &DbAudioSegment,
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO audio_format_segments (
            id, audio_format_id, segment_index, role, file_id, start_sample, end_sample, start_byte, end_byte, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            segment.id,
            segment.audio_format_id,
            segment.segment_index,
            segment.role.as_str(),
            segment.file_id,
            segment.start_sample,
            segment.end_sample,
            segment.start_byte,
            segment.end_byte,
            reg,
            segment.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// `INSERT OR REPLACE` a cached `release_metadata` row. Not a synced table —
/// no `_updated_at` stamp.
fn insert_release_metadata_row(conn: &Connection, meta: &DbReleaseMetadata) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT OR REPLACE INTO release_metadata (
            id, release_id, source, json, fetched_at
        ) VALUES (?, ?, ?, ?, ?)
        "#,
        params![
            meta.id,
            meta.release_id,
            meta.source,
            meta.json,
            meta.fetched_at.to_rfc3339()
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

fn upsert_library_image_row(
    conn: &Connection,
    image: &DbLibraryImage,
    reg: &str,
) -> Result<(), DbError> {
    let table = image_table(&image.image_type);
    conn.execute(
        &format!(
            "INSERT INTO {table} (id, content_type, file_size, width, height, source, source_url, cloud_path, _updated_at, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET \
                 content_type = excluded.content_type, \
                 file_size = excluded.file_size, \
                 width = excluded.width, \
                 height = excluded.height, \
                 source = excluded.source, \
                 source_url = excluded.source_url, \
                 cloud_path = excluded.cloud_path, \
                 _updated_at = excluded._updated_at"
        ),
        params![
            image.id,
            image.content_type.as_str(),
            image.file_size,
            image.width,
            image.height,
            image.source,
            image.source_url,
            image.cloud_path,
            reg,
            image.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// A release's `(album_id, source_folder_name)` — the release-scoped context a
/// browsable key is built from. The release row always exists when one of its
/// blobs is being keyed (it was just inserted), so a missing row is a broken
/// invariant surfaced as an error, not masked. `source_folder_name` is `None`
/// for a non-folder import.
fn release_path_context(
    conn: &Connection,
    release_id: &str,
) -> Result<(String, Option<String>), DbError> {
    conn.query_row(
        "SELECT album_id, source_folder_name FROM releases WHERE id = ?",
        params![release_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
    )
    .map_err(DbError::from)
}

/// Build a release-scoped browsable key: look up the release's `(album_id,
/// source_folder)` context once, then let `make_key` shape it into the final
/// path. Shared by the audio and cover resolvers; the artist-image resolver keys
/// off the artist id alone and stands apart.
fn resolve_release_path(
    conn: &Connection,
    release_id: &str,
    make_key: impl FnOnce(&str, &str, Option<&str>) -> String,
) -> Result<String, DbError> {
    let (album_id, source_folder) = release_path_context(conn, release_id)?;
    Ok(make_key(&album_id, release_id, source_folder.as_deref()))
}

/// The `cloud_path` for a release file on a browsable home:
/// `{album_id}/{release_id}/{source_folder}/{filename}` (relative to the
/// `release_files` namespace coven prepends), mirroring the imported folder. Ids
/// are immutable and unique, so the key is stable and collision-free by
/// construction — no disambiguation.
fn resolve_audio_cloud_path(
    conn: &Connection,
    release_id: &str,
    original_filename: &str,
) -> Result<String, DbError> {
    resolve_release_path(conn, release_id, |album_id, release_id, source_folder| {
        crate::storage::readable_path::audio_key(
            album_id,
            release_id,
            source_folder,
            original_filename,
        )
    })
}

/// The `cloud_path` for a cover image on a browsable home:
/// `{album_id}/{release_id}/cover.{ext}` (relative to the `images` namespace
/// coven prepends). The cover's id is its release id. Covers are bae's own art,
/// not part of the imported folder, so they carry no `{source_folder}` level.
fn resolve_cover_cloud_path(
    conn: &Connection,
    release_id: &str,
    content_type: &ContentType,
) -> Result<String, DbError> {
    resolve_release_path(conn, release_id, |album_id, release_id, _source_folder| {
        crate::storage::readable_path::cover_cloud_path(album_id, release_id, content_type)
    })
}

/// The `cloud_path` for an artist image on a browsable home:
/// `{artist_id}/artist.{ext}` (relative to the `images` namespace). Keyed by the
/// artist id alone, so it needs no DB lookup.
fn resolve_artist_cloud_path(artist_id: &str, content_type: &ContentType) -> String {
    crate::storage::readable_path::artist_cloud_path(artist_id, content_type)
}

/// Insert one row into `release_identities`. Shared by the atomic import path
/// (`finalize_import_atomic` / `set_identity_atomic`, inside a transaction) and
/// `insert_release_identities` (on the connection directly).
fn insert_release_identity_row(
    conn: &Connection,
    release_id: &str,
    identity: &crate::import::ReleaseIdentity,
    reg: &str,
    now: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO release_identities (
            id, release_id, source, source_group_id, source_release_id,
            _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            uuid::Uuid::new_v4().to_string(),
            release_id,
            identity.source.as_str(),
            identity.source_group_id,
            identity.source_release_id,
            reg,
            now,
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// Replace every `album_artists` row for `album_id` with `artists` (delete
/// then insert). Factored out of `update_release_metadata_user_edit` so the
/// `album_artists` schema is written in one place.
fn replace_album_artists(
    conn: &Connection,
    album_id: &str,
    artists: &[DbAlbumArtist],
    reg: &str,
    now: &str,
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM album_artists WHERE album_id = ?",
        params![album_id],
    )?;
    for aa in artists {
        conn.execute(
            r#"INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
            VALUES (?, ?, ?, ?, ?, ?)"#,
            params![aa.id, album_id, aa.artist_id, aa.position, reg, now],
        )?;
    }
    Ok(())
}

/// Replace `track_artists` rows for every id in `track_ids`, then insert the
/// new rows. Callers pass the affected track ids explicitly because `artists`
/// may not cover every track (a track legitimately has no per-track artists
/// when it inherits from the album).
fn replace_track_artists(
    conn: &Connection,
    track_ids: &[&str],
    artists: &[DbTrackArtist],
    reg: &str,
    now: &str,
) -> Result<(), DbError> {
    for track_id in track_ids {
        conn.execute(
            "DELETE FROM track_artists WHERE track_id = ?",
            params![track_id],
        )?;
    }
    for ta in artists {
        conn.execute(
            r#"INSERT INTO track_artists (id, track_id, artist_id, position, _updated_at, created_at)
            VALUES (?, ?, ?, ?, ?, ?)"#,
            params![ta.id, ta.track_id, ta.artist_id, ta.position, reg, now],
        )?;
    }
    Ok(())
}

/// Resolve queue track-ids to their display rows in position order, preserving
/// duplicates. The same track may be queued more than once, so a repeated id
/// Per-track display metadata: one row per distinct track in the queue, fetched
/// once and joined onto every queue entry that plays that track. Carries no
/// identity (no entry id, no track id) — `resolve_queue_entries` supplies those
/// from the entries.
struct TrackQueueMeta {
    title: String,
    artist_names: String,
    duration_ms: Option<i64>,
    album_title: String,
    cover_image_id: Option<String>,
}

/// Join per-track metadata onto each queue entry, preserving order and
/// duplicates. The same track id appears once in `meta_by_track` but resolves
/// once per entry — so the metadata lookup is `get`, not `remove` (which would
/// consume the row on first hit and silently drop later occurrences). Keying
/// display rows on each entry's per-instance id (rather than a position) is what
/// lets the UI target duplicate tracks independently.
fn resolve_queue_entries(
    meta_by_track: &std::collections::HashMap<String, TrackQueueMeta>,
    entries: &[QueueEntry],
) -> Vec<QueueItem> {
    entries
        .iter()
        .filter_map(|entry| {
            let Some(meta) = meta_by_track.get(&entry.track_id) else {
                // A queue entry whose track has no metadata means the queue
                // references a track no longer in the library — an inconsistency
                // (library deletion clears the track from the queue), not a
                // normal skip. Drop it from the projection but surface it.
                tracing::warn!(
                    "queue entry {} references track {} with no metadata; dropping from the queue projection",
                    entry.id.0,
                    entry.track_id
                );
                return None;
            };
            Some(QueueItem {
                entry_id: entry.id.0.clone(),
                track_id: entry.track_id.clone(),
                title: meta.title.clone(),
                artist_names: meta.artist_names.clone(),
                duration_ms: meta.duration_ms,
                album_title: meta.album_title.clone(),
                cover_image_id: meta.cover_image_id.clone(),
            })
        })
        .collect()
}

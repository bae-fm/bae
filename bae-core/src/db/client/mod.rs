use crate::db::models::*;
use crate::playback::QueueEntry;
use crate::queue::QueueItem;
use crate::util::content_type::ContentType;
use chrono::{DateTime, Utc};
use coven::rusqlite::{params, Connection, OptionalExtension, Row};
use coven::{ClockRef, Coven, CovenError, CovenHandle, DbError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, warn};

mod album;
mod artist;
mod blobs;
mod identity;
mod playback;
mod release;
mod track;

#[cfg(test)]
mod tests;

/// The table a host-provided image blob's row lives in. The image type IS the
/// table (`covers` / `artist_images`), so there is no `type` column. A fixed
/// match over the enum, so the interpolated name is always a trusted literal.
fn image_table(image_type: &LibraryImageType) -> &'static str {
    match image_type {
        LibraryImageType::Cover => "covers",
        LibraryImageType::Artist => "artist_images",
    }
}

fn image_namespace(image_type: &LibraryImageType) -> &'static str {
    match image_type {
        LibraryImageType::Cover => crate::sync::COVERS_NAMESPACE,
        LibraryImageType::Artist => crate::sync::ARTIST_IMAGES_NAMESPACE,
    }
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

#[cfg(any(test, feature = "test-utils"))]
fn row_to_outbox_entry(row: &Row<'_>) -> coven::rusqlite::Result<coven::OutboxEntry> {
    let op_tag: String = row.get(1)?;
    let operation = match op_tag.as_str() {
        "upload" => {
            let scope_str: String = row.get(5)?;
            let scope = coven::BlobScope::from_outbox_str(&scope_str)
                .unwrap_or_else(|| panic!("invalid cloud_outbox.scope: {scope_str:?}"));
            coven::OutboxOperation::Upload {
                file_id: row.get(2)?,
                source_path: row.get(4)?,
                scope,
                retain_pinned: row.get(6)?,
            }
        }
        "delete" => coven::OutboxOperation::Delete,
        "cancel" => coven::OutboxOperation::Cancel,
        other => panic!("invalid cloud_outbox.operation: {other:?}"),
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
fn row_to_library_image(row: &Row, image_type: LibraryImageType) -> DbLibraryImage {
    DbLibraryImage {
        id: row.get("id").unwrap(),
        image_type,
        content_type: ContentType::from_mime(&row.get::<_, String>("content_type").unwrap()),
        file_size: row.get("file_size").unwrap(),
        width: row.get("width").unwrap(),
        height: row.get("height").unwrap(),
        source: row.get("source").unwrap(),
        source_url: row.get("source_url").unwrap(),
        cloud_path: row.get("cloud_path").unwrap(),
        created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>("created_at").unwrap())
            .unwrap()
            .with_timezone(&Utc),
    }
}

/// Build an ORDER BY clause from sort criteria.
/// Returns `(order_by_clause, needs_artist_join)`.
fn build_order_by(sort: &[AlbumSortCriterion], default: &str) -> (String, bool) {
    if sort.is_empty() {
        return (default.to_string(), false);
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
    (clause, needs_artist_join)
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

/// Shared SELECT list for album-summary queries. Emits `artist_names`
/// (primary artist + album_artists, comma-joined) and `release_ids_json`
/// (releases in created_at order). Callers append `FROM albums a`, any
/// `art_sort` join (see `album_summary_artist_join`), and their own
/// `ORDER BY` / `WHERE` / `LIMIT`.
const ALBUM_SUMMARY_SELECT: &str = "SELECT \
    a.id, a.title, a.year, a.is_compilation, a.primary_release_id, \
    (SELECT art_primary.name FROM artists art_primary WHERE art_primary.id = a.artist_id) \
    || COALESCE(( \
        SELECT ', ' || GROUP_CONCAT(ar.name, ', ') \
        FROM album_artists aa \
        JOIN artists ar ON ar.id = aa.artist_id \
        WHERE aa.album_id = a.id \
        ORDER BY aa.position \
    ), '') AS artist_names, \
    COALESCE(( \
        SELECT json_group_array(r.id) \
        FROM releases r \
        WHERE r.album_id = a.id \
        ORDER BY r.created_at \
    ), '[]') AS release_ids_json";

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
        self.inner
            .handle
            .sql(move |sql| f(sql.connection()).map_err(CovenError::from))
            .await
            .map_err(Self::coven_error)
    }

    /// Stamp a synced row's `_updated_at` from coven's SQL context.
    async fn register_stamp(&self) -> Result<String, DbError> {
        self.inner
            .handle
            .sql(|sql| Ok(sql.stamp()))
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

    /// Open a test database using a coven library directory derived from `path`.
    pub async fn new(
        database_path: &str,
        clock: ClockRef,
        device_id: String,
        synced_tables: Vec<coven::SyncedTable>,
    ) -> Result<Self, DbError> {
        info!("Opening database at {}", database_path);
        let path = Path::new(database_path);
        let library_root = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| DbError(format!("database path has no parent: {database_path}")))?;
        let library_dir = coven::LibraryDir::new(library_root);
        let config = coven::Config::with_defaults(
            "test-library".to_string(),
            device_id,
            library_dir,
            "Test Library".to_string(),
        );
        let key_service = coven::KeyService::new(config.library_id.clone());
        Self::open(config, clock, key_service, synced_tables, None)
    }

    /// Test convenience: open over `path` with a fresh device id and bae's real
    /// synced-table set, so unit/integration tests don't repeat the wiring.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_test(database_path: &str, clock: ClockRef) -> Result<Self, DbError> {
        Self::new(
            database_path,
            clock,
            "test-device".to_string(),
            crate::sync::synced_tables(),
        )
        .await
    }

    fn row_to_release(row: &Row) -> DbRelease {
        let metadata_source: String = row.get("metadata_source").unwrap();
        let metadata_source = metadata_source.parse::<ReleaseMetadataSource>().expect(
            "releases.metadata_source must be one of 'musicbrainz' | 'discogs' | 'file_tags'",
        );
        DbRelease {
            id: row.get("id").unwrap(),
            album_id: row.get("album_id").unwrap(),
            release_name: row.get("release_name").unwrap(),
            pressing: Pressing {
                year: row.get("year").unwrap(),
                format: row.get("format").unwrap(),
                label: row.get("label").unwrap(),
                catalog_number: row.get("catalog_number").unwrap(),
                country: row.get("country").unwrap(),
                barcode: row.get("barcode").unwrap(),
            },
            disc_id: row.get("disc_id").unwrap(),
            metadata_source,
            metadata_source_release_id: row.get("metadata_source_release_id").unwrap(),
            remote: row.get("remote").unwrap(),
            source_folder_name: row.get("source_folder_name").unwrap(),
            content_hash: row.get("content_hash").unwrap(),
            album_loudness_lufs: row.get("album_loudness_lufs").unwrap(),
            album_peak_linear: row.get("album_peak_linear").unwrap(),
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>("created_at").unwrap())
                .unwrap()
                .with_timezone(&Utc),
        }
    }

    fn row_to_file(row: &Row) -> DbFile {
        DbFile {
            id: row.get("id").unwrap(),
            release_id: row.get("release_id").unwrap(),
            original_filename: row.get("original_filename").unwrap(),
            file_size: row.get("file_size").unwrap(),
            content_type: ContentType::from_mime(&row.get::<_, String>("content_type").unwrap()),
            cloud_path: row.get("cloud_path").unwrap(),
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>("created_at").unwrap())
                .unwrap()
                .with_timezone(&Utc),
        }
    }

    fn row_to_artist(row: &Row) -> DbArtist {
        DbArtist {
            id: row.get("id").unwrap(),
            name: row.get("name").unwrap(),
            sort_name: row.get("sort_name").unwrap(),
            discogs_artist_id: row.get("discogs_artist_id").unwrap(),
            musicbrainz_artist_id: row.get("musicbrainz_artist_id").unwrap(),
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>("created_at").unwrap())
                .unwrap()
                .with_timezone(&Utc),
        }
    }

    /// Parse a DbAlbum from a SQL row.
    fn row_to_album(row: &Row) -> DbAlbum {
        DbAlbum {
            id: row.get("id").unwrap(),
            title: row.get("title").unwrap(),
            artist_id: row.get("artist_id").unwrap(),
            year: row.get("year").unwrap(),
            primary_release_id: row.get("primary_release_id").unwrap(),
            is_compilation: row.get("is_compilation").unwrap(),
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>("created_at").unwrap())
                .unwrap()
                .with_timezone(&Utc),
        }
    }

    /// Shared SQL assembly for `find_album_detail` / `find_release_detail`.
    /// Returns the raw per-release aggregate.
    async fn build_release_detail(&self, release: DbRelease) -> Result<DbReleaseDetail, DbError> {
        let db_tracks = self.get_tracks_for_release(&release.id).await?;
        let mut tracks = Vec::with_capacity(db_tracks.len());
        for track in db_tracks {
            let artists = self.get_artists_for_track(&track.id).await?;
            tracks.push(DbTrackWithArtists { track, artists });
        }

        let files = self.get_files_for_release(&release.id).await?;
        let audio_formats = self.get_audio_formats_for_release(&release.id).await?;
        let identities = self.get_release_identities(&release.id).await?;

        Ok(DbReleaseDetail {
            release,
            tracks,
            files,
            audio_formats,
            identities,
        })
    }
}

// ─── Row-map helpers (free functions; take `&Row`) ──────────────────────────

fn row_to_track(row: &Row) -> coven::rusqlite::Result<DbTrack> {
    Ok(DbTrack {
        id: row.get("id")?,
        release_id: row.get("release_id")?,
        title: row.get("title")?,
        side: row.get("side")?,
        track_number: row.get("track_number")?,
        duration_ms: row.get("duration_ms")?,
        discogs_position: row.get("discogs_position")?,
        created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>("created_at")?)
            .unwrap()
            .with_timezone(&Utc),
    })
}

fn row_to_audio_format(row: &Row) -> coven::rusqlite::Result<DbAudioFormat> {
    Ok(DbAudioFormat {
        id: row.get("id")?,
        track_id: row.get("track_id")?,
        content_type: ContentType::from_mime(&row.get::<_, String>("content_type")?),
        pregap_ms: row.get("pregap_ms")?,
        sample_rate: row.get("sample_rate")?,
        bits_per_sample: row.get("bits_per_sample")?,
        channels: row.get("channels")?,
        file_id: row.get("file_id")?,
        start_sample: row.get("start_sample")?,
        end_sample: row.get("end_sample")?,
        end_byte: row.get("end_byte")?,
        track_loudness_lufs: row.get("track_loudness_lufs")?,
        track_peak_linear: row.get("track_peak_linear")?,
        created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>("created_at")?)
            .unwrap()
            .with_timezone(&Utc),
    })
}

fn row_to_import(row: &Row) -> coven::rusqlite::Result<DbImport> {
    let status_str: String = row.get("status")?;
    let status = match status_str.as_str() {
        "importing" | "preparing" => ImportOperationStatus::Importing,
        "complete" => ImportOperationStatus::Complete,
        "failed" => ImportOperationStatus::Failed,
        _ => ImportOperationStatus::Importing,
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

// ─── Synced-row INSERT helpers (run inside `db.call`, against `&Connection`
// or a `&Transaction`, both of which deref to `&Connection`). ───────────────

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
            id, track_id, content_type, pregap_ms, sample_rate, bits_per_sample, channels, file_id, start_sample, end_sample, end_byte, track_loudness_lufs, track_peak_linear, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            af.id,
            af.track_id,
            af.content_type.as_str(),
            af.pregap_ms,
            af.sample_rate,
            af.bits_per_sample,
            af.channels,
            af.file_id,
            af.start_sample,
            af.end_sample,
            af.end_byte,
            af.track_loudness_lufs,
            af.track_peak_linear,
            reg,
            af.created_at.to_rfc3339(),
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

/// The cloud key for a release file on a browsable home:
/// `storage/{album_id}/{release_id}/{source_folder}/{filename}`, mirroring the
/// imported folder. Ids are immutable and unique, so the key is stable and
/// collision-free by construction — no disambiguation.
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

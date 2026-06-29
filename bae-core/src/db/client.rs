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

    /// Insert a new artist
    pub async fn insert_artist(&self, artist: &DbArtist) -> Result<(), DbError> {
        let artist = artist.clone();
        let reg = self.register_stamp().await?;
        self.call(move |conn| {
            conn.execute(
                r#"
                    INSERT INTO artists (
                        id, name, sort_name, discogs_artist_id,
                        musicbrainz_artist_id,
                        _updated_at, created_at
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
        })
        .await
    }
    /// Look up a single artist by a one-parameter equality query. The four
    /// `get_artist_by_*` / `find_artist_by_id` lookups differ only in which
    /// column they match on, so they share this body.
    async fn get_artist_by_sql(
        &self,
        sql: &'static str,
        value: String,
    ) -> Result<Option<DbArtist>, DbError> {
        self.call(move |conn| {
            conn.query_row(sql, params![value], |row| Ok(Self::row_to_artist(row)))
                .optional()
                .map_err(DbError::from)
        })
        .await
    }

    /// Get artist by Discogs artist ID (for deduplication)
    pub async fn get_artist_by_discogs_id(
        &self,
        discogs_artist_id: &str,
    ) -> Result<Option<DbArtist>, DbError> {
        self.get_artist_by_sql(
            "SELECT * FROM artists WHERE discogs_artist_id = ?",
            discogs_artist_id.to_string(),
        )
        .await
    }

    /// Get artist by MusicBrainz artist ID (for deduplication)
    pub async fn get_artist_by_mb_id(&self, mb_id: &str) -> Result<Option<DbArtist>, DbError> {
        self.get_artist_by_sql(
            "SELECT * FROM artists WHERE musicbrainz_artist_id = ?",
            mb_id.to_string(),
        )
        .await
    }

    /// Get artist by name (case-insensitive, first match)
    pub async fn get_artist_by_name(&self, name: &str) -> Result<Option<DbArtist>, DbError> {
        self.get_artist_by_sql(
            "SELECT * FROM artists WHERE name = ? COLLATE NOCASE LIMIT 1",
            name.to_string(),
        )
        .await
    }

    /// Fill in NULL external IDs on an existing artist via COALESCE (never overwrites).
    /// Also updates sort_name if currently NULL.
    pub async fn update_artist_external_ids(
        &self,
        id: &str,
        discogs_id: Option<&str>,
        mb_id: Option<&str>,
        sort_name: Option<&str>,
    ) -> Result<(), DbError> {
        let (id, discogs_id, mb_id, sort_name) = (
            id.to_string(),
            discogs_id.map(str::to_string),
            mb_id.map(str::to_string),
            sort_name.map(str::to_string),
        );
        let reg = self.register_stamp().await?;
        self.call(move |conn| {
            conn.execute(
                r#"
                    UPDATE artists SET
                        discogs_artist_id = COALESCE(discogs_artist_id, ?),
                        musicbrainz_artist_id = COALESCE(musicbrainz_artist_id, ?),
                        sort_name = COALESCE(sort_name, ?),
                        _updated_at = ?
                    WHERE id = ?
                    "#,
                params![discogs_id, mb_id, sort_name, reg, id],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Insert album-artist relationship
    pub async fn insert_album_artist(&self, album_artist: &DbAlbumArtist) -> Result<(), DbError> {
        let album_artist = album_artist.clone();
        let reg = self.register_stamp().await?;
        self.call(move |conn| insert_album_artist_row(conn, &album_artist, &reg))
            .await
    }
    /// Insert track-artist relationship
    pub async fn insert_track_artist(&self, track_artist: &DbTrackArtist) -> Result<(), DbError> {
        let track_artist = track_artist.clone();
        let reg = self.register_stamp().await?;
        self.call(move |conn| insert_track_artist_row(conn, &track_artist, &reg))
            .await
    }
    /// Get artists for an album (ordered by position)
    pub async fn get_artists_for_album(&self, album_id: &str) -> Result<Vec<DbArtist>, DbError> {
        let album_id = album_id.to_string();
        self.call(move |conn| {
            // Primary artist from FK (sort_key = -1 so it's first),
            // then additional artists from junction table ordered by position.
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
            let rows = stmt.query_map(params![album_id, album_id], |row| {
                Ok(Self::row_to_artist(row))
            })?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }
    /// Get artists for a track (ordered by position)
    pub async fn get_artists_for_track(&self, track_id: &str) -> Result<Vec<DbArtist>, DbError> {
        let track_id = track_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
                        SELECT a.* FROM artists a
                        JOIN track_artists ta ON a.id = ta.artist_id
                        WHERE ta.track_id = ?
                        ORDER BY ta.position
                        "#,
            )?;
            let rows = stmt.query_map(params![track_id], |row| Ok(Self::row_to_artist(row)))?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }
    /// Find artist by ID. Caller-provided ID — may not exist.
    pub async fn find_artist_by_id(&self, artist_id: &str) -> Result<Option<DbArtist>, DbError> {
        self.get_artist_by_sql("SELECT * FROM artists WHERE id = ?", artist_id.to_string())
            .await
    }
    /// Resolve track IDs to their album IDs (track -> release -> album).
    /// Returns a map from track_id to album_id for all tracks that were found.
    pub async fn get_album_ids_for_tracks(
        &self,
        track_ids: &[String],
    ) -> Result<HashMap<String, String>, DbError> {
        if track_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let track_ids = track_ids.to_vec();
        self.call(move |conn| {
            let placeholders = track_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT t.id AS track_id, r.album_id FROM tracks t \
                     JOIN releases r ON t.release_id = r.id \
                     WHERE t.id IN ({placeholders})"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows =
                stmt.query_map(coven::rusqlite::params_from_iter(track_ids.iter()), |row| {
                    Ok((
                        row.get::<_, String>("track_id")?,
                        row.get::<_, String>("album_id")?,
                    ))
                })?;
            rows.collect::<coven::rusqlite::Result<HashMap<_, _>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// Search across albums and tracks by title
    pub async fn search_library(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<DbLibrarySearchResults, DbError> {
        let pattern = format!("%{}%", query);
        let limit_i64 = limit as i64;

        self
            .call(move |conn| {
                // Search albums by title, with primary artist name.
                // COALESCE the primary_release_id to the album's first release so
                // callers always see a release id (every album has at least one).
                let mut album_stmt = conn
                    .prepare(
                        r#"
                        SELECT a.id, a.title, a.year,
                               COALESCE(
                                   a.primary_release_id,
                                   (SELECT r.id FROM releases r WHERE r.album_id = a.id ORDER BY r.created_at LIMIT 1)
                               ) AS primary_release_id,
                               art.name as artist_name
                        FROM albums a
                        JOIN artists art ON a.artist_id = art.id
                        WHERE a.title LIKE ?
                        ORDER BY a.title
                        LIMIT ?
                        "#,
                    )?;
                let albums = album_stmt
                    .query_map(params![pattern, limit_i64], |row| {
                        Ok(DbAlbumSearchResult {
                            id: row.get("id")?,
                            title: row.get("title")?,
                            year: row.get("year")?,
                            primary_release_id: row.get("primary_release_id")?,
                            artist_name: row.get("artist_name")?,
                        })
                    })?
                    .collect::<coven::rusqlite::Result<Vec<_>>>()?;

                // Search tracks by title, with album and artist info
                let mut track_stmt = conn
                    .prepare(
                        r#"
                        SELECT t.id, t.title, t.duration_ms, r.album_id,
                               a.title as album_title,
                               art.name as artist_name
                        FROM tracks t
                        JOIN releases r ON t.release_id = r.id
                        JOIN albums a ON r.album_id = a.id
                        JOIN artists art ON a.artist_id = art.id
                        WHERE t.title LIKE ?
                        ORDER BY t.title
                        LIMIT ?
                        "#,
                    )?;
                let tracks = track_stmt
                    .query_map(params![pattern, limit_i64], |row| {
                        Ok(DbTrackSearchResult {
                            id: row.get("id")?,
                            title: row.get("title")?,
                            duration_ms: row.get("duration_ms")?,
                            album_id: row.get("album_id")?,
                            album_title: row.get("album_title")?,
                            artist_name: row.get("artist_name")?,
                        })
                    })?
                    .collect::<coven::rusqlite::Result<Vec<_>>>()?;

                Ok(DbLibrarySearchResults { albums, tracks })
            })
            .await
    }

    /// Insert a new album
    pub async fn insert_album(&self, album: &DbAlbum) -> Result<(), DbError> {
        let album = album.clone();
        let reg = self.register_stamp().await?;
        self.call(move |conn| insert_album_row(conn, &album, &reg))
            .await
    }
    /// Insert a new release.
    pub async fn insert_release(&self, release: &DbRelease) -> Result<(), DbError> {
        let release = release.clone();
        let reg = self.register_stamp().await?;
        self.call(move |conn| insert_release_row(conn, &release, &reg))
            .await
    }
    /// Insert a new track
    pub async fn insert_track(&self, track: &DbTrack) -> Result<(), DbError> {
        let track = track.clone();
        let reg = self.register_stamp().await?;
        self.call(move |conn| insert_track_row(conn, &track, &reg))
            .await
    }
    /// Insert album, release, and tracks in a single transaction
    /// Note: Artists and artist relationships should be inserted separately before calling this
    pub async fn insert_album_with_release_and_tracks(
        &self,
        album: &DbAlbum,
        release: &DbRelease,
        tracks: &[DbTrack],
        metadata: &[DbReleaseMetadata],
        track_artists: &[DbTrackArtist],
    ) -> Result<(), DbError> {
        let (album, release, tracks, metadata, track_artists) = (
            album.clone(),
            release.clone(),
            tracks.to_vec(),
            metadata.to_vec(),
            track_artists.to_vec(),
        );
        // One HLC register stamp for every synced row this transaction writes.
        let reg = self.register_stamp().await?;
        self.call(move |conn| {
            let tx = conn;
            insert_album_row(tx, &album, &reg)?;
            insert_release_row(tx, &release, &reg)?;
            for track in &tracks {
                insert_track_row(tx, track, &reg)?;
            }
            for ta in &track_artists {
                insert_track_artist_row(tx, ta, &reg)?;
            }
            for meta in &metadata {
                insert_release_metadata_row(tx, meta)?;
            }
            Ok(())
        })
        .await?;
        Ok(())
    }

    pub async fn insert_release_with_tracks(
        &self,
        release: &DbRelease,
        tracks: &[DbTrack],
        metadata: &[DbReleaseMetadata],
        track_artists: &[DbTrackArtist],
    ) -> Result<(), DbError> {
        let (release, tracks, metadata, track_artists) = (
            release.clone(),
            tracks.to_vec(),
            metadata.to_vec(),
            track_artists.to_vec(),
        );
        // One HLC register stamp for every synced row this transaction writes.
        let reg = self.register_stamp().await?;
        self.call(move |conn| {
            let tx = conn;
            insert_release_row(tx, &release, &reg)?;
            for track in &tracks {
                insert_track_row(tx, track, &reg)?;
            }
            for ta in &track_artists {
                insert_track_artist_row(tx, ta, &reg)?;
            }
            for meta in &metadata {
                insert_release_metadata_row(tx, meta)?;
            }
            Ok(())
        })
        .await?;
        Ok(())
    }

    /// Write a user-supplied metadata edit (from the EditMetadataSheet) in a
    /// single transaction:
    ///
    /// - Updates album-level fields, release pressing fields, and track
    ///   metadata.
    /// - Replaces `album_artists` and `track_artists` rows for the affected
    ///   album/tracks.
    /// - Does NOT touch `release_metadata` rows — the cached source payload is
    ///   independent of a user edit.
    /// - Does NOT touch `release_identities`, `metadata_source`, or
    ///   `metadata_source_release_id` — identity is orthogonal to metadata.
    ///
    /// `track_updates` maps existing track IDs to their edited rows.
    #[allow(clippy::too_many_arguments)]
    pub async fn update_release_metadata_user_edit(
        &self,
        album_id: &str,
        release_id: &str,
        album: &DbAlbum,
        release: &DbRelease,
        track_updates: &[(String, DbTrack)],
        album_artists: &[DbAlbumArtist],
        track_artists: &[DbTrackArtist],
    ) -> Result<(), DbError> {
        let (album_id, release_id, album, release, track_updates, album_artists, track_artists) = (
            album_id.to_string(),
            release_id.to_string(),
            album.clone(),
            release.clone(),
            track_updates.to_vec(),
            album_artists.to_vec(),
            track_artists.to_vec(),
        );
        let now = self.inner.clock.now().to_rfc3339();
        // One HLC register stamp for every synced row this edit touches.
        let reg = self.register_stamp().await?;
        self.call(move |conn| {
            let tx = conn;

            // 1. Update album.
            tx.execute(
                r#"UPDATE albums SET title = ?, artist_id = ?, year = ?, is_compilation = ?,
                    _updated_at = ? WHERE id = ?"#,
                params![
                    album.title,
                    album.artist_id,
                    album.year,
                    album.is_compilation,
                    reg,
                    album_id,
                ],
            )?;

            // 2. Update release pressing fields.
            tx.execute(
                r#"UPDATE releases SET year = ?, format = ?, label = ?, catalog_number = ?,
                    country = ?, barcode = ?, _updated_at = ? WHERE id = ?"#,
                params![
                    release.pressing.year,
                    release.pressing.format,
                    release.pressing.label,
                    release.pressing.catalog_number,
                    release.pressing.country,
                    release.pressing.barcode,
                    reg,
                    release_id,
                ],
            )?;

            // 3. Update tracks by existing ID.
            for (existing_id, new_track) in &track_updates {
                tx.execute(
                    r#"UPDATE tracks SET title = ?, side = ?, track_number = ?,
                        _updated_at = ? WHERE id = ?"#,
                    params![
                        new_track.title,
                        new_track.side,
                        new_track.track_number,
                        reg,
                        existing_id,
                    ],
                )?;
            }

            // 4. Replace album_artists.
            replace_album_artists(tx, &album_id, &album_artists, &reg, &now)?;

            // 5. Replace track_artists for the affected tracks.
            let track_ids: Vec<&str> = track_updates.iter().map(|(id, _)| id.as_str()).collect();
            replace_track_artists(tx, &track_ids, &track_artists, &reg, &now)?;

            Ok(())
        })
        .await
    }

    /// Get all albums, sorted by the given criteria.
    ///
    /// If `sort` is empty, defaults to `created_at DESC` (newest first).
    pub async fn get_albums(&self, sort: &[AlbumSortCriterion]) -> Result<Vec<DbAlbum>, DbError> {
        let (order_by, needs_artist_join) = build_order_by(sort, "a.created_at DESC");

        let artist_join = if needs_artist_join {
            "JOIN artists art_sort ON a.artist_id = art_sort.id"
        } else {
            ""
        };

        let query = format!(
            "SELECT \
                a.id, a.title, a.artist_id, a.year, a.primary_release_id, \
                a.is_compilation, \
                a.created_at \
            FROM albums a \
            {artist_join} \
            ORDER BY {order_by}"
        );

        self.call(move |conn| {
            let mut stmt = conn.prepare(&query)?;
            let rows = stmt.query_map([], |row| Ok(Self::row_to_album(row)))?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// Get a page of albums with LIMIT/OFFSET for lazy loading.
    pub async fn get_album_page(
        &self,
        sort: &[AlbumSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbAlbumSummary>, DbError> {
        let (order_by, needs_artist_sort_join) = build_order_by(sort, "a.created_at DESC");
        let artist_sort_join = album_summary_artist_join(needs_artist_sort_join);

        let query = format!(
            "{select} \
            FROM albums a \
            {artist_sort_join} \
            ORDER BY {order_by} \
            LIMIT ? OFFSET ?",
            select = ALBUM_SUMMARY_SELECT,
        );

        self.call(move |conn| {
            let mut stmt = conn.prepare(&query)?;
            let mut rows = stmt.query(params![limit as i64, offset as i64])?;
            let mut albums = Vec::new();
            while let Some(row) = rows.next()? {
                albums.push(parse_album_summary_row(row)?);
            }
            Ok(albums)
        })
        .await
    }

    /// Count total albums.
    pub async fn get_album_count(&self) -> Result<u64, DbError> {
        self.call(move |conn| {
            conn.query_row("SELECT COUNT(*) FROM albums", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|c| c as u64)
            .map_err(DbError::from)
        })
        .await
    }

    /// The SELECT for a release's storage summary (`DbReleaseStorageSummary`).
    /// `tail` is the trailing clause that differs per caller: an `ORDER BY` for
    /// the all-releases list, a `WHERE r.id = ?1` for a single-release lookup.
    fn release_storage_summary_query(tail: &str) -> String {
        format!(
            "SELECT \
            r.id AS release_id, \
            r.album_id, \
            a.title AS album_title, \
            r.format, \
            r.remote, \
            (SELECT rf.id FROM release_files rf WHERE rf.release_id = r.id LIMIT 1) AS any_file_id, \
            COALESCE( \
                a.primary_release_id, \
                (SELECT r2.id FROM releases r2 WHERE r2.album_id = a.id ORDER BY r2.created_at LIMIT 1) \
            ) AS primary_release_id, \
            (SELECT art_primary.name FROM artists art_primary WHERE art_primary.id = a.artist_id) \
            || COALESCE(( \
                SELECT ', ' || GROUP_CONCAT(ar.name, ', ') \
                FROM album_artists aa \
                JOIN artists ar ON ar.id = aa.artist_id \
                WHERE aa.album_id = a.id \
                ORDER BY aa.position \
            ), '') AS artist_names, \
            COALESCE(( \
                SELECT COUNT(*) FROM release_files rf WHERE rf.release_id = r.id \
            ), 0) AS file_count, \
            COALESCE(( \
                SELECT SUM(rf.file_size) FROM release_files rf WHERE rf.release_id = r.id \
            ), 0) AS total_size \
        FROM releases r \
        JOIN albums a ON a.id = r.album_id \
        {tail}"
        )
    }

    /// Map one row of [`release_storage_summary_query`] to a
    /// `DbReleaseStorageSummary`.
    fn row_to_release_storage_summary(
        row: &Row,
    ) -> coven::rusqlite::Result<DbReleaseStorageSummary> {
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

    pub async fn get_release_storage_summaries(
        &self,
    ) -> Result<Vec<DbReleaseStorageSummary>, DbError> {
        let query = Self::release_storage_summary_query("ORDER BY a.title, r.created_at");
        self.call(move |conn| {
            let mut stmt = conn.prepare(&query)?;
            let rows = stmt.query_map([], Self::row_to_release_storage_summary)?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// The storage summary for a single release, or `None` if it doesn't exist.
    /// Same shape as one row of `get_release_storage_summaries`; the download
    /// queue uses it at enqueue time to read a release's title / file count /
    /// total size for its Downloads-pane row.
    pub async fn find_release_storage_summary(
        &self,
        release_id: &str,
    ) -> Result<Option<DbReleaseStorageSummary>, DbError> {
        let release_id = release_id.to_string();
        let query = Self::release_storage_summary_query("WHERE r.id = ?1");
        self.call(move |conn| {
            conn.query_row(&query, [release_id], Self::row_to_release_storage_summary)
                .optional()
                .map_err(DbError::from)
        })
        .await
    }

    /// A representative file id for every remote release — one per release, or
    /// `None` for a remote release that has no files. The disconnect flow asks
    /// coven's cache whether each is pinned (kept offline) to count how many
    /// releases become unreachable when the cloud provider is removed; an unpinned
    /// remote release is reachable only through the cloud. Pin/unpin act on all a
    /// release's blobs together, so one file represents the release.
    pub async fn get_remote_release_file_ids(&self) -> Result<Vec<Option<String>>, DbError> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT (SELECT rf.id FROM release_files rf WHERE rf.release_id = r.id LIMIT 1) \
                     FROM releases r WHERE r.remote = 1",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, Option<String>>(0))?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// Raw album-summary lookup for a single album. Shares the JSON
    /// aggregates with `get_album_page` so the resolver output matches.
    pub async fn find_album_summary(
        &self,
        album_id: &str,
    ) -> Result<Option<DbAlbumSummary>, DbError> {
        let album_id = album_id.to_string();
        let query = format!("{ALBUM_SUMMARY_SELECT} FROM albums a WHERE a.id = ?");
        self.call(move |conn| {
            conn.query_row(&query, params![album_id], |row| {
                Ok(parse_album_summary_row(row))
            })
            .optional()?
            .transpose()
        })
        .await
    }

    /// Count storage rows matching `filter`. Mirrors the filter logic of
    /// `get_storage_page` so `total_count` matches the filtered page's
    /// universe.
    pub async fn get_storage_count(&self, filter: StorageFilter) -> Result<u64, DbError> {
        let where_clause = storage_filter_where(filter);
        let query = format!("SELECT COUNT(*) FROM releases r {where_clause}");

        self.call(move |conn| {
            conn.query_row(&query, [], |row| row.get::<_, i64>(0))
                .map(|c| c as u64)
                .map_err(DbError::from)
        })
        .await
    }

    /// Paginated storage-page query. Joins releases × albums × (optional)
    /// primary-artist sort table; both halves of the returned row are the
    /// raw aggregates the resolver maps to `ReleaseSummary` / `AlbumSummary`.
    pub async fn get_storage_page(
        &self,
        sort: &StorageSortCriterion,
        filter: StorageFilter,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbStorageRow>, DbError> {
        let (order_by, needs_artist_sort_join) = storage_order_by(sort);
        let artist_sort_join = if needs_artist_sort_join {
            "JOIN artists art_sort ON a.artist_id = art_sort.id"
        } else {
            ""
        };
        let where_clause = storage_filter_where(filter);

        let query = format!(
            "SELECT \
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
                a.id AS album_id_out, a.title, a.year, a.is_compilation, \
                a.primary_release_id, \
                (SELECT art_primary.name FROM artists art_primary WHERE art_primary.id = a.artist_id) \
                || COALESCE(( \
                    SELECT ', ' || GROUP_CONCAT(ar.name, ', ') \
                    FROM album_artists aa \
                    JOIN artists ar ON ar.id = aa.artist_id \
                    WHERE aa.album_id = a.id \
                    ORDER BY aa.position \
                ), '') AS artist_names, \
                COALESCE(( \
                    SELECT json_group_array(r2.id) \
                    FROM releases r2 \
                    WHERE r2.album_id = a.id \
                    ORDER BY r2.created_at \
                ), '[]') AS release_ids_json \
            FROM releases r \
            JOIN albums a ON a.id = r.album_id \
            {artist_sort_join} \
            {where_clause} \
            ORDER BY {order_by} \
            LIMIT ? OFFSET ?",
        );

        self.call(move |conn| {
            let mut stmt = conn.prepare(&query)?;
            let mut rows = stmt.query(params![limit as i64, offset as i64])?;
            let mut storage_rows = Vec::new();
            while let Some(row) = rows.next()? {
                let release = DbReleaseSummary {
                    id: row.get("release_id")?,
                    album_id: row.get("album_id")?,
                    format: row.get("release_format")?,
                    remote: row.get("remote")?,
                    any_file_id: row.get("any_file_id")?,
                    file_count: row.get("file_count")?,
                    total_size: row.get("total_size")?,
                };

                let release_ids_json: String = row.get("release_ids_json")?;
                let release_ids: Vec<String> =
                    serde_json::from_str(&release_ids_json).map_err(|e| DbError(e.to_string()))?;

                let album = DbAlbumSummary {
                    id: row.get("album_id_out")?,
                    title: row.get("title")?,
                    year: row.get("year")?,
                    is_compilation: row.get("is_compilation")?,
                    artist_names: row.get("artist_names")?,
                    release_ids,
                    primary_release_id: row.get("primary_release_id")?,
                };

                storage_rows.push(DbStorageRow { release, album });
            }
            Ok(storage_rows)
        })
        .await
    }

    /// Find album by ID. Caller-provided ID — may not exist.
    pub async fn find_album_by_id(&self, album_id: &str) -> Result<Option<DbAlbum>, DbError> {
        let album_id = album_id.to_string();
        self.call(move |conn| {
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
                |row| Ok(Self::row_to_album(row)),
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// Follow DbTrack.release_id -> DbRelease.
    /// FK navigation — row must exist. See method conventions above.
    pub async fn get_release_for_track(&self, track: &DbTrack) -> Result<DbRelease, DbError> {
        let release_id = track.release_id.clone();
        self.call(move |conn| {
            conn.query_row(
                "SELECT * FROM releases WHERE id = ?",
                params![release_id],
                |row| Ok(Self::row_to_release(row)),
            )
            .map_err(DbError::from)
        })
        .await
    }

    /// Follow DbRelease.album_id -> DbAlbum.
    /// FK navigation — row must exist. See method conventions above.
    pub async fn get_album_for_release(&self, release: &DbRelease) -> Result<DbAlbum, DbError> {
        let album_id = release.album_id.clone();
        self.call(move |conn| {
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
                |row| Ok(Self::row_to_album(row)),
            )
            .map_err(DbError::from)
        })
        .await
    }

    /// Get the raw album-detail aggregate: album + artists + releases
    /// (with per-release raw tracks, artists, and files). No formatting,
    /// no derivation. `LibraryManager` resolves this into `AlbumDetail`.
    pub async fn find_album_detail(
        &self,
        album_id: &str,
    ) -> Result<Option<DbAlbumDetail>, DbError> {
        let Some(album) = self.find_album_by_id(album_id).await? else {
            return Ok(None);
        };

        let artists = self.get_artists_for_album(album_id).await?;
        let db_releases = self.get_releases_for_album(album_id).await?;

        let mut releases = Vec::with_capacity(db_releases.len());
        for release in db_releases {
            releases.push(self.build_release_detail(release).await?);
        }

        Ok(Some(DbAlbumDetail {
            album,
            artists,
            releases,
        }))
    }

    /// Get the raw release-detail aggregate for a single release.
    /// `LibraryManager` resolves this into `ReleaseDetail`.
    pub async fn find_release_detail(
        &self,
        release_id: &str,
    ) -> Result<Option<DbReleaseDetail>, DbError> {
        let Some(release) = self.find_release_by_id(release_id).await? else {
            return Ok(None);
        };
        Ok(Some(self.build_release_detail(release).await?))
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

    /// Get all releases for an album
    pub async fn get_releases_for_album(&self, album_id: &str) -> Result<Vec<DbRelease>, DbError> {
        let album_id = album_id.to_string();
        self.call(move |conn| {
            let mut stmt =
                conn.prepare("SELECT * FROM releases WHERE album_id = ? ORDER BY created_at")?;
            let rows = stmt.query_map(params![album_id], |row| Ok(Self::row_to_release(row)))?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// Find track by ID. Caller-provided ID — may not exist.
    pub async fn find_track_by_id(&self, track_id: &str) -> Result<Option<DbTrack>, DbError> {
        let track_id = track_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT * FROM tracks WHERE id = ?",
                params![track_id],
                row_to_track,
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }
    /// Find album_id for a release. Caller-provided ID — may not exist.
    pub async fn find_album_id_for_release(
        &self,
        release_id: &str,
    ) -> Result<Option<String>, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT album_id FROM releases WHERE id = ?",
                params![release_id],
                |row| row.get::<_, String>("album_id"),
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// Enrich a list of queue entries with album/artist metadata for display.
    /// Returns one `QueueItem` per entry, in the same order, each carrying the
    /// entry's per-instance id. The same track queued twice resolves twice (the
    /// metadata is fetched once and joined onto every entry of that track).
    /// Entries whose track is not found are skipped.
    pub async fn get_queue_items(&self, entries: &[QueueEntry]) -> Result<Vec<QueueItem>, DbError> {
        if entries.is_empty() {
            return Ok(Vec::new());
        }

        let entries = entries.to_vec();
        self
            .call(move |conn| {
                let track_ids: Vec<String> =
                    entries.iter().map(|e| e.track_id.clone()).collect();
                let placeholders = track_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let query = format!(
                    "SELECT \
                        t.id AS track_id, \
                        t.title, \
                        t.duration_ms, \
                        a.title AS album_title, \
                        a.primary_release_id, \
                        COALESCE( \
                            NULLIF(( \
                                SELECT GROUP_CONCAT(art.name, ', ') \
                                FROM track_artists ta \
                                JOIN artists art ON art.id = ta.artist_id \
                                WHERE ta.track_id = t.id \
                                ORDER BY ta.position \
                            ), ''), \
                            (SELECT art_primary.name FROM artists art_primary WHERE art_primary.id = a.artist_id) \
                        ) AS artist_names \
                    FROM tracks t \
                    JOIN releases r ON r.id = t.release_id \
                    JOIN albums a ON a.id = r.album_id \
                    WHERE t.id IN ({placeholders})"
                );

                let mut stmt = conn.prepare(&query)?;
                let mut meta_by_track: HashMap<String, TrackQueueMeta> = HashMap::new();
                let mut rows = stmt
                    .query(coven::rusqlite::params_from_iter(track_ids.iter()))?;
                while let Some(row) = rows.next()? {
                    let track_id: String = row.get("track_id")?;
                    meta_by_track.insert(
                        track_id,
                        TrackQueueMeta {
                            title: row.get("title")?,
                            artist_names: row.get("artist_names")?,
                            duration_ms: row.get("duration_ms")?,
                            album_title: row.get("album_title")?,
                            cover_image_id: row.get("primary_release_id")?,
                        },
                    );
                }

                Ok(resolve_queue_entries(&meta_by_track, &entries))
            })
            .await
    }
    /// Get ordered track IDs for a release. Cheaper than `get_tracks_for_release`
    /// when callers only need IDs (queue building, play context).
    pub async fn get_track_ids_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<String>, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id FROM tracks WHERE release_id = ? ORDER BY side, track_number, id",
            )?;
            let rows = stmt.query_map(params![release_id], |row| row.get::<_, String>("id"))?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// Every track id in the library, in a deterministic base order (the same
    /// order across calls so a shuffle seed permutes a stable list). Ordered to
    /// match the per-release order — by release, then side, track number, id — so
    /// the library and a single release agree on what "source order" means.
    pub async fn get_all_track_ids(&self) -> Result<Vec<String>, DbError> {
        self.call(move |conn| {
            let mut stmt =
                conn.prepare("SELECT id FROM tracks ORDER BY release_id, side, track_number, id")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>("id"))?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// Return the subset of `track_ids` that exist in the tracks table.
    /// Ordering of returned IDs is unspecified; callers that need a
    /// specific order must re-derive it.
    pub async fn filter_existing_track_ids(
        &self,
        track_ids: &[String],
    ) -> Result<Vec<String>, DbError> {
        if track_ids.is_empty() {
            return Ok(Vec::new());
        }
        let track_ids = track_ids.to_vec();
        self.call(move |conn| {
            let placeholders = track_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let query = format!("SELECT id FROM tracks WHERE id IN ({placeholders})");
            let mut stmt = conn.prepare(&query)?;
            let rows = stmt
                .query_map(coven::rusqlite::params_from_iter(track_ids.iter()), |row| {
                    row.get::<_, String>("id")
                })?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// Get tracks for a release
    pub async fn get_tracks_for_release(&self, release_id: &str) -> Result<Vec<DbTrack>, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM tracks WHERE release_id = ? ORDER BY side, track_number, id",
            )?;
            let rows = stmt.query_map(params![release_id], row_to_track)?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }
    /// Insert a new file record
    pub async fn insert_file(&self, file: &DbFile) -> Result<(), DbError> {
        let file = file.clone();
        let reg = self.register_stamp().await?;
        self.call(move |conn| insert_file_row(conn, &file, &reg))
            .await
    }
    /// Get every audio-format row for a release, joined through its tracks.
    /// One row per track; a single-file CUE rip yields many rows sharing one
    /// `file_id`. The resolver groups them by `file_id` to describe each audio
    /// file's format.
    pub async fn get_audio_formats_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbAudioFormat>, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT af.* FROM audio_formats af \
                     JOIN tracks t ON t.id = af.track_id \
                     WHERE t.release_id = ?",
            )?;
            let rows = stmt.query_map(params![release_id], row_to_audio_format)?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// Get files for a release
    pub async fn get_files_for_release(&self, release_id: &str) -> Result<Vec<DbFile>, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare("SELECT * FROM release_files WHERE release_id = ?")?;
            let rows = stmt.query_map(params![release_id], |row| Ok(Self::row_to_file(row)))?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }
    /// Find file by ID. Caller-provided ID — may not exist.
    pub async fn find_file_by_id(&self, file_id: &str) -> Result<Option<DbFile>, DbError> {
        let file_id = file_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT * FROM release_files WHERE id = ?",
                params![file_id],
                |row| Ok(Self::row_to_file(row)),
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }
    /// Find audio format by track ID. Caller-provided ID — may not exist.
    pub async fn find_audio_format_by_track_id(
        &self,
        track_id: &str,
    ) -> Result<Option<DbAudioFormat>, DbError> {
        let track_id = track_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT * FROM audio_formats WHERE track_id = ?",
                params![track_id],
                row_to_audio_format,
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// All data needed to atomically finalize an import in a single transaction.
    /// Nothing is in the DB yet (except the import record and artists).
    #[allow(clippy::too_many_arguments)]
    pub async fn finalize_import_atomic(
        &self,
        // Album (None = existing album, already in DB)
        album: Option<&DbAlbum>,
        release: &DbRelease,
        tracks_to_files: &[crate::import::TrackFile],
        metadata: &[DbReleaseMetadata],
        track_artists: &[DbTrackArtist],
        album_artists: &[DbAlbumArtist],
        files: &[DbFile],
        audio_formats: &[DbAudioFormat],
        library_image: Option<(&DbLibraryImage, &[u8])>,
        primary_release_id: Option<(&str, &str)>, // (album_id, release_id)
        import_id: &str,
        identities: &[crate::import::ReleaseIdentity],
        // The in-place folder this import's files live in on this device. Every
        // import lands LOCAL, so each file is registered as a coven user-provided
        // external ref under this folder; a later make-Remote uploads from them
        // and drops the refs.
        local_path: &str,
        // The cloud home's storage mode, deciding the blob layout: `Opaque` keys
        // each blob by the hashed id, `Browsable` lays it out at a readable
        // `cloud_path` computed inside this transaction (ready when the gate flips).
        storage: crate::config::HomeStorage,
    ) -> Result<(), DbError> {
        let album = album.cloned();
        let release = release.clone();
        let tracks: Vec<DbTrack> = tracks_to_files
            .iter()
            .map(|tf| tf.db_track().clone())
            .collect();
        let metadata = metadata.to_vec();
        let track_artists = track_artists.to_vec();
        let album_artists = album_artists.to_vec();
        let files = files.to_vec();
        let audio_formats = audio_formats.to_vec();
        let library_image = library_image.map(|(image, bytes)| (image.clone(), bytes.to_vec()));
        let primary_release_id = primary_release_id.map(|(a, r)| (a.to_string(), r.to_string()));
        let import_id = import_id.to_string();
        let identities = identities.to_vec();
        let local_path = local_path.to_string();

        let now_dt = self.inner.clock.now();
        let now = now_dt.to_rfc3339();
        let now_ts = now_dt.timestamp();
        self.inner
            .handle
            .write(move |w| {
                if let Some((image, bytes)) = &library_image {
                    w.put_blob(
                        image_namespace(&image.image_type),
                        image.id.clone(),
                        bytes.clone(),
                    );
                }

                w.sql(move |sql| {
                    let tx = sql.connection();
                    // Every synced row this transaction inserts shares one HLC
                    // register stamp for `_updated_at`; wall-clock `now` stays
                    // for `created_at`.
                    let reg = sql.stamp();

                    // 1. Insert album (if new)
                    if let Some(album) = &album {
                        insert_album_row(tx, album, &reg)?;

                        // Album artists (only for new albums)
                        for aa in &album_artists {
                            insert_album_artist_row(tx, aa, &reg)?;
                        }
                    }

                    // 2. Insert release
                    insert_release_row(tx, &release, &reg)?;

                    // 2b. Insert per-source identity rows. Empty for Unknown
                    //     imports. `release_identities` is uniquely keyed on
                    //     `(release_id, source)`, so a release never carries two
                    //     rows for the same source.
                    for identity in &identities {
                        insert_release_identity_row(tx, &release.id, identity, &reg, &now)?;
                    }

                    // 3. Insert tracks. DbTracks live inside `tracks_to_files`;
                    //    their `duration_ms` was populated by the mapper from
                    //    the CUE sheet or a standalone-file probe.
                    for track in &tracks {
                        insert_track_row(tx, track, &reg)?;
                    }

                    // 4. Insert track artists
                    for ta in &track_artists {
                        insert_track_artist_row(tx, ta, &reg)?;
                    }

                    // 5. Insert release metadata
                    for meta in &metadata {
                        insert_release_metadata_row(tx, meta)?;
                    }

                    // 6. Insert files, and register each as a coven
                    //    user-provided external ref (the user's own file in
                    //    place). Every import lands Local — the files ARE the
                    //    user's files at `local_path`, tracked in coven's
                    //    `local_blob_refs` so the locality-aware read serves
                    //    them and a later make-Remote uploads from them and
                    //    drops the refs. On a browsable home the readable
                    //    cloud_path is computed now (the album/release rows
                    //    exist in this tx), so it is ready when the gate flips;
                    //    an opaque home leaves it NULL (coven hashes the id).
                    //    A populated key on a Local row is harmless.
                    for file in &files {
                        let cloud_path = if storage.is_browsable() {
                            Some(resolve_audio_cloud_path(
                                tx,
                                &file.release_id,
                                &file.original_filename,
                            )?)
                        } else {
                            None
                        };
                        let file = DbFile {
                            cloud_path,
                            ..file.clone()
                        };
                        insert_file_row(tx, &file, &reg)?;
                        let path = std::path::Path::new(&local_path).join(&file.original_filename);
                        register_external_blob_on(
                            tx,
                            &file.id,
                            crate::sync::RELEASE_FILES_NAMESPACE,
                            &path,
                            file.file_size as u64,
                        )?;
                    }

                    // 7. Insert audio formats
                    for af in &audio_formats {
                        insert_audio_format_row(tx, af, &reg)?;
                    }

                    // 8. Write the cover row and its host-provided blob in one
                    //    coven write. On a browsable home its readable cloud_path
                    //    (`{album}/{release}/cover.{ext}`) is computed now,
                    //    ready when the gate flips; an opaque home leaves it
                    //    NULL (hashed). The cover rides the release's gate, so a
                    //    Local release's cover stays private until it is made
                    //    Remote.
                    if let Some((image, _)) = &library_image {
                        let cloud_path = if storage.is_browsable() {
                            Some(resolve_cover_cloud_path(tx, &image.id, &image.content_type)?)
                        } else {
                            None
                        };
                        let image = DbLibraryImage {
                            cloud_path,
                            ..image.clone()
                        };
                        upsert_library_image_row(tx, &image, &reg)?;
                    }

                    // 9. Set album primary_release_id
                    if let Some((album_id, release_id)) = &primary_release_id {
                        tx.execute(
                            "UPDATE albums SET primary_release_id = ?, _updated_at = ? WHERE id = ?",
                            params![release_id, reg, album_id],
                        )?;
                    }

                    // 10. Link import to release and mark complete
                    tx.execute(
                        "UPDATE imports SET release_id = ?, status = ?, updated_at = ? WHERE id = ?",
                        params![
                            release.id,
                            ImportOperationStatus::Complete.as_str(),
                            now_ts,
                            import_id,
                        ],
                    )?;

                    Ok(())
                })?;
                Ok(())
            })
            .await
            .map_err(Self::coven_error)?;
        Ok(())
    }

    /// Delete a release by ID
    ///
    /// This will cascade delete all related records:
    /// - Tracks (via FOREIGN KEY ON DELETE CASCADE)
    /// - Files (via FOREIGN KEY ON DELETE CASCADE)
    /// - Track artists, audio formats (via FOREIGN KEY ON DELETE CASCADE)
    /// - Import records referencing this release (cleared before delete)
    pub async fn delete_release(&self, release_id: &str) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            let tx = conn;
            tx.execute(
                "UPDATE imports SET release_id = NULL WHERE release_id = ?",
                params![release_id],
            )?;
            tx.execute("DELETE FROM releases WHERE id = ?", params![release_id])?;
            Ok(())
        })
        .await
    }
    /// Delete an album by ID
    ///
    /// This will cascade delete all related records:
    /// - Releases (via FOREIGN KEY ON DELETE CASCADE)
    /// - Album artists (via FOREIGN KEY ON DELETE CASCADE)
    /// - Album discogs (via FOREIGN KEY ON DELETE CASCADE)
    /// - All tracks, files, etc. from releases (via cascading)
    /// - Import records referencing this album's releases (cleared before delete)
    pub async fn delete_album(&self, album_id: &str) -> Result<(), DbError> {
        let album_id = album_id.to_string();
        self
            .call(move |conn| {
                let tx = conn;
                tx.execute(
                    "UPDATE imports SET release_id = NULL WHERE release_id IN (SELECT id FROM releases WHERE album_id = ?)",
                    params![album_id],
                )?;
                tx.execute("DELETE FROM albums WHERE id = ?", params![album_id])?;
                Ok(())
            })
            .await
    }
    /// Insert one or more `release_identities` rows for an existing
    /// release. Idempotent at the PK (release_id, source) — duplicates
    /// surface as unique-violation errors. Used for setting identity
    /// outside of the atomic import path.
    pub async fn insert_release_identities(
        &self,
        release_id: &str,
        identities: &[crate::import::ReleaseIdentity],
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let identities = identities.to_vec();
        let now = self.inner.clock.now().to_rfc3339();
        let reg = self.register_stamp().await?;
        self.call(move |conn| {
            for identity in &identities {
                insert_release_identity_row(conn, &release_id, identity, &reg, &now)?;
            }
            Ok(())
        })
        .await
    }

    /// All identity rows for a release. Empty if the release has no
    /// `release_identities` rows (Unknown identity).
    pub async fn get_release_identities(
        &self,
        release_id: &str,
    ) -> Result<Vec<crate::import::ReleaseIdentity>, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
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
        })
        .await
    }

    /// Cached `release_metadata` rows for a release, keyed by `source`.
    ///
    /// Each row's `source` discriminates between the editorial source
    /// payload (`'musicbrainz'` / `'discogs'`) and supporting payloads
    /// captured at import (`'discogs_master'`, `'musicbrainz_release_group'`).
    /// `reset_metadata_to_source` reads these to replay the seeding
    /// projection without re-fetching from the network.
    pub async fn get_release_metadata_by_source(
        &self,
        release_id: &str,
    ) -> Result<HashMap<String, String>, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            let mut stmt =
                conn.prepare("SELECT source, json FROM release_metadata WHERE release_id = ?")?;
            let rows = stmt.query_map(params![release_id], |row| {
                Ok((
                    row.get::<_, String>("source")?,
                    row.get::<_, String>("json")?,
                ))
            })?;
            rows.collect::<coven::rusqlite::Result<HashMap<_, _>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// Insert a single `release_metadata` row. Used by tests to seed cached
    /// payloads without going through the full import pipeline.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_release_metadata(&self, meta: &DbReleaseMetadata) -> Result<(), DbError> {
        let meta = meta.clone();
        self.call(move |conn| insert_release_metadata_row(conn, &meta))
            .await
    }

    /// Look up an album by Exact `release_identities` rows. Returns the
    /// first album that has a release with an identity row matching any
    /// of `identities` on `(source, source_release_id)`. Approximate
    /// identities (`source_release_id = None`) are ignored — they're
    /// group-only claims, not pressing-level claims.
    ///
    /// Used for the per-pressing rejection step of import dedup: a
    /// duplicate is a release whose identity points at a specific
    /// pressing already in the library.
    pub async fn find_album_by_identity_release(
        &self,
        identities: &[crate::import::ReleaseIdentity],
    ) -> Result<Option<DbAlbum>, DbError> {
        let exact_pairs: Vec<(String, String)> = identities
            .iter()
            .filter_map(|id| {
                id.source_release_id
                    .as_deref()
                    .map(|rid| (id.source.as_str().to_string(), rid.to_string()))
            })
            .collect();
        if exact_pairs.is_empty() {
            return Ok(None);
        }

        self.call(move |conn| {
            let placeholders = exact_pairs
                .iter()
                .map(|_| "(?, ?)")
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                r#"
                    SELECT
                        a.id, a.title, a.artist_id, a.year, a.primary_release_id,
                        a.is_compilation, a.created_at
                    FROM albums a
                    JOIN releases r ON r.album_id = a.id
                    JOIN release_identities ri ON ri.release_id = r.id
                    WHERE (ri.source, ri.source_release_id) IN ({placeholders})
                    LIMIT 1
                    "#,
            );
            let mut binds: Vec<&str> = Vec::with_capacity(exact_pairs.len() * 2);
            for (source, release_id) in &exact_pairs {
                binds.push(source);
                binds.push(release_id);
            }
            conn.query_row(
                &sql,
                coven::rusqlite::params_from_iter(binds.iter()),
                |row| Ok(Self::row_to_album(row)),
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// Look up an album by `release_identities` group rows. Returns the
    /// first album that has a release with an identity row matching any
    /// of `identities` on `(source, source_group_id)`. Used for the
    /// cross-source merge step of import dedup.
    pub async fn find_album_by_identity_group(
        &self,
        identities: &[crate::import::ReleaseIdentity],
    ) -> Result<Option<String>, DbError> {
        if identities.is_empty() {
            return Ok(None);
        }

        let pairs: Vec<(String, String)> = identities
            .iter()
            .map(|id| (id.source.as_str().to_string(), id.source_group_id.clone()))
            .collect();

        self.call(move |conn| {
            let placeholders = pairs
                .iter()
                .map(|_| "(?, ?)")
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                r#"
                    SELECT r.album_id
                    FROM releases r
                    JOIN release_identities ri ON ri.release_id = r.id
                    WHERE (ri.source, ri.source_group_id) IN ({placeholders})
                    LIMIT 1
                    "#,
            );
            let mut binds: Vec<&str> = Vec::with_capacity(pairs.len() * 2);
            for (source, group_id) in &pairs {
                binds.push(source);
                binds.push(group_id);
            }
            conn.query_row(
                &sql,
                coven::rusqlite::params_from_iter(binds.iter()),
                |row| row.get::<_, String>("album_id"),
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// Same as `find_album_by_identity_group`, but ignores rows belonging to
    /// `exclude_release_id`. Used by `set_identity` to look for an album
    /// the release would fit into without matching against the release's
    /// own existing (about-to-be-replaced) identity rows.
    pub async fn find_album_by_identity_group_excluding(
        &self,
        identities: &[crate::import::ReleaseIdentity],
        exclude_release_id: &str,
    ) -> Result<Option<String>, DbError> {
        if identities.is_empty() {
            return Ok(None);
        }

        let pairs: Vec<(String, String)> = identities
            .iter()
            .map(|id| (id.source.as_str().to_string(), id.source_group_id.clone()))
            .collect();
        let exclude_release_id = exclude_release_id.to_string();

        self.call(move |conn| {
            let placeholders = pairs
                .iter()
                .map(|_| "(?, ?)")
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                r#"
                    SELECT r.album_id
                    FROM releases r
                    JOIN release_identities ri ON ri.release_id = r.id
                    WHERE (ri.source, ri.source_group_id) IN ({placeholders})
                      AND ri.release_id != ?
                    LIMIT 1
                    "#,
            );
            let mut binds: Vec<&str> = Vec::with_capacity(pairs.len() * 2 + 1);
            for (source, group_id) in &pairs {
                binds.push(source);
                binds.push(group_id);
            }
            binds.push(&exclude_release_id);
            conn.query_row(
                &sql,
                coven::rusqlite::params_from_iter(binds.iter()),
                |row| row.get::<_, String>("album_id"),
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// Replace `release_identities` rows for `release_id`, update the
    /// release's `metadata_source` / `metadata_source_release_id`, and
    /// move the release between albums when the target differs from the
    /// source.
    ///
    /// Everything below runs in one transaction:
    ///
    /// 1. INSERT the destination album when `new_album` is `Some`,
    ///    plus copies of `current_album_id`'s `album_artists` rows
    ///    (so a fresh album lands fully populated, not a bare row that
    ///    drops the artist links the source already had).
    /// 2. Replace `release_identities` for `release_id`.
    /// 3. UPDATE the release's `album_id` and metadata-source columns.
    /// 4. If the release vacated `current_album_id` (the source), check
    ///    inside the transaction whether any releases remain. None →
    ///    delete the source album. Some → repair `primary_release_id`
    ///    if it pointed at the moved release.
    ///
    /// The post-move recheck on `current_album_id` closes a TOCTOU
    /// window: a separate writer could have inserted a release into the
    /// source between the manager's pre-flight read and this
    /// transaction. Deciding inside the same transaction prevents the
    /// cascade-delete from removing freshly-arrived releases.
    ///
    /// Metadata columns (pressing fields, album fields, tracks) are
    /// deliberately untouched. Caller decides whether to reseed the
    /// metadata.
    ///
    /// Returns `SetIdentityOutcome::source_album_deleted` so the caller
    /// knows whether to emit `AlbumRemoved` or `AlbumUpdated` for the
    /// source.
    #[allow(clippy::too_many_arguments)]
    pub async fn set_identity_atomic(
        &self,
        release_id: &str,
        new_identities: &[crate::import::ReleaseIdentity],
        new_metadata_source: crate::db::ReleaseMetadataSource,
        new_metadata_source_release_id: Option<&str>,
        current_album_id: &str,
        target_album_id: &str,
        new_album: Option<&DbAlbum>,
        new_metadata: &[DbReleaseMetadata],
    ) -> Result<SetIdentityOutcome, DbError> {
        let release_id = release_id.to_string();
        let new_identities = new_identities.to_vec();
        let new_metadata_source = new_metadata_source.as_str().to_string();
        let new_metadata_source_release_id = new_metadata_source_release_id.map(str::to_string);
        let current_album_id = current_album_id.to_string();
        let target_album_id = target_album_id.to_string();
        let new_album = new_album.cloned();
        let new_metadata = new_metadata.to_vec();
        let now = self.inner.clock.now().to_rfc3339();
        // One HLC register stamp for every synced row this transaction touches.
        let reg = self.register_stamp().await?;

        self
            .call(move |conn| {
                let tx = conn;

                // 1. Insert the destination album (if brand-new). Must come
                //    before the release UPDATE so the FK on `releases.album_id`
                //    points at an existing row.
                if let Some(album) = &new_album {
                    insert_album_row(tx, album, &reg)?;

                    // Copy album_artists from the source. Each row gets a fresh
                    // PK (generated in Rust to match the rest of the codebase)
                    // and is rebound to the new album. The UNIQUE(album_id,
                    // artist_id) constraint is satisfied because we're inserting
                    // into a different album. If the source is about to be
                    // deleted (sole release moved), the SELECT still sees the
                    // source rows because the DELETE happens later in the same
                    // transaction.
                    let source_artists: Vec<(String, i32)> = {
                        let mut stmt = tx
                            .prepare(
                                "SELECT artist_id, position FROM album_artists \
                                 WHERE album_id = ? ORDER BY position",
                            )?;
                        let rows = stmt
                            .query_map(params![current_album_id], |row| {
                                Ok((row.get::<_, String>("artist_id")?, row.get::<_, i32>("position")?))
                            })?;
                        rows.collect::<coven::rusqlite::Result<Vec<_>>>()?
                    };
                    for (artist_id, position) in source_artists {
                        tx.execute(
                            r#"
                            INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
                            VALUES (?, ?, ?, ?, ?, ?)
                            "#,
                            params![
                                uuid::Uuid::new_v4().to_string(),
                                album.id,
                                artist_id,
                                position,
                                reg,
                                now,
                            ],
                        )?;
                    }
                }

                // 2. Replace identity rows.
                tx.execute(
                    "DELETE FROM release_identities WHERE release_id = ?",
                    params![release_id],
                )?;
                for identity in &new_identities {
                    insert_release_identity_row(tx, &release_id, identity, &reg, &now)?;
                }

                // 3. Update release: album, metadata source.
                tx.execute(
                    r#"
                    UPDATE releases SET
                        album_id = ?,
                        metadata_source = ?,
                        metadata_source_release_id = ?,
                        _updated_at = ?
                    WHERE id = ?
                    "#,
                    params![
                        target_album_id,
                        new_metadata_source,
                        new_metadata_source_release_id,
                        reg,
                        release_id,
                    ],
                )?;

                // 4. Replace cached source payload. Always wipe first — Unknown
                //    drops to file_tags (`new_metadata` empty) and the prior
                //    MB/Discogs JSON has no business sticking around. For
                //    Exact/Approximate, the caller hands us the freshly-fetched
                //    payload (matching the new `metadata_source_release_id`) so
                //    a later re-projection can replay the seed without
                //    divergence.
                tx.execute(
                    "DELETE FROM release_metadata WHERE release_id = ?",
                    params![release_id],
                )?;
                for meta in &new_metadata {
                    tx.execute(
                        r#"
                        INSERT INTO release_metadata (id, release_id, source, json, fetched_at)
                        VALUES (?, ?, ?, ?, ?)
                        "#,
                        params![
                            meta.id,
                            release_id,
                            meta.source,
                            meta.json,
                            meta.fetched_at.to_rfc3339(),
                        ],
                    )?;
                }

                // 5. Source-album cleanup. Only runs when the release actually
                //    moved; same-album updates don't vacate anything.
                let mut source_album_deleted = false;
                if target_album_id != current_album_id {
                    // Recheck inside the transaction: how many releases does the
                    // source album hold now (after the UPDATE above)?
                    let remaining: i64 = tx
                        .query_row(
                            "SELECT COUNT(*) FROM releases WHERE album_id = ?",
                            params![current_album_id],
                            |row| row.get(0),
                        )?;

                    if remaining == 0 {
                        // No releases left → delete the album. There can be no
                        // imports to clear because the only way `releases` is
                        // empty is if every prior release left the album, and
                        // imports reference releases (not albums) — moving a
                        // release elsewhere keeps the import row pointing at the
                        // same release, just with a different `album_id`.
                        tx.execute(
                            "DELETE FROM albums WHERE id = ?",
                            params![current_album_id],
                        )?;
                        source_album_deleted = true;
                    } else {
                        // Album survives. If its `primary_release_id` pointed at
                        // the moved release, repoint it at the oldest remaining
                        // release (matching the "first release" fallback used
                        // elsewhere in the read path).
                        let dangling: Option<String> = tx
                            .query_row(
                                "SELECT primary_release_id FROM albums \
                                 WHERE id = ? AND primary_release_id = ?",
                                params![current_album_id, release_id],
                                |row| row.get::<_, Option<String>>(0),
                            )
                            .optional()?
                            .flatten();

                        if dangling.is_some() {
                            let new_primary: Option<String> = tx
                                .query_row(
                                    "SELECT id FROM releases \
                                     WHERE album_id = ? \
                                     ORDER BY created_at ASC, id ASC \
                                     LIMIT 1",
                                    params![current_album_id],
                                    |row| row.get::<_, String>(0),
                                )
                                .optional()?;

                            tx.execute(
                                "UPDATE albums SET primary_release_id = ?, _updated_at = ? \
                                 WHERE id = ?",
                                params![new_primary, reg, current_album_id],
                            )?;
                        }
                    }
                }

                Ok(SetIdentityOutcome {
                    source_album_deleted,
                })
            })
            .await
    }

    /// Upsert a library image record
    pub async fn upsert_library_image(&self, image: &DbLibraryImage) -> Result<(), DbError> {
        let image = image.clone();
        let reg = self.register_stamp().await?;
        self.call(move |conn| upsert_library_image_row(conn, &image, &reg))
            .await
    }

    /// Write a host-provided image blob and its `covers`/`artist_images` row as
    /// one coven batch.
    pub async fn write_library_image_blob(
        &self,
        image: &DbLibraryImage,
        bytes: &[u8],
    ) -> Result<(), DbError> {
        let image = image.clone();
        let namespace = image_namespace(&image.image_type).to_string();
        let id = image.id.clone();
        let bytes = bytes.to_vec();
        self.inner
            .handle
            .write(move |w| {
                w.put_blob(namespace, id, bytes);
                w.sql(move |sql| {
                    let reg = sql.stamp();
                    upsert_library_image_row(sql.connection(), &image, &reg)
                        .map_err(CovenError::from)
                })?;
                Ok(())
            })
            .await
            .map_err(Self::coven_error)
    }

    /// Find a host-provided image (cover / artist image) by its subject id. The
    /// `image_type` selects the table (`covers` / `artist_images`); the id is the
    /// release/artist id. Caller-provided id — may not exist.
    pub async fn find_library_image(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<DbLibraryImage>, DbError> {
        let id = id.to_string();
        let image_type = image_type.clone();
        let table = image_table(&image_type);
        let sql = format!("SELECT * FROM {table} WHERE id = ?");
        self.call(move |conn| {
            conn.query_row(&sql, params![id], |row| {
                Ok(row_to_library_image(row, image_type.clone()))
            })
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// The `_updated_at` version of each given release's `covers` row, for the ids
    /// that have one. The version a cover [`ImageRef`](crate::album_detail::ImageRef)
    /// carries: it moves when the cover bytes change (the upsert bumps it), so the
    /// UI's `(id, version)` cache key and the `AlbumUpdated` re-render fire. Ids
    /// with no cover row are absent from the map.
    pub async fn cover_versions(
        &self,
        release_ids: &[String],
    ) -> Result<HashMap<String, String>, DbError> {
        if release_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let ids = release_ids.to_vec();
        self.call(move |conn| {
            let placeholders = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!("SELECT id, _updated_at FROM covers WHERE id IN ({placeholders})");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(coven::rusqlite::params_from_iter(ids.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut map = HashMap::new();
            for row in rows {
                let (id, version) = row?;
                map.insert(id, version);
            }
            Ok(map)
        })
        .await
    }

    /// The `_updated_at` version of one release's `covers` row, or `None` when it
    /// has no cover. The single-id form of [`cover_versions`](Self::cover_versions).
    pub async fn cover_version(&self, release_id: &str) -> Result<Option<String>, DbError> {
        let ids = [release_id.to_string()];
        Ok(self.cover_versions(&ids).await?.remove(release_id))
    }

    /// Delete a host-provided image row by its subject id, from the table its type
    /// selects. (The row is also cascade-deleted with its subject; this is the
    /// explicit path, e.g. replacing a cover.)
    pub async fn delete_library_image(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<(), DbError> {
        let id = id.to_string();
        let table = image_table(image_type);
        let sql = format!("DELETE FROM {table} WHERE id = ?");
        self.call(move |conn| {
            conn.execute(&sql, params![id])
                .map(|_| ())
                .map_err(DbError::from)
        })
        .await
    }

    /// Update album's primary_release_id
    pub async fn set_album_primary_release(
        &self,
        album_id: &str,
        primary_release_id: &str,
    ) -> Result<(), DbError> {
        let (album_id, primary_release_id) = (album_id.to_string(), primary_release_id.to_string());
        let reg = self.register_stamp().await?;
        self.call(move |conn| {
            conn.execute(
                "UPDATE albums SET primary_release_id = ?, _updated_at = ? WHERE id = ?",
                params![primary_release_id, reg, album_id],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Clear an album's primary_release_id so summary queries fall back
    /// to the first surviving release. Called after deleting the release
    /// that primary_release_id pointed at.
    pub async fn clear_album_primary_release(&self, album_id: &str) -> Result<(), DbError> {
        let album_id = album_id.to_string();
        let reg = self.register_stamp().await?;
        self.call(move |conn| {
            conn.execute(
                "UPDATE albums SET primary_release_id = NULL, _updated_at = ? WHERE id = ?",
                params![reg, album_id],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }
    /// Find release by ID. Caller-provided ID — may not exist.
    pub async fn find_release_by_id(&self, release_id: &str) -> Result<Option<DbRelease>, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT * FROM releases WHERE id = ?",
                params![release_id],
                |row| Ok(Self::row_to_release(row)),
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// Write the single device-local `playback_state` row (id = 'current'),
    /// replacing any existing one. Never synced.
    pub async fn save_playback_state(&self, state: &DbPlaybackState) -> Result<(), DbError> {
        let state = state.clone();
        self.call(move |conn| {
            // Flatten the context substruct back to the table's three
            // nullable columns: all three NULL when no context is playing.
            let (source, shuffle_seed, cursor) = match &state.context {
                Some(ctx) => (Some(&ctx.source), ctx.shuffle_seed, Some(ctx.cursor)),
                None => (None, None, None),
            };
            conn.execute(
                "INSERT OR REPLACE INTO playback_state \
                     (id, source, shuffle_seed, cursor, manual, repeat, \
                      current_track_id, position_ms, volume, is_muted) \
                     VALUES ('current', ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    source,
                    shuffle_seed,
                    cursor,
                    state.manual,
                    state.repeat,
                    state.current_track_id,
                    state.position_ms,
                    state.volume,
                    state.is_muted,
                ],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Read the device-local `playback_state` row, or `None` if none is stored
    /// (or if the row is corrupt — the resume cache is discarded at this
    /// boundary so no caller downstream sees a malformed context).
    pub async fn load_playback_state(&self) -> Result<Option<DbPlaybackState>, DbError> {
        self.call(move |conn| {
            // The closure yields `Option<DbPlaybackState>`: `None` is a corrupt
            // row that discards the whole cache, distinct from the outer `None`
            // for no row at all. The outer `.optional()` then flattens both to
            // a single "no resume state" answer.
            conn.query_row(
                "SELECT source, shuffle_seed, cursor, manual, repeat, \
                     current_track_id, position_ms, volume, is_muted \
                     FROM playback_state WHERE id = 'current'",
                [],
                |row| {
                    // `source` and `cursor` are written together (with
                    // `shuffle_seed`, NULL = sequential): both present is a
                    // context, both absent is none, exactly one present is a
                    // corrupt row.
                    let source: Option<String> = row.get("source")?;
                    let shuffle_seed: Option<i64> = row.get("shuffle_seed")?;
                    let cursor: Option<i64> = row.get("cursor")?;
                    let context = match (source, cursor) {
                        (Some(source), Some(cursor)) => Some(DbPlaybackContext {
                            source,
                            shuffle_seed,
                            cursor,
                        }),
                        (None, None) => None,
                        (Some(source), None) => {
                            warn!(
                                "discarding the playback resume cache: source {source:?} \
                                     present but cursor is NULL"
                            );
                            return Ok(None);
                        }
                        (None, Some(cursor)) => {
                            warn!(
                                "discarding the playback resume cache: cursor {cursor} \
                                     present but source is NULL"
                            );
                            return Ok(None);
                        }
                    };
                    Ok(Some(DbPlaybackState {
                        context,
                        manual: row.get("manual")?,
                        repeat: row.get("repeat")?,
                        current_track_id: row.get("current_track_id")?,
                        position_ms: row.get("position_ms")?,
                        volume: row.get("volume")?,
                        is_muted: row.get("is_muted")?,
                    }))
                },
            )
            .optional()
            .map(Option::flatten)
            .map_err(DbError::from)
        })
        .await
    }

    /// Delete the device-local `playback_state` row (playback stopped).
    pub async fn clear_playback_state(&self) -> Result<(), DbError> {
        self.call(move |conn| {
            conn.execute("DELETE FROM playback_state", [])
                .map(|_| ())
                .map_err(DbError::from)
        })
        .await
    }

    /// Test-only: flip a release's `remote` gate column directly (bumping
    /// `_updated_at`). Production flips it through coven's transitions; tests that
    /// only need a release in a given storage state set it here.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn set_remote_for_test(&self, release_id: &str, remote: bool) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let reg = self.register_stamp().await?;
        self.call(move |conn| {
            conn.execute(
                "UPDATE releases SET remote = ?, _updated_at = ? WHERE id = ?",
                params![remote, reg, release_id],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Test-only: register each of a release's files as a coven user-provided
    /// external ref under `folder` (the in-place files of a Local release), the
    /// new-model equivalent of the removed `release_local_source` upsert. Call
    /// after the file rows are inserted.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn register_release_external_refs_for_test(
        &self,
        release_id: &str,
        folder: &str,
    ) -> Result<(), DbError> {
        let files = self.get_files_for_release(release_id).await?;
        for file in &files {
            let path = std::path::Path::new(folder).join(&file.original_filename);
            self.register_external_blob(
                &file.id,
                crate::sync::RELEASE_FILES_NAMESPACE,
                &path,
                file.file_size as u64,
            )
            .await?;
        }
        Ok(())
    }

    /// Get the release that owns a given file.
    pub async fn find_release_for_file(&self, file_id: &str) -> Result<Option<DbRelease>, DbError> {
        let file_id = file_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT r.* FROM releases r \
                     JOIN release_files rf ON rf.release_id = r.id \
                     WHERE rf.id = ?",
                params![file_id],
                |row| Ok(Self::row_to_release(row)),
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// Count pending upload outbox entries for files belonging to a release.
    pub async fn count_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> Result<i64, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM cloud_outbox co \
                     JOIN release_files rf ON rf.id = co.file_id \
                     WHERE rf.release_id = ? AND co.operation = 'upload'",
                params![release_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(DbError::from)
        })
        .await
    }

    /// Check if any upload outbox entries remain for files belonging to a release.
    pub async fn has_pending_uploads_for_release(&self, release_id: &str) -> Result<bool, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT 1 FROM cloud_outbox co \
                     JOIN release_files rf ON rf.id = co.file_id \
                     WHERE rf.release_id = ? AND co.operation = 'upload' \
                     LIMIT 1",
                params![release_id],
                |_| Ok(()),
            )
            .optional()
            .map(|o| o.is_some())
            .map_err(DbError::from)
        })
        .await
    }

    /// Insert a new import operation record
    pub async fn insert_import(&self, import: &DbImport) -> Result<(), DbError> {
        let import = import.clone();
        self.call(move |conn| {
            conn.execute(
                r#"
                    INSERT INTO imports (
                        id, status, release_id, album_title, artist_name,
                        folder_path, created_at, updated_at, error_message
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                params![
                    import.id,
                    import.status.as_str(),
                    import.release_id,
                    import.album_title,
                    import.artist_name,
                    import.folder_path,
                    import.created_at,
                    import.updated_at,
                    import.error_message,
                ],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }
    /// Find import by ID. Caller-provided ID — may not exist.
    pub async fn find_import_by_id(&self, id: &str) -> Result<Option<DbImport>, DbError> {
        let id = id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT * FROM imports WHERE id = ?",
                params![id],
                row_to_import,
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }
    /// Get all active (non-complete, non-failed) imports
    pub async fn get_active_imports(&self) -> Result<Vec<DbImport>, DbError> {
        self
            .call(move |conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT * FROM imports WHERE status IN ('preparing', 'importing') ORDER BY created_at DESC",
                    )?;
                let rows = stmt.query_map([], row_to_import)?;
                rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                    .map_err(DbError::from)
            })
            .await
    }
    /// Update import status
    pub async fn update_import_status(
        &self,
        id: &str,
        status: ImportOperationStatus,
    ) -> Result<(), DbError> {
        let id = id.to_string();
        let status = status.as_str().to_string();
        let now = self.inner.clock.now().timestamp();
        self.call(move |conn| {
            conn.execute(
                "UPDATE imports SET status = ?, updated_at = ? WHERE id = ?",
                params![status, now, id],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }
    /// Update import with error message and set status to Failed
    pub async fn update_import_error(&self, id: &str, error: &str) -> Result<(), DbError> {
        let (id, error) = (id.to_string(), error.to_string());
        let now = self.inner.clock.now().timestamp();
        self.call(move |conn| {
            conn.execute(
                "UPDATE imports SET status = ?, error_message = ?, updated_at = ? WHERE id = ?",
                params![ImportOperationStatus::Failed.as_str(), error, now, id],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }
    /// Delete an import record from the database.
    /// Used by UI to dismiss stuck imports so they don't reappear after restart.
    pub async fn delete_import(&self, id: &str) -> Result<(), DbError> {
        let id = id.to_string();
        self.call(move |conn| {
            conn.execute("DELETE FROM imports WHERE id = ?", params![id])
                .map(|_| ())
                .map_err(DbError::from)
        })
        .await
    }

    /// Whether a release's source folder name was already imported.
    /// Used for duplicate detection when scanning folders.
    pub async fn is_source_folder_name_imported(&self, name: &str) -> Result<bool, DbError> {
        let name = name.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT 1 FROM releases WHERE source_folder_name = ? LIMIT 1",
                params![name],
                |_| Ok(()),
            )
            .optional()
            .map(|o| o.is_some())
            .map_err(DbError::from)
        })
        .await
    }

    /// Release ids whose stored `content_hash` equals `hash`. Normally zero or
    /// one (the import overwrite path keeps the hash unique), but returns all
    /// matches so a re-import sweeps any pre-existing duplicates.
    pub async fn release_ids_for_content_hash(&self, hash: &str) -> Result<Vec<String>, DbError> {
        let hash = hash.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare("SELECT id FROM releases WHERE content_hash = ?")?;
            let ids = stmt
                .query_map(params![hash], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ids)
        })
        .await
    }

    /// The album id of a release stored from the file structure that hashes to
    /// `hash`, or `None` when no release carries that content hash. The import
    /// Whether some release in the library was imported from this exact file
    /// structure (its `content_hash` matches `hash`). The import view uses this
    /// to mark a scanned folder as already added.
    pub async fn is_content_hash_imported(&self, hash: &str) -> Result<bool, DbError> {
        let hash = hash.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT 1 FROM releases WHERE content_hash = ? LIMIT 1",
                params![hash],
                |_| Ok(()),
            )
            .optional()
            .map(|o| o.is_some())
            .map_err(DbError::from)
        })
        .await
    }

    /// Check, for each candidate in `checks`, whether the library already
    /// holds the same pressing or the same album (group). Drives the
    /// "in library" badges shown in the identify-pipeline result lists.
    ///
    /// Per check:
    ///
    /// - `release_in_library` is true when a `release_identities` row
    ///   matches `(check.source, check.release_id)` — i.e. an Exact
    ///   identity at this specific pressing.
    /// - `album_in_library` is true when a `release_identities` row
    ///   matches `(check.source, check.source_group_id)` — i.e. some
    ///   release in the library shares the candidate's group identity.
    ///
    /// `album_title` / `album_id` carry the matched album's display
    /// info. When both flags are true, they describe the album holding
    /// the matching pressing; when only `album_in_library` is true,
    /// they describe the album holding a different release in the same
    /// group.
    pub async fn check_releases_in_library(
        &self,
        checks: &[super::models::LibraryCheck],
    ) -> Result<Vec<super::models::LibraryStatus>, DbError> {
        // Translate each check into the (source, release_id, group_id) inputs the
        // closure binds — `LibraryCheck` isn't `Send + 'static`-friendly through
        // the closure boundary, so carry plain strings.
        let checks: Vec<(String, String, Option<String>)> = checks
            .iter()
            .map(|c| {
                (
                    c.source.as_str().to_string(),
                    c.release_id.clone(),
                    c.source_group_id.clone(),
                )
            })
            .collect();

        self.call(move |conn| {
            let mut statuses = Vec::with_capacity(checks.len());

            for (source, release_id, group_id) in &checks {
                let mut release_in_library = false;
                let mut album_in_library = false;
                let mut album_title: Option<String> = None;
                let mut album_id: Option<String> = None;

                // Per-pressing match — exact identity at the specific release.
                let row = conn
                    .query_row(
                        r#"
                            SELECT
                                a.id, a.title, a.artist_id, a.year, a.primary_release_id,
                                a.is_compilation, a.created_at
                            FROM albums a
                            JOIN releases r ON r.album_id = a.id
                            JOIN release_identities ri ON ri.release_id = r.id
                            WHERE ri.source = ? AND ri.source_release_id = ?
                            LIMIT 1
                            "#,
                        params![source, release_id],
                        |row| Ok(Self::row_to_album(row)),
                    )
                    .optional()?;

                if let Some(album) = row {
                    release_in_library = true;
                    album_in_library = true;
                    album_title = Some(album.title);
                    album_id = Some(album.id);
                } else if let Some(group_id) = group_id {
                    // Album-level match — any release in the library shares
                    // the candidate's group identity.
                    let row = conn
                        .query_row(
                            r#"
                                SELECT
                                    a.id, a.title, a.artist_id, a.year, a.primary_release_id,
                                    a.is_compilation, a.created_at
                                FROM albums a
                                JOIN releases r ON r.album_id = a.id
                                JOIN release_identities ri ON ri.release_id = r.id
                                WHERE ri.source = ? AND ri.source_group_id = ?
                                LIMIT 1
                                "#,
                            params![source, group_id],
                            |row| Ok(Self::row_to_album(row)),
                        )
                        .optional()?;

                    if let Some(album) = row {
                        album_in_library = true;
                        album_title = Some(album.title);
                        album_id = Some(album.id);
                    }
                }

                statuses.push(super::models::LibraryStatus {
                    release_id: release_id.clone(),
                    release_in_library,
                    album_in_library,
                    album_title,
                    album_id,
                });
            }

            Ok(statuses)
        })
        .await
    }

    // ---- Readable cloud paths (browsable homes) ----

    /// Run a cloud-key resolver, but only on a browsable home: an opaque home
    /// keys blobs by hashed id and has no stored `cloud_path`, so it
    /// short-circuits to `Ok(None)`. The resolver runs on the owned coven
    /// connection (where it reads the release's album id). The two
    /// release-scoped `*_cloud_path_for_storage` accessors differ only in which
    /// resolver they pass.
    async fn cloud_path_if_browsable<F>(
        &self,
        storage: crate::config::HomeStorage,
        resolve: F,
    ) -> Result<Option<String>, DbError>
    where
        F: FnOnce(&Connection) -> Result<String, DbError> + Send + 'static,
    {
        if !storage.is_browsable() {
            return Ok(None);
        }
        self.call(move |conn| resolve(conn).map(Some)).await
    }

    /// The `cloud_path` for a cover image under `storage`: `None` for an opaque
    /// home, or `{album_id}/{release_id}/cover.{ext}` for a browsable one.
    pub async fn cover_cloud_path_for_storage(
        &self,
        storage: crate::config::HomeStorage,
        release_id: &str,
        content_type: &ContentType,
    ) -> Result<Option<String>, DbError> {
        let release_id = release_id.to_string();
        let content_type = content_type.clone();
        self.cloud_path_if_browsable(storage, move |conn| {
            resolve_cover_cloud_path(conn, &release_id, &content_type)
        })
        .await
    }

    /// The `cloud_path` for an artist image under `storage`: `None` for an
    /// opaque home, or `{artist_id}/artist.{ext}` for a browsable one. Keyed by
    /// the artist id alone, so it needs no DB lookup.
    pub fn artist_image_cloud_path_for_storage(
        &self,
        storage: crate::config::HomeStorage,
        artist_id: &str,
        content_type: &ContentType,
    ) -> Option<String> {
        if !storage.is_browsable() {
            return None;
        }
        Some(resolve_artist_cloud_path(artist_id, content_type))
    }

    // ---- Local blob refs ----

    pub async fn register_external_blob(
        &self,
        blob_id: &str,
        namespace: &str,
        path: &Path,
        size: u64,
    ) -> Result<(), DbError> {
        let blob_id = blob_id.to_string();
        let namespace = namespace.to_string();
        let path = path.to_path_buf();
        self.call(move |conn| register_external_blob_on(conn, &blob_id, &namespace, &path, size))
            .await
    }

    pub async fn clear_external_blob(&self, blob_id: &str) -> Result<(), DbError> {
        let blob_id = blob_id.to_string();
        self.call(move |conn| clear_external_blob_on(conn, &blob_id))
            .await
    }

    pub async fn external_blob(&self, blob_id: &str) -> Result<Option<ExternalBlob>, DbError> {
        let blob_id = blob_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT path, size FROM local_blob_refs WHERE blob_id = ?1",
                [blob_id],
                |row| {
                    Ok(ExternalBlob {
                        path: PathBuf::from(row.get::<_, String>(0)?),
                        size: row.get::<_, i64>(1)? as u64,
                    })
                },
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    // ---- Cloud outbox ----

    /// Seed an upload entry in coven's cloud outbox. Production never enqueues
    /// uploads this way — coven's `make_remote` owns that — so this exists only
    /// to exercise the outbox-snapshot / drain machinery in tests.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn add_cloud_outbox_upload(
        &self,
        file_id: &str,
        cloud_key: &str,
        source_path: Option<&str>,
        retain_pinned: bool,
    ) -> Result<(), DbError> {
        let created_at = self.register_stamp().await?;
        let file_id = file_id.to_string();
        let cloud_key = cloud_key.to_string();
        let source_path = source_path.map(str::to_string);
        self.call(move |conn| {
            conn.execute(
                "DELETE FROM cloud_outbox WHERE operation = 'delete' AND cloud_key = ?1",
                [&cloud_key],
            )
            .map_err(DbError::from)?;
            conn.execute(
                "INSERT OR IGNORE INTO cloud_outbox \
                 (operation, file_id, cloud_key, source_path, scope, retain_pinned, created_at) \
                 VALUES ('upload', ?1, ?2, ?3, ?4, ?5, ?6)",
                (
                    file_id,
                    cloud_key,
                    source_path,
                    coven::BlobScope::Master.to_outbox_str(),
                    retain_pinned,
                    created_at,
                ),
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Add a delete entry to the cloud outbox.
    pub async fn add_cloud_outbox_delete(&self, cloud_key: &str) -> Result<(), DbError> {
        let created_at = self.register_stamp().await?;
        let cloud_key = cloud_key.to_string();
        self.call(move |conn| {
            conn.execute(
                "DELETE FROM cloud_outbox \
                 WHERE operation IN ('upload', 'cancel') AND cloud_key = ?1",
                [&cloud_key],
            )
            .map_err(DbError::from)?;
            conn.execute(
                "INSERT OR IGNORE INTO cloud_outbox \
                 (operation, cloud_key, scope, created_at) \
                 VALUES ('delete', ?1, NULL, ?2)",
                (&cloud_key, &created_at),
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Remove a cloud outbox entry by id.
    pub async fn remove_cloud_outbox_entry(&self, id: i64) -> Result<(), DbError> {
        self.call(move |conn| {
            conn.execute("DELETE FROM cloud_outbox WHERE id = ?1", [id])
                .map(|_| ())
                .map_err(DbError::from)
        })
        .await
    }

    /// Remove all pending upload entries for a given cloud key. Used when a file
    /// is deleted before its upload completes.
    pub async fn remove_cloud_outbox_uploads_for_key(
        &self,
        cloud_key: &str,
    ) -> Result<(), DbError> {
        let cloud_key = cloud_key.to_string();
        self.call(move |conn| {
            conn.execute(
                "DELETE FROM cloud_outbox WHERE operation = 'upload' AND cloud_key = ?1",
                [cloud_key],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Clear the backoff timestamp on failed uploads so the next cycle retries.
    pub async fn reset_cloud_outbox_backoff(&self) -> Result<(), DbError> {
        self.call(move |conn| {
            conn.execute(
                "UPDATE cloud_outbox SET last_attempt_at = NULL \
                 WHERE operation = 'upload' AND attempt_count > 0",
                [],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn record_cloud_upload_failure(
        &self,
        id: i64,
        error: &str,
        attempted_at: &str,
    ) -> Result<(), DbError> {
        let error = error.to_string();
        let attempted_at = attempted_at.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE cloud_outbox \
                 SET attempt_count = attempt_count + 1, last_error = ?1, last_attempt_at = ?2 \
                 WHERE id = ?3",
                (error, attempted_at, id),
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_pending_cloud_uploads(&self) -> Result<Vec<coven::OutboxEntry>, DbError> {
        self.pending_outbox("upload").await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_pending_cloud_deletes(&self) -> Result<Vec<coven::OutboxEntry>, DbError> {
        self.pending_outbox("delete").await
    }

    #[cfg(any(test, feature = "test-utils"))]
    async fn pending_outbox(
        &self,
        operation: &'static str,
    ) -> Result<Vec<coven::OutboxEntry>, DbError> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, operation, file_id, cloud_key, source_path, scope, \
                        retain_pinned, attempt_count, last_attempt_at \
                 FROM cloud_outbox WHERE operation = ?1 ORDER BY id",
            )?;
            let rows = stmt.query_map([operation], row_to_outbox_entry)?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// All outbox entries (uploads and deletes), oldest first, each paired with
    /// the album title of the release its `file_id` belongs to (uploads only —
    /// `None` for deletes or an orphaned file). Backs the processing snapshot.
    pub async fn outbox_items(&self) -> Result<Vec<DbOutboxRow>, DbError> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT co.id, co.operation, co.file_id, co.cloud_key, \
                                co.created_at, co.attempt_count, co.last_error, \
                                rf.release_id AS release_id, rf.file_size AS file_size, \
                                rf.original_filename AS file_name, \
                                a.title AS title \
                         FROM cloud_outbox co \
                         LEFT JOIN release_files rf ON rf.id = co.file_id \
                         LEFT JOIN releases r ON r.id = rf.release_id \
                         LEFT JOIN albums a ON a.id = r.album_id \
                         ORDER BY co.id",
            )?;
            let rows = stmt.query_map([], |row| {
                // coven writes `created_at` as its HLC stamp
                // (`millis-counter-device`, the same format `make_remote`
                // enqueues with and `register_stamp` mints); the UI needs an
                // instant for the "queued N ago" label, so take the stamp's
                // physical millis. A value that isn't a coven stamp is corrupt
                // — surface it as a column-conversion error rather than masking
                // it. The index is `created_at`'s position in the SELECT
                // (`co.id`=0, …, `co.created_at`=4) so the diagnostic names the
                // right column.
                let created_at_raw = row.get::<_, String>("created_at")?;
                let created_at = coven::Timestamp::parse(&created_at_raw)
                    .map(|t| t.millis as i64)
                    .ok_or_else(|| {
                        coven::rusqlite::Error::FromSqlConversionFailure(
                            4,
                            coven::rusqlite::types::Type::Text,
                            format!("created_at {created_at_raw:?} is not a coven HLC stamp")
                                .into(),
                        )
                    })?;
                Ok(DbOutboxRow {
                    id: row.get("id")?,
                    operation: OutboxOpKind::parse(&row.get::<_, String>("operation")?)
                        .expect("invalid outbox operation in DB"),
                    file_id: row.get("file_id")?,
                    cloud_key: row.get("cloud_key")?,
                    created_at,
                    attempt_count: row.get("attempt_count")?,
                    last_error: row.get("last_error")?,
                    release_id: row.get("release_id")?,
                    title: row.get("title")?,
                    file_name: row.get("file_name")?,
                    file_size: row.get("file_size")?,
                })
            })?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
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

#[cfg(test)]
mod queue_ordering_tests {
    use super::*;
    use crate::playback::QueueEntryId;
    use std::collections::HashMap;

    fn meta(id: &str) -> TrackQueueMeta {
        TrackQueueMeta {
            title: format!("Title {id}"),
            artist_names: "Artist Name".to_string(),
            duration_ms: Some(1000),
            album_title: "Album Title".to_string(),
            cover_image_id: Some(format!("rel-{id}")),
        }
    }

    fn entry(entry_id: &str, track_id: &str) -> QueueEntry {
        QueueEntry {
            id: QueueEntryId(entry_id.to_string()),
            track_id: track_id.to_string(),
        }
    }

    #[test]
    fn preserves_duplicate_queue_entries_in_order_with_distinct_ids() {
        let mut meta_by_track = HashMap::new();
        meta_by_track.insert("a".to_string(), meta("a"));
        meta_by_track.insert("b".to_string(), meta("b"));

        // The same track queued twice resolves twice, in position order, each
        // carrying its own entry id.
        let resolved = resolve_queue_entries(
            &meta_by_track,
            &[entry("e0", "a"), entry("e1", "a"), entry("e2", "b")],
        );

        let track_ids: Vec<&str> = resolved.iter().map(|i| i.track_id.as_str()).collect();
        assert_eq!(track_ids, vec!["a", "a", "b"]);
        let entry_ids: Vec<&str> = resolved.iter().map(|i| i.entry_id.as_str()).collect();
        assert_eq!(entry_ids, vec!["e0", "e1", "e2"]);
    }

    #[test]
    fn skips_entries_whose_track_is_unknown() {
        let mut meta_by_track = HashMap::new();
        meta_by_track.insert("a".to_string(), meta("a"));
        let resolved =
            resolve_queue_entries(&meta_by_track, &[entry("e0", "a"), entry("e1", "missing")]);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].entry_id, "e0");
    }
}

#[cfg(test)]
mod readable_cloud_path_tests {
    use super::*;

    /// An in-memory DB on the real schema with one artist/album/release, so the
    /// connection-level resolvers can look up a release's album id.
    fn seeded_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("../../migrations/001_initial.sql"))
            .unwrap();
        let now = "2026-01-01T00:00:00Z";
        conn.execute(
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('artist-1', 'Artist Name', ?, ?)",
            params![now, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at) \
             VALUES ('album-1', 'Album Title', 'artist-1', 0, ?, ?)",
            params![now, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at) \
             VALUES ('rel-1', 'album-1', 'file_tags', 1, ?, ?)",
            params![now, now],
        )
        .unwrap();
        conn
    }

    #[test]
    fn audio_key_omits_source_folder_when_release_has_none() {
        // The seeded release has no source_folder_name (a non-folder import).
        // The stored key is namespace-relative; coven prepends the `storage/`
        // audio namespace when it reads/writes the blob.
        let conn = seeded_conn();
        let key = resolve_audio_cloud_path(&conn, "rel-1", "01 Track Title.flac").unwrap();
        assert_eq!(key, "album-1/rel-1/01 Track Title.flac");
    }

    #[test]
    fn audio_key_includes_source_folder_from_the_release_row() {
        let conn = seeded_conn();
        conn.execute(
            "UPDATE releases SET source_folder_name = 'Album Folder [FLAC]' WHERE id = 'rel-1'",
            [],
        )
        .unwrap();
        let key = resolve_audio_cloud_path(&conn, "rel-1", "01 Track Title.flac").unwrap();
        assert_eq!(key, "album-1/rel-1/Album Folder [FLAC]/01 Track Title.flac");
    }

    #[test]
    fn cover_key_is_album_release_cover() {
        let conn = seeded_conn();
        let key = resolve_cover_cloud_path(&conn, "rel-1", &ContentType::Jpeg).unwrap();
        assert_eq!(key, "album-1/rel-1/cover.jpg");
    }

    #[test]
    fn artist_key_is_artist_id() {
        // Keyed by the artist id alone — no DB lookup.
        let key = resolve_artist_cloud_path("artist-1", &ContentType::Png);
        assert_eq!(key, "artist-1/artist.png");
    }

    #[test]
    fn missing_release_is_an_error() {
        // The release row must exist when a blob is keyed; its absence is a
        // broken invariant surfaced as an error, not masked.
        let conn = seeded_conn();
        assert!(resolve_audio_cloud_path(&conn, "no-such-release", "x.flac").is_err());
    }
}

#[cfg(test)]
mod playback_state_load_tests {
    use super::*;
    use coven::SystemClock;

    async fn empty_db() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(path.to_str().unwrap(), Arc::new(SystemClock))
            .await
            .unwrap();
        (db, tmp)
    }

    /// `source` and `cursor` are written together, so a row carrying one without
    /// the other is corrupt: `load_playback_state` discards the whole cache and
    /// returns `None` rather than inventing a cursor.
    #[tokio::test]
    async fn mismatched_source_and_cursor_discards_the_cache() {
        let (db, _tmp) = empty_db().await;

        // Write a row by hand with a present source but a NULL cursor —
        // `save_playback_state` never produces this, so we insert it directly.
        db.call(|conn| {
            conn.execute(
                "INSERT INTO playback_state \
                     (id, source, shuffle_seed, cursor, manual, repeat, \
                      current_track_id, position_ms, volume, is_muted) \
                     VALUES ('current', 'rel-1', NULL, NULL, '[]', 'off', \
                      NULL, NULL, 1.0, 0)",
                [],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();

        assert!(db.load_playback_state().await.unwrap().is_none());
    }
}

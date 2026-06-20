use crate::clock::ClockRef;
use crate::db::models::*;
use crate::util::content_type::ContentType;
use chrono::{DateTime, Utc};
use coven::database::DbError;
use coven::rusqlite::{params, Connection, OptionalExtension, Row};
use coven::sync::session::SyncedTable;
use coven::UpdatedAtStamper;
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use tracing::info;

fn row_to_library_image(row: &Row) -> DbLibraryImage {
    DbLibraryImage {
        id: row.get("id").unwrap(),
        image_type: row.get::<_, String>("type").unwrap().parse().unwrap(),
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
        StorageFilter::Managed => "WHERE r.managed = 1",
        StorageFilter::Unmanaged => "WHERE r.managed = 0",
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

/// SELECT-list fragment for the device-local `release_local_copy` row joined
/// onto a `releases r` query via [`RELEASE_LOCAL_COPY_JOIN`]. `lc_present` is
/// the row marker (NULL when the LEFT JOIN found no row); the other two are the
/// copy's fields. Callers reassemble these via [`parse_release_local_copy`].
const RELEASE_LOCAL_COPY_SELECT: &str = "lc.release_id AS lc_present, \
    lc.unmanaged_path AS lc_unmanaged_path, \
    lc.pinned_locally AS lc_pinned_locally";

/// LEFT JOIN clause pairing with [`RELEASE_LOCAL_COPY_SELECT`].
const RELEASE_LOCAL_COPY_JOIN: &str = "LEFT JOIN release_local_copy lc ON lc.release_id = r.id";

/// Reassemble the [`DbReleaseLocalCopy`] from a row carrying the
/// [`RELEASE_LOCAL_COPY_SELECT`] columns. `None` when the LEFT JOIN found no
/// row (this device holds no local copy).
fn parse_release_local_copy(row: &Row) -> Option<DbReleaseLocalCopy> {
    let release_id: Option<String> = row.get("lc_present").unwrap();
    release_id.map(|release_id| DbReleaseLocalCopy {
        release_id,
        unmanaged_path: row.get("lc_unmanaged_path").unwrap(),
        pinned_locally: row.get("lc_pinned_locally").unwrap(),
    })
}

struct DatabaseInner {
    /// The connection coven owns, on its dedicated thread. Every read and write
    /// runs through `coven_db.call(|conn| …)`; writes to synced tables are
    /// captured by coven's session.
    coven_db: coven::Database,
    /// The last-writer-wins stamper for every synced row's `_updated_at`,
    /// returned (non-optional, already seeded) from `coven::Database::open`. It
    /// wraps the same `Arc<Hlc>` coven's sync loop advances on pull and stamps
    /// envelopes from, so the host's stamps and coven's sync resolve conflicts
    /// off one clock.
    stamper: UpdatedAtStamper,
    /// Wall clock for `created_at` and status timestamps bound into write SQL.
    /// Synced-table `_updated_at` is stamped from [`stamper`] instead.
    clock: ClockRef,
}

/// Database client over coven's owned connection.
///
/// All reads and writes run on coven's single connection thread via
/// [`coven::Database::call`]. Writes to synced tables are captured by coven's
/// attached session for changeset sync; reads share the same serialized thread.
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

    /// The wall clock this database binds into write timestamps. Shared with
    /// callers that mint rows outside `db/client.rs` (e.g. the import mappers)
    /// so they read "now" from the same injected source.
    pub(crate) fn clock(&self) -> &ClockRef {
        &self.inner.clock
    }

    /// The connection coven owns, the integration seam for coven's sync + blob
    /// pipeline. `build_sync_manager` hands this to `coven::SyncManager::new`
    /// (which reads the synced-table set and the shared register clock from it),
    /// and coven's `process_uploads`/`process_deletes` take it directly.
    pub fn coven_db(&self) -> &coven::Database {
        &self.inner.coven_db
    }

    /// Stamp a synced row's `_updated_at` from coven's HLC register. Every
    /// INSERT/UPDATE of a synced table binds this, so the host and coven's sync
    /// layer resolve row conflicts off one shared clock.
    fn register_stamp(&self) -> String {
        self.inner.stamper.stamp()
    }

    /// Open the database at `path`, running coven's bookkeeping migration plus
    /// bae's schema (idempotent, so re-running over a snapshot-bootstrapped DB
    /// is a no-op). coven seeds the register clock off the on-disk rows, attaches
    /// the capture session over `synced_tables`, and owns the connection on a
    /// dedicated thread; the returned stamper mints every synced row's
    /// `_updated_at`.
    pub async fn new(
        database_path: &str,
        clock: ClockRef,
        device_id: String,
        synced_tables: Vec<SyncedTable>,
    ) -> Result<Self, DbError> {
        info!("Opening database at {}", database_path);

        let (coven_db, stamper) =
            coven::Database::open(Path::new(database_path), synced_tables, device_id, |conn| {
                conn.execute_batch(include_str!("../../migrations/001_initial.sql"))
                    .map_err(DbError::from)
            })?;

        Ok(Database {
            inner: Arc::new(DatabaseInner {
                coven_db,
                stamper,
                clock,
            }),
        })
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
            managed: row.get("managed").unwrap(),
            source_folder_name: row.get("source_folder_name").unwrap(),
            content_hash: row.get("content_hash").unwrap(),
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
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| {
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
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| insert_album_artist_row(conn, &album_artist, &reg))
            .await
    }
    /// Insert track-artist relationship
    pub async fn insert_track_artist(&self, track_artist: &DbTrackArtist) -> Result<(), DbError> {
        let track_artist = track_artist.clone();
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| insert_track_artist_row(conn, &track_artist, &reg))
            .await
    }
    /// Get artists for an album (ordered by position)
    pub async fn get_artists_for_album(&self, album_id: &str) -> Result<Vec<DbArtist>, DbError> {
        let album_id = album_id.to_string();
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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

        self.inner
            .coven_db
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
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| insert_album_row(conn, &album, &reg))
            .await
    }
    /// Insert a new release.
    pub async fn insert_release(&self, release: &DbRelease) -> Result<(), DbError> {
        let release = release.clone();
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| insert_release_row(conn, &release, &reg))
            .await
    }
    /// Insert a new track
    pub async fn insert_track(&self, track: &DbTrack) -> Result<(), DbError> {
        let track = track.clone();
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| insert_track_row(conn, &track, &reg))
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
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                insert_album_row(&tx, &album, &reg)?;
                insert_release_row(&tx, &release, &reg)?;
                for track in &tracks {
                    insert_track_row(&tx, track, &reg)?;
                }
                for ta in &track_artists {
                    insert_track_artist_row(&tx, ta, &reg)?;
                }
                for meta in &metadata {
                    insert_release_metadata_row(&tx, meta)?;
                }
                tx.commit().map_err(DbError::from)
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
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                insert_release_row(&tx, &release, &reg)?;
                for track in &tracks {
                    insert_track_row(&tx, track, &reg)?;
                }
                for ta in &track_artists {
                    insert_track_artist_row(&tx, ta, &reg)?;
                }
                for meta in &metadata {
                    insert_release_metadata_row(&tx, meta)?;
                }
                tx.commit().map_err(DbError::from)
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
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;

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
                replace_album_artists(&tx, &album_id, &album_artists, &reg, &now)?;

                // 5. Replace track_artists for the affected tracks.
                let track_ids: Vec<&str> =
                    track_updates.iter().map(|(id, _)| id.as_str()).collect();
                replace_track_artists(&tx, &track_ids, &track_artists, &reg, &now)?;

                tx.commit().map_err(DbError::from)
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

        self.inner
            .coven_db
            .call(move |conn| {
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

        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
            r.managed, \
            {RELEASE_LOCAL_COPY_SELECT}, \
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
        {RELEASE_LOCAL_COPY_JOIN} \
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
            managed: row.get("managed")?,
            local_copy: parse_release_local_copy(row),
            file_count: row.get("file_count")?,
            total_size: row.get("total_size")?,
        })
    }

    pub async fn get_release_storage_summaries(
        &self,
    ) -> Result<Vec<DbReleaseStorageSummary>, DbError> {
        let query = Self::release_storage_summary_query("ORDER BY a.title, r.created_at");
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
                conn.query_row(&query, [release_id], Self::row_to_release_storage_summary)
                    .optional()
                    .map_err(DbError::from)
            })
            .await
    }

    /// Count releases whose audio this device can reach only through cloud
    /// sync: managed, with no `release_local_copy` row on this device. Used by
    /// the disconnect flow to warn the user how many releases will become
    /// unplayable when the cloud provider is removed.
    pub async fn get_cloud_only_release_count(&self) -> Result<u64, DbError> {
        self.inner
            .coven_db
            .call(move |conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM releases r \
                     WHERE r.managed = 1 \
                     AND NOT EXISTS (SELECT 1 FROM release_local_copy lc WHERE lc.release_id = r.id)",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map(|c| c as u64)
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
        self.inner
            .coven_db
            .call(move |conn| {
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

        self.inner
            .coven_db
            .call(move |conn| {
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
                r.managed, \
                {RELEASE_LOCAL_COPY_SELECT}, \
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
            {RELEASE_LOCAL_COPY_JOIN} \
            {artist_sort_join} \
            {where_clause} \
            ORDER BY {order_by} \
            LIMIT ? OFFSET ?",
        );

        self.inner
            .coven_db
            .call(move |conn| {
                let mut stmt = conn.prepare(&query)?;
                let mut rows = stmt.query(params![limit as i64, offset as i64])?;
                let mut storage_rows = Vec::new();
                while let Some(row) = rows.next()? {
                    let release = DbReleaseSummary {
                        id: row.get("release_id")?,
                        album_id: row.get("album_id")?,
                        format: row.get("release_format")?,
                        managed: row.get("managed")?,
                        local_copy: parse_release_local_copy(row),
                        file_count: row.get("file_count")?,
                        total_size: row.get("total_size")?,
                    };

                    let release_ids_json: String = row.get("release_ids_json")?;
                    let release_ids: Vec<String> = serde_json::from_str(&release_ids_json)
                        .map_err(|e| DbError(e.to_string()))?;

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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        let local_copy = self.get_release_local_copy(&release.id).await?;

        Ok(DbReleaseDetail {
            release,
            local_copy,
            tracks,
            files,
            audio_formats,
            identities,
        })
    }

    /// Get all releases for an album
    pub async fn get_releases_for_album(&self, album_id: &str) -> Result<Vec<DbRelease>, DbError> {
        let album_id = album_id.to_string();
        self.inner
            .coven_db
            .call(move |conn| {
                let mut stmt =
                    conn.prepare("SELECT * FROM releases WHERE album_id = ? ORDER BY created_at")?;
                let rows =
                    stmt.query_map(params![album_id], |row| Ok(Self::row_to_release(row)))?;
                rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                    .map_err(DbError::from)
            })
            .await
    }

    /// Find track by ID. Caller-provided ID — may not exist.
    pub async fn find_track_by_id(&self, track_id: &str) -> Result<Option<DbTrack>, DbError> {
        let track_id = track_id.to_string();
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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

    /// Enrich a list of track IDs with album/artist metadata for queue display.
    /// Returns items in the same order as the input IDs (skipping any not found).
    pub async fn get_queue_items(&self, track_ids: &[String]) -> Result<Vec<DbQueueItem>, DbError> {
        if track_ids.is_empty() {
            return Ok(Vec::new());
        }

        let track_ids = track_ids.to_vec();
        self.inner
            .coven_db
            .call(move |conn| {
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
                let mut by_id: HashMap<String, DbQueueItem> = HashMap::new();
                let mut rows = stmt
                    .query(coven::rusqlite::params_from_iter(track_ids.iter()))?;
                while let Some(row) = rows.next()? {
                    let track_id: String = row.get("track_id")?;
                    by_id.insert(
                        track_id.clone(),
                        DbQueueItem {
                            track_id,
                            title: row.get("title")?,
                            artist_names: row.get("artist_names")?,
                            duration_ms: row.get("duration_ms")?,
                            album_title: row.get("album_title")?,
                            cover_image_id: row.get("primary_release_id")?,
                        },
                    );
                }

                Ok(order_queue_items(&by_id, &track_ids))
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
        self.inner
            .coven_db
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id FROM tracks WHERE release_id = ? ORDER BY side, track_number, id",
                )?;
                let rows = stmt.query_map(params![release_id], |row| row.get::<_, String>("id"))?;
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| insert_file_row(conn, &file, &reg))
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        library_image: Option<&DbLibraryImage>,
        primary_release_id: Option<(&str, &str)>, // (album_id, release_id)
        import_id: &str,
        identities: &[crate::import::ReleaseIdentity],
        // This device's local copy of the import: `Some` for an in-place
        // unmanaged import or a managed pin, `None` for managed cloud-only.
        local_copy: Option<&DbReleaseLocalCopy>,
        // Cloud-upload intents, one per file for managed imports (empty for
        // unmanaged). Committed inside this transaction so the release
        // either lands with its uploads queued or doesn't land at all.
        cloud_uploads: &[DbCloudUpload],
        // The cloud home's storage mode, deciding the blob layout: `Opaque`
        // keys every managed blob by the hashed id, `Browsable` lays them out at
        // readable `cloud_path`s computed inside this transaction.
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
        let library_image = library_image.cloned();
        let primary_release_id = primary_release_id.map(|(a, r)| (a.to_string(), r.to_string()));
        let import_id = import_id.to_string();
        let identities = identities.to_vec();
        let local_copy = local_copy.cloned();
        let cloud_uploads = cloud_uploads.to_vec();

        let now_dt = self.inner.clock.now();
        let now = now_dt.to_rfc3339();
        let now_ts = now_dt.timestamp();
        // Every synced row this transaction inserts shares one HLC register stamp
        // for `_updated_at`; wall-clock `now` stays for `created_at`.
        let reg = self.register_stamp();

        self.inner
            .coven_db
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;

                // 1. Insert album (if new)
                if let Some(album) = &album {
                    insert_album_row(&tx, album, &reg)?;

                    // Album artists (only for new albums)
                    for aa in &album_artists {
                        insert_album_artist_row(&tx, aa, &reg)?;
                    }
                }

                // 2. Insert release
                insert_release_row(&tx, &release, &reg)?;

                // 2b. Insert per-source identity rows. Empty for Unknown imports.
                //     `release_identities` is uniquely keyed on `(release_id, source)`,
                //     so a release never carries two rows for the same source.
                for identity in &identities {
                    insert_release_identity_row(&tx, &release.id, identity, &reg, &now)?;
                }

                // 2c. Record this device's local copy (in-place unmanaged import or a
                //     managed pin). Absent for a managed cloud-only import.
                if let Some(local_copy) = &local_copy {
                    upsert_release_local_copy_row(&tx, local_copy)?;
                }

                // 3. Insert tracks. DbTracks live inside `tracks_to_files`; their
                //    `duration_ms` was populated by the mapper from the CUE sheet or
                //    a standalone-file probe.
                for track in &tracks {
                    insert_track_row(&tx, track, &reg)?;
                }

                // 4. Insert track artists
                for ta in &track_artists {
                    insert_track_artist_row(&tx, ta, &reg)?;
                }

                // 5. Insert release metadata
                for meta in &metadata {
                    insert_release_metadata_row(&tx, meta)?;
                }

                // 6. Insert files. Under a browsable home each managed file
                //    gets a readable cloud key (`{artist}/{album}/{filename}`)
                //    computed from the album/artist rows inserted above and
                //    stored on the row; an opaque home (or an unmanaged import,
                //    which queues no uploads) leaves it NULL = hashed-by-id.
                //    The key is computed once here and reused for the enqueue so
                //    the synced row and the upload intent never disagree.
                let mut file_cloud_keys: HashMap<String, String> = HashMap::new();
                for file in &files {
                    let cloud_path = if storage.is_browsable() && !cloud_uploads.is_empty() {
                        let key = resolve_audio_cloud_path(
                            &tx,
                            &file.release_id,
                            &file.original_filename,
                        )?;
                        file_cloud_keys.insert(file.id.clone(), key.clone());
                        Some(key)
                    } else {
                        None
                    };
                    let file = DbFile {
                        cloud_path,
                        ..file.clone()
                    };
                    insert_file_row(&tx, &file, &reg)?;
                }

                // 7. Insert audio formats
                for af in &audio_formats {
                    insert_audio_format_row(&tx, af, &reg)?;
                }

                // Queue the release's cloud uploads inside this transaction,
                //     so a managed release never exists with its upload intents
                //     silently missing. Every blob is encrypted with the library
                //     master key (`BlobScope::Master`). The key matches the file
                //     row's `cloud_path` — the readable key on a browsable home,
                //     else the hashed `storage_path(file_id)`.
                for upload in &cloud_uploads {
                    let cloud_key = file_cloud_keys
                        .get(&upload.file_id)
                        .cloned()
                        .unwrap_or_else(|| crate::storage::local::storage_path(&upload.file_id));
                    coven::Database::enqueue_upload_on(
                        &tx,
                        &upload.file_id,
                        &cloud_key,
                        upload.source_path.as_deref(),
                        coven::blob::BlobScope::Master,
                        &now,
                    )?;
                }

                // 8. Upsert library image (cover art). Under a browsable home a
                //    managed import keys the cover readably
                //    (`{artist}/{album}/cover.{ext}`); an opaque home (or an
                //    unmanaged import that uploads nothing) leaves it NULL.
                if let Some(image) = &library_image {
                    let cloud_path = if storage.is_browsable() && !cloud_uploads.is_empty() {
                        Some(resolve_cover_cloud_path(
                            &tx,
                            &image.id,
                            &image.content_type,
                        )?)
                    } else {
                        None
                    };
                    let image = DbLibraryImage {
                        cloud_path,
                        ..image.clone()
                    };
                    upsert_library_image_row(&tx, &image, &reg)?;
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

                tx.commit().map_err(DbError::from)
            })
            .await?;
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
        self.inner
            .coven_db
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                tx.execute(
                    "UPDATE imports SET release_id = NULL WHERE release_id = ?",
                    params![release_id],
                )?;
                tx.execute("DELETE FROM releases WHERE id = ?", params![release_id])?;
                tx.commit().map_err(DbError::from)
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
        self.inner
            .coven_db
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                tx.execute(
                    "UPDATE imports SET release_id = NULL WHERE release_id IN (SELECT id FROM releases WHERE album_id = ?)",
                    params![album_id],
                )?;
                tx.execute("DELETE FROM albums WHERE id = ?", params![album_id])?;
                tx.commit().map_err(DbError::from)
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
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| insert_release_metadata_row(conn, &meta))
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

        self.inner
            .coven_db
            .call(move |conn| {
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

        self.inner
            .coven_db
            .call(move |conn| {
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

        self.inner
            .coven_db
            .call(move |conn| {
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
        let reg = self.register_stamp();

        self.inner
            .coven_db
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;

                // 1. Insert the destination album (if brand-new). Must come
                //    before the release UPDATE so the FK on `releases.album_id`
                //    points at an existing row.
                if let Some(album) = &new_album {
                    insert_album_row(&tx, album, &reg)?;

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
                    insert_release_identity_row(&tx, &release_id, identity, &reg, &now)?;
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

                tx.commit()?;
                Ok(SetIdentityOutcome {
                    source_album_deleted,
                })
            })
            .await
    }

    /// Upsert a library image record
    pub async fn upsert_library_image(&self, image: &DbLibraryImage) -> Result<(), DbError> {
        let image = image.clone();
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| upsert_library_image_row(conn, &image, &reg))
            .await
    }

    /// Find library image by ID and type. Caller-provided ID — may not exist.
    pub async fn find_library_image(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<DbLibraryImage>, DbError> {
        let (id, image_type) = (id.to_string(), image_type.as_str().to_string());
        self.inner
            .coven_db
            .call(move |conn| {
                conn.query_row(
                    "SELECT * FROM library_images WHERE id = ? AND type = ?",
                    params![id, image_type],
                    |row| Ok(row_to_library_image(row)),
                )
                .optional()
                .map_err(DbError::from)
            })
            .await
    }

    /// Delete a library image by ID and type
    pub async fn delete_library_image(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<(), DbError> {
        let (id, image_type) = (id.to_string(), image_type.as_str().to_string());
        self.inner
            .coven_db
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM library_images WHERE id = ? AND type = ?",
                    params![id, image_type],
                )
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
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| {
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
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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

    /// This device's `release_local_copy` row, if any. `None` means this
    /// device holds no local copy (stream from cloud if managed, else the
    /// release isn't reachable here).
    pub async fn get_release_local_copy(
        &self,
        release_id: &str,
    ) -> Result<Option<DbReleaseLocalCopy>, DbError> {
        let release_id = release_id.to_string();
        self.inner
            .coven_db
            .call(move |conn| {
                conn.query_row(
                    "SELECT release_id, unmanaged_path, pinned_locally \
                     FROM release_local_copy WHERE release_id = ?",
                    params![release_id],
                    |row| {
                        Ok(DbReleaseLocalCopy {
                            release_id: row.get("release_id")?,
                            unmanaged_path: row.get("unmanaged_path")?,
                            pinned_locally: row.get("pinned_locally")?,
                        })
                    },
                )
                .optional()
                .map_err(DbError::from)
            })
            .await
    }

    /// Insert or replace this device's `release_local_copy` row.
    pub async fn upsert_release_local_copy(
        &self,
        copy: &DbReleaseLocalCopy,
    ) -> Result<(), DbError> {
        let copy = copy.clone();
        self.inner
            .coven_db
            .call(move |conn| upsert_release_local_copy_row(conn, &copy))
            .await
    }

    /// Drop this device's `release_local_copy` row (it no longer holds a
    /// local copy). A missing row is a no-op.
    pub async fn delete_release_local_copy(&self, release_id: &str) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        self.inner
            .coven_db
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM release_local_copy WHERE release_id = ?",
                    params![release_id],
                )
                .map(|_| ())
                .map_err(DbError::from)
            })
            .await
    }

    /// Finish a Manage → Pinned transition: mark the shared `managed` fact true
    /// and pin this device's local copy (a verified copy now lives under
    /// `storage/`). Atomic so the two never diverge.
    pub async fn set_release_managed_pinned(&self, release_id: &str) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                set_release_managed_row(&tx, &release_id, true, &reg)?;
                upsert_release_local_copy_row(
                    &tx,
                    &DbReleaseLocalCopy {
                        release_id: release_id.clone(),
                        unmanaged_path: None,
                        pinned_locally: true,
                    },
                )?;
                tx.commit().map_err(DbError::from)
            })
            .await
    }

    /// Finish a Manage → CloudOnly transition: mark the shared `managed` fact
    /// true and drop this device's local copy (the originals are gone / no
    /// longer the live copy). Atomic so the two never diverge.
    pub async fn set_release_managed_cloud_only(&self, release_id: &str) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                set_release_managed_row(&tx, &release_id, true, &reg)?;
                tx.execute(
                    "DELETE FROM release_local_copy WHERE release_id = ?",
                    params![release_id],
                )?;
                tx.commit().map_err(DbError::from)
            })
            .await
    }

    /// Unmanage: mark the shared `managed` fact false and record this device's
    /// local copy at `path` (the files moved back out in place). Atomic.
    pub async fn set_release_unmanaged_path(
        &self,
        release_id: &str,
        path: &str,
    ) -> Result<(), DbError> {
        let (release_id, path) = (release_id.to_string(), path.to_string());
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                set_release_managed_row(&tx, &release_id, false, &reg)?;
                upsert_release_local_copy_row(
                    &tx,
                    &DbReleaseLocalCopy {
                        release_id: release_id.clone(),
                        unmanaged_path: Some(path),
                        pinned_locally: false,
                    },
                )?;
                tx.commit().map_err(DbError::from)
            })
            .await
    }

    /// Set the deferred-delete intent for a Manage → CloudOnly transition on
    /// this device's `release_local_copy` row. The upload observer reads this
    /// when the last upload lands, deletes the originals, then drops the row.
    /// The row always exists when this is called: the intent is only set while
    /// the release is unmanaged (an `unmanaged_path` local copy), and cleared
    /// before the row is dropped, so a missing row is a bug, not a no-op.
    pub async fn set_release_delete_unmanaged_source_on_upload(
        &self,
        release_id: &str,
        delete: bool,
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        self.inner
            .coven_db
            .call(move |conn| {
                let affected = conn.execute(
                    "UPDATE release_local_copy SET delete_unmanaged_source_on_upload = ? \
                         WHERE release_id = ?",
                    params![delete, release_id],
                )?;
                if affected == 0 {
                    return Err(DbError(format!(
                        "release_local_copy row missing for release {release_id}"
                    )));
                }
                Ok(())
            })
            .await
    }

    /// Read the deferred-delete intent from this device's `release_local_copy`
    /// row. The upload observer consults this on the last finished upload, while
    /// the row still records the unmanaged source path. No row means this device
    /// holds no local copy, hence nothing to delete — reads as `false`.
    pub async fn get_release_delete_unmanaged_source_on_upload(
        &self,
        release_id: &str,
    ) -> Result<bool, DbError> {
        let release_id = release_id.to_string();
        self.inner
            .coven_db
            .call(move |conn| {
                let flag: Option<bool> = conn
                    .query_row(
                        "SELECT delete_unmanaged_source_on_upload FROM release_local_copy \
                         WHERE release_id = ?",
                        params![release_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                Ok(flag.unwrap_or(false))
            })
            .await
    }

    /// Get the release that owns a given file.
    pub async fn find_release_for_file(&self, file_id: &str) -> Result<Option<DbRelease>, DbError> {
        let file_id = file_id.to_string();
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| {
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

        self.inner
            .coven_db
            .call(move |conn| {
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
        self.inner
            .coven_db
            .call(move |conn| resolve(conn).map(Some))
            .await
    }

    /// The cloud object key for an audio file under `storage`: `None` for an
    /// opaque home (keyed by the hashed id, the default), or the
    /// `storage/{album_id}/{release_id}/{filename}` path for a browsable one.
    pub async fn audio_cloud_path_for_storage(
        &self,
        storage: crate::config::HomeStorage,
        release_id: &str,
        original_filename: &str,
    ) -> Result<Option<String>, DbError> {
        let release_id = release_id.to_string();
        let original_filename = original_filename.to_string();
        self.cloud_path_if_browsable(storage, move |conn| {
            resolve_audio_cloud_path(conn, &release_id, &original_filename)
        })
        .await
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

    /// Persist the readable cloud key on an existing `release_files` row (the
    /// manage path, where the file row predates its cloud destination). Not an
    /// `_updated_at` bump on its own — the caller flips `managed` later, which
    /// re-emits the whole synced subtree including this column.
    pub async fn set_file_cloud_path(
        &self,
        file_id: &str,
        cloud_path: &str,
    ) -> Result<(), DbError> {
        let file_id = file_id.to_string();
        let cloud_path = cloud_path.to_string();
        let reg = self.register_stamp();
        self.inner
            .coven_db
            .call(move |conn| {
                conn.execute(
                    "UPDATE release_files SET cloud_path = ?, _updated_at = ? WHERE id = ?",
                    params![cloud_path, reg, file_id],
                )
                .map(|_| ())
                .map_err(DbError::from)
            })
            .await
    }

    // ---- Cloud outbox ----

    /// Add an upload entry to the cloud outbox. coven encrypts the blob with the
    /// library master key (`BlobScope::Master`) at drain, long after this enqueue
    /// site is gone.
    pub async fn add_cloud_outbox_upload(
        &self,
        file_id: &str,
        cloud_key: &str,
        source_path: Option<&str>,
    ) -> Result<(), DbError> {
        let created_at = self.inner.clock.now().to_rfc3339();
        self.inner
            .coven_db
            .enqueue_upload(
                file_id,
                cloud_key,
                source_path,
                coven::blob::BlobScope::Master,
                &created_at,
            )
            .await
    }

    /// Add a delete entry to the cloud outbox.
    pub async fn add_cloud_outbox_delete(&self, cloud_key: &str) -> Result<(), DbError> {
        let created_at = self.inner.clock.now().to_rfc3339();
        self.inner
            .coven_db
            .enqueue_delete(cloud_key, &created_at)
            .await
    }

    // The cloud_outbox table is coven's; bae drives the queue through coven's
    // Database API instead of hand-writing its SQL. The only direct read of the
    // shared table is `outbox_items` below, which joins it against bae's own
    // domain tables (no coven API can express that).

    /// Pending upload entries, oldest first.
    pub async fn get_pending_cloud_uploads(&self) -> Result<Vec<coven::db::OutboxEntry>, DbError> {
        self.inner.coven_db.get_pending_cloud_uploads().await
    }

    /// Pending delete entries, oldest first.
    pub async fn get_pending_cloud_deletes(&self) -> Result<Vec<coven::db::OutboxEntry>, DbError> {
        self.inner.coven_db.get_pending_cloud_deletes().await
    }

    /// Remove a cloud outbox entry by id.
    pub async fn remove_cloud_outbox_entry(&self, id: i64) -> Result<(), DbError> {
        self.inner.coven_db.remove_cloud_outbox_entry(id).await
    }

    /// Remove all pending upload entries for a given cloud key. Used when a file
    /// is deleted before its upload completes.
    pub async fn remove_cloud_outbox_uploads_for_key(
        &self,
        cloud_key: &str,
    ) -> Result<(), DbError> {
        self.inner
            .coven_db
            .remove_cloud_outbox_uploads_for_key(cloud_key)
            .await
    }

    /// Record a failed upload attempt; the entry stays queued for retry.
    pub async fn record_cloud_upload_failure(
        &self,
        id: i64,
        error: &str,
        attempted_at: &str,
    ) -> Result<(), DbError> {
        self.inner
            .coven_db
            .record_cloud_upload_failure(id, error, attempted_at)
            .await
    }

    /// Clear the backoff timestamp on failed uploads so the next cycle retries.
    pub async fn reset_cloud_outbox_backoff(&self) -> Result<(), DbError> {
        self.inner.coven_db.reset_cloud_outbox_backoff().await
    }

    /// All outbox entries (uploads and deletes), oldest first, each paired with
    /// the album title of the release its `file_id` belongs to (uploads only —
    /// `None` for deletes or an orphaned file). Backs the processing snapshot.
    pub async fn outbox_items(&self) -> Result<Vec<DbOutboxRow>, DbError> {
        self.inner
            .coven_db
            .call(move |conn| {
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
                    // The queue row stores `created_at` as RFC 3339 text; the UI
                    // needs an instant, so parse it to epoch millis here. bae
                    // writes valid RFC 3339 at enqueue, so a parse failure is a
                    // corrupt value — surface it as a column-conversion error
                    // rather than masking it. The index is `created_at`'s
                    // position in the SELECT (`co.id`=0, …, `co.created_at`=4) so
                    // the diagnostic names the right column.
                    let created_at_raw = row.get::<_, String>("created_at")?;
                    let created_at = crate::config::rfc3339_to_epoch_millis(&created_at_raw)
                        .map_err(|e| {
                            coven::rusqlite::Error::FromSqlConversionFailure(
                                4,
                                coven::rusqlite::types::Type::Text,
                                Box::new(e),
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
            managed,
            source_folder_name, content_hash, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            release.managed,
            release.source_folder_name,
            release.content_hash,
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
            id, track_id, content_type, pregap_ms, sample_rate, bits_per_sample, channels, file_id, start_sample, end_sample, end_byte, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
    conn.execute(
        r#"
        INSERT INTO library_images (id, type, content_type, file_size, width, height, source, source_url, cloud_path, _updated_at, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            type = excluded.type,
            content_type = excluded.content_type,
            file_size = excluded.file_size,
            width = excluded.width,
            height = excluded.height,
            source = excluded.source,
            source_url = excluded.source_url,
            cloud_path = excluded.cloud_path,
            _updated_at = excluded._updated_at
        "#,
        params![
            image.id,
            image.image_type.as_str(),
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

/// Set the shared `releases.managed` fact (bumping `_updated_at` so the change
/// syncs). Shared by the transactional transition methods.
fn set_release_managed_row(
    conn: &Connection,
    release_id: &str,
    managed: bool,
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE releases SET managed = ?, _updated_at = ? WHERE id = ?",
        params![managed, reg, release_id],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// Insert or replace a `release_local_copy` row. Shared by the standalone
/// upsert and the transactional transition methods. The table is device-local
/// (no `_updated_at`, never synced), so there's no clock to bump.
fn upsert_release_local_copy_row(
    conn: &Connection,
    copy: &DbReleaseLocalCopy,
) -> Result<(), DbError> {
    // ON CONFLICT (not INSERT OR REPLACE) so an existing row's
    // `delete_unmanaged_source_on_upload` survives: that column is the
    // deferred-delete intent, owned solely by set/get_release_delete_unmanaged_
    // source_on_upload, and a whole-row replace here would silently reset it.
    conn.execute(
        "INSERT INTO release_local_copy (release_id, unmanaged_path, pinned_locally) \
         VALUES (?, ?, ?) \
         ON CONFLICT (release_id) DO UPDATE SET \
             unmanaged_path = excluded.unmanaged_path, \
             pinned_locally = excluded.pinned_locally",
        params![copy.release_id, copy.unmanaged_path, copy.pinned_locally],
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
/// must resolve every time — hence `get`, not `remove` (which would consume the
/// row on first hit and silently drop later occurrences, desyncing every
/// downstream positional index: skip-to, reorder, remove).
fn order_queue_items(
    by_id: &std::collections::HashMap<String, DbQueueItem>,
    track_ids: &[String],
) -> Vec<DbQueueItem> {
    track_ids
        .iter()
        .filter_map(|id| by_id.get(id).cloned())
        .collect()
}

#[cfg(test)]
mod queue_ordering_tests {
    use super::*;
    use std::collections::HashMap;

    fn item(id: &str) -> DbQueueItem {
        DbQueueItem {
            track_id: id.to_string(),
            title: format!("Title {id}"),
            artist_names: "Artist Name".to_string(),
            duration_ms: Some(1000),
            album_title: "Album Title".to_string(),
            cover_image_id: Some(format!("rel-{id}")),
        }
    }

    #[test]
    fn preserves_duplicate_queue_entries_in_order() {
        let mut by_id = HashMap::new();
        by_id.insert("a".to_string(), item("a"));
        by_id.insert("b".to_string(), item("b"));

        // The same track queued twice must resolve twice, in position order.
        let ordered =
            order_queue_items(&by_id, &["a".to_string(), "a".to_string(), "b".to_string()]);

        let ids: Vec<&str> = ordered.iter().map(|i| i.track_id.as_str()).collect();
        assert_eq!(ids, vec!["a", "a", "b"]);
    }

    #[test]
    fn skips_unknown_ids() {
        let mut by_id = HashMap::new();
        by_id.insert("a".to_string(), item("a"));
        let ordered = order_queue_items(&by_id, &["a".to_string(), "missing".to_string()]);
        assert_eq!(ordered.len(), 1);
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
            "INSERT INTO releases (id, album_id, metadata_source, managed, _updated_at, created_at) \
             VALUES ('rel-1', 'album-1', 'file_tags', 1, ?, ?)",
            params![now, now],
        )
        .unwrap();
        conn
    }

    #[test]
    fn audio_key_omits_source_folder_when_release_has_none() {
        // The seeded release has no source_folder_name (a non-folder import).
        let conn = seeded_conn();
        let key = resolve_audio_cloud_path(&conn, "rel-1", "01 Track Title.flac").unwrap();
        assert_eq!(key, "storage/album-1/rel-1/01 Track Title.flac");
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
        assert_eq!(
            key,
            "storage/album-1/rel-1/Album Folder [FLAC]/01 Track Title.flac"
        );
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

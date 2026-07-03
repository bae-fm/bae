use super::*;

impl Database {
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

                let mut composer_stmt = conn.prepare(&composer_summary_query(
                    Some("WHERE composer.name LIKE ? OR composer.sort_name LIKE ?"),
                    Some("ORDER BY composer.name LIMIT ?"),
                ))?;
                let composers = composer_stmt
                    .query_map(params![pattern, pattern, limit_i64], row_to_composer_summary)?
                    .collect::<coven::rusqlite::Result<Vec<_>>>()?;

                let mut work_stmt = conn.prepare(&work_summary_query(
                    Some("WHERE w.title LIKE ?"),
                    Some("ORDER BY w.title LIMIT ?"),
                ))?;
                let works = work_stmt
                    .query_map(params![pattern, limit_i64], row_to_work_summary)?
                    .collect::<coven::rusqlite::Result<Vec<_>>>()?;

                Ok(DbLibrarySearchResults {
                    albums,
                    tracks,
                    composers,
                    works,
                })
            })
            .await
    }

    /// Insert a new album
    pub async fn insert_album(&self, album: &DbAlbum) -> Result<(), DbError> {
        let album = album.clone();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            insert_album_row(sql.connection(), &album, &reg)
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
            let rows = stmt.query_map([], row_to_album)?;
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
        let select = album_summary_select();

        let query = format!(
            "{select} \
            FROM albums a \
            {artist_sort_join} \
            ORDER BY {order_by} \
            LIMIT ? OFFSET ?",
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

    /// Resolve an album's 0-based position under a sort.
    ///
    /// Wraps the *identical* `build_order_by` + `album_summary_artist_join`
    /// that `get_album_page` uses in a `ROW_NUMBER() OVER (ORDER BY …)`
    /// window, then selects the row for `album_id`. Because the ORDER BY is
    /// the same, the returned index is exactly the offset at which
    /// `get_album_page` would return this album — the caller can load that
    /// page and scroll to the row deterministically. `None` when the album
    /// isn't in the library.
    pub async fn get_album_index(
        &self,
        sort: &[AlbumSortCriterion],
        album_id: &str,
    ) -> Result<Option<u64>, DbError> {
        let (order_by, needs_artist_sort_join) = build_order_by(sort, "a.created_at DESC");
        let artist_sort_join = album_summary_artist_join(needs_artist_sort_join);
        let album_id = album_id.to_string();

        let query = format!(
            "SELECT idx FROM ( \
                SELECT a.id AS id, \
                    ROW_NUMBER() OVER (ORDER BY {order_by}) - 1 AS idx \
                FROM albums a \
                {artist_sort_join} \
            ) WHERE id = ?"
        );

        self.call(move |conn| {
            conn.query_row(&query, params![album_id], |row| row.get::<_, i64>("idx"))
                .optional()
                .map(|idx| idx.map(|i| i as u64))
                .map_err(DbError::from)
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

    /// Raw album-summary lookup for a single album. Shares the JSON
    /// aggregates with `get_album_page` so the resolver output matches.
    pub async fn find_album_summary(
        &self,
        album_id: &str,
    ) -> Result<Option<DbAlbumSummary>, DbError> {
        let album_id = album_id.to_string();
        let query = format!("{} FROM albums a WHERE a.id = ?", album_summary_select());
        self.call(move |conn| {
            conn.query_row(&query, params![album_id], |row| {
                Ok(parse_album_summary_row(row))
            })
            .optional()?
            .transpose()
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
                row_to_album,
            )
            .optional()
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
                row_to_album,
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

    /// Update album's primary_release_id
    pub async fn set_album_primary_release(
        &self,
        album_id: &str,
        primary_release_id: &str,
    ) -> Result<(), DbError> {
        let (album_id, primary_release_id) = (album_id.to_string(), primary_release_id.to_string());
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            let conn = sql.connection();
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
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            let conn = sql.connection();
            conn.execute(
                "UPDATE albums SET primary_release_id = NULL, _updated_at = ? WHERE id = ?",
                params![reg, album_id],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }
}

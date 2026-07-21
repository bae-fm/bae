use super::*;

fn escape_like_pattern(value: &str) -> String {
    value
        .replace('\\', r"\\")
        .replace('%', r"\%")
        .replace('_', r"\_")
}

impl Database {
    /// Track ids for an album's primary release. `None` means no album carries
    /// this id; `Some(vec![])` means the album exists but its primary release has
    /// no tracks.
    pub async fn get_primary_release_track_ids_for_album(
        &self,
        album_id: &str,
    ) -> Result<Option<Vec<String>>, DbError> {
        let Some(album) = self.find_album_by_id(album_id).await? else {
            return Ok(None);
        };

        let releases = self.get_releases_for_album(album_id).await?;
        let release_id = resolve_primary_release_id(
            album.primary_release_id.as_deref(),
            releases.iter().map(|release| release.id.as_str()),
        );

        let Some(release_id) = release_id else {
            return Ok(Some(Vec::new()));
        };

        self.get_track_ids_for_release(&release_id).await.map(Some)
    }

    /// Map each track id to its album id (track → release → album). Track ids that
    /// aren't in the library are absent from the map.
    pub async fn get_album_ids_for_tracks(
        &self,
        track_ids: &[String],
    ) -> Result<HashMap<String, String>, DbError> {
        if track_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let track_ids = track_ids.to_vec();
        self.read(move |conn| {
            let mut album_ids = HashMap::new();
            for chunk in track_ids.chunks(SQL_MAX_IN_VARS) {
                let placeholders = in_clause_placeholders(chunk.len());
                let sql = format!(
                    "SELECT t.id AS track_id, r.album_id FROM tracks t \
                         JOIN releases r ON t.release_id = r.id \
                         WHERE t.id IN ({placeholders})"
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows =
                    stmt.query_map(coven::rusqlite::params_from_iter(chunk.iter()), |row| {
                        Ok((
                            row.get::<_, String>("track_id")?,
                            row.get::<_, String>("album_id")?,
                        ))
                    })?;
                album_ids.extend(rows.collect::<coven::rusqlite::Result<HashMap<_, _>>>()?);
            }
            Ok(album_ids)
        })
        .await
    }

    /// Map each release id to its album id. Release ids that aren't in the library
    /// are absent from the map. The release sibling of
    /// [`get_album_ids_for_tracks`](Self::get_album_ids_for_tracks) — a changeset
    /// that carries a cover but not its release needs it to reach the album.
    pub async fn get_album_ids_for_releases(
        &self,
        release_ids: &[String],
    ) -> Result<HashMap<String, String>, DbError> {
        if release_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let release_ids = release_ids.to_vec();
        self.read(move |conn| {
            let mut album_ids = HashMap::new();
            for chunk in release_ids.chunks(SQL_MAX_IN_VARS) {
                let placeholders = in_clause_placeholders(chunk.len());
                let sql = format!("SELECT id, album_id FROM releases WHERE id IN ({placeholders})");
                let mut stmt = conn.prepare(&sql)?;
                let rows =
                    stmt.query_map(coven::rusqlite::params_from_iter(chunk.iter()), |row| {
                        Ok((
                            row.get::<_, String>("id")?,
                            row.get::<_, String>("album_id")?,
                        ))
                    })?;
                album_ids.extend(rows.collect::<coven::rusqlite::Result<HashMap<_, _>>>()?);
            }
            Ok(album_ids)
        })
        .await
    }

    /// Search albums, tracks, composers, and works by title/name.
    pub async fn search_library(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<DbLibrarySearchResults, DbError> {
        let pattern = format!("%{}%", escape_like_pattern(query));
        let limit_i64 = limit as i64;

        self.read(move |conn| {
            // The stored `primary_release_id` and the album's release ids come
            // back raw; `resolve_primary_release_id` applies the fallback.
            let album_query = format!(
                r#"
                        SELECT a.id, a.title, a.year, a.primary_release_id,
                               {release_ids} AS release_ids_json,
                               art.name as artist_name
                        FROM albums a
                        JOIN artists art ON a.artist_id = art.id
                        WHERE a.title LIKE ? ESCAPE '\'
                           OR art.name LIKE ? ESCAPE '\'
                        ORDER BY a.title
                        LIMIT ?
                        "#,
                release_ids = album_release_ids_json_sql()
            );
            let mut album_stmt = conn.prepare(&album_query)?;
            let albums = album_stmt
                .query_map(params![pattern, pattern, limit_i64], |row| {
                    let release_ids_json: String = row.get("release_ids_json")?;
                    let release_ids: Vec<String> = serde_json::from_str(&release_ids_json)
                        .map_err(|e| {
                            coven::rusqlite::Error::FromSqlConversionFailure(
                                0,
                                coven::rusqlite::types::Type::Text,
                                format!("malformed release_ids_json: {e}").into(),
                            )
                        })?;
                    Ok(DbAlbumSearchResult {
                        id: row.get("id")?,
                        title: row.get("title")?,
                        year: row.get("year")?,
                        primary_release_id: row.get("primary_release_id")?,
                        release_ids,
                        artist_name: row.get("artist_name")?,
                    })
                })?
                .collect::<coven::rusqlite::Result<Vec<_>>>()?;

            let mut track_stmt = conn.prepare(
                r#"
                        SELECT t.id, t.title, t.duration_ms, t.release_id,
                               r.album_id,
                               a.title as album_title,
                               art.name as artist_name
                        FROM tracks t
                        JOIN releases r ON t.release_id = r.id
                        JOIN albums a ON r.album_id = a.id
                        JOIN artists art ON a.artist_id = art.id
                        WHERE t.title LIKE ? ESCAPE '\'
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
                        release_id: row.get("release_id")?,
                        album_id: row.get("album_id")?,
                        album_title: row.get("album_title")?,
                        artist_name: row.get("artist_name")?,
                    })
                })?
                .collect::<coven::rusqlite::Result<Vec<_>>>()?;

            let mut artist_stmt = conn.prepare(&artist_summary_query(
                Some(
                    "WHERE ar.name LIKE ? ESCAPE '\\' \
                         OR ar.sort_name LIKE ? ESCAPE '\\'",
                ),
                Some("ORDER BY ar.name LIMIT ?"),
            ))?;
            let artists = artist_stmt
                .query_map(params![pattern, pattern, limit_i64], row_to_artist_summary)?
                .collect::<coven::rusqlite::Result<Vec<_>>>()?;

            let mut composer_stmt = conn.prepare(&composer_summary_query(
                Some(
                    "WHERE composer.name LIKE ? ESCAPE '\\' \
                         OR composer.sort_name LIKE ? ESCAPE '\\'",
                ),
                Some("ORDER BY composer.name LIMIT ?"),
            ))?;
            let composers = composer_stmt
                .query_map(
                    params![pattern, pattern, limit_i64],
                    row_to_composer_summary,
                )?
                .collect::<coven::rusqlite::Result<Vec<_>>>()?;

            let mut work_stmt = conn.prepare(&work_summary_query(
                Some("WHERE w.title LIKE ? ESCAPE '\\'"),
                Some("ORDER BY w.title LIMIT ?"),
            ))?;
            let works = work_stmt
                .query_map(params![pattern, limit_i64], row_to_work_summary)?
                .collect::<coven::rusqlite::Result<Vec<_>>>()?;

            Ok(DbLibrarySearchResults {
                albums,
                artists,
                tracks,
                composers,
                works,
            })
        })
        .await
    }

    pub async fn insert_album(&self, album: &DbAlbum) -> Result<(), DbError> {
        let album = album.clone();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            insert_album_row(sql.tx(), &album, &reg)
        })
        .await
    }

    /// Every album, sorted by `sort` — or `created_at DESC` (newest first) when
    /// `sort` is empty.
    pub async fn get_albums(&self, sort: &[AlbumSortCriterion]) -> Result<Vec<DbAlbum>, DbError> {
        let (order_by, needs_artist_join) = build_order_by(sort, "a.created_at DESC");
        let artist_join = album_summary_artist_join(needs_artist_join);

        let query = format!(
            "SELECT \
                a.id, a.title, a.artist_id, a.year, a.primary_release_id, \
                a.is_compilation, \
                a.created_at \
            FROM albums a \
            {artist_join} \
            ORDER BY {order_by}"
        );

        self.read(move |conn| {
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

        self.read(move |conn| {
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

    /// An album's 0-based position under a sort, or `None` when it isn't in the
    /// library. Wraps the *identical* `build_order_by` + `album_summary_artist_join`
    /// that `get_album_page` uses in a `ROW_NUMBER() OVER (ORDER BY …)` window, so
    /// the index is exactly the offset at which `get_album_page` would return this
    /// album — the caller can load that page and scroll to the row deterministically.
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

        self.read(move |conn| {
            conn.query_row(&query, params![album_id], |row| row.get::<_, i64>("idx"))
                .optional()
                .map(|idx| idx.map(|i| i as u64))
                .map_err(DbError::from)
        })
        .await
    }

    pub async fn get_album_count(&self) -> Result<u64, DbError> {
        self.read(move |conn| {
            conn.query_row("SELECT COUNT(*) FROM albums", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|c| c as u64)
            .map_err(DbError::from)
        })
        .await
    }

    /// Raw album-summary lookup for a single album. Shares the JSON aggregates with
    /// `get_album_page` so the resolver output matches.
    pub async fn find_album_summary(
        &self,
        album_id: &str,
    ) -> Result<Option<DbAlbumSummary>, DbError> {
        let album_id = album_id.to_string();
        let query = format!("{} FROM albums a WHERE a.id = ?", album_summary_select());
        self.read(move |conn| {
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
        self.read(move |conn| find_album_by_id_on(conn, &album_id))
            .await
    }

    /// Follow `DbRelease.album_id` → `DbAlbum`. FK navigation — the row must
    /// exist. See the method conventions above.
    pub async fn get_album_for_release(&self, release: &DbRelease) -> Result<DbAlbum, DbError> {
        let album_id = release.album_id.clone();
        self.read(move |conn| {
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

    /// The raw album-detail aggregate: the album, its artists, and its releases
    /// with each one's tracks, files, audio rows, and identities. No formatting, no
    /// derivation — `LibraryManager` resolves this into `AlbumDetail`. `None` for
    /// an unknown album, or one with no releases.
    pub async fn find_album_detail(
        &self,
        album_id: &str,
    ) -> Result<Option<DbAlbumDetail>, DbError> {
        let album_id = album_id.to_string();
        self.read(move |conn| {
            let Some(album) = find_album_by_id_on(conn, &album_id)? else {
                return Ok(None);
            };

            let artists = get_artists_for_album_on(conn, &album_id)?;
            let db_releases = get_releases_for_album_on(conn, &album_id)?;
            if db_releases.is_empty() {
                return Ok(None);
            }

            let mut releases = Vec::with_capacity(db_releases.len());
            for release in db_releases {
                releases.push(build_release_detail_on(conn, release)?);
            }

            Ok(Some(DbAlbumDetail {
                album,
                artists,
                releases,
            }))
        })
        .await
    }
    /// Find album_id for a release. Caller-provided ID — may not exist.
    pub async fn find_album_id_for_release(
        &self,
        release_id: &str,
    ) -> Result<Option<String>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |conn| {
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

    pub async fn delete_album_with_cleanup(
        &self,
        album_id: &str,
        cleanups: Vec<DeleteCleanupPlan>,
    ) -> Result<(), DbError> {
        let album_id = album_id.to_string();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            let conn = sql.tx();
            for cleanup in &cleanups {
                apply_delete_cleanup_on(conn, cleanup, &reg)?;
            }
            conn.execute("DELETE FROM albums WHERE id = ?", params![album_id])?;
            Ok(())
        })
        .await
    }

    pub async fn set_album_primary_release(
        &self,
        album_id: &str,
        primary_release_id: &str,
    ) -> Result<(), DbError> {
        let (album_id, primary_release_id) = (album_id.to_string(), primary_release_id.to_string());
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            let conn = sql.tx();
            conn.execute(
                "UPDATE albums SET primary_release_id = ?, _updated_at = ? WHERE id = ?",
                params![primary_release_id, reg, album_id],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }
}

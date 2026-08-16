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
        self.read(move |sql| {
            let mut album_ids = HashMap::new();
            for chunk in track_ids.chunks(SQL_MAX_IN_VARS) {
                let placeholders = in_clause_placeholders(chunk.len());
                let query = format!(
                    "SELECT t.id AS track_id, r.album_id FROM tracks t \
                         JOIN releases r ON t.release_id = r.id \
                         WHERE t.id IN ({placeholders})"
                );
                album_ids.extend(sql.query(
                    &query,
                    coven::rusqlite::params_from_iter(chunk.iter()),
                    |row| {
                        Ok((
                            row.get::<_, String>("track_id")?,
                            row.get::<_, String>("album_id")?,
                        ))
                    },
                )?);
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
        self.read(move |sql| {
            let mut album_ids = HashMap::new();
            for chunk in release_ids.chunks(SQL_MAX_IN_VARS) {
                let placeholders = in_clause_placeholders(chunk.len());
                let query =
                    format!("SELECT id, album_id FROM releases WHERE id IN ({placeholders})");
                album_ids.extend(sql.query(
                    &query,
                    coven::rusqlite::params_from_iter(chunk.iter()),
                    |row| {
                        Ok((
                            row.get::<_, String>("id")?,
                            row.get::<_, String>("album_id")?,
                        ))
                    },
                )?);
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
        self.read(move |sql| search_library_on(&sql, &pattern, limit_i64))
            .await
    }

    pub(crate) fn subscribe_library_search(
        &self,
        query: &str,
        limit: usize,
    ) -> coven::LiveQuery<LibrarySearchProjection> {
        let pattern = format!("%{}%", escape_like_pattern(query));
        let limit_i64 = limit as i64;
        self.inner.handle.subscribe(move |sql| {
            let results = search_library_on(&sql, &pattern, limit_i64).map_err(CovenError::from)?;
            let release_ids = search_release_ids(&results);
            let artist_ids = results
                .artists
                .iter()
                .map(|artist| artist.artist.id.clone())
                .chain(
                    results
                        .composers
                        .iter()
                        .map(|composer| composer.artist.id.clone()),
                )
                .collect::<Vec<_>>();
            let cover_versions =
                super::blobs::image_versions_on(&sql, LibraryImageType::Cover, &release_ids)
                    .map_err(CovenError::from)?;
            let artist_image_versions =
                super::blobs::image_versions_on(&sql, LibraryImageType::Artist, &artist_ids)
                    .map_err(CovenError::from)?;
            Ok(LibrarySearchProjection {
                results,
                cover_versions,
                artist_image_versions,
            })
        })
    }

    pub async fn insert_album(&self, album: &DbAlbum) -> Result<(), DbError> {
        let album = album.clone();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            insert_album_row(&sql, &album, &reg)
        })
        .await
    }

    #[cfg(test)]
    pub(crate) async fn rename_album_for_test(
        &self,
        album_id: &str,
        title: &str,
    ) -> Result<(), DbError> {
        let album_id = album_id.to_string();
        let title = title.to_string();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            sql.execute(
                "UPDATE albums SET title = ?1, _updated_at = ?2 WHERE id = ?3",
                params![title, reg, album_id],
            )
            .map(|_| ())
            .map_err(DbError::from)
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

        self.read(move |sql| sql.query(&query, [], row_to_album).map_err(DbError::from))
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

        self.read(move |sql| {
            sql.query(&query, params![limit as i64, offset as i64], |row| {
                Ok(parse_album_summary_row(row))
            })?
            .into_iter()
            .collect()
        })
        .await
    }

    pub(crate) fn subscribe_album_page(
        &self,
        sort: &[AlbumSortCriterion],
        offset: u64,
        limit: u64,
    ) -> coven::LiveQuery<AlbumPageProjection> {
        let (order_by, needs_artist_sort_join) = build_order_by(sort, "a.created_at DESC");
        let artist_sort_join = album_summary_artist_join(needs_artist_sort_join);
        let select = album_summary_select();
        let query = format!(
            "{select} FROM albums a {artist_sort_join} ORDER BY {order_by} LIMIT ? OFFSET ?"
        );
        self.inner.handle.subscribe(move |sql| {
            let (rows, cover_versions) =
                album_rows_with_covers_on(&sql, &query, params![limit as i64, offset as i64])?;
            let total_count = album_count_on(&sql).map_err(CovenError::from)?;
            Ok(AlbumPageProjection {
                rows,
                cover_versions,
                total_count,
            })
        })
    }

    pub(crate) fn subscribe_album_browse(
        &self,
        sort: &[AlbumSortCriterion],
        initial_windows: crate::library::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<crate::library::LibraryPageWindows, AlbumBrowseProjection>
    {
        let (order_by, needs_artist_sort_join) = build_order_by(sort, "a.created_at DESC");
        let artist_sort_join = album_summary_artist_join(needs_artist_sort_join);
        let page_query = format!(
            "{} FROM albums a {artist_sort_join} ORDER BY {order_by} LIMIT ? OFFSET ?",
            album_summary_select(),
        );
        let dependency_query = format!(
            "{} FROM albums a {artist_sort_join} ORDER BY {order_by}",
            album_summary_select(),
        );
        self.inner
            .handle
            .subscribe_reconfigurable(initial_windows, move |requested, sql| {
                let total_count = album_count_on(&sql).map_err(CovenError::from)?;
                let dependency_rows = album_rows_on(&sql, &dependency_query, [])?;
                let release_ids = dependency_rows
                    .iter()
                    .flat_map(|row| row.release_ids.iter().cloned())
                    .collect::<Vec<_>>();
                let cover_versions =
                    super::blobs::image_versions_on(&sql, LibraryImageType::Cover, &release_ids)
                        .map_err(CovenError::from)?;
                let windows = requested
                    .iter()
                    .map(|window| {
                        let rows = album_rows_on(
                            &sql,
                            &page_query,
                            params![window.limit as i64, window.offset as i64],
                        )?;
                        Ok(crate::library::LibraryBrowseWindow {
                            window: window.clone(),
                            rows,
                        })
                    })
                    .collect::<Result<Vec<_>, CovenError>>()?;
                Ok(AlbumBrowseProjection {
                    windows,
                    cover_versions,
                    total_count,
                })
            })
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

        self.read(move |sql| {
            sql.query_row(&query, params![album_id], |row| row.get::<_, i64>("idx"))
                .optional()
                .map(|idx| idx.map(|i| i as u64))
                .map_err(DbError::from)
        })
        .await
    }

    pub async fn get_album_count(&self) -> Result<u64, DbError> {
        self.read(|sql| album_count_on(&sql)).await
    }

    /// Raw album-summary lookup for a single album. Shares the JSON aggregates with
    /// `get_album_page` so the resolver output matches.
    pub async fn find_album_summary(
        &self,
        album_id: &str,
    ) -> Result<Option<DbAlbumSummary>, DbError> {
        let album_id = album_id.to_string();
        let query = format!("{} FROM albums a WHERE a.id = ?", album_summary_select());
        self.read(move |sql| {
            sql.query_row(&query, params![album_id], |row| {
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
        self.read(move |sql| find_album_by_id_on(&sql, &album_id))
            .await
    }

    /// Follow `DbRelease.album_id` → `DbAlbum`. FK navigation — the row must
    /// exist. See the method conventions above.
    pub async fn get_album_for_release(&self, release: &DbRelease) -> Result<DbAlbum, DbError> {
        let album_id = release.album_id.clone();
        self.read(move |sql| {
            sql.query_row(
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
        self.read(move |sql| find_album_detail_on(&sql, &album_id))
            .await
    }

    pub(crate) fn subscribe_album_detail(
        &self,
        album_id: &str,
    ) -> coven::LiveQuery<AlbumDetailProjection> {
        let album_id = album_id.to_string();
        self.inner.handle.subscribe(move |sql| {
            let detail = find_album_detail_on(&sql, &album_id).map_err(CovenError::from)?;
            let release_ids = match &detail {
                Some(detail) => detail
                    .releases
                    .iter()
                    .map(|release| release.release.id.clone())
                    .collect::<Vec<_>>(),
                None => Vec::new(),
            };
            let cover_versions =
                super::blobs::image_versions_on(&sql, LibraryImageType::Cover, &release_ids)
                    .map_err(CovenError::from)?;
            Ok(AlbumDetailProjection {
                detail,
                cover_versions,
            })
        })
    }
    /// Find album_id for a release. Caller-provided ID — may not exist.
    pub async fn find_album_id_for_release(
        &self,
        release_id: &str,
    ) -> Result<Option<String>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| {
            sql.query_row(
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
            for cleanup in &cleanups {
                apply_delete_cleanup_on(&sql, cleanup)?;
            }
            sql.execute("DELETE FROM albums WHERE id = ?", params![album_id])?;
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
            sql.execute(
                "UPDATE albums SET primary_release_id = ?, _updated_at = ? WHERE id = ?",
                params![primary_release_id, reg, album_id],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }
}

fn search_library_on(
    sql: &SqlReadContext<'_>,
    pattern: &str,
    limit: i64,
) -> Result<DbLibrarySearchResults, DbError> {
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
    let albums = sql.query(&album_query, params![pattern, pattern, limit], |row| {
        let release_ids_json: String = row.get("release_ids_json")?;
        let release_ids: Vec<String> = serde_json::from_str(&release_ids_json).map_err(|e| {
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
    })?;
    let tracks = sql.query(
        r#"
            SELECT t.id, t.title, t.duration_ms, t.release_id,
                   r.album_id, a.title as album_title, art.name as artist_name
            FROM tracks t
            JOIN releases r ON t.release_id = r.id
            JOIN albums a ON r.album_id = a.id
            JOIN artists art ON a.artist_id = art.id
            WHERE t.title LIKE ? ESCAPE '\'
            ORDER BY t.title
            LIMIT ?
            "#,
        params![pattern, limit],
        |row| {
            Ok(DbTrackSearchResult {
                id: row.get("id")?,
                title: row.get("title")?,
                duration_ms: row.get("duration_ms")?,
                release_id: row.get("release_id")?,
                album_id: row.get("album_id")?,
                album_title: row.get("album_title")?,
                artist_name: row.get("artist_name")?,
            })
        },
    )?;
    let artists = sql.query(
        &artist_summary_query(
            Some("WHERE ar.name LIKE ? ESCAPE '\\' OR ar.sort_name LIKE ? ESCAPE '\\'"),
            Some("ORDER BY ar.name LIMIT ?"),
        ),
        params![pattern, pattern, limit],
        row_to_artist_summary,
    )?;
    let composers = sql.query(
        &composer_summary_query(
            Some(
                "WHERE composer.name LIKE ? ESCAPE '\\' \
                 OR composer.sort_name LIKE ? ESCAPE '\\'",
            ),
            Some("ORDER BY composer.name LIMIT ?"),
        ),
        params![pattern, pattern, limit],
        row_to_composer_summary,
    )?;
    let works = sql.query(
        &work_summary_query(
            Some("WHERE w.title LIKE ? ESCAPE '\\'"),
            Some("ORDER BY w.title LIMIT ?"),
        ),
        params![pattern, limit],
        row_to_work_summary,
    )?;
    Ok(DbLibrarySearchResults {
        albums,
        artists,
        tracks,
        composers,
        works,
    })
}

fn search_release_ids(results: &DbLibrarySearchResults) -> Vec<String> {
    let mut release_ids = results
        .albums
        .iter()
        .filter_map(|album| {
            resolve_primary_release_id(
                album.primary_release_id.as_deref(),
                album.release_ids.iter().map(String::as_str),
            )
        })
        .collect::<Vec<_>>();
    release_ids.extend(results.tracks.iter().map(|track| track.release_id.clone()));
    release_ids.extend(
        results
            .works
            .iter()
            .filter_map(|work| work.representative_release_id.clone()),
    );
    release_ids
}

fn find_album_detail_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
) -> Result<Option<DbAlbumDetail>, DbError> {
    let Some(album) = find_album_by_id_on(sql, album_id)? else {
        return Ok(None);
    };
    let artists = get_artists_for_album_on(sql, album_id)?;
    let db_releases = get_releases_for_album_on(sql, album_id)?;
    if db_releases.is_empty() {
        return Ok(None);
    }
    let releases = db_releases
        .into_iter()
        .map(|release| build_release_detail_on(sql, release))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(DbAlbumDetail {
        album,
        artists,
        releases,
    }))
}

fn album_count_on(sql: &SqlReadContext<'_>) -> Result<u64, DbError> {
    sql.query_row("SELECT COUNT(*) FROM albums", [], |row| {
        row.get::<_, i64>(0)
    })
    .map(|count| count as u64)
    .map_err(DbError::from)
}

fn album_rows_with_covers_on<P: Params>(
    sql: &SqlReadContext<'_>,
    query: &str,
    params: P,
) -> Result<(Vec<DbAlbumSummary>, HashMap<String, String>), CovenError> {
    let rows = album_rows_on(sql, query, params)?;
    let release_ids = rows
        .iter()
        .flat_map(|row| row.release_ids.iter().cloned())
        .collect::<Vec<_>>();
    let cover_versions =
        super::blobs::image_versions_on(sql, LibraryImageType::Cover, &release_ids)
            .map_err(CovenError::from)?;
    Ok((rows, cover_versions))
}

fn album_rows_on<P: Params>(
    sql: &SqlReadContext<'_>,
    query: &str,
    params: P,
) -> Result<Vec<DbAlbumSummary>, CovenError> {
    sql.query(query, params, |row| Ok(parse_album_summary_row(row)))?
        .into_iter()
        .collect::<Result<Vec<_>, DbError>>()
        .map_err(CovenError::from)
}

#[derive(Debug, Clone)]
pub struct AlbumPageProjection {
    pub rows: Vec<DbAlbumSummary>,
    pub cover_versions: HashMap<String, String>,
    pub total_count: u64,
}

#[derive(Debug, Clone)]
pub struct AlbumBrowseProjection {
    pub windows: Vec<crate::library::LibraryBrowseWindow<DbAlbumSummary>>,
    pub cover_versions: HashMap<String, String>,
    pub total_count: u64,
}

#[derive(Debug, Clone)]
pub struct AlbumDetailProjection {
    pub detail: Option<DbAlbumDetail>,
    pub cover_versions: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct LibrarySearchProjection {
    pub results: DbLibrarySearchResults,
    pub cover_versions: HashMap<String, String>,
    pub artist_image_versions: HashMap<String, String>,
}

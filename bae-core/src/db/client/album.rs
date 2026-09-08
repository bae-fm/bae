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

    /// Search albums, tracks, composers, and works by title/name.
    pub async fn search_library(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<DbLibrarySearchResults, DbError> {
        let pattern = format!("%{}%", escape_like_pattern(query));
        let limit_i64 = limit as i64;
        self.read(move |sql| search_library_on(&sql, &pattern, limit_i64))
            .process(LibrarySearchRows::process)
            .await
    }

    pub(crate) fn subscribe_library_search(
        &self,
        query: &str,
        limit: usize,
    ) -> coven::LiveQuery<LibrarySearchProjection> {
        let pattern = format!("%{}%", escape_like_pattern(query));
        let limit_i64 = limit as i64;
        self.inner
            .handle
            .subscribe(move |sql| {
                let results =
                    search_library_on(&sql, &pattern, limit_i64).map_err(CovenError::from)?;
                let album_ids = results
                    .albums
                    .iter()
                    .map(|album| album.id.clone())
                    .collect::<Vec<_>>();
                let release_ids = results
                    .tracks
                    .iter()
                    .map(|track| track.release_id.clone())
                    .chain(
                        results
                            .works
                            .iter()
                            .filter_map(|work| work.representative_release_id.clone()),
                    )
                    .collect::<Vec<_>>();
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
                let mut cover_versions = album_cover_versions_on(&sql, &album_ids)?;
                cover_versions.extend(super::blobs::image_versions_on(
                    &sql,
                    LibraryImageType::Cover,
                    &release_ids,
                )?);
                let artist_image_versions =
                    super::blobs::image_versions_on(&sql, LibraryImageType::Artist, &artist_ids)
                        .map_err(CovenError::from)?;
                Ok((results, cover_versions, artist_image_versions))
            })
            .process(|(results, mut cover_versions, artist_image_versions)| {
                let results = results.process()?;
                let release_ids = search_release_ids(&results)
                    .into_iter()
                    .collect::<HashSet<_>>();
                cover_versions.retain(|id, _| release_ids.contains(id));
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
            Ok(sql.query(
                &query,
                params![limit as i64, offset as i64],
                AlbumSummaryRow::read,
            )?)
        })
        .process(|rows| rows.into_iter().map(AlbumSummaryRow::process).collect())
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
        self.inner
            .handle
            .subscribe(move |sql| {
                let (rows, cover_versions) =
                    album_rows_with_covers_on(&sql, &query, params![limit as i64, offset as i64])?;
                let total_count = album_count_on(&sql).map_err(CovenError::from)?;
                Ok((rows, cover_versions, total_count))
            })
            .process(|(rows, cover_versions, total_count)| {
                Ok(AlbumPageProjection {
                    rows: rows
                        .into_iter()
                        .map(AlbumSummaryRow::process)
                        .collect::<Result<_, _>>()?,
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
                let album_ids = dependency_rows
                    .iter()
                    .map(|row| row.id.clone())
                    .collect::<Vec<_>>();
                let cover_versions = album_cover_versions_on(&sql, &album_ids)?;
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
                Ok((windows, cover_versions, total_count))
            })
            .process(|_, (windows, cover_versions, total_count)| {
                let windows = windows
                    .into_iter()
                    .map(|window| {
                        Ok(crate::library::LibraryBrowseWindow {
                            window: window.window,
                            rows: window
                                .rows
                                .into_iter()
                                .map(AlbumSummaryRow::process)
                                .collect::<Result<_, _>>()?,
                        })
                    })
                    .collect::<Result<_, CovenError>>()?;
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
            .process(|rows| Ok(rows.map(AlbumDetailRows::process)))
            .await
    }

    pub(crate) fn subscribe_album_detail(
        &self,
        album_id: &str,
    ) -> coven::LiveQuery<AlbumDetailProjection> {
        let album_id = album_id.to_string();
        self.inner
            .handle
            .subscribe(move |sql| {
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
                Ok((detail, cover_versions))
            })
            .process(|(detail, cover_versions)| {
                Ok(AlbumDetailProjection {
                    detail: detail.map(AlbumDetailRows::process),
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
) -> Result<LibrarySearchRows, DbError> {
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
        Ok(AlbumSearchRow {
            id: row.get("id")?,
            title: row.get("title")?,
            year: row.get("year")?,
            primary_release_id: row.get("primary_release_id")?,
            release_ids_json: row.get("release_ids_json")?,
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
    Ok(LibrarySearchRows {
        albums,
        artists,
        tracks,
        composers,
        works,
    })
}

struct AlbumSearchRow {
    id: String,
    title: String,
    year: Option<i32>,
    primary_release_id: Option<String>,
    release_ids_json: String,
    artist_name: String,
}

struct LibrarySearchRows {
    albums: Vec<AlbumSearchRow>,
    artists: Vec<DbArtistSummary>,
    tracks: Vec<DbTrackSearchResult>,
    composers: Vec<DbComposerSummary>,
    works: Vec<DbWorkSummary>,
}

impl LibrarySearchRows {
    fn process(self) -> Result<DbLibrarySearchResults, DbError> {
        let albums = self
            .albums
            .into_iter()
            .map(|row| {
                let release_ids = serde_json::from_str(&row.release_ids_json).map_err(|error| {
                    coven::rusqlite::Error::FromSqlConversionFailure(
                        0,
                        coven::rusqlite::types::Type::Text,
                        format!("malformed release_ids_json: {error}").into(),
                    )
                })?;
                Ok(DbAlbumSearchResult {
                    id: row.id,
                    title: row.title,
                    year: row.year,
                    primary_release_id: row.primary_release_id,
                    release_ids,
                    artist_name: row.artist_name,
                })
            })
            .collect::<Result<_, DbError>>()?;
        Ok(DbLibrarySearchResults {
            albums,
            artists: self.artists,
            tracks: self.tracks,
            composers: self.composers,
            works: self.works,
        })
    }
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
) -> Result<Option<AlbumDetailRows>, DbError> {
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
    Ok(Some(AlbumDetailRows {
        album,
        artists,
        releases,
    }))
}

struct AlbumDetailRows {
    album: DbAlbum,
    artists: Vec<DbArtist>,
    releases: Vec<ReleaseDetailRows>,
}

impl AlbumDetailRows {
    fn process(self) -> DbAlbumDetail {
        DbAlbumDetail {
            album: self.album,
            artists: self.artists,
            releases: self
                .releases
                .into_iter()
                .map(ReleaseDetailRows::process)
                .collect(),
        }
    }
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
) -> Result<(Vec<AlbumSummaryRow>, HashMap<String, String>), CovenError> {
    let rows = album_rows_on(sql, query, params)?;
    let album_ids = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
    let cover_versions = album_cover_versions_on(sql, &album_ids)?;
    Ok((rows, cover_versions))
}

fn album_rows_on<P: Params>(
    sql: &SqlReadContext<'_>,
    query: &str,
    params: P,
) -> Result<Vec<AlbumSummaryRow>, CovenError> {
    Ok(sql.query(query, params, AlbumSummaryRow::read)?)
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlbumPageProjection {
    pub rows: Vec<DbAlbumSummary>,
    pub cover_versions: HashMap<String, String>,
    pub total_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlbumBrowseProjection {
    pub windows: Vec<crate::library::LibraryBrowseWindow<DbAlbumSummary>>,
    pub cover_versions: HashMap<String, String>,
    pub total_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlbumDetailProjection {
    pub detail: Option<DbAlbumDetail>,
    pub cover_versions: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibrarySearchProjection {
    pub results: DbLibrarySearchResults,
    pub cover_versions: HashMap<String, String>,
    pub artist_image_versions: HashMap<String, String>,
}

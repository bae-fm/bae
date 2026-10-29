use super::*;

use crate::library::search::search_pattern;

impl Database {
    /// Track ids for an album's primary release. `None` means no album carries
    /// this id; `Some(vec![])` means the album exists but holds no release, or
    /// its primary release has no tracks.
    pub async fn get_primary_release_track_ids_for_album(
        &self,
        album_id: &str,
    ) -> Result<Option<Vec<String>>, DbError> {
        let album_id = album_id.to_string();
        let primary: Option<Option<String>> = self
            .read(move |sql| {
                sql.query_row(
                    &format!(
                        "SELECT {} FROM albums a WHERE a.id = ?",
                        album_primary_release_sql()
                    ),
                    params![album_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(DbError::from)
            })
            .await?;
        match primary {
            None => Ok(None),
            Some(None) => Ok(Some(Vec::new())),
            Some(Some(release_id)) => self.get_track_ids_for_release(&release_id).await.map(Some),
        }
    }

    /// Search albums, tracks, composers, and works by title/name.
    pub async fn search_library(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<DbLibrarySearchResults, DbError> {
        let pattern = search_pattern(query);
        let limit_i64 = limit as i64;
        self.read(move |sql| search_library_on(&sql, &pattern, limit_i64))
            .process(LibrarySearchRows::process)
            .await
    }

    /// Follow the results of one library search. The request is the parsed
    /// query; `None` is no search, which reads nothing and delivers no results.
    /// A new query moves this subscription's request rather than opening
    /// another.
    pub(crate) fn subscribe_library_search(
        &self,
        initial: Option<crate::library::LibrarySearchQuery>,
        limit: usize,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::library::LibrarySearchQuery>,
        LibrarySearchProjection,
    > {
        let limit_i64 = limit as i64;
        self.inner
            .handle
            .subscribe_reconfigurable(initial, move |request, sql| {
                let Some(query) = request else {
                    return Ok(None);
                };
                let pattern = search_pattern(query.as_str());
                Ok(Some(
                    search_library_on(&sql, &pattern, limit_i64).map_err(CovenError::from)?,
                ))
            })
            .process(|raw| {
                let Some(results) = raw else {
                    return Ok(LibrarySearchProjection::default());
                };
                Ok(LibrarySearchProjection {
                    results: results.process()?,
                })
            })
    }

    pub async fn insert_album(&self, album: &DbAlbum) -> Result<(), DbError> {
        let album = album.clone();
        self.call_sql(move |sql| insert_album_row(&sql, &album))
            .await
    }

    /// Every album, sorted by `sort` — or `created_at DESC` (newest first) when
    /// `sort` is empty.
    pub async fn get_albums(&self, sort: &[AlbumSortCriterion]) -> Result<Vec<DbAlbum>, DbError> {
        let (relation, order_by) = album_browse_order(sort, false);

        let query = format!(
            "SELECT \
                a.id, a.title, a.year, \
                a.is_compilation, \
                a.created_at \
            {relation} \
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
        let (relation, order_by) = album_browse_order(sort, false);
        let select = album_summary_select();

        let query = format!(
            "{select} \
            {relation} \
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

    pub(crate) fn subscribe_album_browse(
        &self,
        sort: &[AlbumSortCriterion],
        group_by_artist: bool,
        initial_windows: crate::library::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<crate::library::LibraryPageWindows, AlbumBrowseProjection>
    {
        let (relation, order_by) = album_browse_order(sort, group_by_artist);
        let page_query = format!(
            "{} {relation} \
             ORDER BY {order_by} LIMIT ? OFFSET ?",
            album_summary_select(),
        );
        let section_query = group_by_artist.then(|| {
            album_browse_sections_query(
                sort.iter()
                    .find(|c| c.field == AlbumSortField::Artist)
                    .map_or(SortDirection::Ascending, |c| c.direction),
            )
        });
        self.inner
            .handle
            .subscribe_reconfigurable(initial_windows, move |requested, sql| {
                let total_count = album_count_on(&sql).map_err(CovenError::from)?;
                let sections = match &section_query {
                    Some(query) => sql.query(query, [], |row| {
                        Ok(crate::library::LibraryBrowseSection {
                            id: row.get(0)?,
                            title: row.get(1)?,
                            window: crate::library::LibraryPageWindow {
                                offset: row.get::<_, i64>(2)? as u64,
                                limit: row.get::<_, i64>(3)? as u64,
                            },
                        })
                    })?,
                    None => Vec::new(),
                };
                let row_count = if group_by_artist {
                    sections.iter().map(|section| section.window.limit).sum()
                } else {
                    total_count
                };
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
                Ok((windows, total_count, row_count, sections))
            })
            .process(|(windows, total_count, row_count, sections)| {
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
                    total_count,
                    row_count,
                    sections,
                })
            })
    }

    /// Follow which of the albums `initial` names — the grid's
    /// multi-selection — the library no longer shows, by id. The selection
    /// changes by pointing the same query at new ids through its request
    /// handle, not by opening another one.
    pub(crate) fn subscribe_album_selection(
        &self,
        initial: BTreeSet<String>,
    ) -> coven::ReconfigurableLiveQuery<BTreeSet<String>, Vec<String>> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, move |requested, sql| {
                let requested =
                    serde_json::to_string(requested).expect("album ids serialize as JSON");
                let query = format!(
                    "SELECT requested.value FROM json_each(?1) requested \
                     WHERE NOT EXISTS ( \
                         SELECT 1 FROM albums a \
                         WHERE a.id = requested.value AND {ALBUM_A_IS_SHOWN} \
                     ) \
                     ORDER BY requested.value"
                );
                sql.query(&query, [requested], |row| row.get::<_, String>(0))
                    .map_err(CovenError::from)
            })
    }

    /// The albums of `album_ids` the browse lists, each once at its first
    /// appearance in the browse order `sort` and `group_by_artist` name: the
    /// grid's selection as the grid lists it, whatever order it was picked in.
    pub async fn albums_in_browse_order(
        &self,
        sort: &[AlbumSortCriterion],
        album_ids: Vec<String>,
        group_by_artist: bool,
    ) -> Result<Vec<DbAlbumSummary>, DbError> {
        let (relation, order_by) = album_browse_order(sort, group_by_artist);
        let query = format!(
            "WITH appearance (album_id, idx) AS ( \
                 SELECT a.id, ROW_NUMBER() OVER (ORDER BY {order_by}) \
                 {relation} \
                 WHERE a.id IN (SELECT value FROM json_each(?1))) \
             {select} \
             FROM (SELECT album_id, MIN(idx) AS idx FROM appearance GROUP BY album_id) AS first \
             JOIN album_listing listing ON listing.album_id = first.album_id \
             JOIN albums a ON a.id = first.album_id \
             ORDER BY first.idx",
            select = album_summary_select(),
        );
        let album_ids = serde_json::to_string(&album_ids).expect("album ids serialize as JSON");
        self.read(move |sql| Ok(sql.query(&query, [album_ids], AlbumSummaryRow::read)?))
            .process(|rows| rows.into_iter().map(AlbumSummaryRow::process).collect())
            .await
    }

    /// The album's first appearance in the browse order, or `None` if absent.
    /// Uses the same relation and order as the live page query.
    pub async fn get_album_index(
        &self,
        sort: &[AlbumSortCriterion],
        album_id: &str,
        group_by_artist: bool,
    ) -> Result<Option<u64>, DbError> {
        let (relation, order_by) = album_browse_order(sort, group_by_artist);
        let album_id = album_id.to_string();

        let query = format!(
            "SELECT idx FROM ( \
                SELECT a.id AS id, \
                    ROW_NUMBER() OVER (ORDER BY {order_by}) - 1 AS idx \
                {relation} \
            ) WHERE id = ? ORDER BY idx LIMIT 1"
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
                        id, title, year,
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

    /// Follow the album `initial` names, or none. The detail view moves to
    /// another album through the request handle, not another query.
    pub(crate) fn subscribe_album_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, AlbumDetailProjection> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |album_id, sql| {
                let Some(album_id) = album_id else {
                    return Ok(None);
                };
                find_album_detail_on(&sql, album_id).map_err(CovenError::from)
            })
            .process(|detail| {
                Ok(AlbumDetailProjection {
                    detail: detail.map(AlbumDetailRows::process),
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

    /// Delete the releases the person selected atomically. Refuse if the album
    /// has changed since planning; coven owns retention of unreferenced files.
    /// As for one release, the uploads this abandons are the person's own
    /// doing, and their records are dismissed.
    pub async fn delete_album_with_cleanup(
        &self,
        album_id: &str,
        releases: Vec<ReleaseDeletion>,
    ) -> Result<(), DbError> {
        let rows: Vec<(&str, String)> = releases
            .iter()
            .flat_map(ReleaseDeletion::file_rows)
            .collect();
        let abandoned = self.own_abandonments(&rows).await?;
        let album_id = album_id.to_string();
        self.call_sql(move |sql| {
            let mut current: Vec<String> = sql.query(
                "SELECT id FROM releases WHERE album_id = ? ORDER BY id",
                params![album_id],
                |row| row.get(0),
            )?;
            current.sort();
            let mut planned: Vec<String> = releases
                .iter()
                .filter(|release| release.album_id() == album_id)
                .map(|release| release.release_id().to_string())
                .collect();
            planned.sort();
            if current != planned || planned.len() != releases.len() {
                return Err(DbError::Message(format!(
                    "delete plan for album {album_id} changed after planning"
                )));
            }
            for release in &releases {
                release.apply_on(&sql)?;
            }
            // The releases go; the album row stays, empty, for the reason in
            // `vacate_album_on`.
            sql.execute("DELETE FROM releases WHERE album_id = ?", params![album_id])?;
            Ok(())
        })
        .await?;
        self.dismiss_own_abandonments(&abandoned).await;
        Ok(())
    }

    pub async fn set_album_primary_release(
        &self,
        album_id: &str,
        primary_release_id: &str,
    ) -> Result<(), DbError> {
        let (album_id, primary_release_id) = (album_id.to_string(), primary_release_id.to_string());
        self.call_sql(move |sql| {
            // The release is ordered below the album's lowest, so it is the
            // album's primary on every device that applies this write. One
            // already primary stays where it is.
            let current: Option<String> = sql
                .query_row(
                    &format!(
                        "SELECT {} FROM albums a WHERE a.id = ?",
                        album_primary_release_sql()
                    ),
                    params![album_id],
                    |row| row.get(0),
                )
                .optional()?
                .flatten();
            if current.as_deref() == Some(primary_release_id.as_str()) {
                return Ok(());
            }
            let moved = sql.execute(
                "UPDATE releases SET album_order = \
                     (SELECT MIN(album_order) - 1 FROM releases WHERE album_id = ?1) \
                 WHERE id = ?2 AND album_id = ?1",
                params![album_id, primary_release_id],
            )?;
            if moved != 1 {
                return Err(DbError::Message(format!(
                    "release {primary_release_id} is not one of album {album_id}'s releases"
                )));
            }
            Ok(())
        })
        .await
    }
}

/// The ids of the artists whose name or sort name holds the search pattern
/// `?1` (`artist_search`).
const SEARCH_MATCHED_ARTISTS: &str = "SELECT artist_id FROM artist_search WHERE name GLOB ?1 \
     UNION SELECT artist_id FROM artist_search WHERE sort_name GLOB ?1";

/// The library rows holding the search `pattern` (see [`search_pattern`]),
/// each kind's first `limit` in its order, ties going by id. Each kind finds
/// its rows through its search table, then reads what it shows for those
/// rows alone.
fn search_library_on(
    sql: &SqlReadContext<'_>,
    pattern: &str,
    limit: i64,
) -> Result<LibrarySearchRows, DbError> {
    // An album holds the pattern in its title, or in a credit's name: the
    // name of the artist the credit shows, or the other name it prints.
    let album_query = format!(
        r#"
            SELECT a.id, a.title, a.year,
                   {primary_release} AS primary_release_id,
                   {primary_cover} AS cover_version,
                   {artist_names} AS artist_name
            FROM (
                SELECT a.id, a.title, a.year FROM albums a
                WHERE a.id IN (
                    SELECT album_id FROM album_search WHERE title GLOB ?1
                    UNION
                    SELECT credit.album_id FROM artist_search found
                    JOIN artist_shown_as shown ON shown.shown_artist_id = found.artist_id
                    JOIN album_artists credit ON credit.artist_id = shown.artist_id
                    WHERE found.name GLOB ?1
                    UNION
                    SELECT credit.album_id FROM artist_credit_search found
                    JOIN album_artists credit
                        ON credit.artist_id = found.artist_id
                       AND credit.credit_key = found.name_key
                    WHERE found.name GLOB ?1
                )
                  AND {ALBUM_A_IS_SHOWN}
                ORDER BY a.title, a.id
                LIMIT ?2
            ) a
            ORDER BY a.title, a.id
            "#,
        primary_release = album_primary_release_sql(),
        primary_cover = super::files::cover_version_sql(album_primary_release_sql()),
        artist_names = album_credits_json_sql(),
    );
    let albums = sql.query(&album_query, params![pattern, limit], |row| {
        Ok(AlbumSearchRow {
            id: row.get("id")?,
            title: row.get("title")?,
            year: row.get("year")?,
            primary_release_id: row.get("primary_release_id")?,
            cover_version: row.get("cover_version")?,
            artist_name: credit_line_column(row, "artist_name")?,
        })
    })?;
    let tracks = sql.query(
        &format!(
            r#"
            SELECT t.id, t.title, t.duration_ms, t.release_id,
                   r.album_id, a.title as album_title, {} as artist_name,
                   {} as cover_version
            FROM tracks t
            JOIN releases r ON t.release_id = r.id
            JOIN albums a ON r.album_id = a.id
            WHERE t.id IN (
                SELECT track_id FROM track_search WHERE title GLOB ?1
                ORDER BY sort_title, track_id
                LIMIT ?2
            )
            ORDER BY t.title, t.id
            "#,
            track_credits_json_sql("t"),
            super::files::cover_version_sql("t.release_id"),
        ),
        params![pattern, limit],
        |row| {
            Ok(DbTrackSearchResult {
                id: row.get("id")?,
                title: row.get("title")?,
                duration_ms: row.get("duration_ms")?,
                release_id: row.get("release_id")?,
                album_id: row.get("album_id")?,
                album_title: row.get("album_title")?,
                artist_name: credit_line_column(row, "artist_name")?,
                cover_version: row.get("cover_version")?,
            })
        },
    )?;
    let artists = sql.query(
        &artist_listing_query(
            Some(&format!("WHERE ar.id IN ({SEARCH_MATCHED_ARTISTS})")),
            Some("ORDER BY listing.name, listing.artist_id LIMIT ?2"),
        ),
        params![pattern, limit],
        row_to_artist_summary,
    )?;
    let composers = sql.query(
        &composer_listing_query(
            Some(&format!("WHERE composer.id IN ({SEARCH_MATCHED_ARTISTS})")),
            Some("ORDER BY listing.name, listing.artist_id LIMIT ?2"),
        ),
        params![pattern, limit],
        row_to_composer_summary,
    )?;
    let works = sql.query(
        &work_summary_query(
            Some(
                "WHERE w.id IN (
                    SELECT work_id FROM work_search WHERE title GLOB ?1
                    ORDER BY sort_title, work_id
                    LIMIT ?2
                )",
            ),
            Some("ORDER BY w.title, w.id"),
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

#[derive(Clone, PartialEq)]
struct AlbumSearchRow {
    id: String,
    title: String,
    year: Option<i32>,
    primary_release_id: String,
    cover_version: Option<String>,
    artist_name: String,
}

#[derive(Clone, PartialEq)]
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
                Ok(DbAlbumSearchResult {
                    id: row.id,
                    title: row.title,
                    year: row.year,
                    primary_release_id: row.primary_release_id,
                    cover_version: row.cover_version,
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
    let (primary_release_id, primary_cover_version): (String, Option<String>) = sql.query_row(
        &format!(
            "SELECT {primary}, {cover} FROM albums a WHERE a.id = ?",
            primary = album_primary_release_sql(),
            cover = super::files::cover_version_sql(album_primary_release_sql()),
        ),
        params![album_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let releases = db_releases
        .into_iter()
        .map(|release| build_release_detail_on(sql, release))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(AlbumDetailRows {
        album,
        artists,
        releases,
        primary_release_id,
        primary_cover_version,
    }))
}

#[derive(Clone, PartialEq)]
struct AlbumDetailRows {
    album: DbAlbum,
    artists: Vec<LibraryCredit>,
    releases: Vec<ReleaseDetailRows>,
    primary_release_id: String,
    primary_cover_version: Option<String>,
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
            primary_release_id: self.primary_release_id,
            primary_cover_version: self.primary_cover_version,
        }
    }
}

fn album_count_on(sql: &SqlReadContext<'_>) -> Result<u64, DbError> {
    sql.query_row("SELECT COUNT(*) FROM album_listing", [], |row| {
        row.get::<_, i64>(0)
    })
    .map(|count| count as u64)
    .map_err(DbError::from)
}

fn album_rows_on<P: Params>(
    sql: &SqlReadContext<'_>,
    query: &str,
    params: P,
) -> Result<Vec<AlbumSummaryRow>, CovenError> {
    Ok(sql.query(query, params, AlbumSummaryRow::read)?)
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlbumBrowseProjection {
    pub windows: Vec<crate::library::LibraryBrowseWindow<DbAlbumSummary>>,
    pub total_count: u64,
    pub row_count: u64,
    pub sections: Vec<crate::library::LibraryBrowseSection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlbumDetailProjection {
    pub detail: Option<DbAlbumDetail>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibrarySearchProjection {
    pub results: DbLibrarySearchResults,
}

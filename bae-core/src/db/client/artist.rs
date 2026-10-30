use super::*;

impl Database {
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_artist(&self, artist: &DbArtist) -> Result<(), DbError> {
        let artist = artist.clone();
        self.call_sql(move |sql| insert_artist_row(&sql, &artist))
            .await
    }
    pub async fn insert_album_artist(&self, album_artist: &DbAlbumArtist) -> Result<(), DbError> {
        let album_artist = album_artist.clone();
        self.call_sql(move |sql| insert_album_artist_row(&sql, &album_artist))
            .await
    }
    #[cfg(test)]
    pub async fn insert_track_artist(&self, track_artist: &DbTrackArtist) -> Result<(), DbError> {
        let track_artist = track_artist.clone();
        self.call_sql(move |sql| insert_track_artist_row(&sql, &track_artist))
            .await
    }
    /// The album's credits in list order, each the artist it shows after
    /// merges; a list naming one artist twice gives it twice.
    pub async fn get_artists_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<LibraryCredit>, DbError> {
        let album_id = album_id.to_string();
        self.read(move |sql| get_artists_for_album_on(&sql, &album_id))
            .await
    }
    /// The track's own credits in list order, each the artist it shows after
    /// merges; a list naming one artist twice gives it twice.
    pub async fn get_artists_for_track(
        &self,
        track_id: &str,
    ) -> Result<Vec<LibraryCredit>, DbError> {
        let track_id = track_id.to_string();
        self.read(move |sql| {
            sql.query(
                &format!(
                    r#"
                        SELECT {columns} FROM track_artists credit
                        JOIN artists artist ON artist.id = {shown}
                        LEFT JOIN artist_credits credit_name
                            ON credit_name.artist_id = credit.artist_id
                           AND credit_name.name_key = credit.credit_key
                        WHERE credit.track_id = ?
                        ORDER BY credit.position
                        "#,
                    columns = LIBRARY_CREDIT_COLUMNS,
                    shown = shown_artist_id("credit.artist_id")
                ),
                params![track_id],
                row_to_library_credit,
            )
            .map_err(DbError::from)
        })
        .await
    }
    /// Find artist by ID. Caller-provided ID — may not exist.
    pub async fn find_artist_by_id(&self, artist_id: &str) -> Result<Option<DbArtist>, DbError> {
        let artist_id = artist_id.to_string();
        self.read(move |sql| {
            sql.query_row(
                "SELECT * FROM artists WHERE id = ?",
                params![artist_id],
                row_to_artist,
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    #[cfg(test)]
    pub async fn get_composer_page(
        &self,
        sort: &[ComposerSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbComposerSummary>, DbError> {
        let order = composer_order(sort);
        self.read(move |sql| {
            Ok(
                composer_rows_on(&sql, &order, &order.everything(), limit, offset)?
                    .into_iter()
                    .map(|(row, _)| row)
                    .collect(),
            )
        })
        .await
    }

    /// Follow the composer list under `sort` in the windows its request
    /// names, each read from the row beside it when one is known
    /// (`list_windows`).
    pub(crate) fn subscribe_composer_browse(
        &self,
        sort: &[ComposerSortCriterion],
        initial_windows: crate::library::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<crate::library::LibraryPageWindows, ComposerBrowseProjection>
    {
        let order = composer_order(sort);
        let places = KnownPlaces::new();
        self.inner
            .handle
            .subscribe_reconfigurable(initial_windows, move |requested, sql| {
                let versions = list_order_versions_on(&sql, &["composer_listing"])?;
                let total_count = composer_count_on(&sql).map_err(CovenError::from)?;
                let WindowsRead { windows, found } = read_windows(
                    &order,
                    requested,
                    places.under(&versions),
                    |seek, limit, offset| composer_rows_on(&sql, &order, seek, limit, offset),
                )?;
                places.keep(versions, found);
                Ok(ComposerBrowseProjection {
                    windows: windows
                        .into_iter()
                        .map(|(window, rows)| crate::library::LibraryBrowseWindow { window, rows })
                        .collect(),
                    total_count,
                })
            })
    }

    pub async fn get_artist_count(&self) -> Result<u64, DbError> {
        self.read(|sql| artist_count_on(&sql)).await
    }

    pub async fn get_artist_page(
        &self,
        sort: &[ArtistSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbArtistSummary>, DbError> {
        let order = artist_order(sort);
        self.read(move |sql| {
            Ok(
                artist_rows_on(&sql, &order, &order.everything(), limit, offset)?
                    .into_iter()
                    .map(|(row, _)| row)
                    .collect(),
            )
        })
        .await
    }

    /// Follow the artist list under `sort` in the windows its request
    /// names, each read from the row beside it when one is known
    /// (`list_windows`).
    pub(crate) fn subscribe_artist_browse(
        &self,
        sort: &[ArtistSortCriterion],
        initial_windows: crate::library::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<crate::library::LibraryPageWindows, ArtistBrowseProjection>
    {
        let order = artist_order(sort);
        let places = KnownPlaces::new();
        self.inner
            .handle
            .subscribe_reconfigurable(initial_windows, move |requested, sql| {
                let versions = list_order_versions_on(&sql, &["artist_listing"])?;
                let total_count = artist_count_on(&sql).map_err(CovenError::from)?;
                let WindowsRead { windows, found } = read_windows(
                    &order,
                    requested,
                    places.under(&versions),
                    |seek, limit, offset| artist_rows_on(&sql, &order, seek, limit, offset),
                )?;
                places.keep(versions, found);
                Ok(ArtistBrowseProjection {
                    windows: windows
                        .into_iter()
                        .map(|(window, rows)| crate::library::LibraryBrowseWindow { window, rows })
                        .collect(),
                    total_count,
                })
            })
    }

    /// The artist's summary row plus every album whose credit list names it,
    /// in discography order: year
    /// ascending with unknown years last, then title (case-insensitive), then
    /// id. `None` when the artist doesn't exist or has no album links —
    /// exactly the artists `get_artist_page` never lists.
    pub async fn find_artist_detail(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbArtistDetail>, DbError> {
        let artist_id = artist_id.to_string();
        self.read(move |sql| find_artist_detail_on(&sql, &artist_id))
            .process(|rows| rows.map(ArtistDetailRows::process).transpose())
            .await
    }

    /// Follow the artist `initial` names, or none. The detail view moves to
    /// another artist through the request handle, not another query.
    pub(crate) fn subscribe_artist_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, ArtistDetailProjection> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |artist_id, sql| {
                let Some(artist_id) = artist_id else {
                    return Ok(None);
                };
                find_artist_detail_on(&sql, artist_id).map_err(CovenError::from)
            })
            .process(|detail| {
                Ok(ArtistDetailProjection {
                    detail: detail.map(ArtistDetailRows::process).transpose()?,
                })
            })
    }

    #[cfg(test)]
    pub async fn find_composer_detail(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbComposerDetail>, DbError> {
        let artist_id = artist_id.to_string();
        self.read(move |sql| find_composer_detail_on(&sql, &artist_id))
            .process(|rows| Ok(rows.map(ComposerDetailRows::process)))
            .await
    }

    /// Follow the composer `initial` names, or none. The detail view moves to
    /// another composer through the request handle, not another query.
    pub(crate) fn subscribe_composer_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, ComposerDetailProjection> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |artist_id, sql| {
                let Some(artist_id) = artist_id else {
                    return Ok(None);
                };
                find_composer_detail_on(&sql, artist_id).map_err(CovenError::from)
            })
            .process(|detail| {
                Ok(ComposerDetailProjection {
                    detail: detail.map(ComposerDetailRows::process),
                })
            })
    }

    /// The `works` row id minted for a MusicBrainz work, if this library already
    /// imported it. Works are deduped on their MBID, never on their row id.
    pub async fn find_work_id_by_musicbrainz_id(
        &self,
        musicbrainz_work_id: &str,
    ) -> Result<Option<String>, DbError> {
        let musicbrainz_work_id = musicbrainz_work_id.to_string();
        self.read(move |sql| {
            sql.query_row(
                "SELECT id FROM works WHERE id = ?1 OR musicbrainz_work_id = ?2 \
                 ORDER BY id = ?1 DESC LIMIT 1",
                params![
                    crate::db::identity::work_id(&musicbrainz_work_id),
                    musicbrainz_work_id
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    #[cfg(test)]
    pub async fn find_work_detail(&self, work_id: &str) -> Result<Option<DbWorkDetail>, DbError> {
        let work_id = work_id.to_string();
        self.read(move |sql| find_work_detail_on(&sql, &work_id))
            .await
    }

    /// Follow the work `initial` names, or none. The detail view moves to
    /// another work through the request handle, not another query.
    pub(crate) fn subscribe_work_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, WorkDetailProjection> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |work_id, sql| {
                let Some(work_id) = work_id else {
                    return Ok(WorkDetailProjection { detail: None });
                };
                Ok(WorkDetailProjection {
                    detail: find_work_detail_on(&sql, work_id).map_err(CovenError::from)?,
                })
            })
    }
}

fn find_work_detail_on(
    sql: &SqlReadContext<'_>,
    work_id: &str,
) -> Result<Option<DbWorkDetail>, DbError> {
    let work = sql
        .query_row(
            &work_summary_query(Some("WHERE w.id = ?"), None),
            params![work_id],
            row_to_work_summary,
        )
        .optional()?;
    let Some(work) = work else {
        return Ok(None);
    };
    let child_works = sql.query(
        &work_summary_query(
            Some("JOIN work_parts wp ON wp.child_work_id = w.id AND wp.parent_work_id = ?"),
            Some("ORDER BY wp.position, w.title"),
        ),
        params![work.work.id],
        row_to_work_summary,
    )?;
    let releases = work_release_rows(sql, &work.work.id)?;
    let tracks = sql.query(
        "SELECT tw.id AS track_work_id, tw.work_id,
                tw.position, tw.source AS link_source, tw.created_at,
                t.id AS track_id, t.release_id AS track_release_id,
                t.title AS track_title, t.medium_id AS track_medium_id, t.side AS track_side,
                t.track_number AS track_track_number,
                t.duration_ms AS track_duration_ms,
                t.created_at AS track_created_at,
                a.id AS album_id, a.title AS album_title,
                a.year AS album_year,
                a.is_compilation AS album_is_compilation,
                a.created_at AS album_created_at
         FROM track_works tw
         JOIN tracks t ON t.id = tw.track_id
         JOIN releases r ON r.id = t.release_id
         JOIN albums a ON a.id = r.album_id
         WHERE tw.work_id = ?
         ORDER BY a.title, r.album_order, r.id, t.position, tw.position",
        params![work.work.id],
        |row| {
            Ok(DbWorkTrackSummary {
                link: DbTrackWork {
                    id: row.get("track_work_id")?,
                    track_id: row.get("track_id")?,
                    work_id: row.get("work_id")?,
                    position: row.get("position")?,
                    source: parsed_column(row, "link_source")?,
                    created_at: rfc3339_column(row, "created_at")?,
                },
                track: row_to_track_with_prefix(row, "track_")?,
                album: row_to_album_with_prefix(row, "album_")?,
            })
        },
    )?;
    Ok(Some(DbWorkDetail {
        work,
        child_works,
        releases,
        tracks,
    }))
}

fn find_artist_detail_on(
    sql: &SqlReadContext<'_>,
    artist_id: &str,
) -> Result<Option<ArtistDetailRows>, DbError> {
    let artist = sql
        .query_row(
            &artist_listing_query(None, Some("WHERE listing.artist_id = ?"), None),
            params![artist_id],
            row_to_artist_summary,
        )
        .optional()?;
    let Some(artist) = artist else {
        return Ok(None);
    };
    let albums_query = format!(
        "{select} \
         FROM album_artist_link link \
         JOIN albums a ON a.id = link.album_id \
         WHERE link.artist_id = ?1 \
         ORDER BY CASE WHEN a.year IS NULL THEN 1 ELSE 0 END, \
                  a.year, a.title COLLATE NOCASE, a.id",
        select = album_summary_select(),
    );
    let albums = sql.query(&albums_query, params![artist_id], AlbumSummaryRow::read)?;
    let credit_names = super::artist_credit_names::credit_names_of_on(
        sql,
        std::slice::from_ref(&artist.artist.id),
    )?
    .remove(&artist.artist.id)
    .unwrap_or_default();
    Ok(Some(ArtistDetailRows {
        artist,
        credit_names,
        albums,
    }))
}

#[derive(Clone, PartialEq)]
struct ArtistDetailRows {
    artist: DbArtistSummary,
    credit_names: Vec<crate::credits::ArtistCreditName>,
    albums: Vec<AlbumSummaryRow>,
}

impl ArtistDetailRows {
    fn process(self) -> Result<DbArtistDetail, DbError> {
        Ok(DbArtistDetail {
            artist: self.artist,
            credit_names: self.credit_names,
            albums: self
                .albums
                .into_iter()
                .map(AlbumSummaryRow::process)
                .collect::<Result<_, _>>()?,
        })
    }
}

fn find_composer_detail_on(
    sql: &SqlReadContext<'_>,
    artist_id: &str,
) -> Result<Option<ComposerDetailRows>, DbError> {
    let composer = sql
        .query_row(
            &composer_listing_query(None, Some("WHERE listing.artist_id = ?"), None),
            params![artist_id],
            row_to_composer_summary,
        )
        .optional()?;
    let Some(composer) = composer else {
        return Ok(None);
    };
    let works = sql.query(
        &work_summary_query(
            Some(
                "JOIN composer_work credited ON credited.work_id = w.id AND credited.artist_id = ?",
            ),
            Some("ORDER BY w.title"),
        ),
        params![composer.artist.id],
        row_to_work_summary,
    )?;
    let parent_ids = works
        .iter()
        .filter_map(|work| work.parent_work_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let parent_summaries = if parent_ids.is_empty() {
        HashMap::new()
    } else {
        let placeholders = (0..parent_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let filter = format!("WHERE w.id IN ({placeholders})");
        let query = work_summary_query(Some(&filter), None);
        sql.query(
            &query,
            rusqlite::params_from_iter(parent_ids.iter()),
            |row| row_to_work_summary(row).map(|summary| (summary.work.id.clone(), summary)),
        )?
        .into_iter()
        .collect::<HashMap<_, _>>()
    };
    let release_roles_query = "SELECT rar.id AS release_artist_role_id, rar.release_id,
                rar.artist_id, rar.position,
                rar.source AS role_source, rar.source_credit, rar.created_at,
                a.id AS album_id, a.title AS album_title,
                a.year AS album_year,
                a.is_compilation AS album_is_compilation,
                a.created_at AS album_created_at
         FROM composer_unlinked_credit unlinked
         JOIN release_artist_roles rar ON rar.id = unlinked.credit_id
         JOIN releases r ON r.id = rar.release_id
         JOIN albums a ON a.id = r.album_id
         WHERE unlinked.artist_id = ?
         ORDER BY a.title, r.album_order, r.id, rar.position";
    let unlinked_release_roles =
        sql.query(release_roles_query, params![composer.artist.id], |row| {
            Ok(DbReleaseRoleSummary {
                role: DbReleaseArtistRole {
                    id: row.get("release_artist_role_id")?,
                    release_id: row.get("release_id")?,
                    artist_id: row.get("artist_id")?,
                    position: row.get("position")?,
                    source: parsed_column(row, "role_source")?,
                    source_credit: row.get("source_credit")?,
                    created_at: rfc3339_column(row, "created_at")?,
                },
                album: row_to_album_with_prefix(row, "album_")?,
            })
        })?;
    let track_roles_query = "SELECT tar.id AS track_artist_role_id, tar.track_id, tar.artist_id,
                tar.position, tar.source AS role_source,
                tar.source_credit, tar.created_at,
                t.id AS track_id, t.release_id AS track_release_id,
                t.title AS track_title, t.medium_id AS track_medium_id, t.side AS track_side,
                t.track_number AS track_track_number,
                t.duration_ms AS track_duration_ms,
                t.created_at AS track_created_at,
                a.id AS album_id, a.title AS album_title,
                a.year AS album_year,
                a.is_compilation AS album_is_compilation,
                a.created_at AS album_created_at,
                art.id AS artist_id, art.name AS artist_name,
                art.sort_name AS artist_sort_name,
                art.discogs_artist_id AS artist_discogs_artist_id,
                art.musicbrainz_artist_id AS artist_musicbrainz_artist_id,
                art.created_at AS artist_created_at
         FROM composer_unlinked_credit unlinked
         JOIN track_artist_roles tar ON tar.id = unlinked.credit_id
         JOIN tracks t ON t.id = tar.track_id
         JOIN releases r ON r.id = t.release_id
         JOIN albums a ON a.id = r.album_id
         JOIN artists art ON art.id = unlinked.artist_id
         WHERE unlinked.artist_id = ?
         ORDER BY a.title, r.album_order, r.id, t.position, tar.position";
    let unlinked_track_roles = sql.query(
        track_roles_query,
        params![composer.artist.id],
        row_to_track_role_summary,
    )?;
    Ok(Some(ComposerDetailRows {
        composer,
        works,
        parents: parent_summaries,
        unlinked_release_roles,
        unlinked_track_roles,
    }))
}

#[derive(Clone, PartialEq)]
struct ComposerDetailRows {
    composer: DbComposerSummary,
    works: Vec<DbWorkSummary>,
    parents: HashMap<String, DbWorkSummary>,
    unlinked_release_roles: Vec<DbReleaseRoleSummary>,
    unlinked_track_roles: Vec<DbTrackRoleSummary>,
}

impl ComposerDetailRows {
    fn process(self) -> DbComposerDetail {
        let Self {
            composer,
            works,
            parents: parent_summaries,
            unlinked_release_roles,
            unlinked_track_roles,
        } = self;
        let mut grouped: HashMap<String, Vec<DbWorkSummary>> = HashMap::new();
        let mut ungrouped = Vec::new();
        for work in works {
            if let Some(parent_id) = work.parent_work_id.clone() {
                grouped.entry(parent_id).or_default().push(work);
            } else {
                let id = work.work.id.clone();
                ungrouped.push(DbComposerWorkGroup {
                    id,
                    parent: None,
                    works: vec![work],
                });
            }
        }
        let mut work_groups = grouped
            .into_iter()
            .map(|(parent_id, mut works)| {
                works.sort_by_key(work_summary_sort_key);
                DbComposerWorkGroup {
                    id: parent_id.clone(),
                    parent: parent_summaries.get(&parent_id).cloned(),
                    works,
                }
            })
            .collect::<Vec<_>>();
        work_groups.extend(ungrouped);
        work_groups.sort_by_key(composer_work_group_sort_key);

        DbComposerDetail {
            composer,
            work_groups,
            unlinked_release_roles,
            unlinked_track_roles,
        }
    }
}

fn composer_work_group_sort_key(group: &DbComposerWorkGroup) -> String {
    if let Some(parent) = group.parent.as_ref() {
        return work_summary_sort_key(parent);
    }
    let work = group
        .works
        .first()
        .expect("composer work group without parent must contain a work");
    work_summary_sort_key(work)
}

fn composer_count_on(sql: &SqlReadContext<'_>) -> Result<u64, DbError> {
    sql.query_row("SELECT COUNT(*) FROM composer_listing", [], |row| {
        row.get::<_, i64>(0)
    })
    .map(|count| count as u64)
    .map_err(DbError::from)
}

/// The composer list's rows `seek` reads in `order`, at most `limit` from
/// `offset`, each with its key.
pub(super) fn composer_rows_on(
    sql: &SqlReadContext<'_>,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
    offset: u64,
) -> Result<Vec<Keyed<DbComposerSummary>>, DbError> {
    let statement = listing_window_query(composer_listing_query, order, seek);
    Ok(seek.bound(limit, offset, |params| {
        sql.query(&statement, params, |row| {
            Ok((row_to_composer_summary(row)?, order.key_of(row)?))
        })
    })?)
}

/// The artist list's rows `seek` reads in `order`, at most `limit` from
/// `offset`, each with its key.
pub(super) fn artist_rows_on(
    sql: &SqlReadContext<'_>,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
    offset: u64,
) -> Result<Vec<Keyed<DbArtistSummary>>, DbError> {
    let statement = listing_window_query(artist_listing_query, order, seek);
    Ok(seek.bound(limit, offset, |params| {
        sql.query(&statement, params, |row| {
            Ok((row_to_artist_summary(row)?, order.key_of(row)?))
        })
    })?)
}

fn artist_count_on(sql: &SqlReadContext<'_>) -> Result<u64, DbError> {
    sql.query_row("SELECT COUNT(*) FROM artist_listing", [], |row| {
        row.get::<_, i64>(0)
    })
    .map(|count| count as u64)
    .map_err(DbError::from)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComposerBrowseProjection {
    pub windows: Vec<crate::library::LibraryBrowseWindow<DbComposerSummary>>,
    pub total_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComposerDetailProjection {
    pub detail: Option<DbComposerDetail>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArtistBrowseProjection {
    pub windows: Vec<crate::library::LibraryBrowseWindow<DbArtistSummary>>,
    pub total_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArtistDetailProjection {
    pub detail: Option<DbArtistDetail>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkDetailProjection {
    pub detail: Option<DbWorkDetail>,
}

use super::*;

impl Database {
    /// Insert a new artist
    pub async fn insert_artist(&self, artist: &DbArtist) -> Result<(), DbError> {
        let artist = artist.clone();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            insert_artist_row(sql.tx(), &artist, &reg)
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
        self.read(move |conn| {
            conn.query_row(sql, params![value], row_to_artist)
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
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            update_artist_external_ids_row(
                sql.tx(),
                &id,
                discogs_id.as_deref(),
                mb_id.as_deref(),
                sort_name.as_deref(),
                &reg,
            )
        })
        .await
    }

    /// Insert album-artist relationship
    pub async fn insert_album_artist(&self, album_artist: &DbAlbumArtist) -> Result<(), DbError> {
        let album_artist = album_artist.clone();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            insert_album_artist_row(sql.tx(), &album_artist, &reg)
        })
        .await
    }
    /// Insert track-artist relationship
    pub async fn insert_track_artist(&self, track_artist: &DbTrackArtist) -> Result<(), DbError> {
        let track_artist = track_artist.clone();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            insert_track_artist_row(sql.tx(), &track_artist, &reg)
        })
        .await
    }
    /// Get artists for an album (ordered by position)
    pub async fn get_artists_for_album(&self, album_id: &str) -> Result<Vec<DbArtist>, DbError> {
        let album_id = album_id.to_string();
        self.read(move |conn| get_artists_for_album_on(conn, &album_id))
            .await
    }
    /// Get artists for a track (ordered by position)
    pub async fn get_artists_for_track(&self, track_id: &str) -> Result<Vec<DbArtist>, DbError> {
        let track_id = track_id.to_string();
        self.read(move |conn| {
            let mut stmt = conn.prepare(
                r#"
                        SELECT a.* FROM artists a
                        JOIN track_artists ta ON a.id = ta.artist_id
                        WHERE ta.track_id = ?
                        ORDER BY ta.position
                        "#,
            )?;
            let rows = stmt.query_map(params![track_id], row_to_artist)?;
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

    pub async fn get_composer_count(&self) -> Result<u64, DbError> {
        self.read(move |conn| {
            let query = format!(
                "SELECT COUNT(*) FROM ({})",
                composer_summary_query(None, None)
            );
            conn.query_row(&query, [], |row| row.get::<_, i64>(0))
                .map(|count| count as u64)
                .map_err(DbError::from)
        })
        .await
    }

    pub async fn get_composer_page(
        &self,
        sort: &[ComposerSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbComposerSummary>, DbError> {
        let order_by = composer_order_by(sort);
        let tail = format!("ORDER BY {order_by} LIMIT ? OFFSET ?");
        let query = composer_summary_query(None, Some(&tail));
        self.read(move |conn| {
            let mut stmt = conn.prepare(&query)?;
            let rows = stmt.query_map(
                params![limit as i64, offset as i64],
                row_to_composer_summary,
            )?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    pub async fn get_artist_count(&self) -> Result<u64, DbError> {
        self.read(move |conn| {
            let query = format!(
                "SELECT COUNT(*) FROM ({})",
                artist_summary_query(None, None)
            );
            conn.query_row(&query, [], |row| row.get::<_, i64>(0))
                .map(|count| count as u64)
                .map_err(DbError::from)
        })
        .await
    }

    pub async fn get_artist_page(
        &self,
        sort: &[ArtistSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbArtistSummary>, DbError> {
        let order_by = artist_order_by(sort);
        let tail = format!("ORDER BY {order_by} LIMIT ? OFFSET ?");
        let query = artist_summary_query(None, Some(&tail));
        self.read(move |conn| {
            let mut stmt = conn.prepare(&query)?;
            let rows =
                stmt.query_map(params![limit as i64, offset as i64], row_to_artist_summary)?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// The artist's summary row plus every album it is an album artist of
    /// (primary FK or `album_artists` junction), in discography order: year
    /// ascending with unknown years last, then title (case-insensitive), then
    /// id. `None` when the artist doesn't exist or has no album links —
    /// exactly the artists `get_artist_page` never lists.
    pub async fn find_artist_detail(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbArtistDetail>, DbError> {
        let artist_id = artist_id.to_string();
        self.read(move |conn| {
            let artist = conn
                .query_row(
                    &artist_summary_query(Some("WHERE ar.id = ?"), None),
                    params![artist_id],
                    row_to_artist_summary,
                )
                .optional()?;
            let Some(artist) = artist else {
                return Ok(None);
            };

            let albums_query = format!(
                "{select} \
                 FROM albums a \
                 WHERE a.artist_id = ?1 \
                    OR EXISTS ( \
                        SELECT 1 FROM album_artists aa \
                        WHERE aa.album_id = a.id AND aa.artist_id = ?1 \
                    ) \
                 ORDER BY CASE WHEN a.year IS NULL THEN 1 ELSE 0 END, \
                          a.year, a.title COLLATE NOCASE, a.id",
                select = album_summary_select()
            );
            let mut stmt = conn.prepare(&albums_query)?;
            let mut rows = stmt.query(params![artist_id])?;
            let mut albums = Vec::new();
            while let Some(row) = rows.next()? {
                albums.push(parse_album_summary_row(row)?);
            }
            Ok(Some(DbArtistDetail { artist, albums }))
        })
        .await
    }

    pub async fn find_composer_detail(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbComposerDetail>, DbError> {
        let artist_id = artist_id.to_string();
        self.read(move |conn| {
            let composer = conn
                .query_row(
                    &composer_summary_query(Some("WHERE composer.id = ?"), None),
                    params![artist_id],
                    row_to_composer_summary,
                )
                .optional()?;
            let Some(composer) = composer else {
                return Ok(None);
            };

            let mut works_stmt = conn.prepare(&work_summary_query(
                Some("JOIN work_artists wa_filter ON wa_filter.work_id = w.id AND wa_filter.artist_id = ?"),
                Some("ORDER BY w.title"),
            ))?;
            let works = works_stmt
                .query_map(params![composer.artist.id], row_to_work_summary)?
                .collect::<coven::rusqlite::Result<Vec<_>>>()?;
            let parent_ids: Vec<String> = works
                .iter()
                .filter_map(|work| work.parent_work_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let parent_summaries = if parent_ids.is_empty() {
                HashMap::new()
            } else {
                let placeholders = (0..parent_ids.len())
                    .map(|_| "?")
                    .collect::<Vec<_>>()
                    .join(",");
                let filter = format!("WHERE w.id IN ({placeholders})");
                let query = work_summary_query(Some(&filter), None);
                let mut parent_stmt = conn.prepare(&query)?;
                let rows = parent_stmt
                    .query_map(
                        coven::rusqlite::params_from_iter(parent_ids.iter()),
                        row_to_work_summary,
                    )?
                    .map(|row| row.map(|summary| (summary.work.id.clone(), summary)))
                    .collect::<coven::rusqlite::Result<HashMap<_, _>>>()?;
                rows
            };
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
            let mut work_groups: Vec<DbComposerWorkGroup> = grouped
                .into_iter()
                .map(|(parent_id, mut works)| {
                    works.sort_by_key(work_summary_sort_key);
                    DbComposerWorkGroup {
                        id: parent_id.clone(),
                        parent: parent_summaries.get(&parent_id).cloned(),
                        works,
                    }
                })
                .collect();
            work_groups.extend(ungrouped);
            work_groups.sort_by_key(composer_work_group_sort_key);

            let release_roles_query = format!(
                "SELECT rar.id AS release_artist_role_id, rar.release_id,
                        rar.artist_id, rar.position,
                        rar.source AS role_source, rar.source_credit, rar.created_at,
                        a.id AS album_id, a.title AS album_title,
                        a.artist_id AS album_artist_id, a.year AS album_year,
                        a.primary_release_id AS album_primary_release_id,
                        a.is_compilation AS album_is_compilation,
                        a.created_at AS album_created_at
                 FROM release_artist_roles rar
                 JOIN releases r ON r.id = rar.release_id
                 JOIN albums a ON a.id = r.album_id
                 WHERE rar.artist_id = ?
                   AND {}
                 ORDER BY a.title, r.created_at, rar.position",
                unlinked_release_composer_role_predicate("rar")
            );
            let mut release_roles_stmt = conn.prepare(&release_roles_query)?;
            let unlinked_release_roles = release_roles_stmt
                .query_map(params![composer.artist.id], |row| {
                    Ok(DbReleaseRoleSummary {
                        role: DbReleaseArtistRole {
                            id: row.get("release_artist_role_id")?,
                            release_id: row.get("release_id")?,
                            artist_id: row.get("artist_id")?,
                            position: row.get("position")?,
                            source: metadata_source_column(row, "role_source")?,
                            source_credit: row.get("source_credit")?,
                            created_at: rfc3339_column(row, "created_at")?,
                        },
                        album: row_to_joined_album(row)?,
                    })
                })?
                .collect::<coven::rusqlite::Result<Vec<_>>>()?;

            let track_roles_query = format!(
                "SELECT tar.id AS track_artist_role_id, tar.track_id, tar.artist_id,
                        tar.position, tar.source AS role_source,
                        tar.source_credit, tar.created_at,
                        t.id AS track_id, t.release_id AS track_release_id,
                        t.title AS track_title, t.side AS track_side,
                        t.track_number AS track_track_number,
                        t.duration_ms AS track_duration_ms,
                        t.discogs_position AS track_discogs_position,
                        t.created_at AS track_created_at,
                        a.id AS album_id, a.title AS album_title,
                        a.artist_id AS album_artist_id, a.year AS album_year,
                        a.primary_release_id AS album_primary_release_id,
                        a.is_compilation AS album_is_compilation,
                        a.created_at AS album_created_at,
                        art.id AS artist_id, art.name AS artist_name,
                        art.sort_name AS artist_sort_name,
                        art.discogs_artist_id AS artist_discogs_artist_id,
                        art.musicbrainz_artist_id AS artist_musicbrainz_artist_id,
                        art.created_at AS artist_created_at
                 FROM track_artist_roles tar
                 JOIN tracks t ON t.id = tar.track_id
                 JOIN releases r ON r.id = t.release_id
                 JOIN albums a ON a.id = r.album_id
                 JOIN artists art ON art.id = tar.artist_id
                 WHERE tar.artist_id = ?
                   AND {}
                 ORDER BY a.title, r.created_at, t.side, t.track_number, tar.position",
                unlinked_track_composer_role_predicate("tar")
            );
            let mut track_roles_stmt = conn.prepare(&track_roles_query)?;
            let unlinked_track_roles = track_roles_stmt
                .query_map(params![composer.artist.id], row_to_track_role_summary)?
                .collect::<coven::rusqlite::Result<Vec<_>>>()?;

            Ok(Some(DbComposerDetail {
                composer,
                work_groups,
                unlinked_release_roles,
                unlinked_track_roles,
            }))
        })
        .await
    }

    pub async fn find_work_detail(&self, work_id: &str) -> Result<Option<DbWorkDetail>, DbError> {
        let work_id = work_id.to_string();
        self.read(move |conn| {
            let work = conn
                .query_row(
                    &work_summary_query(Some("WHERE w.id = ?"), None),
                    params![work_id],
                    row_to_work_summary,
                )
                .optional()?;
            let Some(work) = work else {
                return Ok(None);
            };

            let mut child_stmt = conn.prepare(&work_summary_query(
                Some("JOIN work_parts wp ON wp.child_work_id = w.id AND wp.parent_work_id = ?"),
                Some("ORDER BY wp.position, w.title"),
            ))?;
            let child_works = child_stmt
                .query_map(params![work.work.id], row_to_work_summary)?
                .collect::<coven::rusqlite::Result<Vec<_>>>()?;

            let releases = work_release_rows(conn, &work.work.id)?;

            let mut tracks_stmt = conn.prepare(
                "SELECT tw.id AS track_work_id, tw.work_id,
                        tw.position, tw.source AS link_source, tw.created_at,
                        t.id AS track_id, t.release_id AS track_release_id,
                        t.title AS track_title, t.side AS track_side,
                        t.track_number AS track_track_number,
                        t.duration_ms AS track_duration_ms,
                        t.discogs_position AS track_discogs_position,
                        t.created_at AS track_created_at,
                        a.id AS album_id, a.title AS album_title,
                        a.artist_id AS album_artist_id, a.year AS album_year,
                        a.primary_release_id AS album_primary_release_id,
                        a.is_compilation AS album_is_compilation,
                        a.created_at AS album_created_at
                 FROM track_works tw
                 JOIN tracks t ON t.id = tw.track_id
                 JOIN releases r ON r.id = t.release_id
                 JOIN albums a ON a.id = r.album_id
                 WHERE tw.work_id = ?
                 ORDER BY a.title, r.created_at, t.side, t.track_number, tw.position",
            )?;
            let tracks = tracks_stmt
                .query_map(params![work.work.id], |row| {
                    Ok(DbWorkTrackSummary {
                        link: DbTrackWork {
                            id: row.get("track_work_id")?,
                            track_id: row.get("track_id")?,
                            work_id: row.get("work_id")?,
                            position: row.get("position")?,
                            source: metadata_source_column(row, "link_source")?,
                            created_at: rfc3339_column(row, "created_at")?,
                        },
                        track: row_to_joined_track(row)?,
                        album: row_to_joined_album(row)?,
                    })
                })?
                .collect::<coven::rusqlite::Result<Vec<_>>>()?;

            Ok(Some(DbWorkDetail {
                work,
                child_works,
                releases,
                tracks,
            }))
        })
        .await
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

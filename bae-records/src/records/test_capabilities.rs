use super::*;

impl Records {
    /// Whether the coven file `table`'s row `row_id` references waits in
    /// this device's upload queue, live.
    pub fn subscribe_row_file_queued_for_test(
        &self,
        table: &str,
        row_id: &str,
    ) -> coven::LiveQuery<bool, coven::CovenError> {
        let statement = format!(
            "SELECT EXISTS (SELECT 1 FROM {table} AS t \
             JOIN coven_uploads AS upload ON upload.file = t.{column} WHERE t.id = ?1)",
            column = file_column_for_test(table),
        );
        let row_id = row_id.to_string();
        self.store.subscribe(move |sql| Ok(sql.query_row(&statement, [&row_id], |row| row.get(0))?))
    }

    /// How many of the files waiting in this device's upload queue `table`'s
    /// rows reference.
    pub async fn queued_upload_count_in_table_for_test(&self, table: &str) -> Result<i64, DbError> {
        let statement = format!(
            "SELECT COUNT(*) FROM coven_uploads AS upload \
             JOIN {table} AS t ON t.{column} = upload.file",
            column = file_column_for_test(table),
        );
        self.store.read(move |sql| Ok(sql.query_row(&statement, [], |row| row.get(0))?)).await
    }

    pub async fn queued_upload_count_for_root_for_test(
        &self,
        root_table: &str,
        root_id: &str,
    ) -> Result<usize, DbError> {
        Ok(self.queued_upload_rows_for_root_for_test(root_table, root_id).await?.len())
    }

    pub async fn queued_upload_rows_for_root_for_test(
        &self,
        root_table: &str,
        root_id: &str,
    ) -> Result<Vec<(String, String)>, DbError> {
        if root_table != "releases" {
            return Err(DbError::Message(format!("unknown upload root {root_table}")));
        }
        // The release's own files and its cover, whose row the release's id
        // keys.
        let root_id = root_id.to_string();
        self.store
            .read(move |sql| {
                Ok(sql.query(
                    "SELECT 'release_files', f.id FROM release_files AS f \
                 JOIN coven_uploads AS upload ON upload.file = f.file \
                 WHERE f.release_id = ?1 \
                 UNION ALL \
                 SELECT 'covers', cover.id FROM covers AS cover \
                 JOIN coven_uploads AS upload ON upload.file = cover.file \
                 WHERE cover.id = ?1",
                    [root_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?)
            })
            .await
    }

    pub async fn work_links_for_test(
        &self,
        musicbrainz_work_id: &str,
    ) -> Result<(Vec<String>, Vec<String>), DbError> {
        let musicbrainz_work_id = musicbrainz_work_id.to_string();
        self.store
            .read(move |sql| {
                let work_ids = sql.query(
                    "SELECT id FROM works WHERE musicbrainz_work_id = ?1",
                    [&musicbrainz_work_id],
                    |row| row.get::<_, String>(0),
                )?;
                let linked_release_ids = sql.query(
                    "SELECT DISTINCT t.release_id FROM track_works tw \
                 JOIN tracks t ON t.id = tw.track_id \
                 JOIN works w ON w.id = tw.work_id \
                 WHERE w.musicbrainz_work_id = ?1 ORDER BY t.release_id",
                    [&musicbrainz_work_id],
                    |row| row.get::<_, String>(0),
                )?;
                Ok((work_ids, linked_release_ids))
            })
            .await
    }

    /// Each track of the release in play order: its title, the MusicBrainz
    /// ids of the artists credited on it in a role, and of the works it
    /// performs, each in order.
    pub async fn track_sources_for_test(
        &self,
        release_id: &str,
    ) -> Result<Vec<(String, Vec<String>, Vec<String>)>, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| {
                let tracks = sql.query(
                    "SELECT id, title FROM tracks WHERE release_id = ?1 ORDER BY position",
                    [&release_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )?;
                let mut out = Vec::with_capacity(tracks.len());
                for (track_id, title) in tracks {
                    let roles = sql.query(
                    "SELECT COALESCE(a.musicbrainz_artist_id, a.name) FROM track_artist_roles r \
                     JOIN artists a ON a.id = r.artist_id \
                     WHERE r.track_id = ?1 ORDER BY r.position",
                    [&track_id],
                    |row| row.get::<_, String>(0),
                )?;
                    let works = sql.query(
                        "SELECT w.musicbrainz_work_id FROM track_works tw \
                     JOIN works w ON w.id = tw.work_id \
                     WHERE tw.track_id = ?1 ORDER BY tw.position",
                        [&track_id],
                        |row| row.get::<_, String>(0),
                    )?;
                    out.push((title, roles, works));
                }
                Ok(out)
            })
            .await
    }

    pub async fn committed_track_files_for_test(
        &self,
        release_id: &str,
    ) -> Result<Vec<(String, String)>, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| {
                sql.query(
                    "SELECT t.title, rf.original_filename \
                 FROM tracks t \
                 JOIN audio_formats af ON af.track_id = t.id \
                 JOIN audio_format_segments seg \
                   ON seg.audio_format_id = af.id AND seg.role = 'main' \
                 JOIN release_files rf ON rf.id = seg.file_id \
                 WHERE t.release_id = ?1 \
                 ORDER BY t.position",
                    [&release_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .map_err(DbError::from)
            })
            .await
    }
}

impl Records {
    /// Write `rows` in one write through the records' own row inserts, each
    /// kind in the order [`LibraryRows`] lists them.
    ///
    /// [`LibraryRows`]: crate::test_fixtures::library::LibraryRows
    pub async fn insert_library_rows_for_test(
        &self,
        rows: crate::test_fixtures::library::LibraryRows,
    ) -> Result<(), DbError> {
        let now = self.store.now_utc();
        let labels: Vec<LabelParts> = rows
            .releases
            .iter()
            .map(|release| LabelParts::new(&*self.ids, now, &release.pressing.labels))
            .collect();
        self.store
            .call_sql(move |sql| {
                let conn = &sql;
                for artist in &rows.artists {
                    insert_artist_row(conn, artist)?;
                }
                for album in &rows.albums {
                    insert_album_row(conn, album)?;
                }
                for credit in &rows.album_artists {
                    insert_album_artist_row(conn, credit)?;
                }
                for (release, labels) in rows.releases.iter().zip(&labels) {
                    insert_release_row(conn, release, &label_write(labels))
                        .map_err(IdentityWriteError::into_db_error)?;
                }
                for medium in &rows.mediums {
                    insert_release_medium_row(conn, medium)?;
                }
                for track in &rows.tracks {
                    insert_track_row(conn, track)?;
                }
                for credit in &rows.track_artists {
                    insert_track_artist_row(conn, credit)?;
                }
                for work in &rows.works {
                    insert_work_row(conn, work)?;
                }
                for link in &rows.work_artists {
                    insert_work_artist_row(conn, link)?;
                }
                for part in &rows.work_parts {
                    insert_work_part_row(conn, part)?;
                }
                for link in &rows.track_works {
                    insert_track_work_row(conn, link)?;
                }
                for role in &rows.release_artist_roles {
                    insert_release_artist_role_row(conn, role)?;
                }
                for role in &rows.track_artist_roles {
                    insert_track_artist_role_row(conn, role)?;
                }
                Ok(())
            })
            .await
    }

    /// Save `release_id`'s metadata as a person's edit of it does: the
    /// album, the release, its tracks and their credits as they stand, with
    /// what `edit` changes of them.
    pub async fn save_release_metadata_for_test(
        &self,
        release_id: &str,
        edit: impl FnOnce(&mut ReleaseMetadataRows),
    ) -> Result<(), IdentityWriteError> {
        let mut rows = self.release_metadata_for_test(release_id).await?;
        edit(&mut rows);
        self.update_release_metadata_user_edit(rows).await
    }

    /// `release_id`'s metadata as an edit of it starts from: the album, the
    /// release, its tracks and their credits as they stand, every credited
    /// artist picked.
    async fn release_metadata_for_test(
        &self,
        release_id: &str,
    ) -> Result<ReleaseMetadataRows, DbError> {
        let release = self.find_release_by_id(release_id).await?.expect("the release");
        let album = self.find_album_by_id(&release.album_id).await?.expect("its album");
        let tracks = self.get_tracks_for_release(release_id).await?;
        let album_id = album.id.clone();
        let track_ids: Vec<String> = tracks.iter().map(|track| track.id.clone()).collect();
        let (album_credits, track_credits) = self
            .store
            .read(move |sql| {
                let mut track_credits = Vec::new();
                for track_id in &track_ids {
                    track_credits.push((
                        track_id.clone(),
                        credits_for_test(&sql, "track_artists", "track_id", track_id)?,
                    ));
                }
                Ok((credits_for_test(&sql, "album_artists", "album_id", &album_id)?, track_credits))
            })
            .await?;
        let mut credits = Vec::new();
        let credited = album_credits.iter().chain(track_credits.iter().flat_map(|(_, list)| list));
        for (artist_id, ..) in credited {
            credits.push(self.find_artist_by_id(artist_id).await?.expect("a credited artist"));
        }
        let now = self.store.now_utc();
        let album_artists = album_credits
            .into_iter()
            .map(|(artist_id, position, credit, join_phrase)| DbAlbumArtist {
                credit,
                join_phrase,
                ..DbAlbumArtist::new(&album.id, &artist_id, position, now)
            })
            .collect();
        let track_artists = track_credits
            .into_iter()
            .flat_map(|(track_id, list)| {
                list.into_iter().map(move |(artist_id, position, credit, join_phrase)| {
                    DbTrackArtist {
                        credit,
                        join_phrase,
                        ..DbTrackArtist::new(&track_id, &artist_id, position, now)
                    }
                })
            })
            .collect();
        let track_updates = tracks.into_iter().map(|track| (track.id.clone(), track)).collect();
        Ok(ReleaseMetadataRows {
            album,
            release,
            track_updates,
            picked: credits.iter().map(|artist| artist.id.clone()).collect(),
            credits,
            album_artists,
            track_artists,
        })
    }

    /// Save an edit of `release_id` applying a catalog that credits the
    /// roles and works `links` hold, as an edit session's save does: its
    /// album, pressing, tracks and credits as they stand, and `links` in
    /// place of every role credit and work link the release had.
    #[cfg(desktop)]
    pub async fn apply_release_links_for_test(
        &self,
        release_id: &str,
        links: crate::test_fixtures::library::LibraryRows,
    ) -> Result<(), IdentityWriteError> {
        let metadata = self.release_metadata_for_test(release_id).await?;
        let mediums = self.get_release_mediums(release_id).await?.0;
        let mut artists = metadata.credits;
        let linked = links
            .release_artist_roles
            .iter()
            .map(|role| &role.artist_id)
            .chain(links.track_artist_roles.iter().map(|role| &role.artist_id))
            .chain(links.work_artists.iter().map(|link| &link.artist_id));
        for artist_id in linked {
            if !artists.iter().any(|artist| artist.id == *artist_id) {
                artists.push(self.find_artist_by_id(artist_id).await?.expect("a linked artist"));
            }
        }
        let picked = artists.iter().map(|artist| artist.id.clone()).collect();
        let rows = ReleaseRows {
            album: metadata.album,
            release: metadata.release,
            mediums,
            tracks: metadata
                .track_updates
                .into_iter()
                .map(|(_, track)| TrackRow {
                    id: track.id,
                    release_id: track.release_id,
                    title: track.title,
                    medium_id: track.medium_id,
                    side: track.side,
                    track_number: track.track_number,
                    created_at: track.created_at,
                })
                .collect(),
            artists,
            album_artists: metadata.album_artists,
            track_artists: metadata.track_artists,
            work_graph: WorkGraphRows {
                works: links.works,
                work_artists: links.work_artists,
                work_parts: links.work_parts,
                track_works: links.track_works,
            },
            release_artist_roles: links.release_artist_roles,
            track_artist_roles: links.track_artist_roles,
        };
        self.save_release_edit_atomic(
            None,
            ReleaseEditWrite::Applied { rows, picked },
            AlbumFields::Written,
        )
        .await
    }

    /// The release a person's metadata edit of `album_id` saves through: its
    /// first.
    async fn edited_release_of_for_test(&self, album_id: &str) -> Result<String, DbError> {
        let releases = self.get_releases_for_album(album_id).await?;
        Ok(releases.into_iter().next().expect("the album holds a release").id)
    }

    /// Rename `album_id` as a person's metadata edit of its first release
    /// does.
    pub async fn rename_album_for_test(
        &self,
        album_id: &str,
        title: &str,
    ) -> Result<(), IdentityWriteError> {
        let release = self.edited_release_of_for_test(album_id).await?;
        self.save_release_metadata_for_test(&release, |rows| rows.album.title = title.to_string())
            .await
    }

    /// Credit `album_id` to the library artists `artists`, in that order, as
    /// a person's metadata edit of its first release does, picking each.
    pub async fn credit_album_for_test(
        &self,
        album_id: &str,
        artists: &[&str],
    ) -> Result<(), IdentityWriteError> {
        let release = self.edited_release_of_for_test(album_id).await?;
        let mut credited = Vec::new();
        for artist in artists {
            credited.push(self.find_artist_by_id(artist).await?.expect("a library artist"));
        }
        let now = self.store.now_utc();
        self.save_release_metadata_for_test(&release, |rows| {
            rows.album_artists = artists
                .iter()
                .zip(0..)
                .map(|(artist, position)| DbAlbumArtist::new(album_id, artist, position, now))
                .collect();
            for artist in credited {
                if !rows.picked.contains(&artist.id) {
                    rows.picked.push(artist.id.clone());
                    rows.credits.push(artist);
                }
            }
        })
        .await
    }

    /// Rename `track_id` as a person's metadata edit of its release does.
    pub async fn rename_track_for_test(
        &self,
        track_id: &str,
        title: &str,
    ) -> Result<(), IdentityWriteError> {
        let track = self.find_track_by_id(track_id).await?.expect("the track to rename");
        self.save_release_metadata_for_test(&track.release_id, |rows| {
            for (id, track) in &mut rows.track_updates {
                if id == track_id {
                    track.title = title.to_string();
                }
            }
        })
        .await
    }
}

/// One credit as a test writes it back: the artist, its place, the other
/// printed name it prints if any, and what follows it.
type CreditForTest = (String, u32, Option<crate::credits::CreditName>, Option<String>);

/// The credits `table` holds for `owner_column = owner`, in order.
fn credits_for_test(
    sql: &SqlReadContext<'_>,
    table: &str,
    owner_column: &str,
    owner: &str,
) -> Result<Vec<CreditForTest>, DbError> {
    Ok(sql.query(
        &format!(
            "SELECT c.artist_id, c.position, c.credit_key, n.name, c.join_phrase FROM {table} AS c \
             LEFT JOIN artist_credits AS n \
               ON n.artist_id = c.artist_id AND n.name_key = c.credit_key \
             WHERE c.{owner_column} = ?1 ORDER BY c.position"
        ),
        [owner],
        |row| {
            let credit = match (row.get::<_, Option<String>>(2)?, row.get::<_, Option<String>>(3)?)
            {
                (Some(key), Some(name)) => Some(crate::credits::CreditName { key, name }),
                _ => None,
            };
            Ok((row.get(0)?, row.get(1)?, credit, row.get(4)?))
        },
    )?)
}

/// The column of `table` that references a coven file.
fn file_column_for_test(table: &str) -> &'static str {
    match table {
        "release_files" | "covers" | "artist_images" => "file",
        "people" => "avatar_id",
        other => panic!("{other} references no coven file"),
    }
}

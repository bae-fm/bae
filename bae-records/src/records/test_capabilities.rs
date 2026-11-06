use super::*;

impl Records {
    /// Whether the coven file `table`'s row `row_id` references waits in
    /// this device's upload queue, live.
    pub fn subscribe_row_file_queued_for_test(
        &self,
        table: &str,
        row_id: &str,
    ) -> coven::LiveQuery<bool> {
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
    /// Rename `track_id` as a person's metadata edit of its release does,
    /// writing the album, the release and the track's credits back as they
    /// stand.
    pub async fn rename_track_for_test(
        &self,
        track_id: &str,
        title: &str,
    ) -> Result<(), IdentityWriteError> {
        let track = self.find_track_by_id(track_id).await?.expect("the track to rename");
        let release = self.find_release_by_id(&track.release_id).await?.expect("its release");
        let album = self.find_album_by_id(&release.album_id).await?.expect("its album");
        let (album_id, track_id) = (album.id.clone(), track.id.clone());
        let (album_credits, track_credits) = self
            .store
            .read(move |sql| {
                Ok((
                    credits_for_test(&sql, "album_artists", "album_id", &album_id)?,
                    credits_for_test(&sql, "track_artists", "track_id", &track_id)?,
                ))
            })
            .await?;
        let mut credits = Vec::new();
        for (artist_id, ..) in album_credits.iter().chain(&track_credits) {
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
            .map(|(artist_id, position, credit, join_phrase)| DbTrackArtist {
                credit,
                join_phrase,
                ..DbTrackArtist::new(&track.id, &artist_id, position, now)
            })
            .collect();
        let renamed = DbTrack { title: title.to_string(), ..track };
        self.update_release_metadata_user_edit(ReleaseMetadataRows {
            album,
            release,
            track_updates: vec![(renamed.id.clone(), renamed)],
            picked: credits.iter().map(|artist| artist.id.clone()).collect(),
            credits,
            album_artists,
            track_artists,
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

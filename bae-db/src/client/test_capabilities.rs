use super::*;

impl Database {
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
        self.inner
            .store
            .subscribe(move |sql| Ok(sql.query_row(&statement, [&row_id], |row| row.get(0))?))
    }

    /// How many of the files waiting in this device's upload queue `table`'s
    /// rows reference.
    pub async fn queued_upload_count_in_table_for_test(&self, table: &str) -> Result<i64, DbError> {
        let statement = format!(
            "SELECT COUNT(*) FROM coven_uploads AS upload \
             JOIN {table} AS t ON t.{column} = upload.file",
            column = file_column_for_test(table),
        );
        self.inner.store.read(move |sql| Ok(sql.query_row(&statement, [], |row| row.get(0))?)).await
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
        self.inner
            .store
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
        self.inner
            .store
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
        self.inner
            .store
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
        self.inner
            .store
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

/// The column of `table` that references a coven file.
fn file_column_for_test(table: &str) -> &'static str {
    match table {
        "release_files" | "covers" | "artist_images" => "file",
        "people" => "avatar_id",
        other => panic!("{other} references no coven file"),
    }
}

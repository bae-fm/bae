use super::*;

impl Database {
    pub async fn queued_upload_count_for_test(&self) -> Result<usize, DbError> {
        Ok(self.inner.handle.queued_uploads().await?.len())
    }

    pub async fn queued_upload_count_for_root_for_test(
        &self,
        root_table: &str,
        root_id: &str,
    ) -> Result<usize, DbError> {
        Ok(self
            .inner
            .handle
            .queued_uploads_for_root(root_table, root_id)
            .await?
            .len())
    }

    pub async fn queued_upload_rows_for_root_for_test(
        &self,
        root_table: &str,
        root_id: &str,
    ) -> Result<Vec<(String, String)>, DbError> {
        Ok(self
            .inner
            .handle
            .queued_uploads_for_root(root_table, root_id)
            .await?
            .into_iter()
            .map(|upload| {
                (
                    upload.blob.table().to_string(),
                    upload.blob.row_id().to_string(),
                )
            })
            .collect())
    }

    pub async fn queued_delete_count_for_test(&self) -> Result<usize, DbError> {
        Ok(self.inner.handle.queued_deletes().await?.len())
    }

    pub async fn has_queued_delete_for_test(
        &self,
        namespace: &str,
        blob_id: &str,
    ) -> Result<bool, DbError> {
        Ok(self
            .inner
            .handle
            .queued_deletes()
            .await?
            .iter()
            .any(|delete| delete.namespace == namespace && delete.blob_id == blob_id))
    }

    pub async fn first_queued_upload_failure_for_test(
        &self,
    ) -> Result<Option<(u64, bool)>, DbError> {
        Ok(self
            .inner
            .handle
            .queued_uploads()
            .await?
            .first()
            .map(|upload| (upload.attempt_count, upload.last_error.is_some())))
    }

    pub async fn pending_and_blocked_writes_for_test(
        &self,
    ) -> Result<(String, String), CovenError> {
        Ok((
            format!("{:?}", self.inner.handle.pending_writes().await?),
            format!("{:?}", self.inner.handle.blocked_writes().await?),
        ))
    }

    pub async fn rename_release_files_table_for_test(&self) -> Result<(), DbError> {
        self.rename_host_table_for_test("release_files", "release_files_unavailable")
            .await
    }

    pub async fn rename_tracks_table_for_test(&self) -> Result<(), DbError> {
        self.rename_host_table_for_test("tracks", "tracks_unavailable")
            .await
    }

    pub async fn rename_covers_table_for_test(&self) -> Result<(), DbError> {
        self.rename_host_table_for_test("covers", "covers_unavailable")
            .await
    }

    /// Take away the table a folder scan reads the user's stored file
    /// decisions from, the way a database left behind by an older build is
    /// missing what the current one reads.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn rename_candidate_file_edit_table_for_test(&self) -> Result<(), DbError> {
        self.rename_host_table_for_test(
            "import_candidate_file_edit",
            "import_candidate_file_edit_unavailable",
        )
        .await
    }

    async fn rename_host_table_for_test(&self, from: &str, to: &str) -> Result<(), DbError> {
        let statement = format!("ALTER TABLE {from} RENAME TO {to}");
        self.call_sql(move |sql| {
            sql.execute(&statement, [])?;
            Ok(())
        })
        .await
    }

    pub async fn library_row_counts_for_test(&self) -> Result<(i64, i64, i64), DbError> {
        self.read(|sql| {
            Ok((
                sql.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0))?,
                sql.query_row("SELECT COUNT(*) FROM albums", [], |row| row.get(0))?,
                sql.query_row("SELECT COUNT(*) FROM artists", [], |row| row.get(0))?,
            ))
        })
        .await
    }

    pub async fn failed_import_work_state_for_test(
        &self,
        shared_work_id: &str,
        exclusive_work_id: &str,
        part_work_id: &str,
        shared_composer_id: &str,
        exclusive_composer_id: &str,
    ) -> Result<(bool, bool, bool, i64, bool, bool, i64), DbError> {
        let shared_work_id = shared_work_id.to_string();
        let exclusive_work_id = exclusive_work_id.to_string();
        let part_work_id = part_work_id.to_string();
        let shared_composer_id = shared_composer_id.to_string();
        let exclusive_composer_id = exclusive_composer_id.to_string();
        self.read(move |sql| {
            let work_exists = |id: &str| -> coven::rusqlite::Result<bool> {
                sql.query_row(
                    "SELECT EXISTS(SELECT 1 FROM works WHERE musicbrainz_work_id = ?1)",
                    [id],
                    |row| row.get(0),
                )
            };
            let composer_exists = |id: &str| -> coven::rusqlite::Result<bool> {
                sql.query_row(
                    "SELECT EXISTS(SELECT 1 FROM artists WHERE musicbrainz_artist_id = ?1)",
                    [id],
                    |row| row.get(0),
                )
            };
            Ok((
                work_exists(&shared_work_id)?,
                work_exists(&exclusive_work_id)?,
                work_exists(&part_work_id)?,
                sql.query_row("SELECT COUNT(*) FROM work_parts", [], |row| row.get(0))?,
                composer_exists(&shared_composer_id)?,
                composer_exists(&exclusive_composer_id)?,
                sql.query_row(
                    "SELECT COUNT(*) FROM work_artists wa \
                     WHERE wa.work_id NOT IN (SELECT work_id FROM track_works)",
                    [],
                    |row| row.get(0),
                )?,
            ))
        })
        .await
    }

    pub async fn work_links_for_test(
        &self,
        musicbrainz_work_id: &str,
    ) -> Result<(Vec<String>, Vec<String>), DbError> {
        let musicbrainz_work_id = musicbrainz_work_id.to_string();
        self.read(move |sql| {
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

    pub async fn committed_track_files_for_test(
        &self,
        release_id: &str,
    ) -> Result<Vec<(String, String)>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| {
            sql.query(
                "SELECT t.title, rf.original_filename \
                 FROM tracks t \
                 JOIN audio_formats af ON af.track_id = t.id \
                 JOIN audio_format_segments seg \
                   ON seg.audio_format_id = af.id AND seg.role = 'main' \
                 JOIN release_files rf ON rf.id = seg.file_id \
                 WHERE t.release_id = ?1 \
                 ORDER BY t.side, t.track_number",
                [&release_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .map_err(DbError::from)
        })
        .await
    }

    pub async fn content_hash_query_plan_for_test(&self) -> Result<Vec<String>, DbError> {
        self.read(|sql| {
            sql.query(
                "EXPLAIN QUERY PLAN \
                 SELECT 1 FROM releases WHERE content_hash = ? LIMIT 1",
                ["hash"],
                |row| row.get::<_, String>(3),
            )
            .map_err(DbError::from)
        })
        .await
    }
}

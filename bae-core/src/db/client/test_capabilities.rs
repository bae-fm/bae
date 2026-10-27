use super::*;

impl Database {
    /// Run `statement` with `params` in one write: how a test seeds rows no
    /// one call writes on its own.
    pub async fn execute_for_test(
        &self,
        statement: &str,
        params: Vec<String>,
    ) -> Result<(), DbError> {
        let statement = statement.to_string();
        self.call_sql(move |sql| {
            sql.execute(&statement, rusqlite::params_from_iter(params.iter()))?;
            Ok(())
        })
        .await
    }

    /// How many rows of `table` have `column` equal to `value`.
    pub async fn count_rows_for_test(
        &self,
        table: &str,
        column: &str,
        value: &str,
    ) -> Result<i64, DbError> {
        let query = format!("SELECT COUNT(*) FROM \"{table}\" WHERE \"{column}\" = ?");
        let value = value.to_string();
        self.read(move |sql| {
            sql.query_row(&query, rusqlite::params![value], |row| row.get(0))
                .map_err(DbError::from)
        })
        .await
    }

    pub fn store_id_for_test(&self) -> coven::StoreId {
        self.store_id()
    }

    /// Every row of every table bae can read, rendered, by table name: what
    /// a test compares to show an operation left the store as it found it.
    /// Coven's own tables are closed to bae's connections and are left out;
    /// the files coven keeps for bae are files under the library directory.
    pub async fn every_row_for_test(
        &self,
    ) -> Result<std::collections::BTreeMap<String, Vec<String>>, DbError> {
        self.read(|sql| {
            let tables: Vec<String> = sql.query(
                "SELECT name FROM sqlite_master WHERE type = 'table' \
                 AND substr(name, 1, 7) <> '_coven_' AND substr(name, 1, 7) <> 'sqlite_' \
                 ORDER BY name",
                [],
                |row| row.get(0),
            )?;
            let mut every_row = std::collections::BTreeMap::new();
            for table in tables {
                let mut rows = sql.query(&format!("SELECT * FROM \"{table}\""), [], |row| {
                    (0..row.as_ref().column_count())
                        .map(|column| {
                            row.get_ref(column).map(|value| match value {
                                rusqlite::types::ValueRef::Text(text) => {
                                    String::from_utf8_lossy(text).into_owned()
                                }
                                other => format!("{other:?}"),
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .map(|values| values.join(" | "))
                })?;
                rows.sort();
                every_row.insert(table, rows);
            }
            Ok(every_row)
        })
        .await
    }

    /// Every row each write still waiting to upload records, oldest write
    /// first: a row it changes as its table's name, and a row its SQL touched
    /// with no net change as `"<table> (unchanged)"`. Sorted within a write.
    pub async fn queued_write_rows_for_test(&self) -> Result<Vec<Vec<String>>, DbError> {
        Ok(self
            .inner
            .handle
            .test_queued_writes()
            .await?
            .into_iter()
            .map(|write| {
                let mut tables: Vec<String> = write
                    .parts
                    .iter()
                    .flat_map(|part| {
                        part.rows
                            .iter()
                            .map(|change| change.row.table.clone())
                            .chain(
                                part.executions
                                    .iter()
                                    .map(|row| format!("{} (unchanged)", row.row.table)),
                            )
                    })
                    .collect();
                tables.sort();
                tables
            })
            .collect())
    }

    /// How many writes this device committed and has not yet sent to storage.
    pub async fn unsent_writes_for_test(&self) -> Result<u64, DbError> {
        Ok(self
            .inner
            .handle
            .unsent_work()
            .await
            .map_err(DbError::from)?
            .writes)
    }

    /// How many files wait in this device's upload queue.
    pub async fn queued_upload_count_for_test(&self) -> Result<i64, DbError> {
        self.read(|sql| {
            Ok(
                sql.query_row("SELECT COUNT(*) FROM coven_upload_queue", [], |row| {
                    row.get(0)
                })?,
            )
        })
        .await
    }

    /// Whether the file of `table`'s row `row_id` waits in this device's
    /// upload queue, live.
    pub fn subscribe_row_file_queued_for_test(
        &self,
        table: &str,
        row_id: &str,
    ) -> coven::LiveQuery<bool> {
        let (table, row_id) = (table.to_string(), row_id.to_string());
        self.inner.handle.subscribe(move |sql| {
            Ok(sql.query_row(
                "SELECT EXISTS (SELECT 1 FROM coven_uploads \
                 WHERE table_name = ?1 AND key = coven_key(?2))",
                [&table, &row_id],
                |row| row.get(0),
            )?)
        })
    }

    /// How many of the files waiting in this device's upload queue are
    /// declared by `table`'s rows.
    pub async fn queued_upload_count_in_table_for_test(&self, table: &str) -> Result<i64, DbError> {
        let table = table.to_string();
        self.read(move |sql| {
            Ok(sql.query_row(
                "SELECT COUNT(*) FROM coven_uploads WHERE table_name = ?1",
                [table],
                |row| row.get(0),
            )?)
        })
        .await
    }

    pub async fn queued_upload_count_for_root_for_test(
        &self,
        root_table: &str,
        root_id: &str,
    ) -> Result<usize, DbError> {
        Ok(self
            .queued_upload_rows_for_root_for_test(root_table, root_id)
            .await?
            .len())
    }

    pub async fn queued_upload_rows_for_root_for_test(
        &self,
        root_table: &str,
        root_id: &str,
    ) -> Result<Vec<(String, String)>, DbError> {
        if root_table != "releases" {
            return Err(DbError::Message(format!(
                "unknown upload root {root_table}"
            )));
        }
        // The release's own files and its cover, whose row the release's id
        // keys.
        let root_id = root_id.to_string();
        self.read(move |sql| {
            Ok(sql.query(
                "SELECT upload.table_name, f.id FROM release_files AS f \
                 JOIN coven_uploads AS upload \
                     ON upload.table_name = 'release_files' AND upload.key = coven_key(f.id) \
                 WHERE f.release_id = ?1 \
                 UNION ALL \
                 SELECT upload.table_name, ?1 FROM coven_uploads AS upload \
                 WHERE upload.table_name = 'covers' AND upload.key = coven_key(?1)",
                [root_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?)
        })
        .await
    }

    /// Write rows into this device's local-only tables — state no sync ever
    /// carries — the way the import pane's own writers would leave them.
    pub async fn execute_local_sql_for_test(&self, statements: &str) -> Result<(), DbError> {
        let statements = statements.to_string();
        self.call_sql(move |sql| {
            sql.execute_batch(&statements)?;
            Ok(())
        })
        .await
    }

    /// Clear a row's user-provided file as a change another device made would,
    /// abandoning this device's pending attachment on it.
    pub async fn clear_user_file_for_test(&self, table: &str, id: &str) -> Result<(), DbError> {
        let (table, id) = (table.to_string(), id.to_string());
        self.call_sql(move |sql| {
            sql.clear_user_file(&table, id.as_str())?;
            Ok(())
        })
        .await
    }

    /// This device's abandoned attachments, oldest first.
    pub async fn abandoned_attachments_for_test(
        &self,
    ) -> Result<Vec<coven::AbandonedAttachment>, DbError> {
        self.inner
            .handle
            .abandoned_attachments()
            .await
            .map_err(DbError::from)
    }

    /// Answer one text-valued query against this device's library.
    pub async fn query_texts_for_test(&self, query: &str) -> Result<Vec<String>, DbError> {
        let query = query.to_string();
        self.read(move |sql| {
            sql.query(&query, [], |row| row.get::<_, String>(0))
                .map_err(DbError::from)
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

    /// Each track of the release in play order: its title, the MusicBrainz
    /// ids of the artists credited on it in a role, and of the works it
    /// performs, each in order.
    pub async fn track_sources_for_test(
        &self,
        release_id: &str,
    ) -> Result<Vec<(String, Vec<String>, Vec<String>)>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| {
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
        self.read(move |sql| {
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

    pub async fn content_hash_query_plan_for_test(&self) -> Result<Vec<String>, DbError> {
        self.read(|sql| {
            sql.query(
                "EXPLAIN QUERY PLAN \
                 SELECT release_id FROM local_release_origin WHERE content_hash = ?",
                ["hash"],
                |row| row.get::<_, String>(3),
            )
            .map_err(DbError::from)
        })
        .await
    }
}

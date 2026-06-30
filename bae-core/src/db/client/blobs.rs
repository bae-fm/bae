use super::*;

impl Database {
    /// Upsert a library image record
    pub async fn upsert_library_image(&self, image: &DbLibraryImage) -> Result<(), DbError> {
        let image = image.clone();
        let reg = self.register_stamp().await?;
        self.call(move |conn| upsert_library_image_row(conn, &image, &reg))
            .await
    }

    /// Write a host-provided image blob and its `covers`/`artist_images` row as
    /// one coven batch.
    pub async fn write_library_image_blob(
        &self,
        image: &DbLibraryImage,
        bytes: &[u8],
    ) -> Result<(), DbError> {
        let image = image.clone();
        let namespace = image_namespace(&image.image_type).to_string();
        let id = image.id.clone();
        let bytes = bytes.to_vec();
        self.inner
            .handle
            .write(move |w| {
                w.put_blob(namespace, id, bytes);
                w.sql(move |sql| {
                    let reg = sql.stamp();
                    upsert_library_image_row(sql.connection(), &image, &reg)
                        .map_err(CovenError::from)
                })?;
                Ok(())
            })
            .await
            .map_err(Self::coven_error)
    }

    /// Find a host-provided image (cover / artist image) by its subject id. The
    /// `image_type` selects the table (`covers` / `artist_images`); the id is the
    /// release/artist id. Caller-provided id — may not exist.
    pub async fn find_library_image(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<DbLibraryImage>, DbError> {
        let id = id.to_string();
        let image_type = image_type.clone();
        let table = image_table(&image_type);
        let sql = format!("SELECT * FROM {table} WHERE id = ?");
        self.call(move |conn| {
            conn.query_row(&sql, params![id], |row| {
                row_to_library_image(row, image_type.clone())
            })
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// The `_updated_at` version of each given release's `covers` row, for the ids
    /// that have one. The version a cover [`ImageRef`](crate::album_detail::ImageRef)
    /// carries: it moves when the cover bytes change (the upsert bumps it), so the
    /// UI's `(id, version)` cache key and the `AlbumUpdated` re-render fire. Ids
    /// with no cover row are absent from the map.
    pub async fn cover_versions(
        &self,
        release_ids: &[String],
    ) -> Result<HashMap<String, String>, DbError> {
        if release_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let ids = release_ids.to_vec();
        self.call(move |conn| {
            let placeholders = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!("SELECT id, _updated_at FROM covers WHERE id IN ({placeholders})");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(coven::rusqlite::params_from_iter(ids.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut map = HashMap::new();
            for row in rows {
                let (id, version) = row?;
                map.insert(id, version);
            }
            Ok(map)
        })
        .await
    }

    /// The `_updated_at` version of one release's `covers` row, or `None` when it
    /// has no cover. The single-id form of [`cover_versions`](Self::cover_versions).
    pub async fn cover_version(&self, release_id: &str) -> Result<Option<String>, DbError> {
        let ids = [release_id.to_string()];
        Ok(self.cover_versions(&ids).await?.remove(release_id))
    }

    /// Delete a host-provided image row by its subject id, from the table its type
    /// selects. (The row is also cascade-deleted with its subject; this is the
    /// explicit path, e.g. replacing a cover.)
    pub async fn delete_library_image(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<(), DbError> {
        let id = id.to_string();
        let table = image_table(image_type);
        let sql = format!("DELETE FROM {table} WHERE id = ?");
        self.call(move |conn| {
            conn.execute(&sql, params![id])
                .map(|_| ())
                .map_err(DbError::from)
        })
        .await
    }

    // ---- Readable cloud paths (browsable homes) ----

    /// Run a cloud-key resolver, but only on a browsable home: an opaque home
    /// keys blobs by hashed id and has no stored `cloud_path`, so it
    /// short-circuits to `Ok(None)`. The resolver runs on the owned coven
    /// connection (where it reads the release's album id). The two
    /// release-scoped `*_cloud_path_for_storage` accessors differ only in which
    /// resolver they pass.
    async fn cloud_path_if_browsable<F>(
        &self,
        storage: crate::config::HomeStorage,
        resolve: F,
    ) -> Result<Option<String>, DbError>
    where
        F: FnOnce(&Connection) -> Result<String, DbError> + Send + 'static,
    {
        if !storage.is_browsable() {
            return Ok(None);
        }
        self.call(move |conn| resolve(conn).map(Some)).await
    }

    /// The `cloud_path` for a cover image under `storage`: `None` for an opaque
    /// home, or `{album_id}/{release_id}/cover.{ext}` for a browsable one.
    pub async fn cover_cloud_path_for_storage(
        &self,
        storage: crate::config::HomeStorage,
        release_id: &str,
        content_type: &ContentType,
    ) -> Result<Option<String>, DbError> {
        let release_id = release_id.to_string();
        let content_type = content_type.clone();
        self.cloud_path_if_browsable(storage, move |conn| {
            resolve_cover_cloud_path(conn, &release_id, &content_type)
        })
        .await
    }

    /// The `cloud_path` for an artist image under `storage`: `None` for an
    /// opaque home, or `{artist_id}/artist.{ext}` for a browsable one. Keyed by
    /// the artist id alone, so it needs no DB lookup.
    pub fn artist_image_cloud_path_for_storage(
        &self,
        storage: crate::config::HomeStorage,
        artist_id: &str,
        content_type: &ContentType,
    ) -> Option<String> {
        if !storage.is_browsable() {
            return None;
        }
        Some(resolve_artist_cloud_path(artist_id, content_type))
    }

    // ---- Local blob refs ----

    pub async fn register_external_blob(
        &self,
        blob_id: &str,
        namespace: &str,
        path: &Path,
        size: u64,
    ) -> Result<(), DbError> {
        let blob_id = blob_id.to_string();
        let namespace = namespace.to_string();
        let path = path.to_path_buf();
        self.call(move |conn| register_external_blob_on(conn, &blob_id, &namespace, &path, size))
            .await
    }

    pub async fn clear_external_blob(&self, blob_id: &str) -> Result<(), DbError> {
        let blob_id = blob_id.to_string();
        self.call(move |conn| clear_external_blob_on(conn, &blob_id))
            .await
    }

    pub async fn external_blob(&self, blob_id: &str) -> Result<Option<ExternalBlob>, DbError> {
        let blob_id = blob_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT path, size FROM local_blob_refs WHERE blob_id = ?1",
                [blob_id],
                |row| {
                    Ok(ExternalBlob {
                        path: PathBuf::from(row.get::<_, String>(0)?),
                        size: row.get::<_, i64>(1)? as u64,
                    })
                },
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    // ---- Cloud outbox ----

    /// Seed an upload entry in coven's cloud outbox. Production never enqueues
    /// uploads this way — coven's `make_remote` owns that — so this exists only
    /// to exercise the outbox-snapshot / drain machinery in tests.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn add_cloud_outbox_upload(
        &self,
        file_id: &str,
        cloud_key: &str,
        source_path: Option<&str>,
        retain_pinned: bool,
    ) -> Result<(), DbError> {
        let created_at = self.register_stamp().await?;
        let file_id = file_id.to_string();
        let cloud_key = cloud_key.to_string();
        let source_path = source_path.map(str::to_string);
        self.call(move |conn| {
            conn.execute(
                "DELETE FROM cloud_outbox WHERE operation = 'delete' AND cloud_key = ?1",
                [&cloud_key],
            )
            .map_err(DbError::from)?;
            conn.execute(
                "INSERT OR IGNORE INTO cloud_outbox \
                 (operation, file_id, cloud_key, source_path, scope, retain_pinned, created_at) \
                 VALUES ('upload', ?1, ?2, ?3, ?4, ?5, ?6)",
                (
                    file_id,
                    cloud_key,
                    source_path,
                    coven::BlobScope::Master.to_outbox_str(),
                    retain_pinned,
                    created_at,
                ),
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Seed a tombstone-cancel entry — the coven-internal row its upload drain
    /// queues when an inline tombstone delete fails. Test-only: bae never
    /// enqueues cancels itself, but the UI snapshot query must tolerate them.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn add_cloud_outbox_cancel(&self, cloud_key: &str) -> Result<(), DbError> {
        let created_at = self.register_stamp().await?;
        let cloud_key = cloud_key.to_string();
        self.call(move |conn| {
            conn.execute(
                "INSERT OR IGNORE INTO cloud_outbox \
                 (operation, cloud_key, scope, created_at) \
                 VALUES ('cancel', ?1, NULL, ?2)",
                (&cloud_key, &created_at),
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Add a delete entry to the cloud outbox.
    pub async fn add_cloud_outbox_delete(&self, cloud_key: &str) -> Result<(), DbError> {
        let created_at = self.register_stamp().await?;
        let cloud_key = cloud_key.to_string();
        self.call(move |conn| {
            conn.execute(
                "DELETE FROM cloud_outbox \
                 WHERE operation IN ('upload', 'cancel') AND cloud_key = ?1",
                [&cloud_key],
            )
            .map_err(DbError::from)?;
            conn.execute(
                "INSERT OR IGNORE INTO cloud_outbox \
                 (operation, cloud_key, scope, created_at) \
                 VALUES ('delete', ?1, NULL, ?2)",
                (&cloud_key, &created_at),
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Remove a cloud outbox entry by id.
    pub async fn remove_cloud_outbox_entry(&self, id: i64) -> Result<(), DbError> {
        self.call(move |conn| {
            conn.execute("DELETE FROM cloud_outbox WHERE id = ?1", [id])
                .map(|_| ())
                .map_err(DbError::from)
        })
        .await
    }

    /// Remove all pending upload entries for a given cloud key. Used when a file
    /// is deleted before its upload completes.
    pub async fn remove_cloud_outbox_uploads_for_key(
        &self,
        cloud_key: &str,
    ) -> Result<(), DbError> {
        let cloud_key = cloud_key.to_string();
        self.call(move |conn| {
            conn.execute(
                "DELETE FROM cloud_outbox WHERE operation = 'upload' AND cloud_key = ?1",
                [cloud_key],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Clear the backoff timestamp on failed uploads so the next cycle retries.
    pub async fn reset_cloud_outbox_backoff(&self) -> Result<(), DbError> {
        self.call(move |conn| {
            conn.execute(
                "UPDATE cloud_outbox SET last_attempt_at = NULL \
                 WHERE operation = 'upload' AND attempt_count > 0",
                [],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn record_cloud_upload_failure(
        &self,
        id: i64,
        error: &str,
        attempted_at: &str,
    ) -> Result<(), DbError> {
        let error = error.to_string();
        let attempted_at = attempted_at.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE cloud_outbox \
                 SET attempt_count = attempt_count + 1, last_error = ?1, last_attempt_at = ?2 \
                 WHERE id = ?3",
                (error, attempted_at, id),
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_pending_cloud_uploads(&self) -> Result<Vec<coven::OutboxEntry>, DbError> {
        self.pending_outbox("upload").await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_pending_cloud_deletes(&self) -> Result<Vec<coven::OutboxEntry>, DbError> {
        self.pending_outbox("delete").await
    }

    #[cfg(any(test, feature = "test-utils"))]
    async fn pending_outbox(
        &self,
        operation: &'static str,
    ) -> Result<Vec<coven::OutboxEntry>, DbError> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, operation, file_id, cloud_key, source_path, scope, \
                        retain_pinned, attempt_count, last_attempt_at \
                 FROM cloud_outbox WHERE operation = ?1 ORDER BY id",
            )?;
            let rows = stmt.query_map([operation], row_to_outbox_entry)?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// The user-facing outbox entries (uploads and deletes), oldest first, each
    /// paired with the album title of the release its `file_id` belongs to
    /// (uploads only — `None` for deletes or an orphaned file). Backs the
    /// processing snapshot. coven's internal `cancel` rows (tombstone removals
    /// it retries after an upload) are excluded — they render nothing.
    pub async fn outbox_items(&self) -> Result<Vec<DbOutboxRow>, DbError> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT co.id, co.operation, co.file_id, co.cloud_key, \
                                co.created_at, co.attempt_count, co.last_error, \
                                rf.release_id AS release_id, rf.file_size AS file_size, \
                                rf.original_filename AS file_name, \
                                a.title AS title \
                         FROM cloud_outbox co \
                         LEFT JOIN release_files rf ON rf.id = co.file_id \
                         LEFT JOIN releases r ON r.id = rf.release_id \
                         LEFT JOIN albums a ON a.id = r.album_id \
                         WHERE co.operation IN ('upload', 'delete') \
                         ORDER BY co.id",
            )?;
            let rows = stmt.query_map([], |row| {
                // coven writes `created_at` as its HLC stamp
                // (`millis-counter-device`, the same format `make_remote`
                // enqueues with and `register_stamp` mints); the UI needs an
                // instant for the "queued N ago" label, so take the stamp's
                // physical millis. A value that isn't a coven stamp is corrupt
                // — surface it as a column-conversion error rather than masking
                // it. The index is `created_at`'s position in the SELECT
                // (`co.id`=0, …, `co.created_at`=4) so the diagnostic names the
                // right column.
                let created_at_raw = row.get::<_, String>("created_at")?;
                let created_at = coven::Timestamp::parse(&created_at_raw)
                    .map(|t| t.millis as i64)
                    .ok_or_else(|| {
                        coven::rusqlite::Error::FromSqlConversionFailure(
                            4,
                            coven::rusqlite::types::Type::Text,
                            format!("created_at {created_at_raw:?} is not a coven HLC stamp")
                                .into(),
                        )
                    })?;
                let operation_raw = row.get::<_, String>("operation")?;
                // Parse to the full domain, then keep only the displayed subset.
                // A non-domain string is genuine corruption (column-conversion
                // error, as before). A `cancel` (coven's tombstone retry) is kept
                // out of the snapshot by the WHERE filter; reaching it here means
                // that filter drifted, so warn with the row's key and drop it.
                let operation = match OutboxOpKind::parse(&operation_raw) {
                    Some(OutboxOpKind::Upload) => DisplayedOutboxOp::Upload,
                    Some(OutboxOpKind::Delete) => DisplayedOutboxOp::Delete,
                    Some(OutboxOpKind::Cancel) => {
                        let cloud_key = row.get::<_, String>("cloud_key")?;
                        tracing::warn!(
                            cloud_key,
                            "cancel row reached the outbox snapshot; the WHERE filter drifted — ignoring"
                        );
                        return Ok(None);
                    }
                    None => {
                        return Err(column_conversion_error(
                            row,
                            "operation",
                            format!("invalid cloud_outbox.operation: {operation_raw:?}"),
                        ));
                    }
                };
                Ok(Some(DbOutboxRow {
                    id: row.get("id")?,
                    operation,
                    file_id: row.get("file_id")?,
                    cloud_key: row.get("cloud_key")?,
                    created_at,
                    attempt_count: row.get("attempt_count")?,
                    last_error: row.get("last_error")?,
                    release_id: row.get("release_id")?,
                    title: row.get("title")?,
                    file_name: row.get("file_name")?,
                    file_size: row.get("file_size")?,
                }))
            })?;
            rows.collect::<coven::rusqlite::Result<Vec<Option<DbOutboxRow>>>>()
                .map(|rows| rows.into_iter().flatten().collect())
                .map_err(DbError::from)
        })
        .await
    }
}

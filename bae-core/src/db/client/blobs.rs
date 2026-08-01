use super::*;

impl Database {
    pub async fn upsert_library_image(&self, image: &DbLibraryImage) -> Result<(), DbError> {
        let image = image.clone();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            upsert_library_image_row(&sql, &image, &reg)
        })
        .await
    }

    /// Write a host-provided image blob and its `covers`/`artist_images` row as one
    /// coven batch — storing an image for the first time, and replacing one that is
    /// already there.
    ///
    /// A coven blob id names one immutable byte-string, so replacing an image's
    /// bytes means a **new blob**, never new bytes under the live id (coven refuses
    /// that outright). `image.blob_id` is the new blob; when the row already
    /// references a different one, the old blob is deleted in the same batch. coven
    /// admits both halves: the new blob id is referenced by no row before the
    /// closure runs, and the old one is referenced by none after it repoints the
    /// row.
    ///
    /// The replaced blob's cloud object is always tombstoned, because it is never the
    /// object the new blob writes: an image's cloud key is a pure function of its
    /// blob id under both home layouts — the hashed id on an opaque home, the
    /// `cover-{blob_id}` / `artist-{blob_id}` readable path on a browsable one — and
    /// a blob id is minted fresh per stored image. Whether the replaced blob ever
    /// reached the cloud needs no separate check: tombstoning a key holding no
    /// object is a no-op the GC cleans up.
    pub async fn write_library_image_blob(
        &self,
        image: &DbLibraryImage,
        bytes: &[u8],
    ) -> Result<(), DbError> {
        let image = image.clone();
        let namespace = image.image_type.namespace();
        let replaced = self
            .find_library_image(&image.id, &image.image_type)
            .await?
            .filter(|existing| existing.blob_id != image.blob_id)
            .map(|existing| {
                crate::sync::image_blob_ref(namespace, &existing.blob_id, existing.cloud_path)
            });

        // The cloud object the replaced blob occupies, captured as an exact row
        // reference while the row still points at it. Only a blob that actually
        // reached the cloud has one to remove — a Local image carries no stored
        // locator, and coven refuses a tombstone for it.
        let stale_remote = if replaced.is_some() {
            self.inner
                .handle
                .row_blob_ref(image_table(&image.image_type), &image.id)
                .await
                .ok()
                .filter(|current| current.stored().is_some())
        } else {
            None
        };

        let new_blob =
            crate::sync::image_blob_ref(namespace, &image.blob_id, image.cloud_path.clone());
        let bytes = bytes.to_vec();
        self.inner
            .handle
            .write(
                move |w| {
                    w.put_blob(new_blob.namespace.clone(), new_blob.id.clone(), bytes);
                    if let Some(old) = replaced {
                        w.delete_blob(old);
                    }
                    Ok(())
                },
                move |sql| {
                    let reg = sql.stamp();
                    upsert_library_image_row(&sql, &image, &reg).map_err(CovenError::from)?;
                    if let Some(stale) = &stale_remote {
                        sql.enqueue_blob_delete(stale).map_err(CovenError::from)?;
                    }
                    Ok(())
                },
            )
            .await
            .map(|_| ())
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
        let query = format!("SELECT * FROM {table} WHERE id = ?");
        self.read(move |sql| {
            sql.query_row(&query, params![id], |row| {
                row_to_library_image(row, image_type.clone())
            })
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// The `_updated_at` version of each given release's `covers` row; ids with no
    /// cover row are absent from the map. This is the version a cover
    /// [`ImageRef`](crate::album_detail::ImageRef) carries — it moves whenever the
    /// cover bytes change, which is what invalidates the UI's `(id, version)` cache
    /// key and makes the `AlbumUpdated` re-render show the new art.
    pub async fn cover_versions(
        &self,
        release_ids: &[String],
    ) -> Result<HashMap<String, String>, DbError> {
        self.image_versions(LibraryImageType::Cover, release_ids)
            .await
    }

    /// The `_updated_at` version of each given artist's `artist_images` row, for
    /// the ids that have one. Ids with no artist image row are absent from the
    /// map.
    pub async fn artist_image_versions(
        &self,
        artist_ids: &[String],
    ) -> Result<HashMap<String, String>, DbError> {
        self.image_versions(LibraryImageType::Artist, artist_ids)
            .await
    }

    async fn image_versions(
        &self,
        image_type: LibraryImageType,
        ids: &[String],
    ) -> Result<HashMap<String, String>, DbError> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let ids = ids.to_vec();
        self.read(move |sql| image_versions(&sql, image_type, &ids))
            .await
    }

    /// The `_updated_at` version of one release's `covers` row, or `None` when it
    /// has no cover. The single-id form of [`cover_versions`](Self::cover_versions).
    pub async fn cover_version(&self, release_id: &str) -> Result<Option<String>, DbError> {
        let ids = [release_id.to_string()];
        Ok(self.cover_versions(&ids).await?.remove(release_id))
    }

    /// Run a cloud-key resolver, but only on a browsable home: an opaque home keys
    /// blobs by hashed id and stores no `cloud_path`, so it returns `Ok(None)`
    /// without running the resolver. The resolver runs on coven's connection —
    /// that is where it reads the release's album id.
    async fn cloud_path_if_browsable<F>(
        &self,
        storage: crate::config::HomeStorage,
        resolve: F,
    ) -> Result<Option<String>, DbError>
    where
        F: for<'conn> FnOnce(&SqlReadContext<'conn>) -> Result<String, DbError> + Send + 'static,
    {
        if !storage.is_browsable() {
            return Ok(None);
        }
        self.read(move |sql| resolve(&sql).map(Some)).await
    }

    /// The `cloud_path` for a cover image under `storage`: `None` for an opaque
    /// home, or `{album_id}/{release_id}/cover-{blob_id}.{ext}` for a browsable one.
    /// `blob_id` is the blob these bytes become, so a replaced cover keys to a new
    /// object rather than overwriting the one it replaces.
    pub async fn cover_cloud_path_for_storage(
        &self,
        storage: crate::config::HomeStorage,
        release_id: &str,
        blob_id: &str,
        content_type: &ContentType,
    ) -> Result<Option<String>, DbError> {
        let release_id = release_id.to_string();
        let blob_id = blob_id.to_string();
        let content_type = content_type.clone();
        self.cloud_path_if_browsable(storage, move |sql| {
            resolve_cover_cloud_path(sql, &release_id, &blob_id, &content_type)
        })
        .await
    }

    /// The `cloud_path` for an artist image under `storage`: `None` for an
    /// opaque home, or `{artist_id}/artist-{blob_id}.{ext}` for a browsable one.
    /// Keyed by the artist and its blob id alone, so it needs no DB lookup.
    pub fn artist_image_cloud_path_for_storage(
        &self,
        storage: crate::config::HomeStorage,
        artist_id: &str,
        blob_id: &str,
        content_type: &ContentType,
    ) -> Option<String> {
        artist_image_cloud_path_for_storage(storage, artist_id, blob_id, content_type)
    }

    /// Test-only: point a row's blob at a file the user owns, in a write of its
    /// own. Production registers refs inside the atomic import transaction that
    /// inserts the row, which is where the binding belongs; this exists for the
    /// tests that repoint an already-inserted row at a file they just wrote.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn register_external_blob(
        &self,
        table: &str,
        row_id: &str,
        path: &Path,
    ) -> Result<(), DbError> {
        let table = table.to_string();
        let row_id = row_id.to_string();
        let path = path.to_path_buf();
        self.call(move |conn| conn.register_external_blob(&table, &row_id, &path))
            .await
    }

    /// Where the user's own file for a release file lives on disk, or `None`
    /// when the row carries no external registration.
    ///
    /// `release_files` is bae's only user-provided table — every other blob is
    /// coven's own copy — so a file id is all this needs. Callers that want the
    /// *bytes* read them through coven instead (`LibraryManager::read_release_blob`);
    /// this is for the paths themselves: re-reading a file's tags, or telling a
    /// user where their music actually is.
    pub async fn external_blob(
        &self,
        file_id: &str,
    ) -> Result<Option<coven::ExternalBlob>, DbError> {
        self.inner
            .handle
            .external_blob(crate::sync::RELEASE_FILES_NAMESPACE, file_id)
            .await
    }

    /// Whether a queued cloud upload exists for `file_id` — a failed attempt
    /// stays queued for retry, so it still counts. Playback's read-failure
    /// classification uses this to tell "source file gone while its upload is
    /// queued" from "source file gone, files moved or deleted".
    pub async fn has_pending_cloud_upload(&self, file_id: &str) -> Result<bool, DbError> {
        // coven owns the upload queue; a queued upload names the blob-bearing
        // row it belongs to, which for audio is the `release_files` row itself.
        Ok(self
            .inner
            .handle
            .queued_uploads()
            .await?
            .iter()
            .any(|upload| upload.table_name == "release_files" && upload.row_id == file_id))
    }

    /// What coven's durable cloud queue is holding, joined to the bae context the
    /// Storage Manager renders it with. Backs the processing snapshot.
    ///
    /// The two halves come from the two owners: coven reports every queued upload
    /// and cloud tombstone (oldest first, surviving restarts), and bae looks up
    /// the file name, size, and album title of each upload's `release_files` row.
    /// An upload whose row has since gone keeps its place in the queue with no
    /// context — it is still work owed to the cloud.
    pub async fn outbox_queue(&self) -> Result<DbOutboxQueue, DbError> {
        let uploads = self.inner.handle.queued_uploads().await?;
        let deletes = self.inner.handle.queued_deletes().await?;

        let file_ids: Vec<String> = uploads.iter().map(|u| u.row_id.clone()).collect();
        let context = self.outbox_upload_context(&file_ids).await?;

        let uploads = uploads
            .into_iter()
            .map(|upload| {
                let context = context.get(&upload.row_id);
                Ok(DbOutboxUpload {
                    // `releases` is bae's only gated root, so every upload bae
                    // enqueues names one. Anything else is a shape bae does not
                    // produce; it renders in the ungrouped bucket rather than
                    // being silently dropped from the queue.
                    release_id: (upload.root_table == "releases").then_some(upload.root_id),
                    file_id: upload.row_id,
                    attempt_count: upload.attempt_count,
                    last_error: upload.last_error,
                    created_at: stamp_millis(&upload.created_at)?,
                    file_name: context.map(|c| c.file_name.clone()),
                    file_size: context.map(|c| c.file_size),
                    album_title: context.and_then(|c| c.album_title.clone()),
                })
            })
            .collect::<Result<Vec<_>, DbError>>()?;

        let deletes = deletes
            .into_iter()
            .map(|delete| {
                Ok(DbOutboxDelete {
                    namespace: delete.namespace,
                    blob_id: delete.blob_id,
                    created_at: stamp_millis(&delete.created_at)?,
                })
            })
            .collect::<Result<Vec<_>, DbError>>()?;

        Ok(DbOutboxQueue { uploads, deletes })
    }

    /// The queue-pane context for a batch of `release_files` ids: the file's
    /// name and stored size, plus its release's album title. Ids with no row are
    /// absent from the map.
    async fn outbox_upload_context(
        &self,
        file_ids: &[String],
    ) -> Result<HashMap<String, OutboxUploadContext>, DbError> {
        if file_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let file_ids = file_ids.to_vec();
        self.read(move |sql| {
            let mut map = HashMap::new();
            for chunk in file_ids.chunks(SQL_MAX_IN_VARS) {
                let placeholders = in_clause_placeholders(chunk.len());
                let query = format!(
                    "SELECT rf.id, rf.original_filename, rf.file_size, a.title \
                     FROM release_files rf \
                     LEFT JOIN releases r ON r.id = rf.release_id \
                     LEFT JOIN albums a ON a.id = r.album_id \
                     WHERE rf.id IN ({placeholders})"
                );
                let rows = sql.query(
                    &query,
                    coven::rusqlite::params_from_iter(chunk.iter()),
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            OutboxUploadContext {
                                file_name: row.get(1)?,
                                file_size: row.get(2)?,
                                album_title: row.get(3)?,
                            },
                        ))
                    },
                )?;
                map.extend(rows);
            }
            Ok(map)
        })
        .await
    }
}

/// The bae half of a queued upload's rendering: what its `release_files` row
/// and that row's album say about it.
#[derive(Debug, Clone)]
struct OutboxUploadContext {
    file_name: String,
    file_size: i64,
    album_title: Option<String>,
}

/// coven stamps a queue entry's `created_at` with its HLC (`millis-counter-device`);
/// the UI needs an instant for the "queued N ago" label, so take the stamp's
/// physical millis. A value that isn't a coven stamp is corrupt, not a default.
fn stamp_millis(raw: &str) -> Result<i64, DbError> {
    coven::Timestamp::parse(raw)
        .map(|t| t.millis as i64)
        .ok_or_else(|| DbError::Message(format!("queue created_at {raw:?} is not a coven stamp")))
}

/// `ids` is a whole page's worth of releases (or, for the storage page, each row's
/// release plus its album's), so it is unbounded and must be chunked under
/// SQLite's variable limit like every other batched `IN` query here.
fn image_versions(
    sql: &SqlReadContext<'_>,
    image_type: LibraryImageType,
    ids: &[String],
) -> Result<HashMap<String, String>, DbError> {
    let table = image_table(&image_type);
    let mut map = HashMap::new();
    for chunk in ids.chunks(SQL_MAX_IN_VARS) {
        let placeholders = in_clause_placeholders(chunk.len());
        let query = format!("SELECT id, _updated_at FROM {table} WHERE id IN ({placeholders})");
        map.extend(sql.query(
            &query,
            coven::rusqlite::params_from_iter(chunk.iter()),
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?);
    }
    Ok(map)
}

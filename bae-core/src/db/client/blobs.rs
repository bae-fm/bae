use super::*;

/// The bae rows needed to label one exact coven outbox snapshot.
///
/// This is an absolute live-query request: when coven changes the durable
/// queue, the subscription is reconfigured to follow precisely the release
/// and release-file rows named by that snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OutboxDisplayRequest {
    release_file_ids: Vec<String>,
    release_ids: Vec<String>,
}

/// Display values read reactively for an [`OutboxDisplayRequest`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OutboxDisplayContext {
    file_names: HashMap<String, String>,
}

/// What the display read found for one queued upload's release root.
///
/// Absence from the map is a third answer — no `releases` row at all — and the
/// three are separate on purpose: a release that is gone and a release whose
/// album is gone are different defects with different causes, and one message
/// covering both says neither.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReleaseAlbumTitle {
    /// The release row and the album row it names are both there.
    Known(String),
    /// The release row is there and names an album row that is not.
    AlbumMissing { album_id: String },
}

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
            .write_with_blobs(
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
    /// cover bytes change, which moves the UI's `(id, version)` cache key and makes
    /// the subscribed album value show the new art.
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
        self.read(move |sql| image_versions_on(&sql, image_type, &ids))
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
            .any(|upload| {
                upload.blob.table() == crate::sync::RELEASE_FILES_NAMESPACE
                    && upload.blob.row_id() == file_id
            }))
    }

    /// What coven's durable cloud queue is holding, joined to the bae context the
    /// Storage Manager renders it with. Backs the processing snapshot.
    ///
    /// The two halves come from the two owners: coven reports every queued upload
    /// and cloud tombstone (oldest first, surviving restarts), and bae reads
    /// display context from each upload's declared blob-bearing table. Missing
    /// context is an invalid queue snapshot and is surfaced to the subscriber.
    pub async fn outbox_queue(&self) -> Result<DbOutboxQueue, DbError> {
        let snapshot = self.inner.handle.cloud_outbox_snapshot().await?;
        self.outbox_queue_from(snapshot).await
    }

    /// Subscribe to coven's durable uploads, deletes, and make-Remote intents
    /// as one committed stream. Display context is joined by
    /// [`outbox_queue_from`](Self::outbox_queue_from) after each value arrives.
    pub fn subscribe_cloud_outbox(&self) -> coven::CloudOutboxLiveQuery {
        self.inner.handle.subscribe_cloud_outbox()
    }

    pub async fn outbox_queue_from(
        &self,
        snapshot: coven::CloudOutboxSnapshot,
    ) -> Result<DbOutboxQueue, DbError> {
        let request = Self::outbox_display_request(&snapshot)?;
        let context = self.outbox_display_context(request).await?;
        Self::outbox_queue_from_context(snapshot, context)
    }

    /// Identify the bae rows that label an exact durable outbox snapshot.
    /// Invalid roots fail here rather than producing a partially labelled UI.
    pub(crate) fn outbox_display_request(
        snapshot: &coven::CloudOutboxSnapshot,
    ) -> Result<OutboxDisplayRequest, DbError> {
        let mut release_file_ids = snapshot
            .uploads
            .iter()
            .filter(|upload| upload.blob.table() == crate::sync::RELEASE_FILES_NAMESPACE)
            .map(|upload| upload.blob.row_id().to_string())
            .collect::<Vec<_>>();
        release_file_ids.sort();
        release_file_ids.dedup();

        let mut release_ids = snapshot
            .uploads
            .iter()
            .map(|upload| {
                if upload.root_table != "releases" {
                    return Err(DbError::Message(format!(
                        "queued upload root {}:{} is not a release",
                        upload.root_table, upload.root_id
                    )));
                }
                Ok(upload.root_id.clone())
            })
            .collect::<Result<Vec<_>, DbError>>()?;
        release_ids.extend(
            snapshot
                .make_remotes
                .iter()
                .map(|transition| {
                    if transition.root_table != "releases" {
                        return Err(DbError::Message(format!(
                            "make-Remote root {}:{} is not a release",
                            transition.root_table, transition.root_id
                        )));
                    }
                    Ok(transition.root_id.clone())
                })
                .collect::<Result<Vec<_>, DbError>>()?,
        );
        release_ids.sort();
        release_ids.dedup();

        Ok(OutboxDisplayRequest {
            release_file_ids,
            release_ids,
        })
    }

    /// Follow the bae display rows named by the current coven outbox. A title,
    /// release-to-album link, or original filename change produces a new value
    /// even while the durable outbox itself is unchanged.
    pub(crate) fn subscribe_outbox_display(
        &self,
        initial: OutboxDisplayRequest,
    ) -> coven::ReconfigurableLiveQuery<OutboxDisplayRequest, OutboxDisplayContext> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |request, sql| {
                Ok(OutboxDisplayContext {
                    file_names: outbox_release_file_names_on(&sql, &request.release_file_ids)?,
                })
            })
    }

    async fn outbox_display_context(
        &self,
        request: OutboxDisplayRequest,
    ) -> Result<OutboxDisplayContext, DbError> {
        self.read(move |sql| {
            Ok(OutboxDisplayContext {
                file_names: outbox_release_file_names_on(&sql, &request.release_file_ids)?,
            })
        })
        .await
    }

    pub(crate) fn outbox_queue_from_context(
        snapshot: coven::CloudOutboxSnapshot,
        context: OutboxDisplayContext,
    ) -> Result<DbOutboxQueue, DbError> {
        let coven::CloudOutboxSnapshot {
            uploads,
            deletes,
            make_remotes,
        } = snapshot;

        let OutboxDisplayContext { file_names } = context;

        let uploads = uploads
            .into_iter()
            .map(|upload| {
                let label = match upload.blob.table() {
                    // A file whose row went with its release has nothing left
                    // to name it, and needs nothing: the entry is on its way
                    // out, which is what the row says it is.
                    crate::sync::RELEASE_FILES_NAMESPACE => file_names
                        .get(upload.blob.row_id())
                        .map_or(crate::library::UploadFileLabel::Unwinding, |file_name| {
                            crate::library::UploadFileLabel::Filename(file_name.clone())
                        }),
                    crate::sync::COVERS_NAMESPACE => crate::library::UploadFileLabel::Cover,
                    crate::sync::ARTIST_IMAGES_NAMESPACE => {
                        crate::library::UploadFileLabel::ArtistImage
                    }
                    table => {
                        return Err(DbError::Message(format!(
                            "queued upload names undeclared blob table {table:?}"
                        )));
                    }
                };
                let release_id = upload.root_id;
                let album_title = upload.root_label;
                Ok(DbOutboxUpload {
                    album_title,
                    release_id,
                    blob: upload.blob,
                    phase: upload.phase,
                    provider_bytes_total: upload.provider_bytes_total,
                    attempt_count: upload.attempt_count,
                    last_error: upload.last_error,
                    created_at: stamp_millis(&upload.created_at)?,
                    label,
                })
            })
            .collect::<Result<Vec<_>, DbError>>()?;

        let make_remotes = make_remotes
            .into_iter()
            .map(|transition| {
                let album_title = transition.root_label.clone();
                Ok(DbMakeRemote {
                    transition,
                    album_title,
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

        Ok(DbOutboxQueue {
            uploads,
            deletes,
            make_remotes,
        })
    }
}

/// Original filenames for queued `release_files` rows. The queue already
/// declares each upload's table, so image rows never enter this lookup.
fn outbox_release_file_names_on(
    sql: &SqlReadContext<'_>,
    file_ids: &[String],
) -> Result<HashMap<String, String>, DbError> {
    let mut map = HashMap::new();
    for chunk in file_ids.chunks(SQL_MAX_IN_VARS) {
        let placeholders = in_clause_placeholders(chunk.len());
        let query = format!(
            "SELECT rf.id, rf.original_filename \
             FROM release_files rf \
             WHERE rf.id IN ({placeholders})"
        );
        let rows = sql.query(
            &query,
            coven::rusqlite::params_from_iter(chunk.iter()),
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?;
        map.extend(rows);
    }
    Ok(map)
}

impl Database {
    /// The album title `release_id` belongs under, read at the moment work is
    /// queued for it.
    ///
    /// This is what the queue snapshots onto its own rows. It is read here,
    /// where the release is certainly present — queueing work for it is what
    /// the caller is doing — rather than when the queue is rendered, which is
    /// exactly when the row may be gone.
    pub(crate) async fn release_album_title(&self, release_id: &str) -> Result<String, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| {
            let titles = outbox_release_titles_on(&sql, std::slice::from_ref(&release_id))?;
            album_title_of(&titles, &release_id, "queueing an upload for")
        })
        .await
    }
}

/// The album title one outbox entry renders under, or which row is missing.
///
/// Both absences are the same defect class — durable queue work outliving the
/// release it was queued for — and both stay loud rather than rendering the
/// entry with a placeholder: the queue naming a row that is gone is a state the
/// writers must make impossible, and a reader that quietly papers over it is
/// how it would go unnoticed. What each says is different, though, so each says
/// it: one names a release row, the other the album row a live release points at.
fn album_title_of(
    titles: &HashMap<String, ReleaseAlbumTitle>,
    release_id: &str,
    entry: &str,
) -> Result<String, DbError> {
    match titles.get(release_id) {
        Some(ReleaseAlbumTitle::Known(title)) => Ok(title.clone()),
        Some(ReleaseAlbumTitle::AlbumMissing { album_id }) => Err(DbError::Message(format!(
            "{entry} release {release_id} names album {album_id}, which has no row"
        ))),
        None => Err(DbError::Message(format!(
            "{entry} names release {release_id}, which has no row"
        ))),
    }
}

/// Album titles for the release roots coven groups uploads under. The title
/// belongs to the root, not to whichever audio/image row happens to be first.
///
/// The join is outer and the title is read as nullable, which is the whole
/// point: a release whose album row is gone comes back as a row with no title
/// rather than as no row, so the caller can tell that apart from a release that
/// is gone itself. Reading the outer join's title as NOT NULL — which this did
/// — turns the first case into a column-type error naming neither.
fn outbox_release_titles_on(
    sql: &SqlReadContext<'_>,
    release_ids: &[String],
) -> Result<HashMap<String, ReleaseAlbumTitle>, DbError> {
    let mut map = HashMap::new();
    for chunk in release_ids.chunks(SQL_MAX_IN_VARS) {
        let placeholders = in_clause_placeholders(chunk.len());
        let query = format!(
            "SELECT r.id, r.album_id, a.title \
             FROM releases r \
             LEFT JOIN albums a ON a.id = r.album_id \
             WHERE r.id IN ({placeholders})"
        );
        let rows = sql.query(
            &query,
            coven::rusqlite::params_from_iter(chunk.iter()),
            |row| {
                let release_id: String = row.get(0)?;
                let album_id: String = row.get(1)?;
                let title: Option<String> = row.get(2)?;
                Ok((
                    release_id,
                    match title {
                        Some(title) => ReleaseAlbumTitle::Known(title),
                        None => ReleaseAlbumTitle::AlbumMissing { album_id },
                    },
                ))
            },
        )?;
        map.extend(rows);
    }
    Ok(map)
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
pub(super) fn image_versions_on(
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

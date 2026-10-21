use super::*;

/// The bae rows needed to label one exact coven outbox snapshot.
///
/// This is an absolute live-query request: when coven changes the durable
/// queue, the subscription is reconfigured to follow precisely the
/// release-file rows named by that snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OutboxDisplayRequest {
    release_file_ids: Vec<String>,
}

/// Display values read reactively for an [`OutboxDisplayRequest`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OutboxDisplayContext {
    pub(crate) file_names: HashMap<String, (String, String)>,
}

impl Database {
    /// Replace an image's bytes and row atomically; coven manages the old file.
    pub async fn write_library_image(
        &self,
        image: &DbLibraryImage,
        bytes: &[u8],
    ) -> Result<(), DbError> {
        self.write_library_image_after_planning(image, bytes, || async {})
            .await
    }

    #[cfg(test)]
    pub(crate) async fn write_library_image_after_planning_for_test<F, Fut>(
        &self,
        image: &DbLibraryImage,
        bytes: &[u8],
        after_planning: F,
    ) -> Result<(), DbError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        self.write_library_image_after_planning(image, bytes, after_planning)
            .await
    }

    async fn write_library_image_after_planning<F, Fut>(
        &self,
        image: &DbLibraryImage,
        bytes: &[u8],
        after_planning: F,
    ) -> Result<(), DbError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let image = image.clone();
        let namespace = image.image_type.namespace();
        let planned = self
            .find_image_version(&image.image_type, &image.id)
            .await?;
        after_planning().await;
        // coven attaches the bytes to the row whose file id column — the row's
        // own `id` — names them.
        let file_id = image.id.clone();
        let bytes = bytes.to_vec();
        self.write_with_files(
            move |w| {
                w.put_file(namespace, file_id, bytes);
                Ok(())
            },
            move |sql| {
                // Refuse a replacement when another edit changed this row.
                let current = image_version_on(&sql, &image.image_type, &image.id)
                    .map_err(CovenError::from)?;
                if current != planned {
                    return Err(CovenError::from(DbError::Message(format!(
                        "{} image for {} changed after planning",
                        namespace, image.id
                    ))));
                }
                upsert_library_image_row(&sql, &image).map_err(CovenError::from)?;
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

    /// The version of each given release's cover (its file's content hash); ids
    /// with no cover row are absent from the map. This is the version a cover
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

    /// The version of each given artist's `artist_images` row, for the ids
    /// that have one. Ids with no artist image row are absent from the map.
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

    /// The version of one release's cover, or `None` when it has no cover.
    pub async fn cover_version(&self, release_id: &str) -> Result<Option<String>, DbError> {
        self.find_image_version(&LibraryImageType::Cover, release_id)
            .await
    }

    async fn find_image_version(
        &self,
        image_type: &LibraryImageType,
        id: &str,
    ) -> Result<Option<String>, DbError> {
        let (image_type, id) = (image_type.clone(), id.to_string());
        self.read(move |sql| image_version_on(&sql, &image_type, &id))
            .await
    }

    /// The original this device has for a file. Unattached rows read bae's local
    /// paths; attached rows read coven's original-file registration.
    pub async fn external_file(&self, file_id: &str) -> Result<Option<coven::UserFile>, DbError> {
        let id = file_id.to_string();
        let local = self
            .read(move |sql| {
                sql.query_row(
                    "SELECT f.hash IS NOT NULL, l.path, l.modified_at, f.file_size \
                 FROM release_files f LEFT JOIN local_release_files l ON l.file_id = f.id \
                 WHERE f.id = ?",
                    [id],
                    |row| {
                        Ok((
                            row.get::<_, bool>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(DbError::from)
            })
            .await?;
        match local {
            None => Ok(None),
            Some((true, _, _, _)) => self
                .inner
                .handle
                .user_file("release_files", file_id)
                .await
                .map_err(DbError::from),
            Some((false, Some(path), Some(modified_at), size)) => {
                let modified_at =
                    chrono::DateTime::parse_from_rfc3339(&modified_at).map_err(|error| {
                        DbError::Message(format!("invalid original modification time: {error}"))
                    })?;
                Ok(Some(coven::UserFile {
                    path: path.into(),
                    size: u64::try_from(size)
                        .map_err(|error| DbError::Message(error.to_string()))?,
                    modified_at: modified_at.into(),
                }))
            }
            Some((false, None, None, _)) => Ok(None),
            Some(_) => Err(DbError::Message(format!(
                "incomplete original for {file_id}"
            ))),
        }
    }

    /// Whether an attached file is still waiting for upload.
    pub async fn has_pending_cloud_upload(&self, file_id: &str) -> Result<bool, DbError> {
        Ok(self
            .inner
            .handle
            .uploads()
            .await?
            .files
            .iter()
            .any(|upload| {
                upload.file.table() == "release_files"
                    && upload.file.key() == &coven::RowKey::from(file_id)
            }))
    }

    /// The releases whose own files are queued, in upload queue order. Image
    /// uploads, a release's cover among them, remain visible in the queue but
    /// are no release uploading.
    pub async fn uploading_releases(&self) -> Result<Vec<String>, DbError> {
        let queue = self.outbox_queue().await?;
        let mut seen = HashSet::new();
        Ok(queue
            .uploads
            .into_iter()
            .filter_map(|entry| entry.subject.uploading_release().map(str::to_string))
            .filter(|id| seen.insert(id.clone()))
            .collect())
    }

    /// What coven's durable cloud queue is holding, joined to the bae context the
    /// Storage Manager renders it with. Backs the processing snapshot.
    ///
    /// Coven reports every queued upload, surviving restarts, and bae reads
    /// display context from each upload's declared file table. An upload outliving
    /// its deleted row remains visible without the row's former label.
    pub async fn outbox_queue(&self) -> Result<DbOutboxQueue, DbError> {
        let (snapshot, _, names) = self.outbox_queue_parts().await?;
        Self::outbox_queue_from_context(snapshot, names)
    }

    /// coven's durable queue as read now, the display request it needs, and
    /// the display names that request reads — the parts
    /// `outbox_queue_from_context` joins.
    pub(crate) async fn outbox_queue_parts(
        &self,
    ) -> Result<
        (
            coven::UploadQueue,
            OutboxDisplayRequest,
            OutboxDisplayContext,
        ),
        DbError,
    > {
        let snapshot = self.inner.handle.uploads().await?;
        let request = Self::outbox_display_request(&snapshot)?;
        let names = self.outbox_display_context(request.clone()).await?;
        Ok((snapshot, request, names))
    }

    /// Subscribe to coven's durable uploads as one committed stream.
    /// Each value is labelled by the display rows
    /// `subscribe_outbox_display` follows and
    /// joined by `outbox_queue_from_context`.
    pub fn subscribe_cloud_outbox(&self) -> coven::LiveQuery<coven::UploadQueue> {
        self.inner.handle.subscribe_uploads()
    }

    /// Identify the bae rows that label an exact durable outbox snapshot.
    /// Invalid roots fail here rather than producing a partially labelled UI.
    pub(crate) fn outbox_display_request(
        snapshot: &coven::UploadQueue,
    ) -> Result<OutboxDisplayRequest, DbError> {
        let mut release_file_ids = snapshot
            .files
            .iter()
            .filter(|upload| upload.file.table() == "release_files")
            .map(|upload| file_row_id(&upload.file).map(str::to_string))
            .collect::<Result<Vec<_>, DbError>>()?;
        release_file_ids.sort();
        release_file_ids.dedup();

        Ok(OutboxDisplayRequest { release_file_ids })
    }

    /// Follow the bae display rows named by the current coven outbox. An
    /// original filename change produces a new value even while the durable
    /// outbox itself is unchanged.
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
        snapshot: coven::UploadQueue,
        context: OutboxDisplayContext,
    ) -> Result<DbOutboxQueue, DbError> {
        let mut uploads = Vec::with_capacity(snapshot.files.len());
        for upload in snapshot.files {
            let id = file_row_id(&upload.file)?;
            use crate::library::UploadSubject;
            let subject = match upload.file.table() {
                "release_files" => match context.file_names.get(id) {
                    Some((release_id, name)) => UploadSubject::ReleaseFile {
                        release_id: release_id.clone(),
                        name: name.clone(),
                    },
                    None => UploadSubject::DeletedReleaseFile,
                },
                "covers" => UploadSubject::Cover {
                    release_id: id.to_string(),
                },
                "artist_images" => UploadSubject::ArtistImage,
                "people" => UploadSubject::Avatar,
                table => {
                    return Err(DbError::Message(format!(
                        "upload names undeclared file table {table:?}"
                    )))
                }
            };
            uploads.push(DbOutboxUpload { upload, subject });
        }
        Ok(DbOutboxQueue {
            paused: snapshot.paused,
            uploads,
        })
    }
}

/// Original filenames for queued `release_files` rows. The queue already
/// declares each upload's table, so image rows never enter this lookup.
fn outbox_release_file_names_on(
    sql: &SqlReadContext<'_>,
    file_ids: &[String],
) -> Result<HashMap<String, (String, String)>, DbError> {
    let mut map = HashMap::new();
    for chunk in file_ids.chunks(SQL_MAX_IN_VARS) {
        let query = format!(
            "SELECT id, release_id, original_filename FROM release_files WHERE id IN ({})",
            in_clause_placeholders(chunk.len())
        );
        map.extend(
            sql.query(&query, rusqlite::params_from_iter(chunk.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    (row.get::<_, String>(1)?, row.get::<_, String>(2)?),
                ))
            })?,
        );
    }
    Ok(map)
}

pub(super) fn file_row_id(file: &coven::FileRef) -> Result<&str, DbError> {
    match file.key().values() {
        [rusqlite::types::Value::Text(id)] => Ok(id),
        key => Err(DbError::Message(format!(
            "file {} has unexpected key {key:?}",
            file.table()
        ))),
    }
}

/// A library image's version: the content hash coven records when it attaches
/// the row's file. coven fills `hash` on every attachment and never rewrites
/// stored bytes, so the version moves exactly when the image's bytes do. Every
/// image row carries a file — each write pairs the row with its bytes — so a
/// row without a hash fails the read.
pub(super) fn image_version(hash: &[u8]) -> String {
    hex::encode(hash)
}

/// The version of one image row, or `None` when there is no such row.
pub(super) fn image_version_on<Q: QueryOne>(
    sql: &Q,
    image_type: &LibraryImageType,
    id: &str,
) -> Result<Option<String>, DbError> {
    let table = image_table(image_type);
    Ok(sql
        .query_row(
            &format!("SELECT hash FROM {table} WHERE id = ?"),
            params![id],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?
        .map(|hash| image_version(&hash)))
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
        let query = format!("SELECT id, hash FROM {table} WHERE id IN ({placeholders})");
        map.extend(
            sql.query(&query, rusqlite::params_from_iter(chunk.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    image_version(&row.get::<_, Vec<u8>>(1)?),
                ))
            })?,
        );
    }
    Ok(map)
}

use super::*;

impl Database {
    /// Replace an image's bytes and row atomically; coven manages the old file.
    /// Replacing an image this device is still uploading abandons that upload;
    /// the person asked for that, so its record is dismissed.
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
        let (image_type, id) = (image.image_type.clone(), image.id.clone());
        let planned = self
            .read(move |sql| image_revision_on(&sql, &image_type, &id))
            .await?;
        after_planning().await;
        let abandoned = self
            .own_abandonments(&[(namespace, image.id.clone())])
            .await?;
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
                // Refuse a replacement when another write of the image landed
                // since planning. The image's upload completing changes none
                // of this.
                let current = image_revision_on(&sql, &image.image_type, &image.id)
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
        .map_err(Self::coven_error)?;
        self.dismiss_own_abandonments(&abandoned).await;
        Ok(())
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

    /// The original this device holds for a release file: the person's file it
    /// was imported from, or the copy a detach made, as `local_release_files`
    /// records it. `None` when this device holds none, as for a release
    /// another device imported.
    pub async fn external_file(&self, file_id: &str) -> Result<Option<coven::UserFile>, DbError> {
        let id = file_id.to_string();
        let local = self
            .read(move |sql| {
                sql.query_row(
                    "SELECT l.path, l.modified_at, f.file_size \
                     FROM release_files f JOIN local_release_files l ON l.file_id = f.id \
                     WHERE f.id = ?",
                    [id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(DbError::from)
            })
            .await?;
        let Some((path, modified_at, size)) = local else {
            return Ok(None);
        };
        let modified_at = chrono::DateTime::parse_from_rfc3339(&modified_at).map_err(|error| {
            DbError::Message(format!("invalid original modification time: {error}"))
        })?;
        Ok(Some(coven::UserFile {
            path: path.into(),
            size: u64::try_from(size).map_err(|error| DbError::Message(error.to_string()))?,
            modified_at: modified_at.into(),
        }))
    }

    /// The file a row carries as this device sees it: this device's pending
    /// attachment while its upload runs, otherwise the stored file the row
    /// names. `None` when the row names no file here.
    pub(crate) async fn current_file(
        &self,
        table: &str,
        row_id: &str,
    ) -> Result<Option<coven::FileRef>, DbError> {
        match self.file_ref(table, row_id).await {
            Ok(file) => Ok(Some(file)),
            Err(coven::DbError::FileAbsent { .. }) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// The releases whose own files are queued, in upload queue order
    /// (`UPLOADING_RELEASES`). Image uploads, a release's cover among them,
    /// remain visible in the queue but are no release uploading.
    pub async fn uploading_releases(&self) -> Result<Vec<String>, DbError> {
        self.read(|sql| {
            sql.query(
                &format!("SELECT release_id FROM ({UPLOADING_RELEASES}) ORDER BY position"),
                [],
                |row| row.get::<_, String>(0),
            )
            .map_err(DbError::from)
        })
        .await
    }
}

/// A library image's version: the revision of the write that last set its
/// bytes, then the content hash of the stored file its row names once there is
/// one. The bytes a device reads for an image change only with one of them:
/// a write changes the revision on every device as it arrives, while the row
/// names the new file only once its upload completes (coven §17.2), which
/// changes the hash. So the version moves whenever any device's bytes for the
/// image can, though not only then.
pub(super) fn image_version(revision: &str, hash: Option<&[u8]>) -> String {
    match hash {
        Some(hash) => format!("{revision}.{}", hex::encode(hash)),
        None => revision.to_string(),
    }
}

/// [`image_version`] of the image row `alias` names, as a SQL expression.
fn image_version_sql(alias: &str) -> String {
    format!(
        "(CASE WHEN {alias}.hash IS NULL THEN {alias}.revision \
          ELSE {alias}.revision || '.' || lower(hex({alias}.hash)) END)"
    )
}

/// The version of the cover of the release `release_id` (a SQL expression),
/// found by the covers' key; NULL when the release has no cover.
pub(super) fn cover_version_sql(release_id: &str) -> String {
    format!(
        "(SELECT {version} FROM covers AS cover WHERE cover.id = {release_id})",
        version = image_version_sql("cover")
    )
}

/// The version of the image of the artist `artist_id` (a SQL expression),
/// found by the artist images' key; NULL when the artist has no image.
pub(super) fn artist_image_version_sql(artist_id: &str) -> String {
    format!(
        "(SELECT {version} FROM artist_images AS artist_image \
          WHERE artist_image.id = {artist_id})",
        version = image_version_sql("artist_image")
    )
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
            &format!("SELECT revision, hash FROM {table} WHERE id = ?"),
            params![id],
            |row| {
                Ok(image_version(
                    &row.get::<_, String>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?.as_deref(),
                ))
            },
        )
        .optional()?)
}

/// The revision of the write that last set one image's bytes, or `None` when
/// there is no such row.
fn image_revision_on<Q: QueryOne>(
    sql: &Q,
    image_type: &LibraryImageType,
    id: &str,
) -> Result<Option<String>, DbError> {
    let table = image_table(image_type);
    Ok(sql
        .query_row(
            &format!("SELECT revision FROM {table} WHERE id = ?"),
            params![id],
            |row| row.get::<_, String>(0),
        )
        .optional()?)
}

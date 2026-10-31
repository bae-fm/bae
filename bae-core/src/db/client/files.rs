use super::*;

impl Database {
    /// Replace an image's bytes and row atomically: the write stages the bytes
    /// as a new coven file and points the row at it, and the file the row named
    /// before goes with its last reference.
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
            .read(move |sql| image_file_on(&sql, &image_type, &id))
            .await?;
        after_planning().await;
        let bytes = bytes.to_vec();
        self.write_with_files(
            move |w| Ok(w.put_file(namespace, bytes, coven::Place::Cloud)),
            move |sql, file| {
                // Refuse a replacement when another write of the image landed
                // since planning: the row names another file than it did.
                let current =
                    image_file_on(&sql, &image.image_type, &image.id).map_err(CovenError::from)?;
                if current != planned {
                    return Err(CovenError::from(DbError::Message(format!(
                        "{} image for {} changed after planning",
                        namespace, image.id
                    ))));
                }
                upsert_library_image_row(&sql, &image, &file).map_err(CovenError::from)?;
                Ok(())
            },
        )
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

    /// The original coven records on this device for a release file's coven
    /// file: the person's file it was imported from, or the copy making the
    /// release local again wrote. `None` when there is no such row or this
    /// device records no original for it, as for a release another device
    /// imported.
    pub async fn external_file(&self, file_id: &str) -> Result<Option<coven::UserFile>, DbError> {
        let Some(row) = self.find_file_by_id(file_id).await? else {
            return Ok(None);
        };
        Ok(self.inner.handle.user_file(row.file).await?)
    }

    /// coven's record of the file `file`, as this device reads it now.
    pub(crate) async fn coven_file(&self, file: coven::FileId) -> Result<coven::FileRef, DbError> {
        Ok(self.file_ref(file).await?)
    }

    /// The coven file a library image's row names, as this device reads it
    /// now; `None` when there is no such row.
    pub(crate) async fn library_image_file(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<coven::FileRef>, DbError> {
        let (image_type, id) = (image_type.clone(), id.to_string());
        let Some(file) = self
            .read(move |sql| image_file_on(&sql, &image_type, &id))
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(self.coven_file(file).await?))
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

/// A library image's version: the coven file its row names. A new image is a
/// new file, and the image's references are bound, so every device receives
/// the row naming a new file together with the stored file, and the version
/// moves with that write.
///
/// This is that version of the image row `alias` names, as a SQL expression.
pub(super) fn image_version_sql(alias: &str) -> String {
    format!("{alias}.file")
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
            &format!(
                "SELECT {version} FROM {table} AS image WHERE image.id = ?",
                version = image_version_sql("image")
            ),
            params![id],
            |row| row.get::<_, String>(0),
        )
        .optional()?)
}

/// The coven file one image row names, or `None` when there is no such row.
pub(super) fn image_file_on<Q: QueryOne>(
    sql: &Q,
    image_type: &LibraryImageType,
    id: &str,
) -> Result<Option<coven::FileId>, DbError> {
    let table = image_table(image_type);
    Ok(sql
        .query_row(
            &format!("SELECT file FROM {table} WHERE id = ?"),
            params![id],
            |row| row.get::<_, coven::FileId>(0),
        )
        .optional()?)
}

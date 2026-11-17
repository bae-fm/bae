use super::*;
use bae_store::file_version_sql;

/// Why an image's replacement was refused. Nothing of it landed.
#[derive(Debug, thiserror::Error)]
pub enum ImageWriteRefusal {
    /// Another write of the image landed after the replacement was planned:
    /// its row names another file than the planner saw.
    #[error("the {image_type:?} image of {subject_id} changed after planning")]
    Superseded { image_type: LibraryImageType, subject_id: String },
}

/// Why an image's replacement did not commit.
pub type ImageWriteError = WriteError<ImageWriteRefusal>;

impl Records {
    /// Replace an image's bytes and row atomically: the write stages the bytes
    /// as a new coven file and points the row at it, and the file the row named
    /// before goes with its last reference.
    pub async fn write_library_image(
        &self,
        image: &DbLibraryImage,
        bytes: &[u8],
    ) -> Result<(), ImageWriteError> {
        self.write_library_image_after_planning(image, bytes, || async {}).await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn write_library_image_after_planning_for_test<F, Fut>(
        &self,
        image: &DbLibraryImage,
        bytes: &[u8],
        after_planning: F,
    ) -> Result<(), ImageWriteError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        self.write_library_image_after_planning(image, bytes, after_planning).await
    }

    async fn write_library_image_after_planning<F, Fut>(
        &self,
        image: &DbLibraryImage,
        bytes: &[u8],
        after_planning: F,
    ) -> Result<(), ImageWriteError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let image = image.clone();
        let namespace = image.image_type.namespace();
        let (image_type, id) = (image.image_type.clone(), image.id.clone());
        let planned = self.store.read(move |sql| image_file_on(&sql, &image_type, &id)).await?;
        after_planning().await;
        let bytes = bytes.to_vec();
        self.store
            .write_staged(
                move |w| Ok(w.put_file(namespace, bytes, coven::Place::Cloud)),
                move |sql, file| {
                    // Refuse a replacement when another write of the image landed
                    // since planning: the row names another file than it did.
                    let current = image_file_on(&sql, &image.image_type, &image.id)?;
                    if current != planned {
                        return Err(WriteError::Refused(ImageWriteRefusal::Superseded {
                            image_type: image.image_type.clone(),
                            subject_id: image.id.clone(),
                        }));
                    }
                    upsert_library_image_row(&sql, &image, &file)?;
                    Ok(())
                },
            )
            .await
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
        self.store
            .read(move |sql| {
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
        self.find_image_version(&LibraryImageType::Cover, release_id).await
    }

    async fn find_image_version(
        &self,
        image_type: &LibraryImageType,
        id: &str,
    ) -> Result<Option<String>, DbError> {
        let (image_type, id) = (image_type.clone(), id.to_string());
        self.store.read(move |sql| image_version_on(&sql, &image_type, &id)).await
    }

    /// The original coven records on this device for a release file's coven
    /// file: the person's file it was imported from, or the copy making the
    /// release local again wrote. `None` when there is no such row or this
    /// device records no original for it, as for a release another device
    /// imported.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn external_file(&self, file_id: &str) -> Result<Option<coven::UserFile>, DbError> {
        let Some(row) = self.find_file_by_id(file_id).await? else {
            return Ok(None);
        };
        self.store.user_file(row.file).await
    }

    /// The coven file a library image's row names, as this device reads it
    /// now; `None` when there is no such row.
    pub async fn library_image_file(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<coven::FileRef>, DbError> {
        let (image_type, id) = (image_type.clone(), id.to_string());
        let Some(file) = self.store.read(move |sql| image_file_on(&sql, &image_type, &id)).await?
        else {
            return Ok(None);
        };
        Ok(Some(self.store.file_ref(file).await?))
    }

    /// The releases whose own files are queued, in upload queue order
    /// (`UPLOADING_RELEASES`). Image uploads, a release's cover among them,
    /// remain visible in the queue but are no release uploading.
    pub async fn uploading_releases(&self) -> Result<Vec<String>, DbError> {
        self.store
            .read(|sql| {
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

/// The version of the cover of the release `release_id` (a SQL expression),
/// found by the covers' key; NULL when the release has no cover.
pub fn cover_version_sql(release_id: &str) -> String {
    format!(
        "(SELECT {version} FROM {table} AS cover WHERE cover.id = {release_id})",
        table = image_table(&LibraryImageType::Cover),
        version = file_version_sql("cover.file")
    )
}

/// The version of the image of the artist `artist_id` (a SQL expression),
/// found by the artist images' key; NULL when the artist has no image.
pub fn artist_image_version_sql(artist_id: &str) -> String {
    format!(
        "(SELECT {version} FROM {table} AS artist_image \
          WHERE artist_image.id = {artist_id})",
        table = image_table(&LibraryImageType::Artist),
        version = file_version_sql("artist_image.file")
    )
}

/// The version of one image row, or `None` when there is no such row.
pub(super) fn image_version_on<Q: coven::SqlRead>(
    sql: &Q,
    image_type: &LibraryImageType,
    id: &str,
) -> Result<Option<String>, DbError> {
    let table = image_table(image_type);
    Ok(sql
        .query_row(
            &format!(
                "SELECT {version} FROM {table} AS image WHERE image.id = ?",
                version = file_version_sql("image.file")
            ),
            params![id],
            |row| row.get::<_, String>(0),
        )
        .optional()?)
}

/// The coven file one image row names, or `None` when there is no such row.
pub fn image_file_on<Q: coven::SqlRead>(
    sql: &Q,
    image_type: &LibraryImageType,
    id: &str,
) -> Result<Option<coven::FileId>, DbError> {
    let table = image_table(image_type);
    Ok(sql
        .query_row(&format!("SELECT file FROM {table} WHERE id = ?"), params![id], |row| {
            row.get::<_, coven::FileId>(0)
        })
        .optional()?)
}

/// Whether this device keeps each of the release `?1`'s files whole, as
/// `coven_pins` says.
const RELEASE_FILES_KEPT: &str = "SELECT f.id, pin.file IS NOT NULL \
     FROM release_files AS f \
     LEFT JOIN coven_pins AS pin ON pin.file = f.file \
     WHERE f.release_id = ?1";

/// [`RELEASE_FILES_KEPT`] for `release_id`, by release file id.
fn release_files_kept_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<std::collections::BTreeMap<String, bool>, DbError> {
    Ok(sql
        .query(RELEASE_FILES_KEPT, [release_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
        })?
        .into_iter()
        .collect())
}

impl Records {
    /// Whether this device keeps each of the release's files whole now, by
    /// release file id.
    pub async fn release_files_kept(
        &self,
        release_id: &str,
    ) -> Result<std::collections::BTreeMap<String, bool>, DbError> {
        let release_id = release_id.to_string();
        self.store.read(move |sql| release_files_kept_on(&sql, &release_id)).await
    }

    /// Whether this device keeps each of the release's files whole, live: it
    /// reruns as a file is pinned or unpinned.
    pub fn subscribe_release_files_kept(
        &self,
        release_id: &str,
    ) -> coven::LiveQuery<std::collections::BTreeMap<String, bool>, DbError> {
        let release_id = release_id.to_string();
        self.store.subscribe(move |sql| release_files_kept_on(&sql, &release_id))
    }
}

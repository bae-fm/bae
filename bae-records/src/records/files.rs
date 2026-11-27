use super::*;

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
    /// Replace an artist's image bytes and row atomically: the write stages
    /// the bytes as a new coven file and points the row at it, and the file
    /// the row named before goes with its last reference.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn write_artist_image(
        &self,
        image: &DbArtistImage,
        bytes: &[u8],
    ) -> Result<(), ImageWriteError> {
        let image = image.clone();
        let id = image.artist_id.clone();
        let planned =
            self.store.read(move |sql| image_file_on(&sql, &LibraryImageType::Artist, &id)).await?;
        let bytes = bytes.to_vec();
        self.store
            .write_staged(
                move |w| {
                    Ok(w.put_file(
                        crate::synced::ARTIST_IMAGES_NAMESPACE,
                        bytes,
                        coven::Place::Cloud,
                    ))
                },
                move |sql, file| {
                    // Refuse a replacement when another write of the image landed
                    // since planning: the row names another file than it did.
                    let current = image_file_on(&sql, &LibraryImageType::Artist, &image.artist_id)?;
                    if current != planned {
                        return Err(WriteError::Refused(ImageWriteRefusal::Superseded {
                            image_type: LibraryImageType::Artist,
                            subject_id: image.artist_id.clone(),
                        }));
                    }
                    upsert_artist_image_row(&sql, &image, &file)?;
                    Ok(())
                },
            )
            .await
    }

    /// Replace a release's cover atomically: the write stages the copies (and
    /// the original, when the cover keeps one) as new coven files and points
    /// the release's row at them, and the files the row named before go with
    /// their last reference.
    pub async fn write_cover(&self, cover: NewCover) -> Result<(), ImageWriteError> {
        self.write_cover_after_planning(cover, || async {}).await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn write_cover_after_planning_for_test<F, Fut>(
        &self,
        cover: NewCover,
        after_planning: F,
    ) -> Result<(), ImageWriteError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        self.write_cover_after_planning(cover, after_planning).await
    }

    async fn write_cover_after_planning<F, Fut>(
        &self,
        cover: NewCover,
        after_planning: F,
    ) -> Result<(), ImageWriteError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let release_id = cover.release_id.clone();
        let planned = self
            .store
            .read(move |sql| image_file_on(&sql, &LibraryImageType::Cover, &release_id))
            .await?;
        after_planning().await;
        self.store
            .write_staged(
                {
                    let cover = cover.clone();
                    move |w| Ok(stage_cover(w, &cover))
                },
                move |sql, staged| {
                    // Refuse a replacement when another write of the cover landed
                    // since planning: the row names another file than it did.
                    let current = image_file_on(&sql, &LibraryImageType::Cover, &cover.release_id)?;
                    if current != planned {
                        return Err(WriteError::Refused(ImageWriteRefusal::Superseded {
                            image_type: LibraryImageType::Cover,
                            subject_id: cover.release_id.clone(),
                        }));
                    }
                    upsert_cover_row(&sql, &cover, &staged)?;
                    Ok(())
                },
            )
            .await
    }

    /// The artist's image row; `None` when the artist has none.
    pub async fn find_artist_image(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbArtistImage>, DbError> {
        let artist_id = artist_id.to_string();
        self.store
            .read(move |sql| {
                sql.query_row(
                    "SELECT id, content_type FROM artist_images WHERE id = ?",
                    params![artist_id],
                    |row| {
                        Ok(DbArtistImage {
                            artist_id: row.get("id")?,
                            content_type: ContentType::from_mime(
                                &row.get::<_, String>("content_type")?,
                            ),
                        })
                    },
                )
                .optional()
                .map_err(DbError::from)
            })
            .await
    }

    /// The format of the release's cover copies; `None` when it has no cover.
    pub async fn cover_content_type(
        &self,
        release_id: &str,
    ) -> Result<Option<ContentType>, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| {
                sql.query_row(
                    "SELECT content_type FROM covers WHERE id = ?",
                    params![release_id],
                    |row| Ok(ContentType::from_mime(&row.get::<_, String>(0)?)),
                )
                .optional()
                .map_err(DbError::from)
            })
            .await
    }

    /// What the release's cover is made from and the coven file that holds
    /// it, read in one statement; `None` when it has no cover.
    pub async fn cover_picture(&self, release_id: &str) -> Result<Option<CoverPicture>, DbError> {
        let release_id = release_id.to_string();
        let Some((source, file)) = self
            .store
            .read(move |sql| {
                Ok(sql
                    .query_row(
                        "SELECT c.source_file, c.embedded_file, \
                                COALESCE(image.file, audio.file, c.original_file) \
                         FROM covers AS c \
                         LEFT JOIN release_files AS image ON image.id = c.source_file \
                         LEFT JOIN release_files AS audio ON audio.id = c.embedded_file \
                         WHERE c.id = ?",
                        params![release_id],
                        |row| {
                            Ok((
                                CoverSource::of_columns(row.get(0)?, row.get(1)?),
                                row.get::<_, Option<coven::FileId>>(2)?,
                            ))
                        },
                    )
                    .optional()?)
            })
            .await?
        else {
            return Ok(None);
        };
        let file = file.ok_or_else(|| DbError::unreadable("a cover's file", &source))?;
        Ok(Some(CoverPicture { source, file: self.store.file_ref(file).await? }))
    }

    /// What the release's cover is made from; `None` when it has no cover.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn cover_source_for_test(
        &self,
        release_id: &str,
    ) -> Result<Option<CoverSource>, DbError> {
        Ok(self.cover_picture(release_id).await?.map(|picture| picture.source))
    }

    /// The coven file of the release's cover copy `copy`, as this device
    /// reads it now; `None` when the release has no cover.
    pub async fn cover_copy_file(
        &self,
        release_id: &str,
        copy: CoverCopy,
    ) -> Result<Option<coven::FileRef>, DbError> {
        let column = match copy {
            CoverCopy::Browse => "browse_file",
            CoverCopy::Large => "large_file",
        };
        self.cover_file_in(release_id, column).await
    }

    /// The coven file of the release's cover original, as this device reads
    /// it now; `None` when the release has no cover or the cover is made from
    /// one of the release's files.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn cover_original_file_for_test(
        &self,
        release_id: &str,
    ) -> Result<Option<coven::FileRef>, DbError> {
        self.cover_file_in(release_id, "original_file").await
    }

    /// The coven file of the preview of the release file `file_id`, as this
    /// device reads it now; `None` when the file has no preview.
    pub async fn release_preview_file(
        &self,
        file_id: &str,
    ) -> Result<Option<coven::FileRef>, DbError> {
        let file_id = file_id.to_string();
        let Some(file) = self
            .store
            .read(move |sql| {
                Ok(sql
                    .query_row(
                        "SELECT file FROM release_file_previews WHERE id = ?",
                        params![file_id],
                        |row| row.get::<_, coven::FileId>(0),
                    )
                    .optional()?)
            })
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(self.store.file_ref(file).await?))
    }

    async fn cover_file_in(
        &self,
        release_id: &str,
        column: &'static str,
    ) -> Result<Option<coven::FileRef>, DbError> {
        let release_id = release_id.to_string();
        let Some(file) = self
            .store
            .read(move |sql| {
                Ok(sql
                    .query_row(
                        &format!("SELECT {column} FROM covers WHERE id = ?"),
                        params![release_id],
                        |row| row.get::<_, Option<coven::FileId>>(0),
                    )
                    .optional()?
                    .flatten())
            })
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(self.store.file_ref(file).await?))
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

    /// The audio files and track sheets of the release `release_id`, in its
    /// file order, each with the path of its coven file's original on this
    /// device (the person's imported file, or the copy making the release
    /// local wrote), `None` where this device has none.
    pub async fn release_audio_and_sheet_originals(
        &self,
        release_id: &str,
    ) -> Result<Vec<(DbFile, Option<std::path::PathBuf>)>, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| {
                sql.query(AUDIO_AND_SHEET_ORIGINALS, params![release_id], |row| {
                    let file = super::read::row_to_file(row)?;
                    let path = row
                        .get::<_, Option<Vec<u8>>>("original_path")?
                        .map(coven::path_from_bytes)
                        .transpose()
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Blob,
                                Box::new(error),
                            )
                        })?;
                    Ok((file, path))
                })
                .map_err(DbError::from)
            })
            .await
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
    #[cfg(any(test, feature = "test-utils"))]
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

/// The audio and track sheet rows of the release `?1`, in its file order,
/// with the path of each one's original on this device (`coven_user_files`
/// lists only those), NULL where it has none.
const AUDIO_AND_SHEET_ORIGINALS: &str = concat!(
    "SELECT ",
    release_file_columns!(),
    ", original.path AS original_path \
     FROM release_files AS f \
     LEFT JOIN coven_user_files AS original ON original.file = f.file \
     WHERE f.release_id = ?1 AND f.role IN ('audio', 'track_sheet') \
     ORDER BY f.original_filename COLLATE natural_order, f.id"
);

// Each cover's version, by the release it is the cover of: the version of its
// browse copy.
sqlive_compose::fragment!(
    CoverVersion { version: Option<String> },
    r#"
    SELECT cover.id,
           (SELECT stored.version
            FROM @bae_store::file_version::StoredFileVersion AS stored
            WHERE stored.file = cover.browse_file) AS version
    FROM covers AS cover
"#
);

// Every cover's version, whose SQL a statement built as text holds.
sqlive_compose::read!(
    CoverVersions,
    r#"
    SELECT cover.*
    FROM @crate::records::files::CoverVersion AS cover
"#
);

// Each artist image's version, by the artist it is the image of.
sqlive_compose::fragment!(
    ArtistImageVersion { version: Option<String> },
    r#"
    SELECT image.id,
           (SELECT stored.version
            FROM @bae_store::file_version::StoredFileVersion AS stored
            WHERE stored.file = image.file) AS version
    FROM artist_images AS image
"#
);

// Every artist image's version, whose SQL a statement built as text holds.
sqlive_compose::read!(
    ArtistImageVersions,
    r#"
    SELECT image.*
    FROM @crate::records::files::ArtistImageVersion AS image
"#
);

/// The version of the cover of the release `release_id` (a SQL expression),
/// found by the covers' key; NULL when the release has no cover.
pub fn cover_version_sql(release_id: &str) -> String {
    image_version_sql(&LibraryImageType::Cover, release_id)
}

/// The version of the image of the artist `artist_id` (a SQL expression),
/// found by the artist images' key; NULL when the artist has no image.
pub fn artist_image_version_sql(artist_id: &str) -> String {
    image_version_sql(&LibraryImageType::Artist, artist_id)
}

/// The version of the image of `image_type` keyed by `id` (a SQL
/// expression), as a statement built as text holds it.
fn image_version_sql(image_type: &LibraryImageType, id: &str) -> String {
    format!("({})", image_version_select(image_type, id))
}

/// The select of the version of the image of `image_type` keyed by `id` (a SQL
/// expression), which returns no row when there is no such image.
fn image_version_select(image_type: &LibraryImageType, id: &str) -> String {
    let versions = match image_type {
        LibraryImageType::Cover => CoverVersions::SQL,
        LibraryImageType::Artist => ArtistImageVersions::SQL,
    };
    format!(
        "SELECT image_version.version FROM ({versions}) AS image_version \
         WHERE image_version.id = {id}"
    )
}

/// The version of one image row, or `None` when there is no such row.
pub(super) fn image_version_on<Q: coven::SqlRead>(
    sql: &Q,
    image_type: &LibraryImageType,
    id: &str,
) -> Result<Option<String>, DbError> {
    Ok(sql
        .query_row(&image_version_select(image_type, "?"), params![id], |row| {
            row.get::<_, String>(0)
        })
        .optional()?)
}

/// The coven file one image row names (a cover's browse copy), or `None` when there is no such row.
pub fn image_file_on<Q: coven::SqlRead>(
    sql: &Q,
    image_type: &LibraryImageType,
    id: &str,
) -> Result<Option<coven::FileId>, DbError> {
    let table = image_table(image_type);
    let column = image_file_column(image_type);
    Ok(sql
        .query_row(&format!("SELECT {column} FROM {table} WHERE id = ?"), params![id], |row| {
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

/// The SQL condition that a file whose content type is `content_type` (an SQL
/// expression) is an image bae can show: one of the formats a cover can be made
/// from. The gallery shows these images, and a release's Files list leaves them
/// out, so each image of a release is in exactly one of them.
pub fn showable_image_sql(content_type: &str) -> String {
    let types = bae_util::content_type::ContentType::SUPPORTED_COVERS
        .iter()
        .map(|content_type| format!("'{}'", content_type.as_str()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{content_type} IN ({types})")
}

/// The common table expression `unplayed_files (file_id)`: the files of
/// `files` that a release's Files list shows. `files` is a relation with the
/// columns `file_id` and `showable_image`, which is true for each file the
/// release's gallery shows as an image. `track_reads` is a relation with the
/// columns `audio_file_id` and `sheet_file_id`, either `NULL`: the audio file
/// each track's segment reads, and the CUE sheet that carves a track.
///
/// The list leaves out the files a track plays from (a file a track reads and
/// a sheet that carves a track) and the images the gallery shows, which the
/// gallery shows instead. Every other file is listed: a sheet that carves no
/// track, an audio file no track reads, and an image in a format bae cannot
/// show. The import pane supplies the scan's rows and the library its own, so
/// both list a release's files by this one rule.
pub fn unplayed_files_cte(files: &str, track_reads: &str) -> String {
    format!(
        "unplayed_files (file_id) AS ( \
             SELECT listed.file_id FROM {files} AS listed \
             WHERE NOT listed.showable_image \
               AND NOT EXISTS (SELECT 1 FROM {track_reads} AS read \
                               WHERE read.audio_file_id = listed.file_id \
                                  OR read.sheet_file_id = listed.file_id))"
    )
}

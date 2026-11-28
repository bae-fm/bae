use super::*;

/// Why an image's replacement was refused. Nothing of it landed.
#[derive(Debug, thiserror::Error)]
pub enum ImageWriteRefusal {
    /// Another write of the image landed after the replacement was planned:
    /// its row names another file than the planner saw.
    #[error("the {image_type:?} image of {subject_id} changed after planning")]
    Superseded { image_type: LibraryImageType, subject_id: String },
}

/// One stored picture of the library, a cover's copy or an artist's image, as
/// a reader of its bytes takes it.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryPicture {
    /// The format the picture's bytes are in.
    pub content_type: ContentType,
    /// The coven file with the bytes, as this device reads it now.
    pub file: coven::FileRef,
}

// The format of one release's cover copies and the coven files of both; no
// row for a release with no cover.
coven::read!(
    writer OneCoverCopies(release: String) {
        browse_file: coven::FileId,
        large_file: coven::FileId,
    },
    r#"
    SELECT cover.content_type, cover.browse_file, cover.large_file
    FROM covers AS cover
    WHERE cover.id = :release
"#
);

// The format and coven file of one artist's image; no row for an artist with
// no image.
coven::read!(
    writer OneArtistPicture(artist: String) {
        file: coven::FileId,
    },
    r#"
    SELECT image.content_type, image.file
    FROM artist_images AS image
    WHERE image.id = :artist
"#
);

// One release's cover's version; no row for a release with no cover.
coven::read!(
    writer OneCoverVersion(release: String),
    r#"
    SELECT cover.*
    FROM @crate::records::files::CoverVersion AS cover
    WHERE cover.id = :release
"#
);

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

    /// The artist's image, its format and bytes' file read as one statement;
    /// `None` when the artist has none.
    pub async fn artist_picture(&self, artist_id: &str) -> Result<Option<LibraryPicture>, DbError> {
        let statement = OneArtistPictureParams { artist: artist_id.to_owned() };
        let row =
            self.store.read(move |sql| bae_store::statement_optional(&sql, &statement)).await?;
        match row {
            Some(row) => self.picture(&row.content_type, row.file).await.map(Some),
            None => Ok(None),
        }
    }

    async fn picture(
        &self,
        content_type: &str,
        file: coven::FileId,
    ) -> Result<LibraryPicture, DbError> {
        Ok(LibraryPicture {
            content_type: ContentType::from_mime(content_type),
            file: self.store.file_ref(file).await?,
        })
    }

    /// The format of the release's cover copies; `None` when it has no cover.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn cover_content_type(
        &self,
        release_id: &str,
    ) -> Result<Option<ContentType>, DbError> {
        let copy = self.cover_copy(release_id, CoverCopy::Large).await?;
        Ok(copy.map(|copy| copy.content_type))
    }

    /// What the release's cover is made from and the coven file that holds
    /// it, read in one statement; `None` when it has no cover.
    pub async fn cover_picture(&self, release_id: &str) -> Result<Option<CoverPicture>, DbError> {
        let release_id = release_id.to_string();
        let statement = CoverPictureOfReleaseParams { release: release_id };
        let Some(row) =
            self.store.read(move |sql| bae_store::statement_optional(&sql, &statement)).await?
        else {
            return Ok(None);
        };
        let source = CoverSource::of_columns(row.source_file, row.embedded_file);
        let file = row.file;
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

    /// The release's cover copy `copy`, its format and bytes' file read as one
    /// statement; `None` when the release has no cover.
    pub async fn cover_copy(
        &self,
        release_id: &str,
        copy: CoverCopy,
    ) -> Result<Option<LibraryPicture>, DbError> {
        let statement = OneCoverCopiesParams { release: release_id.to_owned() };
        let row =
            self.store.read(move |sql| bae_store::statement_optional(&sql, &statement)).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let file = match copy {
            CoverCopy::Browse => row.browse_file,
            CoverCopy::Large => row.large_file,
        };
        self.picture(&row.content_type, file).await.map(Some)
    }

    /// The coven file of the release's cover original, as this device reads
    /// it now; `None` when the release has no cover or the cover is made from
    /// one of the release's files.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn cover_original_file_for_test(
        &self,
        release_id: &str,
    ) -> Result<Option<coven::FileRef>, DbError> {
        let release_id = release_id.to_string();
        let statement = CoverOriginalParams { release: release_id };
        let row =
            self.store.read(move |sql| bae_store::statement_optional(&sql, &statement)).await?;
        let Some(file) = row.and_then(|row| row.original_file) else {
            return Ok(None);
        };
        Ok(Some(self.store.file_ref(file).await?))
    }

    /// The coven file of the preview of the release file `file_id`, as this
    /// device reads it now; `None` when the file has no preview.
    pub async fn release_preview_file(
        &self,
        file_id: &str,
    ) -> Result<Option<coven::FileRef>, DbError> {
        let file_id = file_id.to_string();
        let statement = PreviewOfFileParams { file: file_id };
        let Some(row) =
            self.store.read(move |sql| bae_store::statement_optional(&sql, &statement)).await?
        else {
            return Ok(None);
        };
        let file = row.file;
        Ok(Some(self.store.file_ref(file).await?))
    }

    /// The version of one release's cover, or `None` when it has no cover.
    pub async fn cover_version(&self, release_id: &str) -> Result<Option<String>, DbError> {
        let statement = OneCoverVersionParams { release: release_id.to_owned() };
        let row =
            self.store.read(move |sql| bae_store::statement_optional(&sql, &statement)).await?;
        Ok(row.and_then(|row| row.cover.version))
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
        let statement = AudioAndSheetOriginalsParams { release: release_id.to_string() };
        self.store
            .read(move |sql| {
                bae_store::statement_rows(&sql, &statement)?
                    .into_iter()
                    .map(|row| {
                        let path =
                            row.original_path.map(coven::path_from_bytes).transpose().map_err(
                                |error| {
                                    DbError::unreadable("an original's path", error.to_string())
                                },
                            )?;
                        Ok((row.record.into_file()?, path))
                    })
                    .collect()
            })
            .await
    }

    /// The coven file a library image's row names, as this device reads it
    /// now; `None` when there is no such row.
    #[cfg(any(test, feature = "test-utils"))]
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

    /// The releases whose own files are queued, in upload queue order, as
    /// their storage rows place them. Image uploads, a release's cover among
    /// them, remain visible in the queue but are no release uploading.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn uploading_releases(&self) -> Result<Vec<String>, DbError> {
        let rows = self.store.read_rows(UploadingReleasesParams {}).await?;
        Ok(rows.into_iter().map(|row| row.release_id).collect())
    }
}

// The releases whose own files are queued, in upload queue order.
#[cfg(any(test, feature = "test-utils"))]
coven::read!(
    writer UploadingReleases,
    r#"
    SELECT queued.release_id
    FROM @crate::release_storage::QueuedReleases AS queued
    ORDER BY queued.first_queued_position
"#
);

// The audio and track sheet rows of the release `release`, in its file order,
// with the path of each one's original on this device (`coven_user_files`
// lists only those), NULL where it has none.
coven::read!(
    writer AudioAndSheetOriginals(release: String) { original_path: Option<Vec<u8>> },
    r#"
    SELECT record.*, original.path AS original_path
    FROM @crate::records::file_fragments::ReleaseFileRecord AS record
    LEFT JOIN coven_user_files AS original ON original.file = record.file
    WHERE record.release_id = :release AND record.role IN ('audio', 'track_sheet')
    ORDER BY record.original_filename COLLATE natural_order, record.id
"#
);

// Each cover's version, by the release it is the cover of: the version of its
// browse copy. `source_file` is the release file the cover is made from, NULL
// for a cover made from no file of the release.
coven::fragment!(
    CoverVersion { version: Option<String> },
    r#"
    SELECT cover.id, cover.source_file,
           (SELECT stored.version
            FROM @bae_store::file_version::StoredFileVersion AS stored
            WHERE stored.file = cover.browse_file) AS version
    FROM covers AS cover
"#
);

impl CoverVersion {
    /// The version of the cover's browse copy.
    pub fn into_version(self) -> Option<String> {
        self.version
    }
}

// Each artist image's version, by the artist it is the image of.
coven::fragment!(
    ArtistImageVersion { version: Option<String> },
    r#"
    SELECT image.id,
           (SELECT stored.version
            FROM @bae_store::file_version::StoredFileVersion AS stored
            WHERE stored.file = image.file) AS version
    FROM artist_images AS image
"#
);

impl ArtistImageVersion {
    /// The image's version; `None` while its file has none.
    pub fn into_version(self) -> Option<String> {
        self.version
    }
}

// The coven file the release `release`'s cover is read from, its browse
// copy; no row when the release has no cover.
coven::read!(
    writer CoverFile(release: String) { browse_file: coven::FileId },
    r#"
    SELECT cover.browse_file
    FROM covers AS cover
    WHERE cover.id = :release
"#
);

// The coven file of the artist `artist`'s image; no row when the artist has
// no image.
coven::read!(
    writer ArtistImageFile(artist: String) { file: coven::FileId },
    r#"
    SELECT image.file
    FROM artist_images AS image
    WHERE image.id = :artist
"#
);

// What the release `release`'s cover is made from and the coven file that
// holds it; no row when the release has no cover.
coven::read!(
    writer CoverPictureOfRelease(release: String) { file: Option<coven::FileId> },
    r#"
    SELECT cover.source_file, cover.embedded_file,
           COALESCE(image.file, audio.file, cover.original_file) AS file
    FROM covers AS cover
    LEFT JOIN release_files AS image ON image.id = cover.source_file
    LEFT JOIN release_files AS audio ON audio.id = cover.embedded_file
    WHERE cover.id = :release
"#
);

// The coven file of the release `release`'s cover original, none when the
// cover is made from one of the release's files; no row when the release
// has no cover.
#[cfg(any(test, feature = "test-utils"))]
coven::read!(
    writer CoverOriginal(release: String) { original_file: Option<coven::FileId> },
    r#"
    SELECT cover.original_file
    FROM covers AS cover
    WHERE cover.id = :release
"#
);

// The coven file of the release file `file`'s preview; no row when it has
// none.
coven::read!(
    writer PreviewOfFile(file: String) { file: coven::FileId },
    r#"
    SELECT preview.file
    FROM release_file_previews AS preview
    WHERE preview.id = :file
"#
);

/// The coven file one image row names (a cover's browse copy), or `None` when there is no such row.
pub fn image_file_on<Q: bae_store::WriterRead>(
    sql: &Q,
    image_type: &LibraryImageType,
    id: &str,
) -> Result<Option<coven::FileId>, DbError> {
    let id = id.to_owned();
    Ok(match image_type {
        LibraryImageType::Cover => {
            bae_store::statement_optional(sql, &CoverFileParams { release: id })?
                .map(|row| row.browse_file)
        }
        LibraryImageType::Artist => {
            bae_store::statement_optional(sql, &ArtistImageFileParams { artist: id })?
                .map(|row| row.file)
        }
    })
}

// Whether this device keeps each of the release `:release`'s files whole, as
// `coven_pins` says.
coven::read!(
    writer ReleaseFilesKeptRead(release: String) { kept: bool },
    r#"
    SELECT f.id, pin.file IS NOT NULL AS kept
    FROM release_files AS f
    LEFT JOIN coven_pins AS pin ON pin.file = f.file
    WHERE f.release_id = :release
"#
);

/// The rows of [`ReleaseFilesKeptRead`], by release file id.
fn release_files_kept_of(
    rows: Vec<ReleaseFilesKeptReadRow>,
) -> std::collections::BTreeMap<String, bool> {
    rows.into_iter().map(|row| (row.id, row.kept)).collect()
}

impl Records {
    /// Whether this device keeps each of the release's files whole now, by
    /// release file id.
    pub async fn release_files_kept(
        &self,
        release_id: &str,
    ) -> Result<std::collections::BTreeMap<String, bool>, DbError> {
        let statement = ReleaseFilesKeptReadParams { release: release_id.to_string() };
        Ok(release_files_kept_of(self.store.read_rows(statement).await?))
    }

    /// Whether this device keeps each of the release's files whole, live: it
    /// reruns as a file is pinned or unpinned.
    pub fn subscribe_release_files_kept(
        &self,
        release_id: &str,
    ) -> coven::LiveQuery<std::collections::BTreeMap<String, bool>, DbError> {
        let statement = ReleaseFilesKeptReadParams { release: release_id.to_string() };
        self.store.subscribe(statement).process(|rows| Ok(release_files_kept_of(rows)))
    }
}

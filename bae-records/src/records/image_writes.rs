//! The rows of artist images, covers and file previews, and the coven files
//! their writes stage.

use super::*;
use bae_store::WriteStatements as _;

coven::write!(
    UpsertArtistImage(artist_id: &'p str, file: &'p coven::FileId, content_type: &'p str),
    r#"
    INSERT INTO artist_images (id, file, content_type) VALUES (:artist_id, :file, :content_type)
    ON CONFLICT (id) DO UPDATE SET
        file = excluded.file,
        content_type = excluded.content_type
"#
);

/// Write an artist image's row, pointing it at `file`, the coven file its
/// write staged.
pub fn upsert_artist_image_row(
    conn: &CovenWrite<'_>,
    image: &DbArtistImage,
    file: &coven::FileId,
) -> Result<(), DbError> {
    conn.run(&UpsertArtistImageParams {
        artist_id: &image.artist_id,
        file,
        content_type: image.content_type.as_str(),
    })?;
    Ok(())
}

/// The coven files a cover's write staged, which its row references.
#[derive(Debug, Clone)]
pub struct StagedCover {
    pub browse: coven::FileId,
    pub large: coven::FileId,
    /// The kept original; `None` when the cover is made from a file of the
    /// release.
    pub original: Option<coven::FileId>,
}

/// Stage a cover's copies, and its original when it keeps one, as coven
/// files for the cloud, each in its own namespace.
pub fn stage_cover(w: &mut coven::WriteBatch<'_>, cover: &NewCover) -> StagedCover {
    let browse = w.put_file(
        crate::synced::COVER_BROWSE_NAMESPACE,
        cover.copies.browse.clone(),
        coven::Place::Cloud,
    );
    let large = w.put_file(
        crate::synced::COVER_LARGE_NAMESPACE,
        cover.copies.large.clone(),
        coven::Place::Cloud,
    );
    let original = match &cover.picture {
        NewCoverPicture::ReleaseFile { .. } | NewCoverPicture::Embedded { .. } => None,
        NewCoverPicture::Original { bytes } => Some(w.put_file(
            crate::synced::COVER_ORIGINALS_NAMESPACE,
            bytes.clone(),
            coven::Place::Cloud,
        )),
    };
    StagedCover { browse, large, original }
}

coven::write!(
    UpsertCover(
        release_id: &'p str,
        source_file: Option<&'p str>,
        embedded_file: Option<&'p str>,
        original_file: Option<&'p coven::FileId>,
        browse_file: &'p coven::FileId,
        large_file: &'p coven::FileId,
        content_type: &'p str,
    ),
    r#"
    INSERT INTO covers
        (id, source_file, embedded_file, original_file, browse_file, large_file, content_type)
    VALUES
        (:release_id, :source_file, :embedded_file, :original_file, :browse_file, :large_file,
         :content_type)
    ON CONFLICT (id) DO UPDATE SET
        source_file = excluded.source_file,
        embedded_file = excluded.embedded_file,
        original_file = excluded.original_file,
        browse_file = excluded.browse_file,
        large_file = excluded.large_file,
        content_type = excluded.content_type
"#
);

/// Write a cover's row, pointing it at the files its write staged.
pub fn upsert_cover_row(
    conn: &CovenWrite<'_>,
    cover: &NewCover,
    staged: &StagedCover,
) -> Result<(), DbError> {
    let (source_file, embedded_file) = match &cover.picture {
        NewCoverPicture::ReleaseFile { file_id } => (Some(file_id.as_str()), None),
        NewCoverPicture::Embedded { file_id } => (None, Some(file_id.as_str())),
        NewCoverPicture::Original { .. } => (None, None),
    };
    conn.run(&UpsertCoverParams {
        release_id: &cover.release_id,
        source_file,
        embedded_file,
        original_file: staged.original.as_ref(),
        browse_file: &staged.browse,
        large_file: &staged.large,
        content_type: cover.copies.content_type.as_str(),
    })?;
    Ok(())
}

/// Stage a release image's preview as a coven file for the cloud.
pub fn stage_preview(w: &mut coven::WriteBatch<'_>, preview: &NewReleasePreview) -> coven::FileId {
    w.put_file(
        crate::synced::RELEASE_PREVIEWS_NAMESPACE,
        preview.bytes.clone(),
        coven::Place::Cloud,
    )
}

coven::write!(
    InsertReleaseFilePreview(
        release_file_id: &'p str,
        file: &'p coven::FileId,
        content_type: &'p str,
    ),
    r#"
    INSERT INTO release_file_previews (id, file, content_type)
    VALUES (:release_file_id, :file, :content_type)
"#
);

/// Write a release file's preview row, pointing it at `file`, the coven file
/// its write staged.
pub fn insert_preview_row(
    conn: &CovenWrite<'_>,
    preview: &NewReleasePreview,
    file: &coven::FileId,
) -> Result<(), DbError> {
    conn.run(&InsertReleaseFilePreviewParams {
        release_file_id: &preview.release_file_id,
        file,
        content_type: preview.content_type.as_str(),
    })?;
    Ok(())
}

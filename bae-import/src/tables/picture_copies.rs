//! The copies of an import candidate's pictures, `import_picture_copy`: what
//! owns a picture (the candidate whose scan holds its file, and the volume the
//! file is on), the copies kept for it, and the writes that drop them when
//! their candidate leaves the import list. Its table is `picture_copies.sql`.

use super::folder_scans::columns::volume_of;
use super::*;
use crate::folder_scanner::FileVersion;
use crate::ImportPicture;
use crate::VolumeKind;
use bae_pictures::{CopyFormat, PictureCopy, PictureStep};
use std::path::Path;

/// A picture of the import list as its copies are kept: the candidate that
/// holds its file and the volume that candidate's watched folder is on, which
/// the picture's reads wait behind, and what names the picture: the kind of
/// source and the version of the file it is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PictureOwner {
    /// Coven's bytes for the watched folder, which names the volume.
    pub(crate) watched_folder_path: Vec<u8>,
    candidate_path: Vec<u8>,
    pub(crate) volume: VolumeKind,
    source_kind: &'static str,
    source_path: Vec<u8>,
    source_size: i64,
    source_modified_at_ns: i64,
}

fn step_column(step: PictureStep) -> i64 {
    i64::from(step.edge())
}

/// A picture of the import list: its owner, and the copy at the step that
/// was asked for when its candidate keeps one.
pub(crate) struct OwnedPicture {
    pub(crate) owner: PictureOwner,
    pub(crate) kept: Option<PictureCopy>,
}

sqlive_compose::write!(
    StorePictureCopy(
        watched_folder_path: &'p [u8],
        candidate_path: &'p [u8],
        source_kind: &'static str,
        source_path: &'p [u8],
        source_size: i64,
        source_modified_at_ns: i64,
        step: i64,
        format: &'static str,
        bytes: &'p [u8],
    ),
    r#"
    INSERT INTO import_picture_copy
        (watched_folder_path, candidate_path, source_kind, source_path, source_size,
         source_modified_at_ns, step, format, bytes)
    SELECT :watched_folder_path, :candidate_path, :source_kind, :source_path, :source_size,
        :source_modified_at_ns, :step, :format, :bytes
    WHERE EXISTS (
        SELECT 1
        FROM @crate::tables::folder_scans::row_fragments::HeldCandidates AS candidate
        WHERE candidate.watched_folder_path = :watched_folder_path
            AND candidate.path = :candidate_path)
    ON CONFLICT DO NOTHING
"#
);

impl ImportTables {
    /// The candidate whose scan holds the picture's file, or `None` when none
    /// does: a file that is not an image of a stored candidate is no picture
    /// of the import list. Embedded art belongs to the candidate its source
    /// names. With a `step`, the owner comes with the copy at that step its
    /// candidate keeps, if it keeps one.
    pub(crate) async fn picture_owner(
        &self,
        source: &ImportPicture,
        step: Option<PictureStep>,
    ) -> Result<Option<OwnedPicture>, DbError> {
        let (source_kind, file, embedded_in) = match source {
            ImportPicture::Folder { file } => ("folder_file", file.clone(), None),
            ImportPicture::Embedded { source, candidate, .. } => {
                ("embedded_art", source.clone(), Some(candidate.clone()))
            }
        };
        let FileVersion { path, size, modified_at_ns } = file;
        let source_path = coven::path_bytes(&path);
        let source_size = to_i64(size, "a picture file's size")?;
        let step = step.map(step_column);
        // The copy joins on the owner's own columns and the version of the
        // source; with no step it matches nothing.
        let kept_join = |owner: &str, candidate: &str| {
            format!(
                "LEFT JOIN import_picture_copy AS k \
                   ON k.watched_folder_path = {owner}.watched_folder_path \
                  AND k.candidate_path = {owner}.{candidate} \
                  AND k.source_kind = ? AND k.source_path = ? AND k.source_size = ? \
                  AND k.source_modified_at_ns = ? AND k.step = ? "
            )
        };
        self.store
            .read(move |sql| {
                let row = |row: &rusqlite::Row<'_>| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<Vec<u8>>>(4)?,
                    ))
                };
                let found = match &embedded_in {
                    None => sql
                        .query_row(
                            &format!(
                                "SELECT f.watched_folder_path, f.candidate_path, r.volume, \
                                        k.format, k.bytes \
                                 FROM scan_candidate_file AS f \
                                 JOIN folder_scan_roots AS r \
                                   ON r.watched_folder_path = f.watched_folder_path \
                                 {} \
                                 WHERE f.absolute_path = ? AND f.size = ? \
                                   AND f.modified_at_ns = ? AND f.role = 'artwork' \
                                 LIMIT 1",
                                kept_join("f", "candidate_path")
                            ),
                            params![
                                source_kind,
                                source_path,
                                source_size,
                                modified_at_ns,
                                step,
                                source_path,
                                source_size,
                                modified_at_ns
                            ],
                            row,
                        )
                        .optional()?,
                    Some(candidate) => sql
                        .query_row(
                            &format!(
                                "SELECT c.watched_folder_path, c.path, r.volume, \
                                        k.format, k.bytes \
                                 FROM scan_candidate AS c \
                                 JOIN folder_scan_roots AS r \
                                   ON r.watched_folder_path = c.watched_folder_path \
                                 {} \
                                 WHERE c.path = ? LIMIT 1",
                                kept_join("c", "path")
                            ),
                            params![
                                source_kind,
                                source_path,
                                source_size,
                                modified_at_ns,
                                step,
                                candidate
                            ],
                            row,
                        )
                        .optional()?,
                };
                found
                    .map(|(watched_folder_path, candidate_path, volume, format, bytes)| {
                        let kept = match (format, bytes) {
                            (Some(format), Some(bytes)) => {
                                let format =
                                    CopyFormat::from_stored_name(&format).ok_or_else(|| {
                                        DbError::unreadable("a kept picture copy's format", &format)
                                    })?;
                                Some(PictureCopy { format, bytes })
                            }
                            _ => None,
                        };
                        Ok(OwnedPicture {
                            owner: PictureOwner {
                                watched_folder_path,
                                candidate_path,
                                volume: volume_of(&volume)?,
                                source_kind,
                                source_path,
                                source_size,
                                source_modified_at_ns: modified_at_ns,
                            },
                            kept,
                        })
                    })
                    .transpose()
            })
            .await
    }

    /// Keep `copy` as the copy of the owner's picture at `step` for its
    /// candidate. A candidate that left the list since the owner was read
    /// keeps nothing, and the copy is not stored.
    pub(crate) async fn store_picture_copy(
        &self,
        owner: &PictureOwner,
        step: PictureStep,
        copy: &PictureCopy,
    ) -> Result<(), DbError> {
        let owner = owner.clone();
        let format = copy.format.stored_name();
        let bytes = copy.bytes.clone();
        self.store
            .write(move |sql| {
                sql.run(&StorePictureCopyParams {
                    watched_folder_path: &owner.watched_folder_path,
                    candidate_path: &owner.candidate_path,
                    source_kind: owner.source_kind,
                    source_path: &owner.source_path,
                    source_size: owner.source_size,
                    source_modified_at_ns: owner.source_modified_at_ns,
                    step: step_column(step),
                    format,
                    bytes: &bytes,
                })?;
                Ok(())
            })
            .await
    }

    /// How many copies the owner's candidate keeps.
    #[cfg(test)]
    pub(crate) async fn picture_copy_count(&self, owner: &PictureOwner) -> Result<u64, DbError> {
        let owner = owner.clone();
        self.store
            .read(move |sql| {
                Ok(sql.query_row(
                    "SELECT COUNT(*) FROM import_picture_copy \
                     WHERE watched_folder_path = ? AND candidate_path = ?",
                    params![owner.watched_folder_path, owner.candidate_path],
                    |row| row.get(0),
                )?)
            })
            .await
    }
}

sqlive_compose::write!(
    ForgetCopiesForHash(content_hash: &'p str),
    r#"
    DELETE FROM import_picture_copy
    WHERE (watched_folder_path, candidate_path) IN (
        SELECT candidate.watched_folder_path, candidate.path
        FROM @crate::tables::folder_scans::row_fragments::HeldCandidates AS candidate
        WHERE candidate.content_hash = :content_hash)
"#
);

/// Drop every copy of the candidates holding `content_hash`, inside the write
/// that imports them: an imported candidate leaves the import list.
pub(super) fn forget_for_hash_on(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.run(&ForgetCopiesForHashParams { content_hash })?;
    Ok(())
}

sqlive_compose::write!(
    ForgetCopiesForFolder(candidate_path: Vec<u8>),
    r#"DELETE FROM import_picture_copy WHERE candidate_path = :candidate_path"#
);

/// Drop every copy of the candidate in the folder `candidate_path`, inside the
/// write that sets it aside.
pub(super) fn forget_for_folder_on(
    sql: &CovenWrite<'_>,
    candidate_path: &Path,
) -> Result<(), DbError> {
    sql.run(&ForgetCopiesForFolderParams { candidate_path: coven::path_bytes(candidate_path) })?;
    Ok(())
}

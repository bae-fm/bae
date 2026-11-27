//! A candidate's cover against what its scan stored: the cover a save may
//! store, and the choice a stored selection names, with the version its scan
//! read the artwork from. The pane and the list's row both read a candidate's
//! cover through it, so the two draw the same image under the same identity.

use super::*;
use crate::cover_art::{CoverChoice, CoverRefusal, PreparedCover};
use crate::file_tag_snapshot::FileTagSnapshot;
use crate::CoverSelection;

/// Why `cover` cannot be the cover of the candidate whose files hash to
/// `content_hash`, or `None` when it can: a folder image must be a file the
/// scan stored as one of the folder's images, in a format a cover can be
/// (`bae_supported_cover`), and tag artwork must be what
/// the file-tag snapshot holds, `snapshot` when the save stores that one with
/// it. Every scan of these files holds the same paths, so any of them
/// answers. A catalog's image names nothing the scan holds.
pub(crate) fn cover_refusal(
    sql: &impl coven::SqlRead,
    content_hash: &str,
    cover: &PreparedCover,
    snapshot: Option<&FileTagSnapshot>,
) -> Result<Option<CoverRefusal>, DbError> {
    match cover {
        PreparedCover::Remote { .. } => Ok(None),
        PreparedCover::Local(file_id) => {
            let file = sql
                .query_row(
                    "SELECT file.role, bae_supported_cover(file.relative_path) \
                     FROM scan_candidate AS candidate \
                     JOIN scan_candidate_file AS file \
                       ON file.watched_folder_path = candidate.watched_folder_path \
                      AND file.candidate_path = candidate.path \
                     WHERE candidate.content_hash = ? AND file.relative_path = ? \
                     LIMIT 1",
                    params![content_hash, file_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
                )
                .optional()?;
            let file_id = file_id.clone();
            Ok(match file {
                None => Some(CoverRefusal::NotInFolder { file_id }),
                Some((role, _)) if role != "artwork" => Some(CoverRefusal::NotAnImage { file_id }),
                Some((_, false)) => Some(CoverRefusal::NotACover { file_id }),
                Some((_, true)) => None,
            })
        }
        PreparedCover::Embedded(source_file_id) => {
            let held = match snapshot {
                Some(snapshot) => snapshot
                    .embedded_cover
                    .as_ref()
                    .is_some_and(|cover| cover.source_relative_path == *source_file_id),
                None => sql.query_row(
                    "SELECT EXISTS (SELECT 1 FROM scan_candidate AS candidate \
                     JOIN scan_candidate_tag_snapshot AS snapshot \
                       ON snapshot.watched_folder_path = candidate.watched_folder_path \
                      AND snapshot.candidate_path = candidate.path \
                     WHERE candidate.content_hash = ? \
                       AND snapshot.embedded_cover_source_relative_path = ?)",
                    params![content_hash, source_file_id],
                    |row| row.get::<_, bool>(0),
                )?,
            };
            Ok((!held)
                .then(|| CoverRefusal::NoTagArtwork { source_file_id: source_file_id.clone() }))
        }
    }
}

/// What the versions of the files a cover selection names are read from, as
/// SQLite holds them: each kind of selection reads its own, and the others are
/// `NULL`. A local selection reads one of the folder's images as the scan read
/// it; an embedded one the file the file-tag snapshot read the artwork from,
/// with the snapshot's reading of it.
#[derive(Clone, PartialEq)]
pub(crate) struct CoverVersionColumns {
    local: (Option<Vec<u8>>, Option<i64>, Option<i64>),
    embedded_source: Option<String>,
    embedded: (Option<i64>, Option<i64>, Option<Vec<u8>>),
}

impl CoverVersionColumns {
    /// The versions as read: the chosen folder image's path, size and
    /// modification time; the file the snapshot read the artwork from; and
    /// that file's size, modification time and path.
    pub(crate) fn new(
        local: (Option<Vec<u8>>, Option<i64>, Option<i64>),
        embedded_source: Option<String>,
        embedded: (Option<i64>, Option<i64>, Option<Vec<u8>>),
    ) -> Self {
        Self { local, embedded_source, embedded }
    }

    /// The choice `selection` names for the candidate at `candidate_path`: a
    /// catalog's image as the selection holds it, one of the folder's images
    /// at the version the scan read, or the artwork the file-tag snapshot
    /// stored with the version of the file the scan read it from.
    ///
    /// bae stores the selection under the candidate's content hash, which
    /// fixes its files, so a selection naming a file or artwork the scan did
    /// not store is a damaged row, and the read fails naming it.
    pub(crate) fn choice(
        self,
        candidate_path: &CandidateKey,
        selection: &CoverSelection,
    ) -> Result<CoverChoice, DbError> {
        match selection {
            CoverSelection::Remote(image, source) => {
                Ok(CoverChoice::Remote { image: image.clone(), source: *source })
            }
            CoverSelection::Local(file_id) => {
                let (Some(path), Some(size), Some(modified_at_ns)) = self.local else {
                    return Err(DbError::unreadable(
                        format!(
                            "the file that candidate {candidate_path}'s stored cover choice names"
                        ),
                        file_id,
                    ));
                };
                Ok(CoverChoice::Local {
                    file_id: file_id.clone(),
                    file: crate::folder_scanner::FileVersion {
                        path: path_column::decode(path)?,
                        size: to_u64(size, "a scanned file's size")?,
                        modified_at_ns,
                    },
                })
            }
            CoverSelection::Embedded(source_file_id) => {
                let Some(stored_source) = self.embedded_source else {
                    return Err(DbError::unreadable(
                        format!(
                            "the tag snapshot of {candidate_path}, whose cover choice is embedded"
                        ),
                        None::<String>,
                    ));
                };
                if stored_source != *source_file_id {
                    return Err(DbError::unreadable(
                        format!(
                            "the source of {candidate_path}'s embedded cover and of its cover \
                             choice"
                        ),
                        (&stored_source, source_file_id),
                    ));
                }
                let (Some(size), Some(modified_at_ns), Some(path)) = &self.embedded else {
                    return Err(DbError::unreadable(
                        format!(
                            "the reading of {source_file_id}, the source of {candidate_path}'s \
                             cover"
                        ),
                        format!("{:?}", self.embedded),
                    ));
                };
                Ok(CoverChoice::Embedded {
                    candidate: candidate_path.clone(),
                    source_file_id: source_file_id.clone(),
                    source: crate::folder_scanner::FileVersion {
                        path: path_column::decode(path.clone())?,
                        size: to_u64(*size, "a file-tag reading's size")?,
                        modified_at_ns: *modified_at_ns,
                    },
                })
            }
        }
    }
}

/// The cover choice that a pane's read selects, as
/// [`crate::tables::import_state::fragments::CoverChoice`] names it.
pub(crate) fn cover_choice_of(
    cover: crate::tables::import_state::fragments::CoverChoice,
    key: &CandidateKey,
) -> Result<CoverChoice, DbError> {
    let versions = CoverVersionColumns {
        local: (cover.cover_local_path, cover.cover_local_size, cover.cover_local_modified),
        embedded_source: cover.cover_embedded_source,
        embedded: (
            cover.cover_embedded_size,
            cover.cover_embedded_modified,
            cover.cover_embedded_path,
        ),
    };
    let selection = crate::tables::import_state::StoredCover::new(
        cover.cover_kind,
        cover.cover_file_id,
        cover.cover_url,
        cover.cover_source,
        cover.cover_copies,
    )
    .selection()?;
    versions.choice(key, &selection)
}

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
/// scan stored as one of the folder's images, and tag artwork must be what
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
            let role = sql
                .query_row(
                    "SELECT file.role FROM scan_candidate AS candidate \
                     JOIN scan_candidate_file AS file \
                       ON file.watched_folder_path = candidate.watched_folder_path \
                      AND file.candidate_path = candidate.path \
                     WHERE candidate.content_hash = ? AND file.relative_path = ? \
                     LIMIT 1",
                    params![content_hash, file_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            Ok(match role.as_deref() {
                None => Some(CoverRefusal::NotInFolder { file_id: file_id.clone() }),
                Some("artwork") => None,
                Some(_) => Some(CoverRefusal::NotAnImage { file_id: file_id.clone() }),
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

/// The joins that read the version of the file a cover selection names, for a
/// statement that names the candidate's scan row as `c` and the selection's
/// kind and file by the expressions `kind` and `file_id`: one of the folder's
/// images as the scan read it, or the file an embedded cover was read from
/// with the file-tag snapshot's reading of it.
pub(crate) fn cover_version_joins(kind: &str, file_id: &str) -> String {
    format!(
        "LEFT JOIN scan_candidate_file AS cover_local \
             ON {kind} = 'local' \
            AND cover_local.watched_folder_path = c.watched_folder_path \
            AND cover_local.candidate_path = c.path \
            AND cover_local.relative_path = {file_id} \
         LEFT JOIN scan_candidate_tag_snapshot AS cover_snapshot \
             ON {kind} = 'embedded' \
            AND cover_snapshot.watched_folder_path = c.watched_folder_path \
            AND cover_snapshot.candidate_path = c.path \
         LEFT JOIN scan_candidate_file_tag AS cover_tags \
             ON cover_tags.watched_folder_path = cover_snapshot.watched_folder_path \
            AND cover_tags.candidate_path = cover_snapshot.candidate_path \
            AND cover_tags.relative_path = {file_id} \
         LEFT JOIN scan_candidate_file AS cover_source \
             ON cover_source.watched_folder_path = cover_snapshot.watched_folder_path \
            AND cover_source.candidate_path = cover_snapshot.candidate_path \
            AND cover_source.relative_path = {file_id}"
    )
}

/// The columns [`cover_version_joins`] makes readable, which
/// [`CoverVersionColumns::read`] reads.
pub(crate) const COVER_VERSION_COLUMNS: &str = "\
    cover_local.absolute_path AS cover_local_path, cover_local.size AS cover_local_size, \
    cover_local.modified_at_ns AS cover_local_modified, \
    cover_snapshot.embedded_cover_source_relative_path AS cover_embedded_source, \
    cover_tags.file_size AS cover_embedded_size, \
    cover_tags.modified_at_ns AS cover_embedded_modified, \
    cover_source.absolute_path AS cover_embedded_path";

/// The [`COVER_VERSION_COLUMNS`] of one row, as SQLite holds them: each kind
/// of selection reads its own, and the others are `NULL`.
pub(crate) struct CoverVersionColumns {
    local: (Option<Vec<u8>>, Option<i64>, Option<i64>),
    embedded_source: Option<String>,
    embedded: (Option<i64>, Option<i64>, Option<Vec<u8>>),
}

impl CoverVersionColumns {
    pub(crate) fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            local: (
                row.get("cover_local_path")?,
                row.get("cover_local_size")?,
                row.get("cover_local_modified")?,
            ),
            embedded_source: row.get("cover_embedded_source")?,
            embedded: (
                row.get("cover_embedded_size")?,
                row.get("cover_embedded_modified")?,
                row.get("cover_embedded_path")?,
            ),
        })
    }

    /// The choice `selection` names for the candidate at `candidate_path`: a
    /// catalog's image as the selection holds it, one of the folder's images
    /// at the version the scan read, or the artwork the file-tag snapshot
    /// stored with the version of the file it was read from.
    ///
    /// The selection is stored under the candidate's content hash, which
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
                        format!("the file candidate {candidate_path}'s stored cover choice names"),
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

/// The choice `selection` names for the candidate at `candidate_path` of the
/// watched folder `watched_folder_path`, read with its own statement.
pub(crate) fn stored_cover_choice(
    sql: &SqlReadContext<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    selection: &CoverSelection,
) -> Result<CoverChoice, DbError> {
    let (kind, file_id) = match selection {
        CoverSelection::Remote(image, source) => {
            return Ok(CoverChoice::Remote { image: image.clone(), source: *source })
        }
        CoverSelection::Local(file_id) => ("local", file_id),
        CoverSelection::Embedded(file_id) => ("embedded", file_id),
    };
    sql.query_row(
        &format!(
            "SELECT {COVER_VERSION_COLUMNS} FROM scan_candidate AS c {joins} \
             WHERE c.watched_folder_path = :root AND c.path = :key",
            joins = cover_version_joins(":kind", ":file_id"),
        ),
        named_params! {
            ":kind": kind,
            ":file_id": file_id,
            ":root": coven::path_bytes(watched_folder_path),
            ":key": candidate_path,
        },
        CoverVersionColumns::read,
    )
    .map_err(DbError::from)?
    .choice(candidate_path, selection)
}

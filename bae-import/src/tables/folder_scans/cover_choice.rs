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

/// The choice `selection` names for the candidate at `candidate_path`: a
/// catalog's image as the selection holds it, one of the folder's images at
/// the version the scan read, or the artwork the file-tag snapshot stored
/// with the version of the file it was read from.
///
/// The selection is stored under the candidate's content hash, which fixes
/// its files, so a selection naming a file or artwork the scan did not store
/// is a damaged row, and the read fails naming it.
pub(crate) fn stored_cover_choice(
    sql: &SqlReadContext<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    selection: &CoverSelection,
) -> Result<CoverChoice, DbError> {
    let mut choices =
        stored_cover_choices(sql, &[(watched_folder_path, candidate_path, selection)])?;
    Ok(choices.remove(0))
}

/// [`stored_cover_choice`] for each of `covers`, in order: one statement
/// reads the versions of every folder image, and one those of every
/// embedded cover's file.
pub(crate) fn stored_cover_choices(
    sql: &SqlReadContext<'_>,
    covers: &[(&Path, &CandidateKey, &CoverSelection)],
) -> Result<Vec<CoverChoice>, DbError> {
    let named = |wanted: fn(&CoverSelection) -> Option<&str>| -> Vec<(usize, (&Path, &CandidateKey, &str))> {
        covers
            .iter()
            .enumerate()
            .filter_map(|(at, (root, key, selection))| {
                wanted(selection).map(|file_id| (at, (*root, *key, file_id)))
            })
            .collect()
    };
    let local = named(|selection| match selection {
        CoverSelection::Local(file_id) => Some(file_id),
        _ => None,
    });
    let embedded = named(|selection| match selection {
        CoverSelection::Embedded(file_id) => Some(file_id),
        _ => None,
    });
    let mut local_versions = if local.is_empty() {
        Vec::new()
    } else {
        let wanted: Vec<_> = local.iter().map(|(_, wanted)| *wanted).collect();
        read::scanned_file_versions(sql, &wanted)?
    }
    .into_iter();
    let mut embedded_versions = if embedded.is_empty() {
        Vec::new()
    } else {
        let wanted: Vec<_> = embedded.iter().map(|(_, wanted)| *wanted).collect();
        tag_snapshot::stored_embedded_cover_versions(sql, &wanted)?
    }
    .into_iter();
    covers
        .iter()
        .map(|(_, candidate_path, selection)| {
            let next = |versions: &mut std::vec::IntoIter<crate::folder_scanner::FileVersion>| {
                versions.next().ok_or_else(|| {
                    DbError::unreadable(format!("the cover versions read for {candidate_path}"), 0)
                })
            };
            Ok(match selection {
                CoverSelection::Remote(image, source) => {
                    CoverChoice::Remote { image: image.clone(), source: *source }
                }
                CoverSelection::Local(file_id) => CoverChoice::Local {
                    file_id: file_id.clone(),
                    file: next(&mut local_versions)?,
                },
                CoverSelection::Embedded(source_file_id) => CoverChoice::Embedded {
                    candidate: (*candidate_path).clone(),
                    source_file_id: source_file_id.clone(),
                    source: next(&mut embedded_versions)?,
                },
            })
        })
        .collect()
}

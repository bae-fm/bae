//! The cover choice a candidate's stored selection names, with the image its
//! scan stored for it. The pane and the list's row both read a candidate's
//! cover through it, so the two draw the same image under the same identity.

use super::*;
use crate::cover_art::CoverChoice;
use crate::CoverSelection;

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
    Ok(match selection {
        CoverSelection::Remote(image, source) => {
            CoverChoice::Remote { image: image.clone(), source: *source }
        }
        CoverSelection::Local(file_id) => CoverChoice::Local {
            file_id: file_id.clone(),
            file: read::scanned_file_version(sql, watched_folder_path, candidate_path, file_id)?,
        },
        CoverSelection::Embedded(source_file_id) => {
            let (source, data) = tag_snapshot::stored_embedded_cover(
                sql,
                watched_folder_path,
                candidate_path,
                source_file_id,
            )?;
            CoverChoice::Embedded { source_file_id: source_file_id.clone(), source, data }
        }
    })
}

//! The files a folder carries that no release read there owns: its
//! sidecar files, each given its role the way a release's are.

use super::*;

/// The files under `folder` that no release read there owns, each given its
/// role the way a release's are — `folder` holds no audio, so every one is an
/// artwork, a document, or another file — or, when a zero-byte track sits
/// among them or two of them show one name, every one as the scan read it.
pub(super) fn categorize_sidecar(
    tree: &CandidateFileIndex,
    folder: &Path,
    fs_root: &Path,
    cancellation: &ScanCancellation,
    reads: &FileReads,
) -> Result<SidecarFiles, FolderScanError> {
    let mut proposed = Vec::new();
    // `None` once a damaged track is among them.
    let mut files = Some(Vec::new());
    for entry in tree.all_files_under(folder) {
        cancellation.check()?;
        let (file, role) = propose_file(entry, folder, fs_root, reads)?;
        let role = match role {
            ProposedRole::Audio => {
                return Err(FolderScanError::Other(format!(
                    "{} is a track, and a folder's sidecar files hold none",
                    file.version.path.display()
                )))
            }
            // A sheet with no audio beside it names nothing here to play.
            ProposedRole::Cue => ProposedRole::Document,
            role => role,
        };
        let release_role = match role {
            ProposedRole::Image => Some(FileRole::Artwork),
            ProposedRole::DamagedImage => Some(FileRole::DamagedImage),
            ProposedRole::Document | ProposedRole::Cue => Some(FileRole::Document),
            ProposedRole::Other => Some(FileRole::Other),
            ProposedRole::DamagedAudio { .. } | ProposedRole::Audio => None,
        };
        match release_role {
            Some(role) => {
                if let Some(files) = files.as_mut() {
                    files.push(CandidateFile { file: file.clone(), role });
                }
            }
            None => files = None,
        }
        proposed.push((file, role));
    }
    let named_once =
        shared_name(proposed.iter().map(|(file, _)| file.relative_path.as_str())).is_none();
    let Some(mut files) = files.filter(|_| named_once) else {
        return Ok(SidecarFiles::Invalid(skipped_files(&proposed)));
    };
    // The release file order (see `categorize_files_from_tree`).
    files.sort_by(|a, b| {
        bae_util::natural_order::compare(&a.file.relative_path, &b.file.relative_path)
    });
    Ok(SidecarFiles::Valid(files))
}

#[cfg(test)]
#[path = "sidecars_tests.rs"]
mod tests;

//! The verbatim export arm: reproducing an imported release's file set
//! byte-for-byte. The output queue, staging, marker, and replace logic live in
//! [`super::output`]; the save (rendered-output) arm in [`super::save`].

use super::*;
use crate::storage::path_fragment::validate_path_fragment;
use tracing::info;

impl LibraryManager {
    /// Copy a release's files verbatim into the staging directory — the Export
    /// arm of `export_release_to_dir`. Updates the queue's per-release percent
    /// (by file index) and re-emits the snapshot after each file.
    pub(super) async fn copy_release_files_to_staging(
        &self,
        release_id: &str,
        folder: &str,
        staging_dir: &std::path::Path,
    ) -> Result<(), LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        info!(
            release_id,
            folder,
            file_count = files.len(),
            kind = "export",
            "Writing release output"
        );
        let total = files.len();
        for (index, file) in files.iter().enumerate() {
            self.export_one_file(file, staging_dir).await?;
            let percent = (((index + 1) * 100) / total.max(1)) as u8;
            self.set_output_progress(release_id, percent);
        }
        Ok(())
    }

    /// Copy one release file's verbatim bytes to `<staging_dir>/<original_filename>`.
    /// `original_filename` may name a subfolder (e.g. `CD1/CDImage.ape`), so its
    /// parent is created first. No per-file temp is needed: the whole staging
    /// directory is the atomic unit, renamed into place only once every file is
    /// written.
    async fn export_one_file(
        &self,
        file: &DbFile,
        staging_dir: &std::path::Path,
    ) -> Result<(), LibraryError> {
        validate_path_fragment(
            &file.release_id,
            &format!("original_filename for file {}", file.id),
            &file.original_filename,
        )?;
        let bytes = self.read_release_blob(file).await?;
        let file_path = staging_dir.join(&file.original_filename);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file_path, &bytes)?;
        Ok(())
    }
}

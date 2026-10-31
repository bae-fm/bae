//! The verbatim export arm: reproducing an imported release's file set
//! byte-for-byte. The output queue, staging, marker, and replace logic live in
//! [`super::output`]; the save (rendered-output) arm in [`super::save`].

use super::local_files::blocking_io;
use super::*;
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

    /// Copy one release file's verbatim bytes to the path its name in the
    /// release gives below `staging_dir`, each name written as coven writes it
    /// (see [`crate::storage::stored_name`]). The name may name a subfolder
    /// (e.g. `CD1/CDImage.ape`), so its parent is created first. No per-file
    /// temp is needed: the whole staging directory is the atomic unit, renamed
    /// into place only once every file is written.
    ///
    /// The file is created new, never over one already there
    /// ([`crate::storage::stored_name::create_new_file`]).
    ///
    /// The bytes stream a window at a time from coven's stream over the file
    /// (the user's own file, the local store, the cache, or the cloud), so a
    /// file of any size costs one window of memory; each write runs on a
    /// blocking thread.
    async fn export_one_file(
        &self,
        file: &DbFile,
        staging_dir: &std::path::Path,
    ) -> Result<(), LibraryError> {
        let relative = crate::storage::stored_name::file_path(
            &file.release_id,
            &file.id,
            &file.original_filename,
        )?
        .local_path();
        let stream = self.database.open_release_file(&file.id).await?;
        let size = stream.plaintext_size();

        let file_path = staging_dir.join(&relative);
        let file_id = file.id.clone();
        let mut output = blocking_io(move || {
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            use crate::storage::stored_name::{create_new_file, CreateFileError};
            match create_new_file(&file_path, &relative, &format!("file {file_id}")) {
                Ok(output) => Ok(output),
                Err(clash @ CreateFileError::Clash { .. }) => {
                    Err(LibraryError::Export(clash.to_string()))
                }
                Err(CreateFileError::Io(error)) => Err(error.into()),
            }
        })
        .await?;
        let mut offset = 0;
        while offset < size {
            let len = EXPORT_WINDOW.min(size - offset);
            let window = stream.read_at(offset, len).await?;
            output = blocking_io(move || {
                std::io::Write::write_all(&mut output, &window)?;
                Ok(output)
            })
            .await?;
            offset += len;
        }
        blocking_io(move || Ok(output.sync_all()?)).await?;
        Ok(())
    }
}

/// How much of a file an export holds in memory at once.
const EXPORT_WINDOW: u64 = 4 * 1024 * 1024;

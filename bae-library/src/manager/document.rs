//! A library release's documents as the person reads them: a rip log, a text
//! file, a track sheet, decoded from the encoding it was written in.

use super::*;

impl LibraryManager {
    /// The text of release `release_id`'s file `file_id`, read through coven
    /// from wherever coven has it and decoded from the encoding it was written
    /// in, as a folder's document is (`ImportServiceHandle::read_candidate_document`).
    /// Only a document or a track sheet is read: the editor offers no other
    /// file to open, so another is refused as a bug.
    pub async fn read_release_document(
        &self,
        release_id: &str,
        file_id: &str,
    ) -> Result<String, LibraryError> {
        let file = self.records.find_release_file(release_id, file_id).await?.ok_or_else(|| {
            LibraryError::FileMissing {
                release_id: release_id.to_string(),
                file_id: file_id.to_string(),
            }
        })?;
        if !file.role.is_readable_text() {
            return Err(LibraryError::Internal(format!(
                "release file {file_id} is no document: it is {}",
                file.role.as_str()
            )));
        }
        let bytes = self.read_release_file(&file).await?;
        Ok(bae_util::text_encoding::decode_text(&bytes).text)
    }
}

#[cfg(test)]
#[path = "document_tests.rs"]
mod tests;

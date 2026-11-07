//! A candidate's documents as the person reads them: a log, a text file, a
//! track sheet, decoded from the encoding it was written in.

use super::*;

impl ImportServiceHandle {
    /// The text of the candidate's file `file_id`, read from the folder now
    /// and decoded from the encoding it was written in. Only a document or a
    /// track sheet is read: another file of the folder, or one the candidate
    /// does not hold, is refused.
    pub async fn read_candidate_document(
        &self,
        candidate_key: &crate::CandidateKey,
        file_id: &str,
    ) -> Result<String, crate::ImportError> {
        use crate::folder_scanner::FileRole;
        let candidate =
            self.services.tables.load_folder_candidate(candidate_key).await?.ok_or_else(|| {
                crate::ImportError::Refused(format!("{candidate_key} is not a stored candidate"))
            })?;
        let entry = candidate
            .files
            .files
            .into_iter()
            .find(|entry| entry.file.relative_path == file_id)
            .ok_or_else(|| {
                crate::ImportError::Refused(format!("{candidate_key} holds no file {file_id}"))
            })?;
        if !matches!(entry.role, FileRole::Document | FileRole::TrackSheet { .. }) {
            return Err(crate::ImportError::Refused(format!("{file_id} is not a document")));
        }
        let path = entry.file.version.path;
        let file = file_id.to_string();
        self.services
            .diagnostics
            .blocking("read a candidate's document", move || {
                bae_util::text_encoding::read_text_file(&path)
                    .map(|decoded| decoded.text)
                    .map_err(|error| crate::ImportError::DocumentRead { file, error })
            })
            .await?
    }
}

#[cfg(test)]
#[path = "documents_tests.rs"]
mod tests;

//! A candidate's documents as the person reads them: a log, a text file, a
//! track sheet, decoded from the encoding it was written in.

use super::*;

/// Why reading a candidate's document was refused.
#[derive(Debug, thiserror::Error)]
pub enum DocumentRefusal {
    /// The candidate left the scan after the person opened it.
    #[error("{0} is not a stored candidate")]
    CandidateGone(crate::CandidateKey),
    /// The file left the candidate after the person opened it.
    #[error("{candidate} holds no file {file}")]
    FileGone { candidate: crate::CandidateKey, file: String },
    /// The candidate lists only documents and track sheets to read, so
    /// another file is bae's own fault.
    #[error("{0} is not a document")]
    NotADocument(String),
}

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
        let candidate = self
            .services
            .tables
            .load_folder_candidate(candidate_key)
            .await?
            .ok_or_else(|| DocumentRefusal::CandidateGone(candidate_key.clone()))?;
        let entry = candidate
            .files
            .files
            .into_iter()
            .find(|entry| entry.file.relative_path == file_id)
            .ok_or_else(|| DocumentRefusal::FileGone {
                candidate: candidate_key.clone(),
                file: file_id.to_string(),
            })?;
        if !matches!(entry.role, FileRole::Document { .. } | FileRole::TrackSheet { .. }) {
            return Err(DocumentRefusal::NotADocument(file_id.to_string()).into());
        }
        let path = entry.file.version.path;
        let file = file_id.to_string();
        let lease = self.services.store.lease_one().await;
        self.services
            .diagnostics
            .blocking("read a candidate's document", move || {
                bae_util::text_encoding::read_text_file(&path, lease.place_blocking())
                    .map(|decoded| decoded.text)
                    .map_err(|error| crate::ImportError::DocumentRead { file, error })
            })
            .await?
    }
}

#[cfg(test)]
#[path = "documents_tests.rs"]
mod tests;

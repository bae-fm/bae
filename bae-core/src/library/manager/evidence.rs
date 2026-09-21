//! Open the logs behind a release's rip verification.

use super::{LibraryError, LibraryManager};
use crate::import::Verification;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceSubject {
    Candidate { key: String },
    Release { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceSelection {
    Verification,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceContent {
    Document { name: String, text: String },
}

impl LibraryManager {
    pub async fn read_evidence(
        &self,
        subject: &EvidenceSubject,
        selection: &EvidenceSelection,
    ) -> Result<Vec<EvidenceContent>, LibraryError> {
        let EvidenceSelection::Verification = selection;
        let contents = match subject {
            EvidenceSubject::Candidate { key } => {
                let detail = self.load_import_candidate(key).await?.ok_or_else(|| {
                    LibraryError::Import(format!("Candidate {key} no longer exists"))
                })?;
                let signals = detail.signals.as_ref().ok_or_else(|| {
                    LibraryError::Import(format!("Candidate {key} has no extracted evidence"))
                })?;
                let verification = signals
                    .verification
                    .as_ref()
                    .ok_or_else(missing_verification)?;
                let mut contents = Vec::new();
                for file in detail
                    .candidate
                    .files()
                    .documents()
                    .filter(|file| is_log(&file.relative_path))
                {
                    let text =
                        crate::text_encoding::decode_text(&tokio::fs::read(&file.path).await?).text;
                    if log_confirms(&text, verification) {
                        contents.push(EvidenceContent::Document {
                            name: file.relative_path.clone(),
                            text,
                        });
                    }
                }
                contents
            }
            EvidenceSubject::Release { id } => {
                let files = self.get_files_for_release(id).await?;
                let detail = self.find_release_detail(id).await?.ok_or_else(|| {
                    LibraryError::Storage(format!("Release {id} no longer exists"))
                })?;
                let verification = detail
                    .verification
                    .as_ref()
                    .ok_or_else(missing_verification)?;
                let mut contents = Vec::new();
                for file in files.iter().filter(|file| is_log(&file.original_filename)) {
                    let text =
                        crate::text_encoding::decode_text(&self.read_release_blob(file).await?)
                            .text;
                    if log_confirms(&text, verification) {
                        contents.push(EvidenceContent::Document {
                            name: file.original_filename.clone(),
                            text,
                        });
                    }
                }
                contents
            }
        };
        if contents.is_empty() {
            return Err(LibraryError::Storage(
                "The source of this evidence is no longer available".into(),
            ));
        }
        Ok(contents)
    }
}

fn is_log(name: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("log"))
}

fn missing_verification() -> LibraryError {
    LibraryError::Storage("This release has no rip verification".into())
}

/// Stored verification contains track results, not a filename. Match the whole
/// result against the release's logs; the disc-ID log may be a different file.
/// Multiple logs declaring the same result are all evidence for that result.
fn log_confirms(text: &str, verification: &Verification) -> bool {
    match crate::import::rip_log::parse_rip_log(text) {
        Ok(log) => Verification::of(&log) == *verification,
        Err(error) => {
            tracing::debug!(%error, "log does not contain a recognized rip result");
            false
        }
    }
}

//! Everything stored about one import candidate under its content hash, as
//! one value.
//!
//! The database loads and saves this whole. A writer loads it, changes what
//! its operation changes, and saves it back against the revisions it loaded,
//! so no two writers ever disagree about which rows belong together and no
//! rule about the candidate has to be stated in SQL. `CandidateAsRead` is
//! what a writer names those revisions with, and it owns every refusal for a
//! candidate that moved between the read and the write.

use crate::folder_scanner::CandidateFileEdits;
use crate::tables::DbCandidateIdentifyResult;
use crate::{CandidateMetadataDraft, MetadataProvenance};
use bae_identify::signals::Signals;

/// One candidate as a caller last read it: which candidate it is, and the two
/// revisions it stood at.
///
/// Every write composed against what its caller read carries this, and is
/// refused when either revision has moved. The two are never one number:
/// `file_edit_revision` is the folder's file shape, which the scan rows carry
/// and the list and pane queries join on, while `metadata_revision` counts
/// writes to the draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateAsRead {
    /// `CategorizedFiles::content_hash` — which candidate this is, and the row
    /// every write against it addresses. Adding, removing, or resizing a file
    /// changes this, which orphans the old row rather than updating it.
    pub content_hash: String,
    /// The file-decision revision the caller read.
    pub file_edit_revision: u64,
    /// The metadata revision the caller read.
    pub metadata_revision: u64,
}

/// What a write was about to store, named in the refusal when the candidate
/// moved out from under it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum CandidateWrite {
    /// A source's projection replacing the candidate's metadata.
    Metadata,
    /// One pane control's edit to the draft.
    PaneEdit,
    /// The person's file decisions, which advance the file revision.
    FileDecisions,
}

impl CandidateAsRead {
    /// `Ok` while the stored candidate still stands at both revisions this
    /// read, otherwise the refusal naming which one moved.
    pub(crate) fn verify(
        &self,
        prep: &CandidatePreparation,
        write: CandidateWrite,
    ) -> Result<(), crate::ImportError> {
        if !prep.file_edits.is_at_revision(self.file_edit_revision) {
            return Err(Self::files_moved(self.file_edit_revision, write));
        }
        if prep.metadata_revision != self.metadata_revision {
            return Err(Self::metadata_moved(self.metadata_revision));
        }
        Ok(())
    }

    /// The refusal for a candidate whose file decisions moved past `expected`.
    pub(crate) fn files_moved(expected: u64, write: CandidateWrite) -> crate::ImportError {
        crate::ImportError::Refused(match write {
            CandidateWrite::Metadata => format!(
                "candidate changed before its metadata was stored: its files moved past \
                 revision {expected}"
            ),
            CandidateWrite::PaneEdit => format!(
                "candidate changed before its edit was stored: its files moved past \
                 revision {expected}"
            ),
            CandidateWrite::FileDecisions => {
                format!("candidate file decisions changed at revision {expected}")
            }
        })
    }

    /// The refusal for a candidate whose metadata moved past `expected`.
    pub(crate) fn metadata_moved(expected: u64) -> crate::ImportError {
        crate::ImportError::Refused(format!("candidate metadata changed from revision {expected}"))
    }
}

/// Who wrote the candidate's draft.
///
/// Stored on the draft rather than on its provenance: a person can write a
/// draft read from nowhere — typed in, or cleared — and it is still theirs.
/// Which author a draft can have depends on where it was read from; see
/// [`MetadataAuthor::check_provenance`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataAuthor {
    /// The blank draft discovery creates when it has no tags to read.
    Nobody,
    /// Discovery, seeding the draft from the folder's own file tags.
    Prefill,
    /// A finished identification run, applying the one pressing it settled
    /// on.
    Identification,
    /// A person: a pick, applying file metadata, a reset, a clear, or any
    /// edit to the draft's fields, tracks or cover.
    Person,
}

impl MetadataAuthor {
    /// `Ok` when a draft written by this author may carry `provenance`.
    ///
    /// Nobody reads a blank draft from anywhere, discovery only ever seeds
    /// from the folder's own tags, and identification only ever concludes a
    /// release. A person may have read the draft from anywhere or nowhere.
    pub fn check_provenance(
        self,
        provenance: Option<&MetadataProvenance>,
    ) -> Result<(), ProvenanceContradiction> {
        match (self, provenance) {
            (Self::Nobody, None)
            | (Self::Prefill, Some(MetadataProvenance::FileMetadata))
            | (Self::Identification, Some(MetadataProvenance::ExternalRelease { .. }))
            | (Self::Person, _) => Ok(()),
            (author, provenance) => {
                Err(ProvenanceContradiction { author, provenance: provenance.cloned() })
            }
        }
    }
}

/// A draft's author that could not have read it from where it says.
#[derive(Debug, thiserror::Error)]
#[error("a draft written by {author:?} cannot be read from {provenance:?}")]
pub struct ProvenanceContradiction {
    pub author: MetadataAuthor,
    pub provenance: Option<MetadataProvenance>,
}

/// One candidate's stored state.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidatePreparation {
    /// `CategorizedFiles::content_hash` — the row's identity.
    pub content_hash: String,
    /// Where the candidate was last seen on disk. Not identity — the hash is.
    pub candidate_key: crate::CandidateKey,
    /// The person's decisions about the folder's files, and the revision
    /// every write against this file shape is checked against.
    pub file_edits: CandidateFileEdits,
    /// Moves with every change to `metadata`, so a source projection cannot
    /// overwrite a newer edit.
    pub metadata_revision: u64,
    pub author: MetadataAuthor,
    /// The draft, its provenance, and its cover with the bytes prepared for
    /// it.
    pub metadata: CandidateMetadataDraft,
    /// What the candidate is linked to in the catalogs. Independent of
    /// `metadata`: no draft write changes it.
    pub link: Option<crate::ReleaseLink>,
    /// What identification concluded, or `None` when nothing has — including
    /// after a file decision cleared a verdict about a shape that is gone.
    pub identification: Option<DbCandidateIdentifyResult>,
    /// The signals identification settled on, or `None` when nothing has
    /// extracted them.
    pub signals: Option<Signals>,
}

impl CandidatePreparation {
    /// The contradictions no stored candidate may hold, named so a save can
    /// refuse them before any row moves.
    pub fn validate(&self) -> Result<(), crate::ImportError> {
        self.author.check_provenance(self.metadata.provenance.as_ref()).map_err(|contradiction| {
            crate::ImportError::Provenance {
                content_hash: self.content_hash.clone(),
                contradiction,
            }
        })
    }
}

/// The candidate `content_hash`'s whole stored state, or `None` for a hash no
/// scan stored a candidate under. A candidate whose files this device already
/// imported holds no draft, so reading it fails as an import of those files
/// does.
pub(crate) async fn stored_preparation(
    tables: &crate::tables::ImportTables,
    content_hash: &str,
) -> Result<Option<CandidatePreparation>, crate::ImportError> {
    match tables.load_candidate_preparation(content_hash).await? {
        None => Ok(None),
        Some(crate::tables::CandidatePreparationRead::Stored(preparation)) => Ok(Some(preparation)),
        Some(crate::tables::CandidatePreparationRead::Imported { release_id }) => {
            Err(crate::ImportError::FilesAlreadyImported { release_id })
        }
    }
}

//! Everything stored about one import candidate under its content hash, as
//! one value.
//!
//! The database loads and saves this whole. A writer loads it, changes what
//! its operation changes, and saves it back against the revisions it loaded,
//! so no two writers ever disagree about which rows belong together and no
//! rule about the candidate has to be stated in SQL.

use crate::db::DbCandidateIdentifyResult;
use crate::import::folder_scanner::CandidateFileEdits;
use crate::import::{CandidateMetadataDraft, CoverSelection};
use crate::signals::Signals;
use std::collections::BTreeSet;

/// Who last wrote the candidate's metadata.
///
/// Identification's answer stands until a person's, and a person's is never
/// overwritten by identification. A draft nobody chose — the blank one
/// discovery creates, or one a person cleared — is anybody's to fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataAuthor {
    Nobody,
    Identification,
    User,
}

/// The stored candidate as a library import finds it at commit time: what
/// the scan lists at the key, and the revisions its preparation is at.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CommittingCandidate {
    pub actionable: bool,
    pub source: crate::import::release_candidate::CandidateSource,
    pub content_hash: String,
    pub file_edit_revision: u64,
    /// The state row's file and metadata revisions, or `None` when no row
    /// is stored under the content hash the import was prepared from.
    pub prepared_revisions: Option<(u64, u64)>,
    pub file_tag_snapshot: Option<crate::import::file_tag_snapshot::FileTagSnapshot>,
}

/// One candidate's stored state.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidatePreparation {
    /// `CategorizedFiles::content_hash` — the row's identity.
    pub content_hash: String,
    /// Where the candidate was last seen on disk. Not identity — the hash is.
    pub folder_path: String,
    /// The person's decisions about the folder's files, and the revision
    /// every write against this file shape is checked against.
    pub file_edits: CandidateFileEdits,
    /// Moves with every change to `metadata`, so a source projection cannot
    /// overwrite a newer edit.
    pub metadata_revision: u64,
    pub author: MetadataAuthor,
    /// The draft, its provenance, its cover, and the provider answers
    /// prepared for it.
    pub metadata: CandidateMetadataDraft,
    /// Whether `metadata.assets` is a complete answer set for this draft: an
    /// image answer for every artist it needs, and bytes for a remote cover.
    /// A candidate stored before assets were prepared has none, and must have
    /// its source applied again before it can import.
    pub assets_prepared: bool,
    /// What identification concluded, or `None` when nothing has — including
    /// after a file decision cleared a verdict about a shape that is gone.
    pub identification: Option<DbCandidateIdentifyResult>,
    /// The signals identification settled on, or `None` when nothing has
    /// extracted them.
    pub signals: Option<Signals>,
}

impl CandidatePreparation {
    /// The Discogs artists whose image answers this draft needs: the ones the
    /// source credits, plus every new artist with a Discogs identity on the
    /// album or on a track that commits.
    pub fn required_discogs_artist_ids(&self) -> BTreeSet<String> {
        self.metadata
            .source_discogs_artist_ids
            .union(
                &self
                    .metadata
                    .draft
                    .release_edit()
                    .new_discogs_artist_ids_for_bound_tracks(),
            )
            .cloned()
            .collect()
    }

    /// The contradictions no stored candidate may hold, named so a save can
    /// refuse them before any row moves.
    pub fn validate(&self) -> Result<(), String> {
        match (&self.metadata.provenance, self.author) {
            (None, MetadataAuthor::Nobody)
            | (Some(_), MetadataAuthor::User | MetadataAuthor::Identification) => {}
            (None, author) => {
                return Err(format!(
                    "candidate {} has no metadata provenance but names {author:?} as its author",
                    self.content_hash
                ))
            }
            (Some(_), MetadataAuthor::Nobody) => {
                return Err(format!(
                    "candidate {} has metadata provenance but no author",
                    self.content_hash
                ))
            }
        }
        if let (Some(CoverSelection::Local(_) | CoverSelection::Embedded(_)) | None, Some(_)) =
            (&self.metadata.cover, &self.metadata.assets.remote_cover)
        {
            return Err("candidate remote-cover bytes have no remote cover selection".into());
        }
        if self.assets_prepared {
            if let (Some(CoverSelection::Remote(_, _)), None) =
                (&self.metadata.cover, &self.metadata.assets.remote_cover)
            {
                return Err("a remote candidate cover has no prepared bytes".into());
            }
            let expected = self.required_discogs_artist_ids();
            let actual: BTreeSet<_> = self
                .metadata
                .assets
                .artist_images
                .iter()
                .map(|asset| asset.discogs_artist_id().to_string())
                .collect();
            if actual.len() != self.metadata.assets.artist_images.len() {
                return Err("candidate artist assets contain a duplicate Discogs artist ID".into());
            }
            if actual != expected {
                return Err(format!(
                    "candidate artist assets do not match the draft's Discogs artist IDs: \
                     expected {expected:?}, got {actual:?}"
                ));
            }
        }
        Ok(())
    }
}

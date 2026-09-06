//! The one writer of a candidate's stored state.
//!
//! Every change to what a candidate is — identification's conclusion, a
//! source a person applied, a field they typed, a file decision — comes
//! through here. Each operation loads the candidate whole, applies the rule
//! that governs it in Rust, and saves it whole against the revisions it
//! loaded. The database beneath knows how to store the value and nothing
//! about when it may change.

mod pane_edits;

use crate::db::{
    CandidateSaveExpectation, CandidateSaveExtras, CandidateSaved, Database,
    DbCandidateIdentifyResult, NewImportCandidateVerdict, ScannedCandidateKey,
};
use crate::import::folder_scanner::CandidateFileEdits;
use crate::import::preparation::CandidatePreparation;
use crate::import::MetadataAuthor;
use crate::library::LibraryError;
use std::collections::HashMap;

/// The writer. Held by `LibraryManager` beside the database it writes
/// through; the import handle resolves keys, holds the commit lock, and
/// prepares provider answers, then hands the change here.
#[derive(Clone)]
pub struct CandidatePreparations {
    database: Database,
}

impl CandidatePreparations {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    /// Record one candidate's terminal identify verdict, keyed by
    /// `content_hash`. Never synced. `identified_at` is stamped here from the
    /// injected clock, not taken from `verdict` — see
    /// [`NewImportCandidateVerdict`]'s doc.
    ///
    /// The candidate's file decisions are left as they are. Discovery creates
    /// the candidate row; a verdict lands on that row, so it cannot recreate
    /// a candidate removed while identification ran.
    ///
    /// A pick identification made belongs to the verdict that made it, so
    /// this write replaces it — with the new verdict's own conclusion, or
    /// with nothing when it concluded none. A pick a person made is theirs
    /// and is left exactly as it is: a run whose signals turn up nothing says
    /// nothing about a release they chose.
    ///
    /// `false` when the row has moved past the file decisions or the draft
    /// this verdict was derived from, or when it names a candidate no row
    /// holds: either way there is nothing to write.
    pub async fn store_verdict(
        &self,
        verdict: &NewImportCandidateVerdict,
    ) -> Result<bool, LibraryError> {
        let Some(mut prep) = self
            .database
            .load_candidate_preparation(&verdict.content_hash)
            .await?
        else {
            return Ok(false);
        };
        if prep.file_edits.revision != verdict.expected_edit_revision
            || prep.metadata_revision != verdict.expected_metadata_revision
        {
            return Ok(false);
        }
        let expected = CandidateSaveExpectation {
            edit_revision: prep.file_edits.revision,
            metadata_revision: prep.metadata_revision,
            scanned: None,
        };
        prep.folder_path = verdict.folder_path.clone();
        prep.identification = Some(DbCandidateIdentifyResult {
            verdict: verdict.verdict.clone(),
            probed_total_duration_ms: verdict.signals.probed_total_duration_ms(),
            identified_at: self.database.now(),
        });
        prep.signals = Some(verdict.signals.clone());
        if prep.author != crate::import::MetadataAuthor::User {
            prep.author = match verdict.metadata.provenance {
                Some(_) => crate::import::MetadataAuthor::Identification,
                None => crate::import::MetadataAuthor::Nobody,
            };
            prep.metadata = verdict.metadata.clone();
            prep.assets_prepared = true;
            prep.metadata_revision += 1;
        }
        Ok(matches!(
            self.database
                .save_candidate_preparation(prep, expected, CandidateSaveExtras::default())
                .await?,
            CandidateSaved::Landed(_)
        ))
    }

    /// Record one candidate's user-set file decisions, **and clear whatever
    /// identification had concluded about it**, in one transaction.
    ///
    /// The two are one operation, not two: binding a sheet or taking a file out
    /// of the tracklist changes what the folder is — a one-track image becomes
    /// a twelve-track disc, and its disc ID becomes computable — so the stored
    /// verdict was derived from a shape that no longer exists. Writing the
    /// decision without clearing it would leave the queue believing an answer
    /// to a question that changed.
    ///
    /// A pick identification concluded from that verdict goes with it, for the
    /// same reason. A pick a person made stays: their choice names a release,
    /// not a shape, and the draft re-derives against the reshaped folder.
    ///
    /// The content hash covers files, never role decisions, so this addresses
    /// the same row the verdict lived in rather than orphaning it — and the
    /// scanned candidates that share the hash have their file rows rewritten
    /// to the settled shape in the same transaction.
    pub(crate) async fn store_file_decisions(
        &self,
        content_hash: &str,
        folder_path: &str,
        expected_revision: u64,
        expected_metadata_revision: u64,
        edits: &CandidateFileEdits,
        settled_candidates: &[(String, crate::import::folder_scanner::CategorizedFiles)],
        mapping_preparation: &crate::import::CandidateMappingPreparation,
    ) -> Result<(u64, Vec<crate::import::folder_scanner::FolderCandidate>), LibraryError> {
        let next_revision = expected_revision.checked_add(1).ok_or_else(|| {
            crate::library::LibraryError::Import(
                "candidate edit revision exhausted the u64 range".to_string(),
            )
        })?;
        let mut prep = self
            .database
            .load_candidate_preparation(content_hash)
            .await?
            .ok_or_else(|| {
                crate::library::LibraryError::Import(format!(
                    "candidate file decisions changed at revision {expected_revision}"
                ))
            })?;
        if prep.file_edits.revision != expected_revision {
            return Err(crate::library::LibraryError::Import(format!(
                "candidate file decisions changed at revision {expected_revision}"
            )));
        }
        if prep.metadata_revision != expected_metadata_revision {
            return Err(crate::library::LibraryError::Import(format!(
                "candidate metadata changed from revision {expected_metadata_revision}"
            )));
        }
        if !prep.assets_prepared {
            return Err(crate::library::LibraryError::Import(format!(
                "candidate {content_hash} has no complete prepared asset set"
            )));
        }
        let expected = CandidateSaveExpectation {
            edit_revision: expected_revision,
            metadata_revision: expected_metadata_revision,
            scanned: None,
        };
        prep.folder_path = folder_path.to_string();
        prep.file_edits = edits.clone();
        prep.file_edits.revision = next_revision;
        // The verdict described a shape that is gone, and so did the pick
        // identification made from it and the signals it read. A person's
        // pick names a release, not a shape, and stays.
        prep.identification = None;
        prep.signals = None;
        let keep_pick = prep.author == crate::import::MetadataAuthor::User;
        if !keep_pick {
            prep.metadata.provenance = None;
            prep.author = crate::import::MetadataAuthor::Nobody;
        }
        prep.metadata.draft = mapping_preparation.draft.clone();
        prep.metadata.source_discogs_artist_ids = if keep_pick {
            mapping_preparation.source_discogs_artist_ids.clone()
        } else {
            Default::default()
        };
        // The prepared answers were made for the draft before it was redrawn;
        // every artist the redrawn draft needs must be among them, and the
        // ones it no longer needs go.
        let required = prep.required_discogs_artist_ids();
        let by_id: HashMap<_, _> = mapping_preparation
            .artist_images
            .iter()
            .map(|asset| (asset.discogs_artist_id(), asset))
            .collect();
        if let Some(missing) = required.iter().find(|id| !by_id.contains_key(id.as_str())) {
            return Err(crate::library::LibraryError::Import(format!(
                "candidate file edit has no prepared image answer for Discogs artist {missing}"
            )));
        }
        prep.metadata.assets.artist_images = mapping_preparation
            .artist_images
            .iter()
            .filter(|asset| required.contains(asset.discogs_artist_id()))
            .cloned()
            .collect();
        let extras = CandidateSaveExtras {
            file_tag_snapshot: None,
            reshaped_files: Some(settled_candidates.to_vec()),
        };
        match self
            .database
            .save_candidate_preparation(prep, expected, extras)
            .await?
        {
            CandidateSaved::Landed(candidates) => Ok((next_revision, candidates)),
            CandidateSaved::Superseded => Err(crate::library::LibraryError::Import(format!(
                "candidate file decisions changed at revision {expected_revision}"
            ))),
        }
    }

    /// Replace the candidate's draft and its provenance as one transaction,
    /// carrying the stored rows' file decisions onto the new tracks. File
    /// decisions about the folder itself live in other tables and are
    /// deliberately untouched.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn replace_metadata(
        &self,
        content_hash: &str,
        folder_path: &str,
        draft: &crate::import::RawReleaseEdit,
        provenance: Option<&crate::import::MetadataProvenance>,
    ) -> Result<u64, LibraryError> {
        let prep = self
            .database
            .load_candidate_preparation(content_hash)
            .await?
            .ok_or_else(|| {
                crate::library::LibraryError::Import(
                    "metadata replacement has no candidate state row".into(),
                )
            })?;
        let mut draft = crate::import::pane::candidate_draft_from_edit(draft.clone()).draft;
        draft.tracks =
            super::edits::preserve_track_decisions(draft.tracks, &prep.metadata.draft.tracks);
        let metadata = crate::import::CandidateMetadataDraft {
            draft,
            source_discogs_artist_ids: Default::default(),
            provenance: provenance.cloned(),
            cover: None,
            assets: crate::import::CandidatePreparedAssets::default(),
        };
        self.apply_metadata(prep, None, folder_path, metadata, None)
            .await
    }

    pub async fn apply_source(
        &self,
        watched_folder_path: &str,
        content_hash: &str,
        folder_path: &str,
        expected_file_edit_revision: u64,
        expected_revision: u64,
        metadata: &crate::import::CandidateMetadataDraft,
    ) -> Result<u64, LibraryError> {
        let prep = self
            .loaded_at(content_hash, expected_file_edit_revision, expected_revision)
            .await?;
        let scanned = ScannedCandidateKey {
            watched_folder_path: watched_folder_path.to_string(),
            candidate_path: folder_path.to_string(),
        };
        self.apply_metadata(prep, Some(scanned), folder_path, metadata.clone(), None)
            .await
    }

    /// Store the exact File Tags reading and replace the candidate metadata it
    /// projects in one transaction. The scan stamp is checked inside that
    /// transaction, so no draft can be committed from facts about an older
    /// candidate shape.
    pub(crate) async fn apply_file_tags(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        content_hash: &str,
        expected_file_edit_revision: u64,
        expected_metadata_revision: u64,
        snapshot: &crate::import::file_tag_snapshot::FileTagSnapshot,
        draft: &crate::import::CandidateDraft,
        cover: Option<&crate::import::CoverSelection>,
    ) -> Result<u64, LibraryError> {
        let prep = self
            .loaded_at(
                content_hash,
                expected_file_edit_revision,
                expected_metadata_revision,
            )
            .await?;
        let scanned = ScannedCandidateKey {
            watched_folder_path: watched_folder_path.to_string(),
            candidate_path: candidate_path.to_string(),
        };
        let metadata = crate::import::CandidateMetadataDraft {
            draft: draft.clone(),
            source_discogs_artist_ids: Default::default(),
            provenance: Some(crate::import::MetadataProvenance::FileTags),
            cover: cover.cloned(),
            assets: crate::import::CandidatePreparedAssets::default(),
        };
        self.apply_metadata(
            prep,
            Some(scanned),
            candidate_path,
            metadata,
            Some(snapshot.clone()),
        )
        .await
    }

    /// The candidate at exactly the revisions a source projection was
    /// prepared against, or the refusal naming which one moved.
    async fn loaded_at(
        &self,
        content_hash: &str,
        expected_file_edit_revision: u64,
        expected_metadata_revision: u64,
    ) -> Result<CandidatePreparation, LibraryError> {
        let prep = self
            .database
            .load_candidate_preparation(content_hash)
            .await?
            .ok_or_else(|| {
                crate::library::LibraryError::Import("candidate metadata row is missing".into())
            })?;
        if prep.file_edits.revision != expected_file_edit_revision {
            return Err(crate::library::LibraryError::Import(format!(
                "candidate changed before its metadata was stored: its files moved past \
                 revision {expected_file_edit_revision}"
            )));
        }
        if prep.metadata_revision != expected_metadata_revision {
            return Err(crate::library::LibraryError::Import(format!(
                "candidate metadata changed from revision {expected_metadata_revision}"
            )));
        }
        Ok(prep)
    }

    /// A source's projection becomes the candidate's metadata: the person
    /// applied it, so they are its author, and its answers are complete.
    async fn apply_metadata(
        &self,
        mut prep: CandidatePreparation,
        scanned: Option<ScannedCandidateKey>,
        folder_path: &str,
        metadata: crate::import::CandidateMetadataDraft,
        file_tag_snapshot: Option<crate::import::file_tag_snapshot::FileTagSnapshot>,
    ) -> Result<u64, LibraryError> {
        let expected = CandidateSaveExpectation {
            edit_revision: prep.file_edits.revision,
            metadata_revision: prep.metadata_revision,
            scanned,
        };
        prep.folder_path = folder_path.to_string();
        prep.author = match metadata.provenance {
            Some(_) => MetadataAuthor::User,
            None => MetadataAuthor::Nobody,
        };
        prep.metadata = metadata;
        prep.assets_prepared = true;
        prep.metadata_revision += 1;
        let revision = prep.metadata_revision;
        let extras = CandidateSaveExtras {
            file_tag_snapshot,
            reshaped_files: None,
        };
        match self
            .database
            .save_candidate_preparation(prep, expected, extras)
            .await?
        {
            CandidateSaved::Landed(_) => Ok(revision),
            CandidateSaved::Superseded => Err(crate::library::LibraryError::Import(
                "candidate changed before its metadata was stored".into(),
            )),
        }
    }
}

//! A candidate's stored state, as one value ([`CandidatePreparation`]), and
//! its one writer.
//!
//! Every change to what a candidate is — identification's conclusion, a
//! source a person applied, a field they typed, a file decision — comes
//! through here. Each operation loads the candidate whole, applies the rule
//! that governs it in Rust, and saves it whole against the revisions it
//! loaded. The database beneath knows how to store the value and nothing
//! about when it may change.

mod pane_edits;
mod preparation;

pub(crate) use preparation::{stored_preparation, CandidateWrite};
pub use preparation::{CandidateAsRead, CandidatePreparation, MetadataAuthor};

use crate::folder_scanner::CandidateFileEdits;
use crate::tables::{
    CandidatePaneWrite, CandidateSaveExpectation, CandidateSaveExtras, CandidateSaved,
    ScannedCandidateKey,
};
use crate::tables::{DbCandidateIdentifyResult, NewImportCandidateVerdict};
use crate::ImportError;
use bae_db::Database;

/// The writer, over the database it writes through. The import service
/// holds one; the import handle resolves keys, holds the commit lock, and
/// fetches the cover bytes a change needs, then hands the change here.
#[derive(Clone)]
pub struct CandidatePreparations {
    tables: crate::tables::ImportTables,
}

impl CandidatePreparations {
    pub fn new(database: Database) -> Self {
        Self { tables: crate::tables::ImportTables::new(database) }
    }

    /// One candidate's whole stored state, or `None` for a hash no scan has
    /// stored a candidate under. A candidate whose files this device imported
    /// holds no draft, so loading it fails as an import of those files does.
    async fn load_stored(
        &self,
        content_hash: &str,
    ) -> Result<Option<CandidatePreparation>, ImportError> {
        stored_preparation(&self.tables, content_hash).await
    }

    /// Record one candidate's terminal verdict under its content hash, stamped
    /// from the injected clock. A verdict that picked one release unattended
    /// links the candidate to it and replaces the draft; one that picked none
    /// leaves the link and the draft alone.
    ///
    /// Nothing lands when the row's file decisions moved past the ones the
    /// verdict answers, or no row holds the candidate. A draft edited since
    /// the run began is replaced like any other: the later write wins.
    ///
    /// A verdict `run` reached that lands ends that run's identification of
    /// the candidate at `candidate_key` in its own transaction, and returns
    /// what that did: no read sees the verdict with the run still going, or
    /// the run ended without the verdict. `None` when the verdict did not
    /// land, which ends nothing.
    pub(crate) async fn store_run_verdict(
        &self,
        verdict: &NewImportCandidateVerdict,
        candidate_key: &crate::CandidateKey,
        run: crate::IdentifyRunId,
    ) -> Result<Option<crate::candidate_runtime::IdentificationMove>, ImportError> {
        let Some((prep, expected, extras)) = self.verdict_save(verdict).await? else {
            return Ok(None);
        };
        Ok(
            match self
                .tables
                .save_run_verdict(prep, expected, extras, candidate_key.clone(), run)
                .await?
            {
                crate::tables::RunVerdictSaved::Landed(ended) => Some(ended),
                crate::tables::RunVerdictSaved::Superseded => None,
            },
        )
    }

    /// Record a verdict no run of this process reached, as a seeded library
    /// or a test stores one: there is no run for its write to end.
    #[cfg(any(test, feature = "test-utils", debug_assertions))]
    pub async fn store_verdict(
        &self,
        verdict: &NewImportCandidateVerdict,
    ) -> Result<bool, ImportError> {
        let Some((prep, expected, extras)) = self.verdict_save(verdict).await? else {
            return Ok(false);
        };
        Ok(matches!(
            self.tables.save_candidate_preparation(prep, expected, extras).await?,
            CandidateSaved::Landed(_)
        ))
    }

    /// The candidate's whole state with `verdict` recorded, ready to save
    /// against the revisions it was loaded at; `None` when no row holds the
    /// candidate or its file decisions moved past the ones the verdict
    /// answers.
    async fn verdict_save(
        &self,
        verdict: &NewImportCandidateVerdict,
    ) -> Result<
        Option<(CandidatePreparation, CandidateSaveExpectation, CandidateSaveExtras)>,
        ImportError,
    > {
        let Some(mut prep) = self.load_stored(&verdict.content_hash).await? else {
            return Ok(None);
        };
        if prep.file_edits.revision != verdict.file_edit_revision {
            return Ok(None);
        }
        let expected = CandidateSaveExpectation {
            edit_revision: prep.file_edits.revision,
            metadata_revision: prep.metadata_revision,
            scanned: None,
        };
        prep.candidate_key = verdict.candidate_key.clone();
        prep.identification = Some(DbCandidateIdentifyResult {
            verdict: verdict.verdict.clone(),
            kept_own_draft: false,
        });
        prep.signals = Some(verdict.signals.clone());
        if let Some(pick) = &verdict.pick {
            prep.author = MetadataAuthor::Identification;
            prep.link = Some(crate::ReleaseLink::Pressing(pick.link.clone()));
            let mut metadata = pick.metadata.clone();
            settle_cover(&mut metadata, &mut prep.metadata);
            prep.metadata = metadata;
            prep.metadata_revision += 1;
        }
        // A run that applied its own pick and whose release passed every check
        // against the folder has left only the draft and its Import to see, so
        // the pane opens there. Otherwise the pane stays where it was.
        let pane = if verdict.pick.is_some() {
            CandidatePaneWrite::OpenOnDraftUnlessCheckFailed
        } else {
            CandidatePaneWrite::Keep
        };
        let extras = CandidateSaveExtras {
            pane,
            releases: verdict.releases.clone(),
            ..CandidateSaveExtras::default()
        };
        Ok(Some((prep, expected, extras)))
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
    /// Applied metadata, its provenance, its author and the release link
    /// remain. The caller's `draft` replaces only tracks whose audio changed,
    /// from the folder's file tags: which tracks exist is the decision, not
    /// what the release is called; unaffected tracks keep their identities
    /// and metadata. A draft identification wrote stays identification's, so with
    /// its verdict cleared it waits for the next run's checks rather than
    /// passing as the person's answer.
    ///
    /// The content hash covers files, never decisions, so this addresses
    /// the same row the verdict lived in rather than orphaning it — and the
    /// scanned candidates that share the hash have their file rows rewritten
    /// to the settled shape in the same transaction.
    pub(crate) async fn store_file_decisions(
        &self,
        read: &CandidateAsRead,
        candidate_key: &crate::CandidateKey,
        edits: &CandidateFileEdits,
        settled_candidates: &[(crate::CandidateKey, crate::folder_scanner::CategorizedFiles)],
        draft: crate::CandidateDraft,
    ) -> Result<(u64, Vec<crate::folder_scanner::FolderCandidate>), ImportError> {
        let next_revision = read.file_edit_revision.checked_add(1).ok_or_else(|| {
            ImportError::Refused("candidate edit revision exhausted the u64 range".to_string())
        })?;
        let mut prep = self.load_stored(&read.content_hash).await?.ok_or_else(|| {
            CandidateAsRead::files_moved(read.file_edit_revision, CandidateWrite::FileDecisions)
        })?;
        read.verify(&prep, CandidateWrite::FileDecisions)?;
        let expected = CandidateSaveExpectation {
            edit_revision: read.file_edit_revision,
            metadata_revision: read.metadata_revision,
            scanned: None,
        };
        prep.candidate_key = candidate_key.clone();
        prep.file_edits = edits.clone();
        prep.file_edits.revision = next_revision;
        // Identification and signals described the prior audio shape. The
        // applied metadata and its source are independent of that verdict.
        prep.identification = None;
        prep.signals = None;
        prep.metadata.draft = draft;
        let extras = CandidateSaveExtras {
            reshaped_files: Some(settled_candidates.to_vec()),
            ..CandidateSaveExtras::default()
        };
        match self.tables.save_candidate_preparation(prep, expected, extras).await? {
            CandidateSaved::Landed(candidates) => Ok((next_revision, candidates)),
            CandidateSaved::Superseded => Err(CandidateAsRead::files_moved(
                read.file_edit_revision,
                CandidateWrite::FileDecisions,
            )),
        }
    }

    /// Replace the candidate's draft and its provenance as one transaction,
    /// laying the new tracks over the stored rows' audio, and storing the
    /// release the provenance names from `releases`. File decisions about the
    /// folder itself and the release link are untouched.
    #[cfg(any(test, feature = "test-utils", debug_assertions))]
    pub async fn replace_metadata(
        &self,
        content_hash: &str,
        candidate_key: &crate::CandidateKey,
        draft: &crate::RawReleaseEdit,
        provenance: Option<&crate::MetadataProvenance>,
        releases: &[bae_catalogs::source_release::SourceRelease],
    ) -> Result<u64, ImportError> {
        let prep = self.load_stored(content_hash).await?.ok_or_else(|| {
            ImportError::Refused("metadata replacement has no candidate state row".into())
        })?;
        let draft = super::pane::edit_over_audio(draft.clone(), &prep.metadata.draft)?;
        let metadata =
            crate::CandidateMetadataDraft { draft, provenance: provenance.cloned(), cover: None };
        self.apply_metadata(prep, None, candidate_key, metadata, None, releases.to_vec()).await
    }

    /// Link the candidate to `link`, or to nothing, leaving everything else
    /// as it is: the state a pick or an unlink leaves the link in, without
    /// fetching or reading any release. The releases the link names come
    /// from `releases` where nothing stores them yet.
    #[cfg(any(test, feature = "test-utils", debug_assertions))]
    pub async fn replace_link(
        &self,
        content_hash: &str,
        link: Option<&crate::ReleaseLink>,
        releases: &[bae_catalogs::source_release::SourceRelease],
    ) -> Result<(), ImportError> {
        let mut prep = self.load_stored(content_hash).await?.ok_or_else(|| {
            ImportError::Refused("link replacement has no candidate state row".into())
        })?;
        prep.link = link.cloned();
        let expected = CandidateSaveExpectation {
            edit_revision: prep.file_edits.revision,
            metadata_revision: prep.metadata_revision,
            scanned: None,
        };
        let extras =
            CandidateSaveExtras { releases: releases.to_vec(), ..CandidateSaveExtras::default() };
        match self.tables.save_candidate_preparation(prep, expected, extras).await? {
            CandidateSaved::Landed(_) => Ok(()),
            CandidateSaved::Superseded => Err(ImportError::Refused(format!(
                "{content_hash} changed while its link was replaced"
            ))),
        }
    }

    pub async fn apply_source(
        &self,
        watched_folder_path: &std::path::Path,
        read: &CandidateAsRead,
        candidate_key: &crate::CandidateKey,
        metadata: &crate::CandidateMetadataDraft,
    ) -> Result<u64, ImportError> {
        let prep = self.loaded_at(read).await?;
        let scanned = ScannedCandidateKey {
            watched_folder_path: watched_folder_path.to_path_buf(),
            candidate_path: candidate_key.clone(),
        };
        self.apply_metadata(prep, Some(scanned), candidate_key, metadata.clone(), None, Vec::new())
            .await
    }

    /// A person's pick: the candidate is linked to `link`, the draft read
    /// from it becomes the candidate's metadata, and — where the candidate
    /// has no result yet — `settled_by_choice` becomes its result, in the
    /// same write.
    ///
    /// A person's chosen release is stored as a result like a run's, unless a
    /// run's result already stands. The read and the write share this load.
    /// `releases` are the pick's releases as read, primary and partners,
    /// stored in the same write.
    #[expect(
        clippy::too_many_arguments,
        reason = "one write's inputs, each a different part of the pick"
    )]
    pub(crate) async fn apply_pick(
        &self,
        watched_folder_path: &std::path::Path,
        read: &CandidateAsRead,
        candidate_key: &crate::CandidateKey,
        link: &bae_identify::PressingLink,
        metadata: &crate::CandidateMetadataDraft,
        settled_by_choice: bae_identify::TerminalVerdict,
        releases: Vec<bae_catalogs::source_release::SourceRelease>,
    ) -> Result<u64, ImportError> {
        let mut prep = self.loaded_at(read).await?;
        prep.link = Some(crate::ReleaseLink::Pressing(link.clone()));
        let scanned = ScannedCandidateKey {
            watched_folder_path: watched_folder_path.to_path_buf(),
            candidate_path: candidate_key.clone(),
        };
        // A pick answers the folder, so the person keeping their own draft
        // over the verdict stands no more.
        match &mut prep.identification {
            Some(identification) => identification.kept_own_draft = false,
            None => {
                prep.identification = Some(DbCandidateIdentifyResult {
                    verdict: settled_by_choice,
                    kept_own_draft: false,
                })
            }
        }
        self.apply_metadata(prep, Some(scanned), candidate_key, metadata.clone(), None, releases)
            .await
    }

    /// The person could not tell which of the pressings the stored verdict
    /// offered their copy is: the candidate is linked to the album those
    /// pressings are of, and `draft` — the draft with what they agree on in
    /// it — replaces the draft, in one write. What the draft was read from
    /// stays as it is: the fields the pressings disagree on are still read
    /// from there. `releases` are the pressings as the person's choice read
    /// them, which replace the copies stored for the verdict's rows where
    /// they read more.
    pub(crate) async fn apply_shared_album(
        &self,
        watched_folder_path: &std::path::Path,
        read: &CandidateAsRead,
        candidate_key: &crate::CandidateKey,
        link: &bae_identify::AlbumLink,
        draft: crate::CandidateDraft,
        releases: Vec<bae_catalogs::source_release::SourceRelease>,
    ) -> Result<u64, ImportError> {
        let mut prep = self.loaded_at(read).await?;
        // Linking the album answers the folder, so the person keeping their
        // own draft over the verdict stands no more.
        let identification = prep.identification.as_mut().ok_or_else(|| {
            ImportError::Refused(format!(
                "{} has no lookup whose pressings to link the album of",
                read.content_hash
            ))
        })?;
        identification.kept_own_draft = false;
        let expected = CandidateSaveExpectation {
            edit_revision: prep.file_edits.revision,
            metadata_revision: prep.metadata_revision,
            scanned: Some(ScannedCandidateKey {
                watched_folder_path: watched_folder_path.to_path_buf(),
                candidate_path: candidate_key.clone(),
            }),
        };
        prep.link = Some(crate::ReleaseLink::Album(link.clone()));
        prep.candidate_key = candidate_key.clone();
        prep.author = MetadataAuthor::Person;
        prep.metadata.draft = draft;
        prep.metadata_revision += 1;
        let revision = prep.metadata_revision;
        // Linking the album is the person's answer: the candidate's
        // identification ends in the same write.
        let extras = CandidateSaveExtras {
            ends_identification: true,
            releases,
            ..CandidateSaveExtras::default()
        };
        match self.tables.save_candidate_preparation(prep, expected, extras).await? {
            CandidateSaved::Landed(_) => Ok(revision),
            CandidateSaved::Superseded => Err(ImportError::Refused(format!(
                "{} changed while its album was being linked",
                read.content_hash
            ))),
        }
    }

    /// The person keeps their own draft over what the stored verdict offered:
    /// none of its releases is the folder's, or it found none. The candidate
    /// is linked to no release; the draft stays as it is.
    pub(crate) async fn keep_own_draft(&self, read: &CandidateAsRead) -> Result<u64, ImportError> {
        let prep = self.loaded_at(read).await?;
        if prep.identification.is_none() {
            return Err(ImportError::Refused(format!(
                "{} has no lookup whose offer to set aside",
                read.content_hash
            )));
        }
        self.set_link_aside(prep, read).await
    }

    /// The person unlinks the candidate from its release. The draft stays as
    /// it is, and a stored lookup stands set aside, so the candidate is
    /// unmatched rather than waiting on the lookup's offer again.
    pub(crate) async fn unlink(&self, read: &CandidateAsRead) -> Result<u64, ImportError> {
        let prep = self.loaded_at(read).await?;
        self.set_link_aside(prep, read).await
    }

    async fn set_link_aside(
        &self,
        mut prep: CandidatePreparation,
        read: &CandidateAsRead,
    ) -> Result<u64, ImportError> {
        prep.link = None;
        if let Some(identification) = prep.identification.as_mut() {
            identification.kept_own_draft = true;
        }
        let expected = CandidateSaveExpectation {
            edit_revision: prep.file_edits.revision,
            metadata_revision: prep.metadata_revision,
            scanned: None,
        };
        let revision = prep.metadata_revision;
        match self
            .tables
            .save_candidate_preparation(prep, expected, CandidateSaveExtras::default())
            .await?
        {
            CandidateSaved::Landed(_) => Ok(revision),
            CandidateSaved::Superseded => Err(ImportError::Refused(format!(
                "{} changed while its release link was being set aside",
                read.content_hash
            ))),
        }
    }

    /// Store the exact file metadata reading and replace the candidate metadata it
    /// projects in one transaction. The scan stamp is checked inside that
    /// transaction, so no draft can be committed from facts about an older
    /// candidate shape.
    pub(crate) async fn apply_file_metadata(
        &self,
        watched_folder_path: &std::path::Path,
        candidate_path: &crate::CandidateKey,
        read: &CandidateAsRead,
        snapshot: &crate::file_tag_snapshot::FileTagSnapshot,
        draft: &crate::CandidateDraft,
        cover: Option<&crate::cover_art::PreparedCover>,
    ) -> Result<u64, ImportError> {
        let prep = self.loaded_at(read).await?;
        let scanned = ScannedCandidateKey {
            watched_folder_path: watched_folder_path.to_path_buf(),
            candidate_path: candidate_path.clone(),
        };
        let metadata = crate::CandidateMetadataDraft {
            draft: draft.clone(),
            provenance: Some(crate::MetadataProvenance::FileMetadata),
            cover: cover.cloned(),
        };
        self.apply_metadata(
            prep,
            Some(scanned),
            candidate_path,
            metadata,
            Some(snapshot.clone()),
            Vec::new(),
        )
        .await
    }

    /// The candidate at exactly the revisions a source projection was
    /// prepared against, or the refusal naming which one moved.
    async fn loaded_at(&self, read: &CandidateAsRead) -> Result<CandidatePreparation, ImportError> {
        let prep = self
            .load_stored(&read.content_hash)
            .await?
            .ok_or_else(|| ImportError::Refused("candidate metadata row is missing".into()))?;
        read.verify(&prep, CandidateWrite::Metadata)?;
        Ok(prep)
    }

    /// A source's projection becomes the candidate's metadata: the person
    /// applied it — a pick, their files' tags, or a cleared draft — so they
    /// are its author. The release link is the caller's to set; the draft
    /// never changes it. `releases` are the catalog releases the link and the
    /// provenance may name, as read, stored in the same write.
    async fn apply_metadata(
        &self,
        mut prep: CandidatePreparation,
        scanned: Option<ScannedCandidateKey>,
        candidate_key: &crate::CandidateKey,
        mut metadata: crate::CandidateMetadataDraft,
        file_tag_snapshot: Option<crate::file_tag_snapshot::FileTagSnapshot>,
        releases: Vec<bae_catalogs::source_release::SourceRelease>,
    ) -> Result<u64, ImportError> {
        settle_cover(&mut metadata, &mut prep.metadata);
        let expected = CandidateSaveExpectation {
            edit_revision: prep.file_edits.revision,
            metadata_revision: prep.metadata_revision,
            scanned,
        };
        prep.candidate_key = candidate_key.clone();
        prep.author = MetadataAuthor::Person;
        prep.metadata = metadata;
        prep.metadata_revision += 1;
        let revision = prep.metadata_revision;
        // The person's source is their answer: the candidate's
        // identification ends in the same write.
        let extras = CandidateSaveExtras {
            file_tag_snapshot,
            reshaped_files: None,
            pane: CandidatePaneWrite::Keep,
            ends_identification: true,
            releases,
        };
        match self.tables.save_candidate_preparation(prep, expected, extras).await? {
            CandidateSaved::Landed(_) => Ok(revision),
            CandidateSaved::Superseded => {
                Err(ImportError::Refused("candidate changed before its metadata was stored".into()))
            }
        }
    }
}

/// Settle the cover a metadata application leaves the candidate with.
///
/// `metadata`'s cover is the image the source itself brought. A source that
/// brought none says nothing about the cover, so the candidate keeps the one
/// `stored` has, whatever it is: the `cover.jpg` beside the audio, the
/// artwork its tags embed, or the image an earlier pick fetched, with the
/// bytes it was fetched with.
fn settle_cover(
    metadata: &mut crate::CandidateMetadataDraft,
    stored: &mut crate::CandidateMetadataDraft,
) {
    if metadata.cover.is_none() {
        metadata.cover = stored.cover.take();
    }
}

#[cfg(test)]
#[path = "preparations_tests.rs"]
mod tests;

//! The one writer of a candidate's stored state.
//!
//! Every change to what a candidate is — identification's conclusion, a
//! source a person applied, a field they typed, a file decision — comes
//! through here. Each operation loads the candidate whole, applies the rule
//! that governs it in Rust, and saves it whole against the revisions it
//! loaded. The database beneath knows how to store the value and nothing
//! about when it may change.

mod pane_edits;
mod reset;

use crate::db::{
    CandidateLookupUpdate, CandidatePaneWrite, CandidateResultWrite, CandidateSaveExpectation, CandidateSaveExtras,
    CandidateSaved, CandidateScanExpectation, Database, DbCandidateIdentifyResult,
    NewImportCandidateVerdict, ScannedCandidateKey,
};
use crate::import::folder_scanner::CandidateFileEdits;
use crate::import::preparation::{CandidateAsRead, CandidatePreparation, CandidateWrite};
use crate::import::MetadataAuthor;
use crate::library::LibraryError;
use std::collections::HashMap;

/// The writer. Held by `LibraryManager` beside the database it writes
/// through; the import handle resolves keys, holds the commit lock, and
/// prepares provider answers, then hands the change here.
#[derive(Clone)]
pub struct CandidatePreparations {
    database: Database,
    /// The candidates a person has open, which a result identification
    /// stores for is read on arrival.
    open: crate::import::OpenCandidates,
}

impl CandidatePreparations {
    pub fn new(database: Database) -> Self {
        Self {
            database,
            open: crate::import::OpenCandidates::default(),
        }
    }

    /// Hold `candidate_key`'s pane open: the result it has now is marked read,
    /// and every result identification stores for it while the returned
    /// guard lives is stored read.
    ///
    /// The key is registered before the mark is written, so a result stored
    /// in between is one the mark clears. A mark that fails lets go of the
    /// key again and says so.
    pub(crate) async fn open_candidate(
        &self,
        candidate_key: &str,
    ) -> Result<crate::import::OpenCandidate, LibraryError> {
        let opened = self.open.open(candidate_key);
        self.database
            .mark_candidate_result_read(candidate_key)
            .await?;
        Ok(opened)
    }

    /// Record one candidate's terminal identify verdict, keyed by the
    /// candidate's content hash. Never synced. `identified_at` is stamped here from the
    /// injected clock, not taken from `verdict` — see
    /// [`NewImportCandidateVerdict`]'s doc.
    ///
    /// The candidate's file decisions are left as they are. Discovery creates
    /// the candidate row; a verdict lands on that row, so it cannot recreate
    /// a candidate removed while identification ran.
    ///
    /// A run that settled on one release replaces the draft this write lands
    /// on, whatever it held and whoever wrote it. A run that settled on no
    /// release — nothing found, several offered, a failure — stores its
    /// result and leaves the draft exactly as it is: it says what the
    /// candidate is not, which is no reason to unmake anyone's work.
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
            .load_candidate_preparation(&verdict.candidate.content_hash)
            .await?
        else {
            return Ok(false);
        };
        if !verdict.candidate.is_current(&prep) {
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
        if let Some(metadata) = &verdict.metadata {
            prep.author = MetadataAuthor::Identification;
            let mut metadata = metadata.clone();
            settle_cover(&mut metadata, &mut prep.metadata);
            prep.assets_prepared = assets_are_prepared(&metadata);
            prep.metadata = metadata;
            prep.metadata_revision += 1;
        }
        // A run that applied its own pick and whose result asks nothing has
        // left only the draft and its Import to see, so the pane opens there.
        // A result that asks something leaves the pane where it was.
        let pane = if verdict.metadata.is_some() {
            CandidatePaneWrite::OpenOnDraftIfReady
        } else {
            CandidatePaneWrite::Keep
        };
        // A run that settled on a release is a pick, and confirms the number
        // that release carries the same way a person's pick does.
        let extras = CandidateSaveExtras {
            lookup_update: CandidateLookupUpdate::ConfirmPick,
            result: CandidateResultWrite::Identified,
            pane,
            ..CandidateSaveExtras::default()
        };
        Ok(matches!(
            self.database
                .save_identified_preparation(prep, expected, extras, self.open.clone())
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
    /// Applied metadata, its provenance and its author remain. The caller
    /// replaces only tracks whose audio changed, using the current prefill
    /// preference: which tracks exist is the decision, not what the release is
    /// called. A draft identification wrote stays identification's, so with
    /// its verdict cleared it waits for the next run's checks rather than
    /// passing as the person's answer.
    ///
    /// The content hash covers files, never role decisions, so this addresses
    /// the same row the verdict lived in rather than orphaning it — and the
    /// scanned candidates that share the hash have their file rows rewritten
    /// to the settled shape in the same transaction.
    pub(crate) async fn store_file_decisions(
        &self,
        read: &CandidateAsRead,
        folder_path: &str,
        edits: &CandidateFileEdits,
        settled_candidates: &[(String, crate::import::folder_scanner::CategorizedFiles)],
        mapping_preparation: &crate::import::CandidateMappingPreparation,
    ) -> Result<(u64, Vec<crate::import::release_candidate::ReleaseCandidate>), LibraryError> {
        let next_revision = read.file_edit_revision.checked_add(1).ok_or_else(|| {
            crate::library::LibraryError::Import(
                "candidate edit revision exhausted the u64 range".to_string(),
            )
        })?;
        let mut prep = self
            .database
            .load_candidate_preparation(&read.content_hash)
            .await?
            .ok_or_else(|| {
                CandidateAsRead::files_moved(read.file_edit_revision, CandidateWrite::FileDecisions)
            })?;
        read.verify(&prep, CandidateWrite::FileDecisions)?;
        if !prep.assets_prepared {
            return Err(crate::library::LibraryError::Import(format!(
                "candidate {} has no complete prepared asset set",
                read.content_hash
            )));
        }
        let expected = CandidateSaveExpectation {
            edit_revision: read.file_edit_revision,
            metadata_revision: read.metadata_revision,
            scanned: None,
        };
        prep.folder_path = folder_path.to_string();
        prep.file_edits = edits.clone();
        prep.file_edits.revision = next_revision;
        // Identification and signals described the prior audio shape. The
        // applied metadata and its source are independent of that verdict.
        prep.identification = None;
        prep.signals = None;
        prep.metadata.draft = mapping_preparation.draft.clone();
        prep.metadata.source_discogs_artist_ids =
            mapping_preparation.source_discogs_artist_ids.clone();
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
        // A file decision lands no pick, so there is no number to confirm;
        // the choices stand as the person left them.
        let extras = CandidateSaveExtras {
            file_tag_snapshot: None,
            reshaped_files: Some(settled_candidates.to_vec()),
            lookup_update: CandidateLookupUpdate::Keep,
            result: CandidateResultWrite::Keep,
            pane: CandidatePaneWrite::Keep,
        };
        match self
            .database
            .save_candidate_preparation(prep, expected, extras)
            .await?
        {
            CandidateSaved::Landed(candidates) => Ok((next_revision, candidates)),
            CandidateSaved::Superseded => Err(CandidateAsRead::files_moved(
                read.file_edit_revision,
                CandidateWrite::FileDecisions,
            )),
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
        let mut draft = crate::import::pane::candidate_draft_from_edit(draft.clone())
            .map_err(|error| LibraryError::Import(error.to_string()))?
            .draft;
        super::pane::apply_metadata_tracks(&mut draft, &prep.metadata.draft)
            .map_err(|error| LibraryError::Import(error.to_string()))?;
        let metadata = crate::import::CandidateMetadataDraft {
            draft,
            source_discogs_artist_ids: Default::default(),
            provenance: provenance.cloned(),
            cover: None,
            assets: crate::import::CandidatePreparedAssets::default(),
        };
        self.apply_metadata(
            prep,
            None,
            folder_path,
            metadata,
            None,
            CandidateResultWrite::Keep,
        )
        .await
    }

    pub async fn apply_source(
        &self,
        watched_folder_path: &str,
        read: &CandidateAsRead,
        folder_path: &str,
        metadata: &crate::import::CandidateMetadataDraft,
    ) -> Result<u64, LibraryError> {
        let prep = self.loaded_at(read).await?;
        let scanned = ScannedCandidateKey {
            watched_folder_path: watched_folder_path.to_string(),
            candidate_path: folder_path.to_string(),
        };
        self.apply_metadata(
            prep,
            Some(scanned),
            folder_path,
            metadata.clone(),
            None,
            CandidateResultWrite::Keep,
        )
        .await
    }

    /// A source projection becomes the candidate's metadata, and — where the
    /// candidate has no result yet — `settled_by_choice` becomes its result,
    /// in the same write.
    ///
    /// A release a person chose is an answer about the candidate exactly as a
    /// run's is, so it is stored where a run's is. That is what keeps the queue
    /// sweep from asking a question the person has already answered: the sweep
    /// reads results and knows nothing about who reached them. A run that has
    /// already answered keeps its own result — that is the record of what it
    /// found, and the choice does not unmake it. The read and the write share
    /// this load, so nothing can land a result in between.
    pub(crate) async fn apply_source_as_result(
        &self,
        watched_folder_path: &str,
        read: &CandidateAsRead,
        folder_path: &str,
        metadata: &crate::import::CandidateMetadataDraft,
        settled_by_choice: crate::identify::TerminalVerdict,
    ) -> Result<u64, LibraryError> {
        let mut prep = self.loaded_at(read).await?;
        let scanned = ScannedCandidateKey {
            watched_folder_path: watched_folder_path.to_string(),
            candidate_path: folder_path.to_string(),
        };
        let result = if prep.identification.is_none() {
            prep.identification = Some(DbCandidateIdentifyResult {
                probed_total_duration_ms: prep
                    .signals
                    .as_ref()
                    .map_or(0, |signals| signals.probed_total_duration_ms()),
                verdict: settled_by_choice,
                identified_at: self.database.now(),
            });
            // The person reached this result themselves: there is nothing
            // in it for them to read.
            CandidateResultWrite::Chosen
        } else {
            CandidateResultWrite::Keep
        };
        self.apply_metadata(
            prep,
            Some(scanned),
            folder_path,
            metadata.clone(),
            None,
            result,
        )
        .await
    }

    /// Store the exact file metadata reading and replace the candidate metadata it
    /// projects in one transaction. The scan stamp is checked inside that
    /// transaction, so no draft can be committed from facts about an older
    /// candidate shape.
    pub(crate) async fn apply_file_metadata(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        read: &CandidateAsRead,
        snapshot: &crate::import::file_tag_snapshot::FileTagSnapshot,
        draft: &crate::import::CandidateDraft,
        cover: Option<&crate::import::CoverSelection>,
    ) -> Result<u64, LibraryError> {
        let prep = self.loaded_at(read).await?;
        let scanned = ScannedCandidateKey {
            watched_folder_path: watched_folder_path.to_string(),
            candidate_path: candidate_path.to_string(),
        };
        let metadata = crate::import::CandidateMetadataDraft {
            draft: draft.clone(),
            source_discogs_artist_ids: Default::default(),
            provenance: Some(crate::import::MetadataProvenance::FileMetadata),
            cover: cover.cloned(),
            assets: crate::import::CandidatePreparedAssets::default(),
        };
        self.apply_metadata(
            prep,
            Some(scanned),
            candidate_path,
            metadata,
            Some(snapshot.clone()),
            CandidateResultWrite::Keep,
        )
        .await
    }

    /// The candidate at exactly the revisions a source projection was
    /// prepared against, or the refusal naming which one moved.
    async fn loaded_at(
        &self,
        read: &CandidateAsRead,
    ) -> Result<CandidatePreparation, LibraryError> {
        let prep = self
            .database
            .load_candidate_preparation(&read.content_hash)
            .await?
            .ok_or_else(|| {
                crate::library::LibraryError::Import("candidate metadata row is missing".into())
            })?;
        read.verify(&prep, CandidateWrite::Metadata)?;
        Ok(prep)
    }

    /// A source's projection becomes the candidate's metadata: the person
    /// applied it — a pick, their files' tags, or a cleared draft — so they
    /// are its author, and its answers are complete.
    async fn apply_metadata(
        &self,
        mut prep: CandidatePreparation,
        scanned: Option<ScannedCandidateKey>,
        folder_path: &str,
        mut metadata: crate::import::CandidateMetadataDraft,
        file_tag_snapshot: Option<crate::import::file_tag_snapshot::FileTagSnapshot>,
        result: CandidateResultWrite,
    ) -> Result<u64, LibraryError> {
        settle_cover(&mut metadata, &mut prep.metadata);
        let expected = CandidateSaveExpectation {
            edit_revision: prep.file_edits.revision,
            metadata_revision: prep.metadata_revision,
            scanned: scanned.map(CandidateScanExpectation::Current),
        };
        prep.folder_path = folder_path.to_string();
        prep.author = MetadataAuthor::Person;
        prep.assets_prepared = assets_are_prepared(&metadata);
        prep.metadata = metadata;
        prep.metadata_revision += 1;
        let revision = prep.metadata_revision;
        let extras = CandidateSaveExtras {
            file_tag_snapshot,
            reshaped_files: None,
            lookup_update: CandidateLookupUpdate::ConfirmPick,
            result,
            pane: CandidatePaneWrite::Keep,
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

/// A candidate's cover and the bytes prepared for it, which move together: a
/// remote selection is stored with the image that revision fetched, and one
/// without the other is refused when the rows are written.
struct PreparedCover {
    selection: Option<crate::import::CoverSelection>,
    image: Option<crate::import::cover_art::RemoteImage>,
}

/// The cover a metadata application settles the candidate on.
///
/// `supplied` is the image the source itself brought. A source that brought
/// none says nothing about the cover, so the candidate keeps the one it has,
/// whatever it is: the `cover.jpg` beside the audio, the artwork its tags
/// embed, or the image an earlier pick fetched — which is why the prepared
/// bytes travel with the selection rather than being left behind.
fn settled_cover(supplied: PreparedCover, stored: PreparedCover) -> PreparedCover {
    match supplied.selection {
        Some(_) => supplied,
        None => stored,
    }
}

/// Whether this application holds the bytes every asset it names needs. A
/// remote cover with no prepared image is the one pair that is short, and it
/// is the state a cover chosen from the picker's gallery leaves behind until
/// a source is applied again — so an application that carries that selection
/// forward carries the waiting with it rather than dropping the choice.
fn assets_are_prepared(metadata: &crate::import::CandidateMetadataDraft) -> bool {
    !matches!(
        (&metadata.cover, &metadata.assets.remote_cover),
        (Some(crate::import::CoverSelection::Remote(_, _)), None)
    )
}

/// Move the settled cover onto `metadata`, taking it from `stored` where the
/// application supplies none. The bytes move with it, so what is written is
/// a selection and the image prepared for it, never one of the two.
fn settle_cover(
    metadata: &mut crate::import::CandidateMetadataDraft,
    stored: &mut crate::import::CandidateMetadataDraft,
) {
    let settled = settled_cover(
        PreparedCover {
            selection: metadata.cover.take(),
            image: metadata.assets.remote_cover.take(),
        },
        PreparedCover {
            selection: stored.cover.take(),
            image: stored.assets.remote_cover.take(),
        },
    );
    metadata.cover = settled.selection;
    metadata.assets.remote_cover = settled.image;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::CoverSelection;

    fn image(byte: u8) -> crate::import::cover_art::RemoteImage {
        crate::import::cover_art::RemoteImage {
            bytes: vec![byte],
            content_type: crate::util::content_type::ContentType::Jpeg,
        }
    }

    /// A source that brings no image of its own says nothing about the
    /// cover, so the stored one stands — a remote selection with the bytes
    /// prepared for it, which is what the candidate commits with.
    #[test]
    fn a_source_with_no_image_keeps_the_stored_cover_and_its_bytes() {
        let folders_own = CoverSelection::Local("cover.jpg".to_string());
        let remote = CoverSelection::Remote(
            "https://example.invalid/front".to_string(),
            crate::import::Catalog::Discogs,
        );
        let nothing = || PreparedCover {
            selection: None,
            image: None,
        };

        let kept = settled_cover(
            nothing(),
            PreparedCover {
                selection: Some(folders_own.clone()),
                image: None,
            },
        );
        assert_eq!(kept.selection, Some(folders_own.clone()));
        assert_eq!(kept.image, None);

        let kept = settled_cover(
            nothing(),
            PreparedCover {
                selection: Some(remote.clone()),
                image: Some(image(1)),
            },
        );
        assert_eq!(kept.selection, Some(remote.clone()));
        assert_eq!(kept.image, Some(image(1)));

        let none_either_way = settled_cover(nothing(), nothing());
        assert_eq!(none_either_way.selection, None);
        assert_eq!(none_either_way.image, None);

        let replaced = settled_cover(
            PreparedCover {
                selection: Some(remote.clone()),
                image: Some(image(2)),
            },
            PreparedCover {
                selection: Some(folders_own),
                image: None,
            },
        );
        assert_eq!(replaced.selection, Some(remote));
        assert_eq!(replaced.image, Some(image(2)));
    }
}

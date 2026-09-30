//! Linking a candidate to the album its lookup's offered pressings are of,
//! when the person cannot tell which of them their copy is.

use super::*;
use crate::util::rate_limiter::CallPriority;

impl ImportServiceHandle {
    /// Link the candidate to the album the pressings its stored lookup offers
    /// are of, its pressing unknown, and apply what those pressings agree on
    /// to its draft (see [`crate::import::shared_album`]).
    ///
    /// Runs to completion once asked for, and like a pick it ends whatever
    /// identification the candidate had going and announces the change.
    pub(crate) async fn link_candidate_shared_album(
        &self,
        candidate_key: String,
    ) -> Result<u64, crate::import::ImportError> {
        let this = self.clone();
        self.committed(async move {
            let revision = this
                .link_candidate_shared_album_write(candidate_key.clone())
                .await?;
            this.cancel_identification(&candidate_key);
            this.announce_metadata_changed(candidate_key);
            Ok(revision)
        })
        .await
    }

    async fn link_candidate_shared_album_write(
        &self,
        candidate_key: String,
    ) -> Result<u64, crate::import::ImportError> {
        let (candidate, current) = self.metadata_write_base(&candidate_key).await?;
        let content_hash = candidate.files.content_hash();
        let state = self
            .library_manager
            .load_import_candidate_state(&content_hash)
            .await?
            .filter(|state| state.file_edits.revision == candidate.file_edit_revision)
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} has no state for its files as they stand"),
            })?;
        let identify = state
            .identify
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} has no lookup whose pressings to link the album of"),
            })?;
        // The rows as the pane lists them: ranked by the candidate's own text,
        // as it reads now.
        let text =
            crate::identify::CandidateText::of_stored(state.signals.as_ref(), &state.lookup_choices);
        let shared = crate::identify::view::shared_album_of(identify.verdict, &text).ok_or_else(
            || crate::import::ImportError::Internal {
                detail: format!("{candidate_key}'s lookup offers no several pressings of one album"),
            },
        )?;
        let durations = crate::import::probe::source_durations(&candidate.files)?;
        let audio_durations =
            crate::import::audio_layout::audio_durations(&candidate.files, &durations)?;
        // What picking each row would read, top-ranked first. A release that
        // fails to load fails the whole, leaving the candidate as it was.
        let mut offered = Vec::with_capacity(shared.leads.len());
        let mut records = Vec::with_capacity(shared.leads.len());
        for lead in &shared.leads {
            let release = crate::import::service::prepare_release(
                &self.library_manager,
                lead,
                CallPriority::Interactive,
            )
            .await?;
            records.push(crate::import::search::MetadataResult::of_release(&release));
            let parsed = release.parsed(&audio_durations, self.clock.as_ref(), self.ids.as_ref())?;
            offered.push(crate::import::RawReleaseEdit::from_user_edit(
                crate::import::parsed_album_to_user_edit(&parsed),
                crate::import::pane::CANDIDATE_TRACK_ID_PREFIX,
            ));
        }
        let draft = crate::import::shared_album::shared_draft(
            &current.draft,
            &offered,
            crate::identify::row_facts::folder_pressing_year(&text, &records),
        );
        let (source_discogs_artist_ids, artist_images) = self
            .prepared_artist_images_for_active(
                current.assets.applied_source.as_ref(),
                &draft.release_edit(),
                &draft.tracks,
                current.assets.artist_images,
            )
            .await?;
        let _commit = self
            .commit_lock_for_revision(
                "link the album",
                &candidate_key,
                &content_hash,
                current.file_edit_revision,
            )
            .await?;
        Ok(self
            .preparations
            .apply_shared_album(
                &candidate.watched_folder_path,
                &crate::import::CandidateAsRead {
                    content_hash,
                    file_edit_revision: candidate.file_edit_revision,
                    metadata_revision: current.metadata_revision,
                },
                &candidate_key,
                &shared.link,
                draft,
                source_discogs_artist_ids,
                artist_images,
            )
            .await?)
    }
}

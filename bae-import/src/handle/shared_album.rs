//! Linking a candidate to the album its lookup's offered pressings are of,
//! when the person cannot tell which of them their copy is.

use super::*;
use bae_catalogs::rate_limiter::CallClass;

impl ImportServiceHandle {
    /// Link the candidate to the album the pressings its stored lookup offers
    /// are of, its pressing unknown, and apply what those pressings agree on
    /// to its draft (see `crate::shared_album`).
    ///
    /// Runs to completion once asked for, and like a pick it ends whatever
    /// identification the candidate had going and announces the change.
    pub async fn link_candidate_shared_album(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<u64, crate::ImportError> {
        let this = self.clone();
        self.committed(async move {
            let revision = this.link_candidate_shared_album_write(candidate_key.clone()).await?;
            this.identification_ended(&candidate_key).await;
            this.announce_metadata_changed(candidate_key);
            Ok(revision)
        })
        .await
    }

    async fn link_candidate_shared_album_write(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<u64, crate::ImportError> {
        let (candidate, current) = self.metadata_write_base(&candidate_key).await?;
        let content_hash = candidate.files.content_hash();
        let state = self
            .services
            .tables
            .load_import_candidate_state(&content_hash)
            .await?
            .filter(|state| state.file_edits.is_at_revision(candidate.file_edit_revision))
            .ok_or_else(|| crate::ImportError::Internal {
                detail: format!("{candidate_key} has no state for its files as they stand"),
            })?;
        let identify = state.identify.ok_or_else(|| crate::ImportError::Internal {
            detail: format!("{candidate_key} has no lookup whose pressings to link the album of"),
        })?;
        // The rows as the pane lists them: ranked by the candidate's own text,
        // as it reads now.
        let text =
            bae_identify::CandidateText::of_stored(state.signals.as_ref(), &state.lookup_choices);
        let shared =
            bae_identify::view::shared_album_of(identify.verdict, &text).ok_or_else(|| {
                crate::ImportError::Internal {
                    detail: format!(
                        "{candidate_key}'s lookup offers no several pressings of one album"
                    ),
                }
            })?;
        let lease = self.services.store.lease_one().await;
        let files = candidate.files.clone();
        let audio = self
            .services
            .diagnostics
            .blocking("read a candidate's audio", move || {
                crate::audio_layout::read_audio(&files, &lease)
            })
            .await??;
        // What picking each row would read, top-ranked first. A release that
        // fails to load, or cannot be read onto the folder's audio, fails the
        // whole, leaving the candidate as it was.
        let mut offered = Vec::with_capacity(shared.leads.len());
        let mut records = Vec::with_capacity(shared.leads.len());
        let mut releases = Vec::with_capacity(shared.leads.len());
        for lead in &shared.leads {
            let prepared = self
                .release_reads
                .release(lead, bae_catalogs::payloads::ReadDepth::Complete, CallClass::AWAITED)
                .await?;
            let release = &prepared.release;
            records.push(bae_catalogs::search::MetadataResult::of_release(release));
            offered.push(crate::shared_album::OfferedReading::of(
                &release.read(&audio).map_err(crate::ImportError::from)?,
            ));
            releases.push(prepared.stored);
        }
        let draft = crate::shared_album::shared_draft(
            &current.metadata.draft,
            &offered,
            bae_identify::row_facts::folder_pressing_year(&text, &records),
        );
        let _commit = self
            .commit_lock_for_revision(
                "link the album",
                &candidate_key,
                &content_hash,
                current.file_edits.revision,
            )
            .await?;
        self.services
            .tables
            .apply_shared_album(
                &candidate.watched_folder_path,
                &crate::CandidateAsRead {
                    content_hash,
                    file_edit_revision: candidate.file_edit_revision,
                    metadata_revision: current.metadata_revision,
                },
                &candidate_key,
                &shared.link,
                draft,
                releases,
            )
            .await
    }
}

#[cfg(test)]
#[path = "shared_album_tests.rs"]
mod tests;

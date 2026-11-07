//! The re-identify session of a library release: what it asks about, held
//! from its first run to its end.

use super::ImportServiceHandle;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// What each library release being identified again asks about, by its
/// session's candidate key. A release has no candidate row to store them on,
/// so they are held here from the session's first run to its end. Every clone
/// of the handle shares one store.
pub(super) type ReleaseLookupChoices =
    Arc<Mutex<HashMap<crate::CandidateKey, bae_identify::LookupChoices>>>;

impl ImportServiceHandle {
    /// Identify an existing library release after the person opens the
    /// re-identify sheet, asking about what this session's choices say —
    /// none made yet on its first run. Extraction resolves the disc ID and
    /// artwork from the library rather than from a scanned folder, so —
    /// unlike [`crate::IdentificationHandle::rerun_identify`] — this does not
    /// go through the identification queue: there is no candidate folder to
    /// key a stored verdict by. Its runs go under the release's re-identify
    /// key ([`crate::CandidateKey::reidentify`]).
    pub async fn identify_release(&self, release_id: String) -> Result<(), crate::ImportError> {
        let choices = self
            .release_lookup_choices()
            .entry(crate::CandidateKey::reidentify(&release_id))
            .or_default()
            .clone();
        self.start_release_identification(release_id, choices).await
    }

    /// Make one change to what a library release's session asks about, and
    /// identify it again from the changed choices.
    pub async fn edit_release_lookup_choices(
        &self,
        release_id: String,
        edit: bae_identify::LookupChoiceEdit,
    ) -> Result<(), crate::ImportError> {
        let choices = {
            let mut held = self.release_lookup_choices();
            let choices = held.entry(crate::CandidateKey::reidentify(&release_id)).or_default();
            *choices = choices.clone().edited(edit);
            choices.clone()
        };
        self.start_release_identification(release_id, choices).await
    }

    /// End a library release's re-identify session: stop its identification
    /// and forget what it asked about, so the next session starts afresh.
    pub async fn end_release_identification(
        &self,
        release_id: &str,
    ) -> Result<(), crate::ImportError> {
        let candidate_key = crate::CandidateKey::reidentify(release_id);
        self.release_lookup_choices().remove(&candidate_key);
        self.cancel_identification(&candidate_key).await
    }

    fn release_lookup_choices(
        &self,
    ) -> MutexGuard<'_, HashMap<crate::CandidateKey, bae_identify::LookupChoices>> {
        self.release_lookup_choices.lock().unwrap_or_else(PoisonError::into_inner)
    }

    async fn start_release_identification(
        &self,
        release_id: String,
        choices: bae_identify::LookupChoices,
    ) -> Result<(), crate::ImportError> {
        let candidate_key = crate::CandidateKey::reidentify(&release_id);
        let run = self.new_identification_run();
        // The sheet names a release, not a draft, so no title is read back: a
        // re-identify run searches by words the person typed, else asks its
        // identifiers and stops there.
        let title_search = choices
            .search_words
            .as_ref()
            .and_then(|words| bae_identify::TitleSearch::of(&words.album, &words.artist));
        if !self
            .start_identification(
                run,
                candidate_key.clone(),
                crate::extraction::ExtractionSource::Release { release_id },
                bae_catalogs::rate_limiter::CallClass::AWAITED,
                choices,
                title_search,
            )
            .await?
        {
            tracing::warn!("re-identify for {candidate_key} has no source to ask; no run started");
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "release_identification_tests.rs"]
mod tests;

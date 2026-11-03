//! Picking a candidate's release or its files' own tags: the release a pick
//! reads, and the pick's write, which ends the candidate's identification.

use super::*;

impl ImportServiceHandle {
    /// Land what the documents of `release`, which a pick read, state its
    /// album is on `candidate_key`'s search, and keep what its album was then
    /// read to be.
    pub(super) async fn land_picked(
        &self,
        candidate_key: &crate::CandidateKey,
        release: &bae_catalogs::records::LookupRef,
        links: bae_catalogs::album_links::AlbumLinks,
    ) -> Result<(), crate::ImportError> {
        let kept = self.tables.land_candidate_search_pick(candidate_key, release, links).await?;
        if !kept.is_empty() {
            crate::identify_service::keep_album_links(&self.tables, kept).await;
        }
        Ok(())
    }

    /// Prepare the release an explicit metadata application reads, a fetch
    /// told to `steps`. A settled lead must already have its release stored;
    /// missing it is a broken invariant. Preparation follows any related
    /// documents the release was fetched without. Ordinary pane reads and
    /// applied drafts never call this path.
    pub(super) async fn release_for_pick(
        &self,
        candidate_key: &crate::CandidateKey,
        release: &bae_catalogs::records::LookupRef,
        steps: &crate::CatalogApplySteps,
    ) -> Result<bae_catalogs::source_release::PreparedRelease, crate::ImportError> {
        if self.is_settled_lead(candidate_key, release).await? {
            self.tables.load_source_release(release).await?.ok_or_else(|| {
                crate::ImportError::Internal {
                    detail: format!(
                        "{candidate_key} settled on {} release {} but nothing stored it",
                        release.catalog.as_str(),
                        release.key
                    ),
                }
            })?;
        }
        self.release_reads.applied_release(release, steps).await
    }

    /// Whether `release` is the one this candidate's stored verdict settled on:
    /// its single match, with the source's tracklist already read.
    ///
    /// That pairing is what the writers commit together — the tracklist is read
    /// from the release stored in the same step, before the verdict — so it is
    /// also the exact condition under which the release is guaranteed to be
    /// stored.
    async fn is_settled_lead(
        &self,
        candidate_key: &crate::CandidateKey,
        release: &bae_catalogs::records::LookupRef,
    ) -> Result<bool, crate::ImportError> {
        let Some(candidate) = self.get_release_candidate(candidate_key).await? else {
            return Ok(false);
        };
        let Some(row) =
            self.tables.load_import_candidate_state(&candidate.files.content_hash()).await?
        else {
            return Ok(false);
        };
        let Some(bae_identify::TerminalVerdict::Found { findings, .. }) =
            row.identify.map(|identify| identify.verdict)
        else {
            return Ok(false);
        };
        let [only] = findings.matches.as_slice() else {
            return Ok(false);
        };
        Ok(only.source == release.catalog
            && only.release_id == release.key
            && only.source_tracks.is_some())
    }

    /// Link this candidate to the release `link` names and read its draft
    /// from that release.
    ///
    /// **The release lands before the link does.** A stored link is the
    /// promise that opening that candidate needs no network, so the fetch goes
    /// first and a failure stores nothing: the pane keeps whatever it had and
    /// says the source failed. A release whose tracklist lists another number
    /// of tracks than the folder holds is refused the same way. Identification writes the same record itself
    /// when a verdict settles on exactly one match; this is the path for the
    /// choices only a person can make.
    ///
    /// Nothing comes back. The per-candidate query sees the write and
    /// redraws the pane from it, which is the same thing a relaunch does.
    ///
    /// Runs to completion once asked for, through the handle's `committed`
    /// wrapper: the person's decision stands whether or not they are still
    /// looking at the candidate when its release fetch and write finish. The
    /// decision also ends whatever identification the candidate had going,
    /// through the handle's cancellation, inside that same write, so no run
    /// can answer a candidate a person has already answered.
    ///
    /// Each fetch it waits on is told to `steps`.
    pub async fn select_candidate_release(
        &self,
        candidate_key: crate::CandidateKey,
        link: bae_identify::PressingLink,
        steps: crate::CatalogApplySteps,
    ) -> Result<u64, crate::ImportError> {
        let this = self.clone();
        self.committed(async move {
            let revision =
                this.pick_candidate_release_write(candidate_key.clone(), link, &steps).await?;
            this.identification_ended(&candidate_key).await;
            this.announce_metadata_changed(candidate_key);
            Ok(revision)
        })
        .await
    }

    /// Replace this candidate's draft with what its files' own tags say. The
    /// release link stays as it is. Like a pick, it ends whatever
    /// identification the candidate had going.
    pub async fn select_candidate_file_tags(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<u64, crate::ImportError> {
        let this = self.clone();
        self.committed(async move {
            let revision = this.read_candidate_file_tags_write(candidate_key.clone()).await?;
            this.identification_ended(&candidate_key).await;
            this.announce_metadata_changed(candidate_key);
            Ok(revision)
        })
        .await
    }
}

#[cfg(test)]
#[path = "picks_tests.rs"]
mod tests;

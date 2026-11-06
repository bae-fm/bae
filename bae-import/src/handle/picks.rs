//! Picking a candidate's release or its files' own tags: the release a pick
//! reads, the metadata and cover read from it, and the pick's write, which
//! ends the candidate's identification.

use super::*;
use bae_catalogs::rate_limiter::CallClass;

impl ImportServiceHandle {
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
            self.services.tables.load_source_release(release).await?.ok_or_else(|| {
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
            self.services.tables.load_import_candidate_state(&candidate.files.content_hash()).await?
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

    /// Build the metadata a release's reading writes and fetch the cover it
    /// is read with, told to `steps`, before anything is written. A fetch
    /// that fails fails the whole with its error.
    pub(crate) async fn external_candidate_metadata(
        &self,
        release: &bae_catalogs::source_release::SourceRelease,
        values: bae_catalogs::release_values::ReleaseValues,
        partners: Vec<bae_catalogs::source_release::SourceRelease>,
        current: &crate::CandidateDraft,
        pressing_year: Option<i32>,
        steps: &crate::CatalogApplySteps,
    ) -> Result<crate::CandidateMetadataDraft, crate::ImportError> {
        let draft = crate::pane::external_candidate_draft(values, current, pressing_year)?;
        let cover = self
            .fetch_release_cover(release, &partners, steps)
            .await
            .map_err(|failure| failure.error)?;
        Ok(external_metadata_with_cover(draft, release, cover))
    }

    /// The cover a draft read from an external release is written with: the
    /// first one the release or the partners picked with it offer, with its
    /// bytes, fetched from the catalog that offers it, told to `steps`.
    ///
    /// A catalog with nothing at that address leaves the cover `None`, and
    /// the write keeps the candidate's current cover; a fetch that fails
    /// fails, naming the catalog that offered the cover.
    pub(crate) async fn fetch_release_cover(
        &self,
        release: &bae_catalogs::source_release::SourceRelease,
        partners: &[bae_catalogs::source_release::SourceRelease],
        steps: &crate::CatalogApplySteps,
    ) -> Result<Option<crate::cover_art::PreparedCover>, CoverFetchFailure> {
        let Some(remote) =
            bae_catalogs::source_release::pick_covers(release, partners).into_iter().next()
        else {
            return Ok(None);
        };
        let image = steps
            .during(
                crate::CatalogApplyStep::FetchingCover(remote.source),
                self.services.remote_images.fetch(&remote.image.url),
            )
            .await
            .map_err(|error| CoverFetchFailure { source: remote.source, error })?;
        Ok(image.map(|fetched| crate::cover_art::PreparedCover::Remote {
            image: remote.image,
            source: remote.source,
            fetched,
        }))
    }

    /// Read the candidate's draft from its files' own tags. The release link
    /// stays as it is.
    pub(super) async fn read_candidate_file_tags_write(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<u64, crate::ImportError> {
        let (candidate, current) = self.metadata_write_base(&candidate_key).await?;
        let content_hash = candidate.files.content_hash();
        loop {
            // Read tags before taking the commit lock: a network share can be
            // slow, and every pane control waits on this lock.
            let read = self.read_file_tag_snapshot(&candidate_key, self.services.file_tags.clone()).await?;
            let seed = crate::file_metadata_seed::FileMetadataSeed::project(
                &read.candidate,
                read.snapshot,
                Some(&current.metadata.draft.tracks),
            )?;
            let commit = self
                .commit_lock_for_revision(
                    "pick file tags",
                    &candidate_key,
                    &content_hash,
                    current.file_edits.revision,
                )
                .await?;
            // A scan that stored the candidate while its tags were read gave it
            // a newer generation, and the write only accepts tags read at the
            // current one: read again.
            let stored_generation = self.services
                .tables
                .load_candidate_file_tag_snapshot(
                    &read.candidate.watched_folder_path,
                    &candidate_key,
                )
                .await?
                .map(|stored| stored.scan_generation);
            if stored_generation != Some(seed.snapshot.scan_generation) {
                drop(commit);
                continue;
            }
            return self.services
                .preparations
                .apply_file_metadata(
                    &read.candidate.watched_folder_path,
                    &candidate_key,
                    &crate::CandidateAsRead {
                        content_hash: content_hash.clone(),
                        file_edit_revision: current.file_edits.revision,
                        metadata_revision: current.metadata_revision,
                    },
                    &seed.snapshot,
                    &seed.draft,
                    seed.cover.as_ref(),
                )
                .await;
        }
    }

    /// Link the candidate to the release `link` names and read its draft
    /// from that release, each fetch told to `steps`.
    pub(super) async fn pick_candidate_release_write(
        &self,
        candidate_key: crate::CandidateKey,
        link: bae_identify::PressingLink,
        steps: &crate::CatalogApplySteps,
    ) -> Result<u64, crate::ImportError> {
        let (candidate, current) = self.metadata_write_base(&candidate_key).await?;
        let content_hash = candidate.files.content_hash();
        let prepared = self.release_for_pick(&candidate_key, &link.record, steps).await?;
        let release = &prepared.release;
        // Picking a result of a typed search reads its documents: what they
        // state its album is joins the search's cards.
        self.services.tables
            .land_candidate_search_pick(&candidate_key, release.release(), release.album_links())
            .await?;
        let audio = crate::audio_layout::read_audio(&candidate.files)?;
        let folder_tracks = audio.track_count();
        // A release that cannot be read onto the folder's audio — it lists
        // another number of tracks than the folder holds, several sets of its
        // discs fit alike, or a track it lays on the audio has no title — is
        // not applied, however a person found it: the pick is refused before
        // anything else is fetched or written. One that lists no tracks says
        // nothing either way and is applied.
        let reading = release.read(&audio);
        let listed = bae_catalogs::reading::listing(&reading, &audio);
        let values = reading?;
        // A partner that fails to load fails the pick, leaving the previous
        // one in place.
        let prepared_partners = self
            .release_reads
            .partners(&link.record, &link.partners, CallClass::AWAITED, steps)
            .await?;
        for partner in &prepared_partners {
            self.services.tables
                .land_candidate_search_pick(
                    &candidate_key,
                    partner.release.release(),
                    partner.release.album_links(),
                )
                .await?;
        }
        // The pick's releases as read, stored in the pick's write, which
        // names them.
        let releases: Vec<_> = std::iter::once(prepared.stored.clone())
            .chain(prepared_partners.iter().map(|partner| partner.stored.clone()))
            .collect();
        let prepared_partners: Vec<_> =
            prepared_partners.into_iter().map(|partner| partner.release).collect();
        // The choice is stored as the candidate's result unless a run's result
        // already stands.
        let detail = release.detail(&prepared_partners);
        let picked = bae_catalogs::search::MetadataResult::of_pick(&detail, listed);
        // The records of the pressing picked, as the folder's text reads them
        // now: what the year falls back on.
        let records: Vec<bae_catalogs::search::MetadataResult> = std::iter::once(picked.clone())
            .chain(prepared_partners.iter().map(bae_catalogs::search::MetadataResult::of_release))
            .collect();
        let state = self.services.tables.load_import_candidate_state(&content_hash).await?;
        let text = bae_identify::CandidateText::of_stored(
            state.as_ref().and_then(|state| state.signals.as_ref()),
            &state.as_ref().map(|state| state.lookup_choices.clone()).unwrap_or_default(),
        );
        let metadata = self
            .external_candidate_metadata(
                release,
                values,
                prepared_partners,
                &current.metadata.draft,
                bae_identify::row_facts::folder_pressing_year(&text, &records),
                steps,
            )
            .await?;
        let settled_by_choice = bae_identify::TerminalVerdict::of_pick(picked, folder_tracks);
        let _commit = self
            .commit_lock_for_revision(
                "pick a release",
                &candidate_key,
                &content_hash,
                current.file_edits.revision,
            )
            .await?;
        self.services.preparations
            .apply_pick(
                &candidate.watched_folder_path,
                &crate::CandidateAsRead {
                    content_hash: content_hash.clone(),
                    file_edit_revision: candidate.file_edit_revision,
                    metadata_revision: current.metadata_revision,
                },
                &candidate_key,
                &link,
                &metadata,
                settled_by_choice,
                releases,
            )
            .await
    }

    /// Every release a pick claims, read whole: `primary`, and its
    /// `partners` on other catalogs, from storage or fetched now.
    pub(crate) async fn read_pick(
        &self,
        primary: &bae_catalogs::records::LookupRef,
        partners: &[bae_catalogs::records::LookupRef],
        class: bae_catalogs::rate_limiter::CallClass,
    ) -> Result<
        (
            bae_catalogs::source_release::PreparedRelease,
            Vec<bae_catalogs::source_release::PreparedRelease>,
        ),
        crate::ImportError,
    > {
        let release = self
            .release_reads
            .release(primary, bae_catalogs::payloads::ReadDepth::Complete, class)
            .await?;
        let partners = self
            .release_reads
            .partners(primary, partners, class, &crate::CatalogApplySteps::unread())
            .await?;
        Ok((release, partners))
    }
}

/// A fetch of an external release's cover that failed, naming the catalog
/// that offered it.
#[derive(Debug)]
pub(crate) struct CoverFetchFailure {
    pub(crate) source: bae_catalogs::catalog::Catalog,
    pub(crate) error: bae_catalogs::remote_image::RemoteImageError,
}

/// The metadata `draft`, read from the external `release`, makes with the
/// cover fetched for it.
pub(crate) fn external_metadata_with_cover(
    draft: crate::CandidateDraft,
    release: &bae_catalogs::source_release::SourceRelease,
    cover: Option<crate::cover_art::PreparedCover>,
) -> crate::CandidateMetadataDraft {
    crate::CandidateMetadataDraft {
        draft,
        provenance: Some(crate::MetadataProvenance::ExternalRelease {
            record: release.release().clone(),
        }),
        cover,
    }
}

#[cfg(test)]
#[path = "picks_tests.rs"]
mod tests;

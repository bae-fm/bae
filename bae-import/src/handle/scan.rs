use super::*;
use bae_catalogs::rate_limiter::CallPriority;

impl ImportServiceHandle {
    /// Tell surfaces a folder candidate's files were bound anew.
    pub(super) fn announce_source_candidate(
        &self,
        candidate: crate::folder_scanner::FolderCandidate,
    ) {
        self.event_tx.send(ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }));
    }

    /// Mark the candidate at `candidate_key` skipped or unskipped and
    /// announce the change; a request that changes nothing writes nothing and
    /// sends no event.
    ///
    /// Skipping ends the candidate's identification, since the person has set
    /// it aside.
    pub async fn set_candidate_skipped(
        &self,
        candidate_key: crate::CandidateKey,
        skipped: bool,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        self.committed(
            async move { this.set_candidate_skipped_write(candidate_key, skipped).await },
        )
        .await
    }

    async fn set_candidate_skipped_write(
        &self,
        candidate_key: crate::CandidateKey,
        skipped: bool,
    ) -> Result<(), crate::ImportError> {
        let _commit = self.folder_state_commit.lock("skip a candidate").await;
        let candidate = self.require_release_candidate(&candidate_key).await?;
        if candidate.grouping.is_some() {
            if self.tables.set_grouping_skipped(&candidate_key, skipped).await? {
                if skipped {
                    self.identification_ended(&candidate_key).await;
                }
                self.event_tx.send(ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
                    candidate_key,
                    skipped,
                }));
            }
            return Ok(());
        }
        let changed = self.tables.set_import_candidate_skipped(&candidate.path, skipped).await?;
        if changed {
            if skipped {
                self.identification_ended(&candidate_key).await;
            }
            self.event_tx.send(ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
                candidate_key,
                skipped,
            }));
        }
        Ok(())
    }

    /// The candidate draft a release's reading writes, keeping each current
    /// track on its file. A reading of tracks lays every value it states over
    /// the folder's audio; a release that lists none leaves the draft's tracks
    /// as they are. Where the release states no year, the pressing's is
    /// `pressing_year`, the folder's — see
    /// `identify::row_facts::folder_pressing_year`.
    pub(crate) fn external_candidate_draft(
        values: bae_catalogs::release_values::ReleaseValues,
        current: &crate::CandidateDraft,
        pressing_year: Option<i32>,
    ) -> Result<crate::CandidateDraft, crate::ImportError> {
        let mut draft = if values.tracks.is_empty() {
            crate::pane::album_over_draft(values, current)
        } else {
            crate::pane::values_over_audio(values, current)?
        };
        if draft.pressing.year.trim().is_empty() {
            if let Some(year) = pressing_year {
                draft.pressing.year = year.to_string();
            }
        }
        Ok(draft)
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
        let draft = Self::external_candidate_draft(values, current, pressing_year)?;
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
                self.remote_images.fetch(&remote.image.url),
            )
            .await
            .map_err(|error| CoverFetchFailure { source: remote.source, error })?;
        Ok(image.map(|fetched| crate::cover_art::PreparedCover::Remote {
            image: remote.image,
            source: remote.source,
            fetched,
        }))
    }

    /// The candidate's stored preparation, for a write about to replace its
    /// draft.
    pub(super) async fn metadata_write_base(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<
        (crate::folder_scanner::FolderCandidate, crate::preparation::CandidatePreparation),
        crate::ImportError,
    > {
        let candidate = self.require_release_candidate(candidate_key).await?;
        let current =
            crate::preparation::stored_preparation(&self.tables, &candidate.files.content_hash())
                .await?
                .ok_or_else(|| crate::ImportError::Internal {
                    detail: format!("{candidate_key} has no stored import preparation"),
                })?;
        Ok((candidate, current))
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
            let read = self.read_file_tag_snapshot(&candidate_key, self.file_tags.clone()).await?;
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
            let stored_generation = self
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
            return self
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
        self.tables
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
            .partners(&link.record, &link.partners, CallPriority::Interactive, steps)
            .await?;
        for partner in &prepared_partners {
            self.tables
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
        let state = self.tables.load_import_candidate_state(&content_hash).await?;
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
        self.preparations
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

    /// Clear the candidate's draft, keeping its file decisions and its release
    /// link. Like a pick, it ends the candidate's identification and announces
    /// the change.
    pub async fn clear_candidate_metadata(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<u64, crate::ImportError> {
        let this = self.clone();
        self.committed(async move {
            let revision = this.clear_candidate_metadata_write(candidate_key.clone()).await?;
            this.identification_ended(&candidate_key).await;
            this.announce_metadata_changed(candidate_key);
            Ok(revision)
        })
        .await
    }

    async fn clear_candidate_metadata_write(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<u64, crate::ImportError> {
        let candidate = self.require_release_candidate(&candidate_key).await?;
        let content_hash = candidate.files.content_hash();
        let current = crate::preparation::stored_preparation(&self.tables, &content_hash)
            .await?
            .ok_or_else(|| crate::ImportError::Internal {
                detail: format!("{candidate_key} has no stored import preparation"),
            })?;
        let mut draft = candidate.blank_draft();
        crate::pane::keep_row_identities(&mut draft.tracks, &current.metadata.draft.tracks)?;
        let _commit = self
            .commit_lock_for_revision(
                "clear metadata",
                &candidate_key,
                &content_hash,
                current.file_edits.revision,
            )
            .await?;
        self.preparations
            .apply_source(
                &candidate.watched_folder_path,
                &crate::CandidateAsRead {
                    content_hash: content_hash.clone(),
                    file_edit_revision: candidate.file_edit_revision,
                    metadata_revision: current.metadata_revision,
                },
                &candidate_key,
                &crate::CandidateMetadataDraft { draft, provenance: None, cover: None },
            )
            .await
    }

    /// Keep the candidate's own draft over what its stored lookup offered:
    /// none of the releases it found is the folder's, or it found none. The
    /// candidate is linked to no release and the draft stays as it is; the
    /// decision goes with the verdict, so identifying the candidate again
    /// starts without it.
    pub async fn keep_candidate_draft(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<u64, crate::ImportError> {
        let this = self.clone();
        self.committed(async move { this.keep_candidate_draft_write(candidate_key).await }).await
    }

    async fn keep_candidate_draft_write(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<u64, crate::ImportError> {
        let candidate = self.require_release_candidate(&candidate_key).await?;
        let content_hash = candidate.files.content_hash();
        let current = crate::preparation::stored_preparation(&self.tables, &content_hash)
            .await?
            .ok_or_else(|| crate::ImportError::Internal {
                detail: format!("{candidate_key} has no stored import preparation"),
            })?;
        let _commit = self
            .commit_lock_for_revision(
                "keep the folder's own draft",
                &candidate_key,
                &content_hash,
                current.file_edits.revision,
            )
            .await?;
        self.preparations
            .keep_own_draft(&crate::CandidateAsRead {
                content_hash: content_hash.clone(),
                file_edit_revision: candidate.file_edit_revision,
                metadata_revision: current.metadata_revision,
            })
            .await
    }

    /// Unlink the candidate from its release. The draft stays as it is.
    pub async fn unlink_candidate_release(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<u64, crate::ImportError> {
        let this = self.clone();
        self.committed(async move {
            let revision = this.unlink_candidate_release_write(candidate_key.clone()).await?;
            this.announce_metadata_changed(candidate_key);
            Ok(revision)
        })
        .await
    }

    async fn unlink_candidate_release_write(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<u64, crate::ImportError> {
        let (candidate, current) = self.metadata_write_base(&candidate_key).await?;
        let content_hash = candidate.files.content_hash();
        let _commit = self
            .commit_lock_for_revision(
                "unlink the release",
                &candidate_key,
                &content_hash,
                current.file_edits.revision,
            )
            .await?;
        self.preparations
            .unlink(&crate::CandidateAsRead {
                content_hash,
                file_edit_revision: candidate.file_edit_revision,
                metadata_revision: current.metadata_revision,
            })
            .await
    }

    pub(crate) fn announce_metadata_changed(&self, candidate_key: crate::CandidateKey) {
        self.event_tx
            .send(ImportEvent::Scan(ScanEvent::CandidateMetadataChanged { candidate_key }));
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
#[path = "scan_tests.rs"]
mod tests;

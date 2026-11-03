use super::*;
use crate::candidate_search::{CandidateSearch, SearchRun};
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::rate_limiter::CallPriority;
use bae_catalogs::search::SearchQuery;

impl ImportServiceHandle {
    /// Submit a candidate's typed search, stored before any source is asked,
    /// over whatever search the candidate had: one search per candidate at a
    /// time, because the pane shows one result area. Each source's answer
    /// lands on the stored search as it arrives, which the candidate's
    /// runtime read delivers.
    pub async fn start_candidate_search(
        &self,
        candidate_key: crate::CandidateKey,
        query: SearchQuery,
    ) -> Result<(), crate::ImportError> {
        let sources = bae_preferences::preferences(&self.database).await?.metadata_sources();
        let search = CandidateSearch::started(query.clone(), &sources);
        let run = self.runtime.start_search(&candidate_key, &search).await?;
        for source in search.searching_sources() {
            self.spawn_source_search(candidate_key.clone(), query.clone(), source, run);
        }
        Ok(())
    }

    /// Re-ask only the sources that failed, keeping what the others found and
    /// leaving a source still searching to its answer already out. Asks
    /// nothing when the candidate has no search, or none of its sources
    /// failed.
    pub async fn retry_candidate_search(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<(), crate::ImportError> {
        let Some((run, query, sources)) = self.runtime.retry_search(&candidate_key).await? else {
            debug!("retry_candidate_search: {candidate_key} has no source to re-ask");
            return Ok(());
        };
        for source in sources {
            self.spawn_source_search(candidate_key.clone(), query.clone(), source, run);
        }
        Ok(())
    }

    /// The library has stopped asking `source`: close its part of every
    /// search, wherever the person is looking.
    ///
    /// Called after the preference is written, so every search started from
    /// here on is started without the source and has nothing to close.
    pub async fn stop_asking_source(&self, source: LookupCatalog) -> Result<(), bae_db::DbError> {
        self.runtime.switch_source_off(source).await
    }

    /// Run one source's part of a search and land it on the search's run,
    /// which takes it only while that part is still searching.
    fn spawn_source_search(
        &self,
        candidate_key: crate::CandidateKey,
        query: SearchQuery,
        source: LookupCatalog,
        run: SearchRun,
    ) {
        let catalogs = self.catalogs.clone();
        let runtime = self.runtime.clone();
        let diagnostics = self.diagnostics.clone();
        self.diagnostics.spawn_reported(
            &self.runtime_handle,
            "search one source",
            async move {
                // A lookup that panics lands as failed, so its source stops
                // showing a search under way.
                let found = match bae_util::task_failure::caught("ask one source a search", async {
                    let session = catalogs.search_session(source).await.map_err(|error| {
                        crate::ImportError::from(error).failure(&format!(
                            "asking {} a search for {:?}",
                            source.as_str(),
                            &query
                        ))
                    })?;
                    bae_catalogs::search::search_source(
                        &session,
                        source,
                        &query,
                        CallPriority::Interactive,
                    )
                    .await
                })
                .await
                {
                    Ok(found) => found,
                    Err(failed) => {
                        Err(bae_catalogs::Failure::Internal(diagnostics.bug(failed).into()))
                    }
                };
                land_source_search(&runtime, &diagnostics, &candidate_key, run, source, found)
                    .await;
            },
            // The landing is what panicked, so nothing is left to land.
            |_| {},
        );
    }

    /// Land what the documents of `release`, which a pick read, state its
    /// album is on `candidate_key`'s search, and keep what its album was then
    /// read to be.
    pub(super) async fn land_picked(
        &self,
        candidate_key: &crate::CandidateKey,
        release: &bae_catalogs::records::LookupRef,
        links: bae_catalogs::album_links::AlbumLinks,
    ) -> Result<(), crate::ImportError> {
        let kept = self.runtime.land_picked(candidate_key, release, links).await?;
        if !kept.is_empty() {
            crate::identify_service::keep_album_links(&self.tables, kept).await;
        }
        Ok(())
    }

    /// Ask one source a typed query, check library status, and bundle the
    /// results into release-group cards in one call.
    ///
    /// The one-shot path: an automation client names the source, waits for the
    /// answer, and holds no run of its own. A person's search is a run —
    /// [`Self::start_candidate_search`] — because its two sources land
    /// separately and the pane draws each as it does.
    pub async fn search_with_status(
        &self,
        query: SearchQuery,
        source: LookupCatalog,
    ) -> Result<GroupedSearchResults, crate::ImportError> {
        use bae_db::LibraryCheck;

        let results = bae_catalogs::search::search_provider(
            &self.catalogs.search_session(source).await?,
            source,
            &query,
            CallPriority::Interactive,
        )
        .await?;

        let checks: Vec<LibraryCheck> = results.iter().map(LibraryCheck::from).collect();

        let statuses = self.database.check_releases_in_library(&checks).await?;

        let status_map: HashMap<String, bae_db::LibraryStatus> =
            statuses.into_iter().map(|s| (s.release_id.clone(), s)).collect();

        // `check_releases_in_library` returns exactly one status per input check,
        // keyed by `release_id`, so a miss is a broken invariant. Surface it: a
        // fabricated "not in library" default would silently misclassify it.
        let mut statuses = Vec::with_capacity(results.len());
        for r in &results {
            let status = status_map.get(&r.release_id).cloned().ok_or_else(|| {
                crate::ImportError::Internal {
                    detail: format!("library status missing for release {}", r.release_id),
                }
            })?;
            statuses.push(status);
        }

        // Grouping is the UI's shape, so core computes it: the search surface
        // renders one card per release group with its pressings beneath.
        // A typed search has no candidate text behind it to rank by, so the
        // rows keep the pressing-year order alone.
        let groups = bae_identify::release_group::group_results(
            bae_identify::release_group::unranked(results),
            None,
        );

        Ok(GroupedSearchResults { groups, statuses })
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

/// Land one source's answer on `candidate_key`'s search run `run`. The
/// search was started by a person who is not waiting on this answer, so a
/// landing that cannot be written is reported as a bug.
async fn land_source_search(
    runtime: &crate::CandidateRuntime,
    diagnostics: &bae_diagnostics::Diagnostics,
    candidate_key: &crate::CandidateKey,
    run: SearchRun,
    source: LookupCatalog,
    found: bae_catalogs::search::SourceLookup,
) {
    match runtime.land_search(run, source, found).await {
        Ok(true) => {}
        Ok(false) => debug!(
            "{}'s {} search landed on no part still searching",
            candidate_key,
            source.as_str()
        ),
        Err(error) => diagnostics
            .report_bug(&format!("land {}'s {} search", candidate_key, source.as_str()), &error),
    }
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;

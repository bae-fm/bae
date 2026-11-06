use super::*;
use crate::candidate_search::{CandidateSearch, SearchRun};
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::rate_limiter::CallClass;
use bae_catalogs::search::SearchQuery;

/// Search results grouped by release group, with the per-release library dupe
/// statuses the UI looks up by release id.
#[derive(Debug, Clone)]
pub struct GroupedSearchResults {
    pub groups: Vec<bae_identify::release_group::ReleaseGroup>,
    pub statuses: Vec<bae_records::LibraryStatus>,
}

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
        let sources = bae_preferences::preferences(&self.services.store).await?.metadata_sources();
        let search = CandidateSearch::started(query.clone(), &sources);
        let run = self.services.tables.start_candidate_search(&candidate_key, &search).await?;
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
        let Some((run, query, sources)) =
            self.services.tables.retry_candidate_search(&candidate_key).await?
        else {
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
    pub async fn stop_asking_source(
        &self,
        source: LookupCatalog,
    ) -> Result<(), bae_store::DbError> {
        self.services.tables.switch_search_source_off(source).await
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
        let catalogs = self.services.catalogs.clone();
        let tables = self.services.tables.clone();
        let diagnostics = self.services.diagnostics.clone();
        self.services.diagnostics.spawn_reported(
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
                        CallClass::AWAITED,
                    )
                    .await
                })
                .await
                {
                    Ok(found) => found,
                    Err(failed) => {
                        Err(bae_catalogs::Failure::Internal(diagnostics.panicked(failed).into()))
                    }
                };
                land_source_search(&tables, &diagnostics, &candidate_key, run, source, found).await;
            },
            // The landing is what panicked, so nothing is left to land.
            |_| {},
        );
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
        use bae_records::LibraryCheck;

        let results = bae_catalogs::search::search_provider(
            &self.services.catalogs.search_session(source).await?,
            source,
            &query,
            CallClass::REQUESTED,
        )
        .await?;

        let checks: Vec<LibraryCheck> = results.iter().map(LibraryCheck::from).collect();

        let statuses = self.services.records.check_releases_in_library(&checks).await?;

        let status_map: HashMap<String, bae_records::LibraryStatus> =
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
}

/// Land one source's answer on `candidate_key`'s search run `run`. The
/// search was started by a person who is not waiting on this answer, so a
/// landing that cannot be written is reported as a bug.
async fn land_source_search(
    tables: &crate::tables::ImportTables,
    diagnostics: &bae_diagnostics::Diagnostics,
    candidate_key: &crate::CandidateKey,
    run: SearchRun,
    source: LookupCatalog,
    found: bae_catalogs::search::SourceLookup,
) {
    match tables.land_candidate_search(run, source, found).await {
        Ok(true) => {}
        Ok(false) => debug!(
            "{}'s {} search landed on no part still searching",
            candidate_key,
            source.as_str()
        ),
        Err(error) => diagnostics.report_failure(
            &format!("land {}'s {} search", candidate_key, source.as_str()),
            &error,
        ),
    }
}

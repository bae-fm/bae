use super::*;
use crate::import::candidate_search::CandidateSearch;
use crate::import::search::{search_source, SearchQuery};
use crate::util::rate_limiter::CallPriority;

impl ImportServiceHandle {
    /// Bytes of provider art at `url` for previews and explicit selection.
    /// Candidate preparation persists selected bytes independently; this
    /// session cache only avoids repeated transport within the process.
    ///
    /// `None` when the source serves no image at that address — an offered
    /// cover the archive turns out not to hold. The slot then renders as having
    /// no image, which is what it has, rather than as a failed load.
    pub async fn fetch_remote_image_bytes(
        &self,
        url: String,
    ) -> Result<Option<crate::import::cover_art::RemoteImage>, crate::import::ImportError> {
        self.library_manager.fetch_remote_image(&url).await
    }

    /// Submit a candidate's typed search. Fire-and-forget: the run lands on
    /// the candidate's runtime one source at a time, and every landing reaches
    /// a surface as `ImportEvent::CandidateSearchChanged`.
    ///
    /// A search already running for this key is superseded — one search per
    /// candidate at a time, because the pane shows one result area.
    pub fn start_candidate_search(&self, candidate_key: String, query: SearchQuery) {
        let search =
            CandidateSearch::started(query.clone(), self.library_manager.discogs_is_usable());
        let sources = search.searching_sources();
        let run = self
            .candidate_searches
            .lock()
            .unwrap()
            .start(&candidate_key, search.clone());
        self.publish_candidate_search(candidate_key.clone(), Some(search));
        for source in sources {
            self.spawn_source_search(candidate_key.clone(), query.clone(), source, run);
        }
    }

    /// Re-ask only the sources that failed, keeping what the others found. A
    /// no-op when the candidate has no search, or none of its sources failed.
    pub fn retry_candidate_search(&self, candidate_key: String) {
        let (search, sources, run) = {
            let mut searches = self.candidate_searches.lock().unwrap();
            let Some(mut search) = searches.current(&candidate_key).cloned() else {
                debug!("retry_candidate_search: {candidate_key} has no search to retry");
                return;
            };
            search.restart_failed();
            let sources = search.searching_sources();
            if sources.is_empty() {
                return;
            }
            let run = searches.start(&candidate_key, search.clone());
            (search, sources, run)
        };
        let query = search.query.clone();
        self.publish_candidate_search(candidate_key.clone(), Some(search));
        for source in sources {
            self.spawn_source_search(candidate_key.clone(), query.clone(), source, run);
        }
    }

    /// Drop a candidate's search: its lookups stop mattering and the result
    /// area goes back to the identify verdict.
    pub fn clear_candidate_search(&self, candidate_key: String) {
        self.candidate_searches
            .lock()
            .unwrap()
            .clear(&candidate_key);
        self.publish_candidate_search(candidate_key, None);
    }

    fn publish_candidate_search(&self, candidate_key: String, search: Option<CandidateSearch>) {
        send_event(
            &self.event_tx,
            ImportEvent::CandidateSearchChanged {
                candidate_key,
                search,
            },
        );
    }

    /// Run one source's part of a search and land it on the candidate's
    /// current run.
    ///
    /// The landing folds into the search the driver holds, under its lock, and
    /// only while `run` is still the key's run. Superseding or clearing a run
    /// happens under the same lock, so a run this one has replaced cannot
    /// write over it — and the other source's landing, which folded into the
    /// same held value, is still there.
    fn spawn_source_search(
        &self,
        candidate_key: String,
        query: SearchQuery,
        source: MetadataSource,
        run: u64,
    ) {
        let library_manager = self.library_manager.clone();
        let searches = self.candidate_searches.clone();
        let event_tx = self.event_tx.clone();
        self.runtime_handle.spawn(async move {
            let found =
                search_source(&library_manager, source, &query, CallPriority::Interactive).await;
            // Superseded already: skip the library check nothing will read.
            if !searches.lock().unwrap().is_current(&candidate_key, run) {
                return;
            }
            let outcome = match found {
                Ok(results) => {
                    crate::identify::annotate_with_library_status(results, &library_manager)
                        .await
                        .map_err(|detail| crate::signals::LookupFailure::Diagnostic { detail })
                }
                Err(failure) => Err(failure),
            };

            let landed = searches
                .lock()
                .unwrap()
                .land(&candidate_key, run, source, outcome);
            match landed {
                Some(search) => send_event(
                    &event_tx,
                    ImportEvent::CandidateSearchChanged {
                        candidate_key,
                        search: Some(search),
                    },
                ),
                None => debug!(
                    "{}'s {} search landed on no run; it was cleared or superseded",
                    candidate_key,
                    source.as_str()
                ),
            }
        });
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
        source: MetadataSource,
    ) -> Result<GroupedSearchResults, crate::import::ImportError> {
        use crate::db::LibraryCheck;

        let results = crate::import::search::search_provider(
            &self.library_manager,
            source,
            &query,
            CallPriority::Interactive,
        )
        .await?;

        let checks: Vec<LibraryCheck> = results.iter().map(LibraryCheck::from).collect();

        let statuses = self
            .library_manager
            .check_releases_in_library(&checks)
            .await?;

        let status_map: HashMap<String, crate::db::LibraryStatus> = statuses
            .into_iter()
            .map(|s| (s.release_id.clone(), s))
            .collect();

        // `check_releases_in_library` returns exactly one status per input check,
        // keyed by `release_id`, so a miss is a broken invariant. Surface it: a
        // fabricated "not in library" default would silently misclassify it.
        let mut statuses = Vec::with_capacity(results.len());
        for r in &results {
            let status = status_map.get(&r.release_id).cloned().ok_or_else(|| {
                crate::import::ImportError::Internal {
                    detail: format!("library status missing for release {}", r.release_id),
                }
            })?;
            statuses.push(status);
        }

        // Grouping is the UI's shape, so core computes it: the search surface
        // renders one card per release group with its pressings beneath.
        let groups = crate::import::release_group::group_results(results);

        Ok(GroupedSearchResults { groups, statuses })
    }

    /// Fetch complete artwork galleries when the picker opens. Dispatch on
    /// the owner: library identities and candidate provenance are different
    /// records, never interchangeable string identifiers.
    pub async fn fetch_remote_covers(
        &self,
        target: crate::import::cover_art::CoverTarget,
    ) -> Result<Vec<crate::import::cover_art::RemoteCover>, crate::import::ImportError> {
        match target {
            crate::import::cover_art::CoverTarget::Release(id) => self.release_covers(&id).await,
            crate::import::cover_art::CoverTarget::Candidate(key) => {
                self.candidate_covers(&key).await
            }
        }
    }

    async fn candidate_covers(
        &self,
        key: &str,
    ) -> Result<Vec<crate::import::cover_art::RemoteCover>, crate::import::ImportError> {
        let Some(super::ImportCandidateSnapshot::Folder { candidate, .. }) =
            self.get_candidate(key).await?
        else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{key} is not a scanned candidate"),
            });
        };
        let state = self
            .library_manager
            .load_import_candidate_state(&candidate.files.content_hash())
            .await?;
        let Some(crate::import::MetadataProvenance::ExternalRelease {
            source,
            release_id,
            partners,
        }) = state.and_then(|state| state.metadata_provenance)
        else {
            return Ok(Vec::new());
        };
        let mut covers = Vec::new();
        let primary = crate::import::MetadataRef::new(release_id, source);
        for identity in std::iter::once(primary).chain(partners) {
            let payloads = self
                .library_manager
                .load_release_payloads(&identity)
                .await?
                .ok_or_else(|| crate::import::ImportError::Internal {
                    detail: format!(
                        "{key} names {} release {} without archived metadata",
                        identity.source.as_str(),
                        identity.id
                    ),
                })?;
            for cover in payloads.gallery_covers().await? {
                crate::import::cover_art::push_unique_cover(&mut covers, cover);
            }
        }
        Ok(covers)
    }

    async fn release_covers(
        &self,
        release_id: &str,
    ) -> Result<Vec<crate::import::cover_art::RemoteCover>, crate::import::ImportError> {
        let identities = self
            .library_manager
            .get_release_identities(release_id)
            .await?;

        let mut covers = Vec::new();

        for identity in &identities {
            match identity.source {
                MetadataSource::MusicBrainz => {
                    let gallery = crate::import::cover_art::musicbrainz_gallery(
                        &identity.source_release_id,
                        Some(&identity.source_group_id),
                    )
                    .await?;
                    for cover in gallery {
                        crate::import::cover_art::push_unique_cover(&mut covers, cover);
                    }
                }
                MetadataSource::Discogs => {
                    let rid = &identity.source_release_id;
                    match self
                        .library_manager
                        .fetch_discogs_release_covers(rid, CallPriority::Interactive)
                        .await
                    {
                        Ok(found) => {
                            for cover in found {
                                crate::import::cover_art::push_unique_cover(&mut covers, cover);
                            }
                        }
                        Err(crate::import::ImportError::DiscogsNotConfigured) => {
                            debug!(
                                release_id,
                                source_release_id = %rid,
                                "skipping Discogs cover fetch: Discogs is not configured"
                            );
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
        }

        Ok(covers)
    }

    /// The documents behind external-release provenance, and where they come from.
    ///
    /// Provenance matching the candidate's settled lead **reads** them: identification
    /// stored them before it stored the verdict that named this release, so they
    /// are there, and a miss is a broken invariant rather than a cold cache.
    /// Re-fetching on a miss would serve the pane and hide the break, so it
    /// fails instead.
    ///
    /// Every other external release — another pressing in a list, an explicit
    /// search result, a release being re-identified — is one identification
    /// never fetched. It goes through [`crate::import::service::prepare_release`],
    /// which reads whatever is archived and pays for the rest, so opening it a
    /// second time is local too.
    pub(super) async fn payloads_for_provenance(
        &self,
        candidate_key: &str,
        release: &crate::import::MetadataRef,
    ) -> Result<crate::import::payloads::ReleasePayloads, crate::import::ImportError> {
        if self.is_settled_lead(candidate_key, release).await? {
            return self
                .library_manager
                .load_release_payloads(release)
                .await?
                .ok_or_else(|| crate::import::ImportError::Internal {
                    detail: format!(
                        "{candidate_key} settled on {} release {} but nothing stored its lookups",
                        release.source.as_str(),
                        release.id
                    ),
                });
        }
        crate::import::service::prepare_release(
            &self.library_manager,
            release,
            CallPriority::Interactive,
        )
        .await
    }

    /// Whether `release` is the one this candidate's stored verdict settled on:
    /// its single match, with the source's tracklist already read.
    ///
    /// That pairing is what the writers commit together — the tracklist and the
    /// documents land in the same step, before the verdict — so it is also the
    /// exact condition under which the documents are guaranteed to be readable.
    async fn is_settled_lead(
        &self,
        candidate_key: &str,
        release: &crate::import::MetadataRef,
    ) -> Result<bool, crate::import::ImportError> {
        let Some(super::ImportCandidateSnapshot::Folder { candidate, .. }) =
            self.get_candidate(candidate_key).await?
        else {
            return Ok(false);
        };
        let Some(row) = self
            .library_manager
            .load_import_candidate_state(&candidate.files.content_hash())
            .await?
        else {
            return Ok(false);
        };
        let Some(crate::identify::TerminalVerdict::Found { matches, .. }) =
            row.identify.map(|identify| identify.verdict)
        else {
            return Ok(false);
        };
        let [only] = matches.as_slice() else {
            return Ok(false);
        };
        Ok(only.source == release.source
            && only.release_id == release.id
            && only.source_tracks.is_some())
    }

    /// Replace this candidate's metadata from an external release or its file
    /// tags.
    ///
    /// **The documents land before provenance does.** Stored external provenance is the
    /// promise that opening that candidate needs no network, so the fetch goes
    /// first and a failure stores nothing: the pane keeps whatever it had and
    /// says the source failed. Identification writes the same record itself when
    /// a verdict settles on exactly one match; this is the path for the
    /// choices only a person can make.
    ///
    /// Nothing comes back. The per-candidate query sees the write and
    /// redraws the pane from it, which is the same thing a relaunch does.
    pub async fn select_candidate_metadata_provenance(
        &self,
        candidate_key: String,
        provenance: crate::import::MetadataProvenance,
    ) -> Result<u64, crate::import::ImportError> {
        let revision = self
            .set_candidate_metadata_provenance(candidate_key.clone(), provenance)
            .await?;
        self.announce_metadata_provenance(candidate_key);
        Ok(revision)
    }

    /// The stored verdict describing `candidate_key`'s current file shape —
    /// or `None` when nothing is stored, the stored row describes an earlier
    /// file-edit revision, or the key is not a scanned folder candidate.
    pub(crate) async fn stored_verdict(
        &self,
        candidate_key: &str,
    ) -> Result<Option<crate::identify::TerminalVerdict>, crate::import::ImportError> {
        let Some(super::ImportCandidateSnapshot::Folder { candidate, .. }) =
            self.get_candidate(candidate_key).await?
        else {
            return Ok(None);
        };
        let Some(row) = self
            .library_manager
            .load_import_candidate_state(&candidate.files.content_hash())
            .await?
        else {
            return Ok(None);
        };
        if row.file_edits.revision != candidate.file_edit_revision {
            return Ok(None);
        }
        Ok(row.identify.map(|identify| identify.verdict))
    }
}

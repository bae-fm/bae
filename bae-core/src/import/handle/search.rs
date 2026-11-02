use super::*;
use crate::catalog_model::search::{search_source, SearchQuery};
use crate::import::candidate_search::{CandidateSearch, SearchRun};
use crate::import::cover_art::RemoteCoverGallery;
use bae_catalogs::rate_limiter::CallPriority;

impl ImportServiceHandle {
    /// Bytes of provider art for a slot `pixels` wide on its longer side: the
    /// smallest copy the catalog serves that fills it, or the original when
    /// none does or `pixels` is `None`. The slot names its size and never a
    /// copy, so a large slot cannot be drawn from a thumbnail. Candidate
    /// preparation persists selected bytes independently; this session cache
    /// only avoids repeated transport within the process.
    ///
    /// `None` when the source serves no image at that address — an offered
    /// cover the archive turns out not to hold. The slot then renders as having
    /// no image, which is what it has, rather than as a failed load.
    pub async fn fetch_remote_image_bytes(
        &self,
        image: bae_catalogs::remote_image::RemoteImageSet,
        pixels: Option<u32>,
    ) -> Result<Option<bae_catalogs::remote_image::RemoteImage>, crate::import::ImportError> {
        Ok(self.library_manager.fetch_remote_image(image.url_covering(pixels)).await?)
    }

    /// Submit a candidate's typed search, stored before any source is asked,
    /// over whatever search the candidate had: one search per candidate at a
    /// time, because the pane shows one result area. Each source's answer
    /// lands on the stored search as it arrives, which the candidate's
    /// runtime read delivers.
    pub async fn start_candidate_search(
        &self,
        candidate_key: crate::import::CandidateKey,
        query: SearchQuery,
    ) -> Result<(), crate::import::ImportError> {
        let sources = self
            .library_manager
            .metadata_sources()
            .await
            .map_err(crate::library::LibraryError::from)?;
        let search = CandidateSearch::started(query.clone(), &sources);
        let run = self
            .runtime
            .start_search(&candidate_key, &search)
            .await
            .map_err(crate::library::LibraryError::from)?;
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
        candidate_key: crate::import::CandidateKey,
    ) -> Result<(), crate::import::ImportError> {
        let Some((run, query, sources)) = self
            .runtime
            .retry_search(&candidate_key)
            .await
            .map_err(crate::library::LibraryError::from)?
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
    pub async fn stop_asking_source(&self, source: Catalog) -> Result<(), crate::db::DbError> {
        self.runtime.switch_source_off(source).await
    }

    /// Run one source's part of a search and land it on the search's run,
    /// which takes it only while that part is still searching.
    fn spawn_source_search(
        &self,
        candidate_key: crate::import::CandidateKey,
        query: SearchQuery,
        source: Catalog,
        run: SearchRun,
    ) {
        let library_manager = self.library_manager.clone();
        let runtime = self.runtime.clone();
        let diagnostics = self.diagnostics.clone();
        self.diagnostics.spawn_reported(
            &self.runtime_handle,
            "search one source",
            async move {
                // A lookup that panics lands as failed, so its source stops
                // showing a search under way.
                let found = match bae_util::task_failure::caught(
                    "ask one source a search",
                    search_source(&library_manager, source, &query, CallPriority::Interactive),
                )
                .await
                {
                    Ok(found) => found,
                    Err(failed) => {
                        Err(crate::catalog_model::Failure::Internal(diagnostics.bug(failed).into()))
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
        candidate_key: &crate::import::CandidateKey,
        release: &crate::import::MetadataRef,
        links: crate::catalog_model::album_links::AlbumLinks,
    ) -> Result<(), crate::library::LibraryError> {
        let kept = self.runtime.land_picked(candidate_key, release, links).await?;
        if !kept.is_empty() {
            self.library_manager.keep_album_links(kept).await;
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
        source: Catalog,
    ) -> Result<GroupedSearchResults, crate::import::ImportError> {
        use crate::db::LibraryCheck;

        let results = crate::catalog_model::search::search_provider(
            &self.library_manager,
            source,
            &query,
            CallPriority::Interactive,
        )
        .await?;

        let checks: Vec<LibraryCheck> = results.iter().map(LibraryCheck::from).collect();

        let statuses = self.library_manager.check_releases_in_library(&checks).await?;

        let status_map: HashMap<String, crate::db::LibraryStatus> =
            statuses.into_iter().map(|s| (s.release_id.clone(), s)).collect();

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
        // A typed search has no candidate text behind it to rank by, so the
        // rows keep the pressing-year order alone.
        let groups = crate::import::release_group::group_results(
            crate::import::release_group::unranked(results),
            None,
        );

        Ok(GroupedSearchResults { groups, statuses })
    }

    /// Fetch complete artwork galleries when the picker opens. Dispatch on
    /// the owner: library identities and candidate release links are different
    /// records, never interchangeable string identifiers.
    pub async fn fetch_remote_covers(
        &self,
        target: crate::import::cover_art::CoverTarget,
    ) -> Result<RemoteCoverGallery, crate::import::ImportError> {
        match target {
            crate::import::cover_art::CoverTarget::Release(id) => self.release_covers(&id).await,
            crate::import::cover_art::CoverTarget::Candidate(key) => {
                self.candidate_covers(&key).await
            }
        }
    }

    async fn candidate_covers(
        &self,
        key: &crate::import::CandidateKey,
    ) -> Result<RemoteCoverGallery, crate::import::ImportError> {
        let Some(candidate) = self.get_release_candidate(key).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{key} is not a scanned candidate"),
            });
        };
        let state = self
            .library_manager
            .load_import_candidate_state(&candidate.files.content_hash())
            .await?;
        let link = match state.and_then(|state| state.release_link) {
            None => return Ok(RemoteCoverGallery::Unlinked),
            // An album's artwork is what each catalog shows for the album.
            Some(crate::import::ReleaseLink::Album(album)) => {
                return Ok(RemoteCoverGallery::Linked(
                    self.records_gallery(&album.records()).await?,
                ));
            }
            Some(crate::import::ReleaseLink::Pressing(pressing)) => pressing,
        };
        let mut claimed_releases = Vec::new();
        for claimed in link.claimed() {
            claimed_releases.push(
                self.library_manager.load_source_release(claimed).await?.ok_or_else(|| {
                    crate::import::ImportError::Internal {
                        detail: format!(
                            "{key} names {} release {} that is not stored",
                            claimed.catalog.as_str(),
                            claimed.key
                        ),
                    }
                })?,
            );
        }
        let (primary, partners) =
            claimed_releases.split_first().expect("a pick claims at least its primary");
        Ok(RemoteCoverGallery::Linked(
            self.library_manager.pick_gallery_covers(primary, partners).await?,
        ))
    }

    async fn release_covers(
        &self,
        release_id: &str,
    ) -> Result<RemoteCoverGallery, crate::import::ImportError> {
        let records = self.library_manager.get_release_records(release_id).await?;

        if records.is_empty() {
            return Ok(RemoteCoverGallery::Unlinked);
        }
        Ok(RemoteCoverGallery::Linked(self.records_gallery(&records).await?))
    }

    /// Every image the lookup catalogs among `records` show for what each
    /// record names, each once, in the records' order.
    async fn records_gallery(
        &self,
        records: &[crate::import::ReleaseRecord],
    ) -> Result<Vec<bae_catalogs::remote_image::RemoteCover>, crate::import::ImportError> {
        let mut covers = Vec::new();
        for record in records.iter().filter(|record| Catalog::LOOKUP.contains(&record.catalog())) {
            let gallery = match record {
                crate::import::ReleaseRecord::Pressing { release, album_key, .. } => match release
                    .catalog
                {
                    Catalog::MusicBrainz => {
                        self.library_manager
                            .musicbrainz_gallery(&release.key, album_key.as_deref())
                            .await?
                    }
                    Catalog::Discogs => {
                        self.library_manager
                            .fetch_discogs_release_covers(&release.key, CallPriority::Interactive)
                            .await?
                    }
                    other => unreachable!("{} serves no artwork", other.as_str()),
                },
                crate::import::ReleaseRecord::Album { album } => match album.catalog {
                    Catalog::MusicBrainz => {
                        self.library_manager.musicbrainz_group_gallery(&album.key).await?
                    }
                    Catalog::Discogs => {
                        self.library_manager
                            .fetch_discogs_master_covers(&album.key, CallPriority::Interactive)
                            .await?
                    }
                    other => unreachable!("{} serves no artwork", other.as_str()),
                },
            };
            for cover in gallery {
                bae_catalogs::remote_image::push_unique_cover(&mut covers, cover);
            }
        }
        Ok(covers)
    }

    /// Prepare the release an explicit metadata application reads, a fetch
    /// told to `steps`. A settled lead must already have its release stored;
    /// missing it is a broken invariant. Preparation follows any related
    /// documents the release was fetched without. Ordinary pane reads and
    /// applied drafts never call this path.
    pub(super) async fn release_for_pick(
        &self,
        candidate_key: &crate::import::CandidateKey,
        release: &crate::import::MetadataRef,
        steps: &crate::import::CatalogApplySteps,
    ) -> Result<crate::catalog_model::source_release::PreparedRelease, crate::import::ImportError>
    {
        if self.is_settled_lead(candidate_key, release).await? {
            self.library_manager.load_source_release(release).await?.ok_or_else(|| {
                crate::import::ImportError::Internal {
                    detail: format!(
                        "{candidate_key} settled on {} release {} but nothing stored it",
                        release.catalog.as_str(),
                        release.key
                    ),
                }
            })?;
        }
        crate::import::service::prepare_applied_release(&self.library_manager, release, steps).await
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
        candidate_key: &crate::import::CandidateKey,
        release: &crate::import::MetadataRef,
    ) -> Result<bool, crate::import::ImportError> {
        let Some(candidate) = self.get_release_candidate(candidate_key).await? else {
            return Ok(false);
        };
        let Some(row) = self
            .library_manager
            .load_import_candidate_state(&candidate.files.content_hash())
            .await?
        else {
            return Ok(false);
        };
        let Some(crate::identify::TerminalVerdict::Found { findings, .. }) =
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
        candidate_key: crate::import::CandidateKey,
        link: crate::import::PressingLink,
        steps: crate::import::CatalogApplySteps,
    ) -> Result<u64, crate::import::ImportError> {
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
        candidate_key: crate::import::CandidateKey,
    ) -> Result<u64, crate::import::ImportError> {
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
    runtime: &crate::import::CandidateRuntime,
    diagnostics: &crate::diagnostics::Diagnostics,
    candidate_key: &crate::import::CandidateKey,
    run: SearchRun,
    source: Catalog,
    found: crate::catalog_model::search::SourceLookup,
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

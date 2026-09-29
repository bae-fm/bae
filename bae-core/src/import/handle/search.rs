use super::*;
use crate::import::candidate_search::CandidateSearch;
use crate::import::cover_art::RemoteCoverGallery;
use crate::import::search::{search_source, SearchQuery};
use crate::util::rate_limiter::CallPriority;

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
        image: crate::import::cover_art::RemoteImageSet,
        pixels: Option<u32>,
    ) -> Result<Option<crate::import::cover_art::RemoteImage>, crate::import::ImportError> {
        self.library_manager
            .fetch_remote_image(image.url_covering(pixels))
            .await
    }

    /// Submit a candidate's typed search. Fire-and-forget: the run lands on
    /// the candidate's runtime one source at a time, and every landing reaches
    /// a surface as a `CandidateRuntimeChange` for that key.
    ///
    /// A search already running for this key is superseded — one search per
    /// candidate at a time, because the pane shows one result area.
    pub fn start_candidate_search(&self, candidate_key: String, query: SearchQuery) {
        let search =
            CandidateSearch::started(query.clone(), &self.library_manager.metadata_sources());
        let sources = search.searching_sources();
        let run = self.runtime.start_search(&candidate_key, search);
        for source in sources {
            self.spawn_source_search(candidate_key.clone(), query.clone(), source, run);
        }
    }

    /// Re-ask only the sources that failed, keeping what the others found. A
    /// no-op when the candidate has no search, or none of its sources failed.
    pub fn retry_candidate_search(&self, candidate_key: String) {
        let Some((query, sources, run)) = self.runtime.retry_search(&candidate_key) else {
            debug!("retry_candidate_search: {candidate_key} has no source to re-ask");
            return;
        };
        for source in sources {
            self.spawn_source_search(candidate_key.clone(), query.clone(), source, run);
        }
    }

    /// Drop a candidate's search: its lookups stop mattering and the result
    /// area goes back to the identify verdict.
    pub fn clear_candidate_search(&self, candidate_key: String) {
        self.runtime.clear_search(&candidate_key);
    }

    /// The library has stopped asking `source`: close its part of every search
    /// running right now, wherever the person is looking.
    ///
    /// Called after the preference is written, so every search started from
    /// here on is started without the source and has nothing to close.
    pub fn stop_asking_source(&self, source: Catalog) {
        self.runtime.switch_source_off(source);
    }

    /// Run one source's part of a search and land it on the candidate's
    /// current run. The runtime holds the value the landing folds into and
    /// publishes what it leaves behind, so a superseded run writes nothing.
    fn spawn_source_search(
        &self,
        candidate_key: String,
        query: SearchQuery,
        source: Catalog,
        run: u64,
    ) {
        let library_manager = self.library_manager.clone();
        let runtime = self.runtime.clone();
        self.runtime_handle.spawn(async move {
            let found =
                search_source(&library_manager, source, &query, CallPriority::Interactive).await;
            // Superseded already: skip the library check nothing will read.
            if !runtime.search_run_is_current(&candidate_key, run) {
                return;
            }
            let outcome = match found {
                Ok(results) => {
                    crate::identify::annotate_with_library_status(results, &library_manager)
                        .await
                        .map_err(|detail| {
                            crate::signals::InternalFailure::logged(
                                "checking the library for the releases a search found",
                                detail,
                            )
                            .into()
                        })
                }
                Err(failure) => Err(failure),
            };

            if !runtime.land_search(&candidate_key, run, source, outcome) {
                debug!(
                    "{}'s {} search landed on no run; it was cleared or superseded",
                    candidate_key,
                    source.as_str()
                );
            }
        });
    }

    /// The person opened a result of `candidate_key`'s typed search: read the
    /// documents of its MusicBrainz records not opened before, and land what
    /// they state each album is on the search, so the cards join by it. The
    /// read is the one a pick reads from, so picking the result afterwards
    /// asks for nothing again. Fire-and-forget: each landing reaches a surface
    /// as a `CandidateRuntimeChange`.
    pub fn open_search_result(&self, candidate_key: String, link: crate::import::PressingLink) {
        for release in std::iter::once(link.record).chain(link.partners) {
            if !self.runtime.open_search_result(&candidate_key, &release) {
                continue;
            }
            let this = self.clone();
            let candidate_key = candidate_key.clone();
            self.runtime_handle.spawn(async move {
                let links = match crate::import::service::prepare_release(
                    &this.library_manager,
                    &release,
                    CallPriority::Interactive,
                )
                .await
                {
                    Ok(stored) => stored.album_links(),
                    Err(error) => {
                        warn!(
                            "{} release {} opened in a search could not be read; its album is not known: {error}",
                            release.catalog.as_str(),
                            release.key
                        );
                        crate::import::album_links::AlbumLinks::Unread
                    }
                };
                this.land_opened(&candidate_key, &release, links).await;
            });
        }
    }

    /// Land what `release`'s documents state its album is on
    /// `candidate_key`'s search, and keep what its album was then read to be.
    pub(super) async fn land_opened(
        &self,
        candidate_key: &str,
        release: &crate::import::MetadataRef,
        links: crate::import::album_links::AlbumLinks,
    ) {
        let kept = self.runtime.land_opened(candidate_key, release, links);
        if !kept.is_empty() {
            self.library_manager.keep_album_links(kept).await;
        }
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
        key: &str,
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
                self.library_manager
                    .load_source_release(claimed)
                    .await?
                    .ok_or_else(|| crate::import::ImportError::Internal {
                        detail: format!(
                            "{key} names {} release {} that nothing fetched",
                            claimed.catalog.as_str(),
                            claimed.key
                        ),
                    })?,
            );
        }
        let (primary, partners) = claimed_releases
            .split_first()
            .expect("a pick claims at least its primary");
        Ok(RemoteCoverGallery::Linked(
            self.library_manager
                .pick_gallery_covers(primary, partners)
                .await?,
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
        Ok(RemoteCoverGallery::Linked(
            self.records_gallery(&records).await?,
        ))
    }

    /// Every image the lookup catalogs among `records` show for what each
    /// record names, each once, in the records' order.
    async fn records_gallery(
        &self,
        records: &[crate::import::ReleaseRecord],
    ) -> Result<Vec<crate::import::cover_art::RemoteCover>, crate::import::ImportError> {
        let mut covers = Vec::new();
        for record in records
            .iter()
            .filter(|record| Catalog::LOOKUP.contains(&record.catalog()))
        {
            let gallery = match record {
                crate::import::ReleaseRecord::Pressing {
                    release, album_key, ..
                } => match release.catalog {
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
                        self.library_manager
                            .musicbrainz_group_gallery(&album.key)
                            .await?
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
                crate::import::cover_art::push_unique_cover(&mut covers, cover);
            }
        }
        Ok(covers)
    }

    /// Prepare the release an explicit metadata application reads. A settled
    /// lead must already have its release stored; missing it is a broken
    /// invariant. Preparation follows any related documents the release was
    /// fetched without. Ordinary pane reads and applied drafts never call this
    /// path.
    pub(super) async fn release_for_pick(
        &self,
        candidate_key: &str,
        release: &crate::import::MetadataRef,
    ) -> Result<crate::import::source_release::SourceRelease, crate::import::ImportError> {
        if self.is_settled_lead(candidate_key, release).await? {
            self.library_manager
                .load_source_release(release)
                .await?
                .ok_or_else(|| crate::import::ImportError::Internal {
                    detail: format!(
                        "{candidate_key} settled on {} release {} but nothing stored it",
                        release.catalog.as_str(),
                        release.key
                    ),
                })?;
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
    /// That pairing is what the writers commit together — the tracklist is read
    /// from the release stored in the same step, before the verdict — so it is
    /// also the exact condition under which the release is guaranteed to be
    /// stored.
    async fn is_settled_lead(
        &self,
        candidate_key: &str,
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
    pub async fn select_candidate_release(
        &self,
        candidate_key: String,
        link: crate::import::PressingLink,
    ) -> Result<u64, crate::import::ImportError> {
        let this = self.clone();
        self.committed(async move {
            let revision = this
                .pick_candidate_release_write(candidate_key.clone(), link)
                .await?;
            this.cancel_identification(&candidate_key);
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
        candidate_key: String,
    ) -> Result<u64, crate::import::ImportError> {
        let this = self.clone();
        self.committed(async move {
            let revision = this
                .read_candidate_file_tags_write(candidate_key.clone())
                .await?;
            this.cancel_identification(&candidate_key);
            this.announce_metadata_changed(candidate_key);
            Ok(revision)
        })
        .await
    }
}

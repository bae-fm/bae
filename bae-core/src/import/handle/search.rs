use super::*;
use crate::util::rate_limiter::CallPriority;

enum ProviderSearchParams {
    MusicBrainz(crate::musicbrainz::ReleaseSearchParams),
    Discogs(crate::discogs::client::DiscogsSearchParams),
}

impl SearchQuery {
    fn into_provider_params(self) -> ProviderSearchParams {
        match self {
            SearchQuery::General {
                artist,
                album,
                source: MetadataSource::MusicBrainz,
            } => ProviderSearchParams::MusicBrainz(mb_general_params(artist, album)),
            SearchQuery::General {
                artist,
                album,
                source: MetadataSource::Discogs,
            } => ProviderSearchParams::Discogs(discogs_general_params(artist, album)),
            SearchQuery::CatalogNumber {
                catalog_number,
                source: MetadataSource::MusicBrainz,
            } => ProviderSearchParams::MusicBrainz(crate::musicbrainz::ReleaseSearchParams {
                catalog_number: Some(catalog_number),
                ..Default::default()
            }),
            SearchQuery::CatalogNumber {
                catalog_number,
                source: MetadataSource::Discogs,
            } => ProviderSearchParams::Discogs(crate::discogs::client::DiscogsSearchParams {
                catno: Some(catalog_number),
                ..Default::default()
            }),
            SearchQuery::Barcode {
                barcode,
                source: MetadataSource::MusicBrainz,
            } => ProviderSearchParams::MusicBrainz(crate::musicbrainz::ReleaseSearchParams {
                barcode: Some(barcode),
                ..Default::default()
            }),
            SearchQuery::Barcode {
                barcode,
                source: MetadataSource::Discogs,
            } => ProviderSearchParams::Discogs(crate::discogs::client::DiscogsSearchParams {
                barcode: Some(barcode),
                ..Default::default()
            }),
        }
    }
}

fn mb_general_params(artist: String, album: String) -> crate::musicbrainz::ReleaseSearchParams {
    crate::musicbrainz::ReleaseSearchParams {
        artist: Some(artist),
        album: Some(album),
        ..Default::default()
    }
}

fn discogs_general_params(
    artist: String,
    album: String,
) -> crate::discogs::client::DiscogsSearchParams {
    crate::discogs::client::DiscogsSearchParams {
        artist: Some(artist),
        release_title: Some(album),
        ..Default::default()
    }
}

impl ImportServiceHandle {
    /// Fetch raw cover-art bytes for `url`, for the UI to render a remote cover.
    /// The bytes aren't threaded back through the import command, but the
    /// session-wide LRU cache in `cover_art` keeps them warm, so the commit
    /// worker's later download is a cache hit rather than a re-fetch.
    pub async fn fetch_cover_bytes(
        &self,
        url: String,
    ) -> Result<Vec<u8>, crate::import::ImportError> {
        let (bytes, _content_type) =
            crate::import::cover_art::download_cover_art_bytes(&url).await?;
        Ok(bytes)
    }

    fn discogs_client(&self) -> Result<DiscogsClient, crate::import::ImportError> {
        self.library_manager
            .discogs_client()?
            .ok_or(crate::import::ImportError::DiscogsNotConfigured)
    }

    /// Search for releases, check library status, and bundle the results into
    /// release-group cards in one call.
    pub async fn search_with_status(
        &self,
        query: SearchQuery,
    ) -> Result<GroupedSearchResults, crate::import::ImportError> {
        use crate::db::LibraryCheck;

        let results = match query.into_provider_params() {
            ProviderSearchParams::MusicBrainz(params) => {
                crate::import::search::search_mb(
                    &self.cover_art_archive,
                    params,
                    CallPriority::Interactive,
                )
                .await?
            }
            ProviderSearchParams::Discogs(params) => {
                let client = self.discogs_client()?;
                crate::import::search::search_discogs(&client, params, CallPriority::Interactive)
                    .await?
            }
        };

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

    /// The remote cover art options for a release: read its
    /// `release_identities` and query each source's cover endpoint. MusicBrainz
    /// yields both the per-pressing CAA cover (from `source_release_id`) and the
    /// album-level one (from `source_group_id`); Discogs yields only the
    /// per-pressing cover.
    ///
    /// Covers come back in resolution order and the picker renders them as-is.
    /// An Unknown import has no identity rows, so it returns an empty list —
    /// there's no source to query.
    pub async fn fetch_remote_covers(
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
                    for cover in self
                        .cover_art_archive
                        .fetch_candidates(
                            identity.source_release_id.as_deref(),
                            Some(identity.source_group_id.as_str()),
                        )
                        .await
                    {
                        crate::import::cover_art::push_unique_cover(&mut covers, cover);
                    }
                }
                MetadataSource::Discogs => {
                    // Discogs only exposes per-release covers via the API;
                    // no master-level cover endpoint to mirror MB's CAA.
                    let Some(rid) = &identity.source_release_id else {
                        debug!(
                            release_id,
                            source_group_id = %identity.source_group_id,
                            "skipping Discogs cover fetch: Approximate identity (no source_release_id)"
                        );
                        continue;
                    };
                    let client = match self.library_manager.discogs_client() {
                        Ok(Some(c)) => c,
                        Ok(None) => {
                            debug!(
                                release_id,
                                source_release_id = %rid,
                                "skipping Discogs cover fetch: Discogs client not configured"
                            );
                            continue;
                        }
                        Err(e) => {
                            warn!(
                                release_id,
                                source_release_id = %rid,
                                "skipping Discogs cover fetch: {e}"
                            );
                            continue;
                        }
                    };
                    match client.get_release(rid, CallPriority::Interactive).await {
                        Ok((discogs_release, _raw_json)) => {
                            if let Some(cover) = discogs_release.remote_cover() {
                                covers.push(cover);
                            }
                        }
                        Err(ref e) => {
                            warn!(
                                release_id,
                                source_release_id = %rid,
                                err = %e,
                                "Discogs cover fetch failed; skipping this source"
                            );
                        }
                    }
                }
            }
        }

        Ok(covers)
    }

    /// Prefetch for the confirmation pane: the picker's display detail, the
    /// metadata editor's seed, and the identity claim the pick implies.
    ///
    /// The seed is the commit worker's own projection — `prepare_release`, the
    /// function the worker calls, mapped into the editor's shape. So the editor
    /// shows every album artist the release credits, and an untouched artist list
    /// compares equal at commit instead of reading as a user edit that clears the
    /// junction rows.
    ///
    /// The claim comes from `candidate_key`'s identify state: which signal
    /// turned this release up decides whether the import claims the pressing or
    /// the album. A key with no identify state — a manual search, a candidate
    /// whose pipeline never ran — yields the album-level claim, which is the
    /// honest reading of "the user found this themselves".
    ///
    /// Both fetches go through the network LRU caches, so the worker's later
    /// commit-time fetch of the same release is a cache hit.
    pub async fn prefetch_release(
        &self,
        candidate_key: &str,
        release_id: &str,
        source: MetadataSource,
    ) -> Result<crate::import::search::ImportReleasePrefetch, crate::import::ImportError> {
        let detail = match source {
            MetadataSource::MusicBrainz => {
                crate::import::search::prefetch_mb_release(&self.cover_art_archive, release_id)
                    .await?
            }
            MetadataSource::Discogs => {
                let client = self.discogs_client()?;
                crate::import::search::prefetch_discogs_release(&client, release_id).await?
            }
        };

        let prepared = crate::import::service::prepare_release(
            &self.library_manager,
            &crate::import::MetadataRef::new(release_id, source),
        )
        .await?;

        let claim = self.claim_for_pick(
            candidate_key,
            &crate::import::ClaimRelease::from_detail(&detail),
        );

        let seed = crate::import::parsed_album_to_user_edit(&prepared.parsed);

        // The seed is what the commit compares against; the detail is what the
        // source said. The two describe the same release and come out the same
        // length, so the position and length the source printed ride each
        // seeded row by position. A row the detail runs out for falls back to
        // its own track number, which is the only position anyone could read.
        let source_tracks: Vec<crate::import::track_slots::SourceTrack> = seed
            .tracks
            .iter()
            .enumerate()
            .map(|(index, edit)| {
                let source = detail.tracks.get(index);
                crate::import::track_slots::SourceTrack {
                    edit: edit.clone(),
                    position: match source {
                        Some(track) => track.position.clone(),
                        None => edit.track_number.map(|n| n.to_string()).unwrap_or_default(),
                    },
                    duration_ms: source.and_then(|track| track.duration_ms),
                }
            })
            .collect();

        let slots = match self.get_candidate(candidate_key) {
            Some(super::ImportCandidateSnapshot::Folder { candidate, .. }) => {
                // One FFmpeg open per container, so it goes off the async
                // executor rather than holding it for the length of a folder.
                tokio::task::spawn_blocking(move || {
                    crate::import::track_slots::slot_table(&source_tracks, &candidate.files)
                })
                .await
                .map_err(|e| crate::import::ImportError::Internal {
                    detail: format!("slot table task failed: {e}"),
                })?
            }
            _ => crate::import::track_slots::SlotTable::empty(),
        };

        Ok(crate::import::search::ImportReleasePrefetch {
            detail,
            seed,
            claim,
            slots,
        })
    }

    /// What picking `release` under `candidate_key` claims, and where its
    /// metadata comes from.
    ///
    /// Both surfaces that pick a release land here — the import confirm pane
    /// through [`Self::prefetch_release`], and re-identify directly, since it
    /// commits from the pick and never prefetches. The evidence is the
    /// candidate's own identify state, so no caller supplies or interprets it.
    pub fn claim_for_pick(
        &self,
        candidate_key: &str,
        release: &crate::import::ClaimRelease,
    ) -> crate::import::ClaimLine {
        crate::import::claim_line(&self.identify_state(candidate_key), release)
    }

    /// A candidate's identify state, or `Idle` for a key the service has
    /// recorded nothing against. Absence is the designed initial state, not an
    /// error: a folder whose pipeline hasn't run and a re-identify key opened
    /// this instant both read as "nothing matched yet".
    fn identify_state(&self, candidate_key: &str) -> crate::identify::IdentifyState {
        match self.get_candidate(candidate_key) {
            Some(
                super::ImportCandidateSnapshot::Folder { runtime, .. }
                | super::ImportCandidateSnapshot::Runtime { runtime, .. },
            ) => runtime.identify_state,
            Some(super::ImportCandidateSnapshot::Invalid(_)) | None => {
                crate::identify::IdentifyState::Idle
            }
        }
    }
}

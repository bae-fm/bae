use super::*;
use crate::util::rate_limiter::CallPriority;

/// The row identity the mapping table's tracks carry when a picked release
/// names them.
const IMPORT_TRACK_ID_PREFIX: &str = "import-track";

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
    /// Bytes of provider art at `url` — art that isn't in the library, so the UI
    /// renders it straight from the remote source. The bytes aren't threaded back
    /// through the import command, but the session cache keeps them warm, so the
    /// commit worker's later download is a cache hit rather than a re-fetch. The
    /// returned validator identifies this exact content, so a UI keyed on it
    /// re-decodes only when the bytes at the URL actually change.
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

    /// Search for releases, check library status, and bundle the results into
    /// release-group cards in one call.
    pub async fn search_with_status(
        &self,
        query: SearchQuery,
    ) -> Result<GroupedSearchResults, crate::import::ImportError> {
        use crate::db::LibraryCheck;

        let results = match query.into_provider_params() {
            ProviderSearchParams::MusicBrainz(params) => {
                crate::import::search::search_mb(params, CallPriority::Interactive).await?
            }
            ProviderSearchParams::Discogs(params) => {
                self.library_manager
                    .search_discogs(params, CallPriority::Interactive)
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

    /// The remote cover art options for a release, from its
    /// `release_identities`. A MusicBrainz identity offers both the archive's
    /// per-pressing image (from `source_release_id`) and its album-level one
    /// (from `source_group_id`) at the archive's fixed addresses for them —
    /// costing no request, since the picker's thumbnail fetch is what resolves
    /// each. Discogs has no address to derive, so its per-pressing cover comes
    /// from fetching the release document that carries the URL.
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
                    let release_cover = identity
                        .source_release_id
                        .as_deref()
                        .map(crate::import::cover_art::RemoteCover::musicbrainz_release);
                    let group_cover =
                        crate::import::cover_art::RemoteCover::musicbrainz_release_group(
                            &identity.source_group_id,
                        );
                    for cover in release_cover.into_iter().chain([group_cover]) {
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
                    match self
                        .library_manager
                        .fetch_discogs_release_cover(rid, CallPriority::Interactive)
                        .await
                    {
                        Ok(Some(cover)) => covers.push(cover),
                        Ok(None) => {
                            debug!(
                                release_id,
                                source_release_id = %rid,
                                "skipping Discogs cover fetch: Discogs is not configured"
                            );
                        }
                        Err(error) => {
                            warn!(
                                release_id,
                                source_release_id = %rid,
                                "Discogs cover fetch failed; skipping this source: {error}"
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
    /// The claim is `level` — the user's assertion, carried in from the pick.
    /// `candidate_key`'s identify state supplies only the evidence clause the
    /// line reads: which signal turned this release up. A key with no identify
    /// state — a manual search, a candidate whose pipeline never ran — reads as
    /// "found by searching", which is the honest account of it.
    ///
    /// Both shapes are projected from one set of stored documents, so the
    /// picker and the commit describe the same release from the same bytes.
    pub async fn prefetch_release(
        &self,
        candidate_key: &str,
        release_id: &str,
        source: MetadataSource,
        level: crate::import::ClaimLevel,
    ) -> Result<crate::import::search::ImportReleasePrefetch, crate::import::ImportError> {
        let release = crate::import::MetadataRef::new(release_id, source);
        let payloads = self.payloads_for_pick(candidate_key, &release).await?;
        let detail = payloads.detail()?;
        let parsed = payloads.parsed(self.clock.as_ref(), self.ids.as_ref())?;

        // The evidence behind the claim survives a restart through the stored
        // verdict: with no live run the resumed state carries the same
        // matches, so a disc-ID-proven pick never demotes to "metadata only"
        // just because the app was reopened.
        let claim = crate::import::claim_line(
            &self.identify_state_or_resumed(candidate_key).await?,
            &crate::import::ClaimRelease::from_detail(&detail),
            level,
        );

        let seed = crate::import::parsed_album_to_user_edit(&parsed);

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

        let mapping = match self.get_candidate(candidate_key)? {
            Some(super::ImportCandidateSnapshot::Folder { candidate, .. }) => {
                // One FFmpeg open per container, so it goes off the async
                // executor rather than holding it for the length of a folder.
                tokio::task::spawn_blocking(move || {
                    let slots =
                        crate::import::track_slots::slot_table(&source_tracks, &candidate.files);
                    crate::import::mapping::mapping_table(
                        &candidate.files,
                        Some(crate::import::mapping::PickedTracklist {
                            slots: &slots,
                            track_id_prefix: IMPORT_TRACK_ID_PREFIX,
                            source: crate::import::mapping::TracklistSource::Release,
                        }),
                    )
                })
                .await
                .map_err(|e| crate::import::ImportError::Internal {
                    detail: format!("mapping table task failed: {e}"),
                })?
            }
            _ => crate::import::mapping::MappingTable::empty(),
        };

        Ok(crate::import::search::ImportReleasePrefetch {
            detail,
            seed,
            claim,
            mapping,
        })
    }

    /// The documents behind a pick, and where they come from.
    ///
    /// A pick that is the candidate's settled lead **reads** them: identification
    /// stored them before it stored the verdict that named this release, so they
    /// are there, and a miss is a broken invariant rather than a cold cache.
    /// Re-fetching on a miss would serve the pane and hide the break, so it
    /// fails instead.
    ///
    /// Every other pick — another pressing in a list, a manual search result, a
    /// release being re-identified — is one identification never fetched. It
    /// goes through [`crate::import::service::prepare_release`], which reads
    /// whatever is archived and pays for the rest, so opening it a second time
    /// is local too.
    async fn payloads_for_pick(
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
            self.get_candidate(candidate_key)?
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

    /// What holding `release` at `level` under `candidate_key` claims, and
    /// where its metadata comes from.
    ///
    /// Both surfaces that pick a release land here — the import confirm pane
    /// through [`Self::prefetch_release`], and re-identify directly, since it
    /// commits from the pick and never prefetches. The evidence is the
    /// candidate's own identify state, so no caller supplies or interprets it.
    pub fn claim_for_pick(
        &self,
        candidate_key: &str,
        release: &crate::import::ClaimRelease,
        level: crate::import::ClaimLevel,
    ) -> crate::import::ClaimLine {
        crate::import::claim_line(&self.identify_state(candidate_key), release, level)
    }

    /// Decide a candidate's identity: persist the choice, then build the same
    /// answer [`Self::candidate_answer`] serves — so a fresh launch renders
    /// exactly what the click rendered. Persisting comes first: the decision
    /// is the durable part, and a bundle that fails to build (a manual pick
    /// whose fetch drops) is re-derived by the next open rather than lost
    /// with the choice.
    ///
    /// The surfaces are told once the answer has been built, whether or not it
    /// built: the sidebar row leads with the picked release read back out of
    /// the documents this fetches, so announcing the decision before they are
    /// archived would leave that row on the folder name with nothing to move
    /// it off.
    pub async fn pick_candidate_identity(
        &self,
        candidate_key: String,
        pick: crate::import::IdentityPick,
    ) -> Result<crate::import::DecidedIdentity, crate::import::ImportError> {
        self.set_candidate_identity_pick(candidate_key.clone(), pick.clone())
            .await?;
        let answer = self.answer_for_pick(&candidate_key, pick).await;
        self.announce_identity_pick(candidate_key);
        answer
    }

    /// The candidate's decided identity read back with everything the pane
    /// seeds from it, or `None` while nothing is decided — the selection
    /// query, and the whole of "resume": a stored decision answers exactly
    /// like the click that made it did.
    pub async fn candidate_answer(
        &self,
        candidate_key: String,
    ) -> Result<Option<crate::import::DecidedIdentity>, crate::import::ImportError> {
        let Some(super::ImportCandidateSnapshot::Folder { candidate, .. }) =
            self.get_candidate(&candidate_key)?
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
            // A read racing an edit write: serve nothing rather than mixed
            // state — the next tick re-asks against the settled row.
            tracing::debug!(
                "{candidate_key} pick is at revision {}, candidate at {}; not resuming it",
                row.file_edits.revision,
                candidate.file_edit_revision
            );
            return Ok(None);
        }
        let Some(pick) = row.identity_pick else {
            return Ok(None);
        };
        self.answer_for_pick(&candidate_key, pick).await.map(Some)
    }

    /// Build the answer a pick stands for — the shared half of the command
    /// and the query above.
    async fn answer_for_pick(
        &self,
        candidate_key: &str,
        pick: crate::import::IdentityPick,
    ) -> Result<crate::import::DecidedIdentity, crate::import::ImportError> {
        match pick {
            crate::import::IdentityPick::Release {
                source,
                release_id,
                claim,
            } => {
                let prefetch = self
                    .prefetch_release(candidate_key, &release_id, source, claim)
                    .await?;
                Ok(crate::import::DecidedIdentity::Release {
                    source,
                    release_id,
                    prefetch,
                })
            }
            crate::import::IdentityPick::Unknown => {
                let (seed, mapping) = self.unknown_mapping(candidate_key.to_string()).await?;
                Ok(crate::import::DecidedIdentity::Unknown { seed, mapping })
            }
        }
    }

    /// A candidate's identify state, or `Idle` for a key the service has
    /// recorded nothing against. Absence is the designed initial state, not an
    /// error: a folder whose pipeline hasn't run and a re-identify key opened
    /// this instant both read as "nothing matched yet".
    fn identify_state(&self, candidate_key: &str) -> crate::identify::IdentifyState {
        self.runtime.runtime_for(candidate_key).identify_state
    }

    /// A candidate's identify state with the stored verdict standing in when
    /// no run is live — the restart case, where the runtime map is empty but
    /// the answer is on disk. `Idle` only when there is genuinely nothing.
    pub(crate) async fn identify_state_or_resumed(
        &self,
        candidate_key: &str,
    ) -> Result<crate::identify::IdentifyState, crate::import::ImportError> {
        let live = self.identify_state(candidate_key);
        if !matches!(live, crate::identify::IdentifyState::Idle) {
            return Ok(live);
        }
        Ok(self
            .resumed_identify_state(candidate_key)
            .await?
            .unwrap_or(crate::identify::IdentifyState::Idle))
    }

    /// The stored verdict describing `candidate_key`'s current file shape —
    /// or `None` when nothing is stored, the stored row describes an earlier
    /// file-edit revision, or the key is not a scanned folder candidate.
    pub(crate) async fn stored_verdict(
        &self,
        candidate_key: &str,
    ) -> Result<Option<crate::identify::TerminalVerdict>, crate::import::ImportError> {
        let Some(super::ImportCandidateSnapshot::Folder { candidate, .. }) =
            self.get_candidate(candidate_key)?
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

    /// The identify state a candidate's stored verdict stands back up as,
    /// with a live library check of every release it names — or `None` when
    /// nothing is stored (or the key is not a scanned folder candidate) and a
    /// run is the only way to answer it.
    pub(crate) async fn resumed_identify_state(
        &self,
        candidate_key: &str,
    ) -> Result<Option<crate::identify::IdentifyState>, crate::import::ImportError> {
        let Some(verdict) = self.stored_verdict(candidate_key).await? else {
            return Ok(None);
        };
        let checks: Vec<crate::db::LibraryCheck> = verdict
            .named_releases()
            .into_iter()
            .map(crate::db::LibraryCheck::from)
            .collect();
        let statuses = self
            .library_manager
            .check_releases_in_library(&checks)
            .await?;
        let status_of = |result: &crate::import::search::MetadataResult| {
            statuses
                .iter()
                .find(|status| status.release_id == result.release_id)
                .cloned()
                .expect("every release the verdict names was just checked")
        };
        Ok(Some(verdict.resume_state(&status_of)))
    }
}

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
    /// A release without an external identity has no identity rows, so it returns an empty list —
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
                    let release_cover = crate::import::cover_art::RemoteCover::musicbrainz_release(
                        &identity.source_release_id,
                    );
                    let group_cover =
                        crate::import::cover_art::RemoteCover::musicbrainz_release_group(
                            &identity.source_group_id,
                        );
                    for cover in [release_cover, group_cover] {
                        crate::import::cover_art::push_unique_cover(&mut covers, cover);
                    }
                }
                MetadataSource::Discogs => {
                    // Discogs only exposes per-release covers via the API;
                    // no master-level cover endpoint to mirror MB's CAA.
                    let rid = &identity.source_release_id;
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
        if let crate::import::MetadataProvenance::ExternalRelease {
            source, release_id, ..
        } = &provenance
        {
            self.payloads_for_provenance(
                &candidate_key,
                &crate::import::MetadataRef::new(release_id.clone(), *source),
            )
            .await?;
        }
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

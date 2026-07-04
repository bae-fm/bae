use super::*;

impl ImportServiceHandle {
    /// Fetch raw cover-art bytes for `url`. The UI calls this when it
    /// needs to render a remote cover — the bytes are not threaded
    /// back through the import command, but a session-wide LRU cache
    /// in `cover_art` keeps the URL's bytes warm so the commit
    /// worker's later download is a cache hit, not a re-fetch.
    pub async fn fetch_cover_bytes(&self, url: String) -> Result<Vec<u8>, String> {
        let (bytes, _content_type) =
            crate::import::cover_art::download_cover_art_bytes(&url).await?;
        Ok(bytes)
    }

    fn discogs_client(&self) -> Result<DiscogsClient, String> {
        self.library_manager
            .discogs_client()?
            .ok_or_else(|| "Discogs API key not configured".to_string())
    }

    fn discogs_search_error(e: crate::discogs::client::DiscogsError) -> String {
        format!("Discogs search failed: {e}")
    }

    /// Search for releases, check library status, and bundle the results into
    /// release-group cards in one call.
    pub async fn search_with_status(
        &self,
        query: SearchQuery,
    ) -> Result<GroupedSearchResults, String> {
        use crate::db::LibraryCheck;

        let results = match query {
            SearchQuery::General {
                artist,
                album,
                source,
            } => match source {
                MetadataSource::MusicBrainz => {
                    self.search_musicbrainz(artist, album, None, None).await?
                }
                MetadataSource::Discogs => {
                    let client = self.discogs_client()?;
                    crate::import::search::search_discogs(&client, artist, album, None, None)
                        .await
                        .map_err(Self::discogs_search_error)?
                }
            },
            SearchQuery::CatalogNumber {
                catalog_number,
                source,
            } => match source {
                MetadataSource::MusicBrainz => {
                    crate::import::search::search_mb_by_catalog_number(
                        &self.cover_art_archive,
                        catalog_number,
                    )
                    .await?
                }
                MetadataSource::Discogs => {
                    let client = self.discogs_client()?;
                    crate::import::search::search_discogs_by_catalog_number(&client, catalog_number)
                        .await
                        .map_err(Self::discogs_search_error)?
                }
            },
            SearchQuery::Barcode { barcode, source } => match source {
                MetadataSource::MusicBrainz => {
                    crate::import::search::search_mb_by_barcode(&self.cover_art_archive, barcode)
                        .await?
                }
                MetadataSource::Discogs => {
                    let client = self.discogs_client()?;
                    crate::import::search::search_discogs_by_barcode(&client, barcode)
                        .await
                        .map_err(Self::discogs_search_error)?
                }
            },
        };

        let checks: Vec<LibraryCheck> = results.iter().map(LibraryCheck::from).collect();

        let statuses = self
            .library_manager
            .check_releases_in_library(&checks)
            .await
            .map_err(|e| format!("Failed to check library status: {e}"))?;

        let status_map: HashMap<String, crate::db::LibraryStatus> = statuses
            .into_iter()
            .map(|s| (s.release_id.clone(), s))
            .collect();

        // `check_releases_in_library` returns exactly one status per input
        // check, keyed by `release_id`, so a miss is a broken invariant —
        // surface it rather than fabricating a "not in library" default that
        // would silently misclassify a real failure.
        let mut statuses = Vec::with_capacity(results.len());
        for r in &results {
            let status = status_map
                .get(&r.release_id)
                .cloned()
                .ok_or_else(|| format!("library status missing for release {}", r.release_id))?;
            statuses.push(status);
        }

        // Grouping is the UI's shape, so it happens here in core: the search
        // surface renders one card per release group with the pressings beneath.
        let groups = crate::import::release_group::group_results(results);

        Ok(GroupedSearchResults { groups, statuses })
    }

    pub async fn search_discogs(
        &self,
        artist: String,
        album: String,
        year: Option<String>,
        label: Option<String>,
    ) -> Result<Vec<crate::import::search::MetadataResult>, String> {
        let client = self.discogs_client()?;
        crate::import::search::search_discogs(&client, artist, album, year, label)
            .await
            .map_err(Self::discogs_search_error)
    }

    pub async fn search_musicbrainz(
        &self,
        artist: String,
        album: String,
        year: Option<String>,
        label: Option<String>,
    ) -> Result<Vec<crate::import::search::MetadataResult>, String> {
        crate::import::search::search_musicbrainz(
            &self.cover_art_archive,
            artist,
            album,
            year,
            label,
        )
        .await
    }

    /// Fetch available remote cover art options for a release.
    /// Reads `release_identities` for the release and queries each
    /// source's cover endpoint:
    ///
    /// - **MusicBrainz** — `(source, source_release_id)` pulls the
    ///   per-pressing CAA cover; `(source, source_group_id)` pulls the
    ///   release-group (album-level) CAA cover.
    /// - **Discogs** — `(source, source_release_id)` pulls the per-pressing
    ///   cover via the Discogs API.
    ///
    /// Returns covers in the order they were resolved; the picker uses
    /// this list as-is. Unknown imports (no identity rows) return an
    /// empty list — no source to query.
    pub async fn fetch_remote_covers(
        &self,
        release_id: &str,
    ) -> Result<Vec<crate::import::cover_art::RemoteCover>, String> {
        let identities = self
            .library_manager
            .get_release_identities(release_id)
            .await
            .map_err(|e| format!("{e}"))?;

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
                    match client.get_release(rid).await {
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

    /// Prefetch for the confirmation pane. Fetches the release and builds
    /// the picker/confirm detail — no DB-shape mapping. The fetch goes
    /// through the network LRU caches, so the worker's later commit-time
    /// fetch hits cache for the same response.
    pub async fn prefetch_release(
        &self,
        release_id: &str,
        source: MetadataSource,
    ) -> Result<crate::import::search::ImportSearchReleaseDetail, String> {
        match source {
            MetadataSource::MusicBrainz => {
                crate::import::search::prefetch_mb_release(&self.cover_art_archive, release_id)
                    .await
            }
            MetadataSource::Discogs => {
                let client = self.discogs_client()?;
                crate::import::search::prefetch_discogs_release(&client, release_id).await
            }
        }
    }
}

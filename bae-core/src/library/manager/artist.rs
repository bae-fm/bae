//! Artist domain operations for [`LibraryManager`].

use super::*;

#[derive(Debug, Clone)]
pub(crate) struct ResolvedImportArtists {
    pub ids: Vec<String>,
    pub inserts: Vec<DbArtist>,
    pub external_id_updates: Vec<(String, DbArtist)>,
}

impl LibraryManager {
    pub async fn insert_artist(&self, artist: &DbArtist) -> Result<(), LibraryError> {
        self.database.insert_artist(artist).await?;
        Ok(())
    }

    pub async fn get_artists_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<DbArtist>, LibraryError> {
        Ok(self.database.get_artists_for_album(album_id).await?)
    }

    pub async fn get_artists_for_track(
        &self,
        track_id: &str,
    ) -> Result<Vec<DbArtist>, LibraryError> {
        Ok(self.database.get_artists_for_track(track_id).await?)
    }

    pub async fn get_artist_by_id(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self.database.find_artist_by_id(artist_id).await?)
    }

    /// Search existing artists by library ID, provider ID, display name, or
    /// sort name. The parsed query and result limit are shared with library
    /// search, while this result set includes artists without album links.
    pub async fn search_artists(
        &self,
        query: &crate::library::LibrarySearchQuery,
    ) -> Result<Vec<ArtistSearchResult>, LibraryError> {
        let artists = self
            .database
            .search_artists(query.as_str(), crate::library::SEARCH_RESULT_LIMIT)
            .await?;
        let artist_ids = artists
            .iter()
            .map(|artist| artist.id.clone())
            .collect::<Vec<_>>();
        let images = self.artist_image_refs(&artist_ids).await?;
        Ok(artists
            .into_iter()
            .map(|artist| {
                let image = images.get(&artist.id).cloned();
                ArtistSearchResult { artist, image }
            })
            .collect())
    }

    pub async fn get_artist_count(&self) -> Result<u64, LibraryError> {
        Ok(self.database.get_artist_count().await?)
    }

    pub async fn get_artist_page(
        &self,
        sort: &[crate::db::ArtistSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<ArtistSummary>, LibraryError> {
        let raw = self.database.get_artist_page(sort, offset, limit).await?;
        let artist_ids: Vec<String> = raw.iter().map(|a| a.artist.id.clone()).collect();
        let images = self.artist_image_refs(&artist_ids).await?;
        Ok(raw
            .into_iter()
            .map(|artist| {
                let image = images.get(&artist.artist.id).cloned();
                ArtistSummary::from_raw(artist, image)
            })
            .collect())
    }

    pub(crate) fn subscribe_artist_page(
        &self,
        sort: &[crate::db::ArtistSortCriterion],
        offset: u64,
        limit: u64,
    ) -> coven::LiveQuery<crate::db::ArtistPageProjection> {
        self.database.subscribe_artist_page(sort, offset, limit)
    }

    pub(crate) fn resolve_artist_page(
        &self,
        projection: crate::db::ArtistPageProjection,
    ) -> (Vec<ArtistSummary>, u64) {
        let images = projection
            .image_versions
            .into_iter()
            .map(|(id, version)| {
                let image = ImageRef {
                    id: id.clone(),
                    version,
                    image_type: crate::db::LibraryImageType::Artist,
                };
                (id, image)
            })
            .collect::<HashMap<_, _>>();
        let rows = projection
            .rows
            .into_iter()
            .map(|row| {
                let image = images.get(&row.artist.id).cloned();
                ArtistSummary::from_raw(row, image)
            })
            .collect();
        (rows, projection.total_count)
    }

    pub async fn get_artist_detail(
        &self,
        artist_id: &str,
    ) -> Result<Option<ArtistDetail>, LibraryError> {
        let Some(raw) = self.database.find_artist_detail(artist_id).await? else {
            return Ok(None);
        };
        let images = self
            .artist_image_refs(std::slice::from_ref(&raw.artist.artist.id))
            .await?;
        let release_ids: Vec<String> = raw
            .albums
            .iter()
            .flat_map(|a| a.release_ids.iter().cloned())
            .collect();
        let covers = self.cover_refs(&release_ids).await?;
        let albums = raw
            .albums
            .into_iter()
            .map(|album| AlbumSummary::from_raw(album, |rid| covers.get(rid).cloned()))
            .collect();
        let image = images.get(&raw.artist.artist.id).cloned();
        Ok(Some(ArtistDetail {
            artist: ArtistSummary::from_raw(raw.artist, image),
            albums,
        }))
    }

    pub(crate) fn subscribe_artist_detail(
        &self,
        artist_id: &str,
    ) -> coven::LiveQuery<crate::db::ArtistDetailProjection> {
        self.database.subscribe_artist_detail(artist_id)
    }

    pub(crate) fn resolve_artist_detail_projection(
        &self,
        projection: crate::db::ArtistDetailProjection,
    ) -> Option<ArtistDetail> {
        let raw = projection.detail?;
        let images = projection
            .image_versions
            .into_iter()
            .map(|(id, version)| {
                (
                    id.clone(),
                    ImageRef {
                        id,
                        version,
                        image_type: crate::db::LibraryImageType::Artist,
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let covers = projection
            .cover_versions
            .into_iter()
            .map(|(id, version)| {
                (
                    id.clone(),
                    ImageRef {
                        id,
                        version,
                        image_type: crate::db::LibraryImageType::Cover,
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let image = images.get(&raw.artist.artist.id).cloned();
        Some(ArtistDetail {
            artist: ArtistSummary::from_raw(raw.artist, image),
            albums: raw
                .albums
                .into_iter()
                .map(|album| AlbumSummary::from_raw(album, |id| covers.get(id).cloned()))
                .collect(),
        })
    }

    /// Resolve each parsed artist to an existing DB row, or to a row for finalize to
    /// insert. `ids` comes back in input order, so a caller can zip it with `artists`
    /// to map parsed IDs to DB IDs.
    ///
    /// Lookup chain: the cross-source Various Artists alias,
    /// `discogs_artist_id`, then `musicbrainz_artist_id`; failing those, a
    /// deferred insert. A match carries any new source IDs out as a deferred
    /// COALESCE update.
    pub(crate) async fn resolve_artists_for_import(
        &self,
        artists: &[DbArtist],
    ) -> Result<ResolvedImportArtists, LibraryError> {
        self.resolve_artists_for_import_with_existing(artists, &std::collections::HashSet::new())
            .await
    }

    pub(crate) async fn resolve_artists_for_import_with_existing(
        &self,
        artists: &[DbArtist],
        explicit_existing_ids: &std::collections::HashSet<String>,
    ) -> Result<ResolvedImportArtists, LibraryError> {
        let mut ids = Vec::with_capacity(artists.len());
        let mut inserts = Vec::new();
        let mut external_id_updates = Vec::new();

        for artist in artists {
            if explicit_existing_ids.contains(&artist.id) {
                ids.push(artist.id.clone());
                continue;
            }
            let existing = self.find_existing_artist_for_import(artist).await?;
            let actual_id = if let Some(existing_artist) = existing {
                let id = existing_artist.id;
                external_id_updates.push((id.clone(), artist.clone()));
                id
            } else {
                inserts.push(artist.clone());
                artist.id.clone()
            };

            ids.push(actual_id);
        }

        Ok(ResolvedImportArtists {
            ids,
            inserts,
            external_id_updates,
        })
    }

    pub(crate) async fn resolve_artist_assignments(
        &self,
        assignments: &[crate::import::ArtistAssignment],
    ) -> Result<ResolvedImportArtists, LibraryError> {
        let now = self.clock.now();
        let mut resolved = ResolvedImportArtists {
            ids: Vec::with_capacity(assignments.len()),
            inserts: Vec::new(),
            external_id_updates: Vec::new(),
        };
        for assignment in assignments {
            match assignment {
                crate::import::ArtistAssignment::Existing { artist } => {
                    if self
                        .database
                        .find_artist_by_id(&artist.artist_id)
                        .await?
                        .is_none()
                    {
                        return Err(LibraryError::Import(format!(
                            "artist '{}' no longer exists",
                            artist.artist_id
                        )));
                    }
                    resolved.ids.push(artist.artist_id.clone());
                }
                crate::import::ArtistAssignment::New { seed } => {
                    let artist = DbArtist {
                        id: self.ids.new_id(),
                        name: seed.name.clone(),
                        sort_name: seed.sort_name.clone(),
                        discogs_artist_id: seed.discogs_artist_id.clone(),
                        musicbrainz_artist_id: seed.musicbrainz_artist_id.clone(),
                        created_at: now,
                    };
                    let one = self.resolve_artists_for_import(&[artist]).await?;
                    resolved.ids.extend(one.ids);
                    resolved.inserts.extend(one.inserts);
                    resolved.external_id_updates.extend(one.external_id_updates);
                }
            }
        }
        Ok(resolved)
    }

    /// Resolve each parsed artist to an existing DB row, inserting immediately when
    /// there is none. For metadata edits, whose write path updates an
    /// already-finalized release.
    pub async fn find_or_create_artists(
        &self,
        artists: &[DbArtist],
    ) -> Result<Vec<String>, LibraryError> {
        let resolved = self.resolve_artists_for_import(artists).await?;
        for artist in &resolved.inserts {
            self.database.insert_artist(artist).await?;
        }
        for (artist_id, artist) in &resolved.external_id_updates {
            self.database
                .update_artist_external_ids(
                    artist_id,
                    artist.discogs_artist_id.as_deref(),
                    artist.musicbrainz_artist_id.as_deref(),
                    artist.sort_name.as_deref(),
                )
                .await?;
        }
        Ok(resolved.ids)
    }

    async fn find_existing_artist_for_import(
        &self,
        artist: &DbArtist,
    ) -> Result<Option<DbArtist>, LibraryError> {
        // Match any known Various Artists ID across sources, so Discogs "Various"
        // merges with MusicBrainz "Various Artists".
        if artist.is_various_artists() {
            let va = &crate::db::VARIOUS_ARTISTS;
            if let Some(existing) = self.database.get_artist_by_discogs_id(va.discogs).await? {
                return Ok(Some(existing));
            }
            if let Some(existing) = self.database.get_artist_by_mb_id(va.musicbrainz).await? {
                return Ok(Some(existing));
            }
        }

        let by_discogs = match artist.discogs_artist_id.as_deref() {
            Some(id) => self.database.get_artist_by_discogs_id(id).await?,
            None => None,
        };
        let by_musicbrainz = match artist.musicbrainz_artist_id.as_deref() {
            Some(id) => self.database.get_artist_by_mb_id(id).await?,
            None => None,
        };
        let matched = match (by_discogs, by_musicbrainz) {
            (Some(discogs), Some(musicbrainz)) if discogs.id != musicbrainz.id => {
                return Err(LibraryError::Import(format!(
                    "artist '{}' has source IDs belonging to different library artists",
                    artist.name
                )))
            }
            (Some(artist), _) | (_, Some(artist)) => Some(artist),
            (None, None) => None,
        };
        if let Some(existing) = matched.as_ref() {
            if source_id_conflicts(
                existing.discogs_artist_id.as_deref(),
                artist.discogs_artist_id.as_deref(),
            ) || source_id_conflicts(
                existing.musicbrainz_artist_id.as_deref(),
                artist.musicbrainz_artist_id.as_deref(),
            ) {
                return Err(LibraryError::Import(format!(
                    "artist '{}' has conflicting source IDs",
                    artist.name
                )));
            }
        }
        Ok(matched)
    }
}

fn source_id_conflicts(existing: Option<&str>, incoming: Option<&str>) -> bool {
    matches!((existing, incoming), (Some(a), Some(b)) if a != b)
}

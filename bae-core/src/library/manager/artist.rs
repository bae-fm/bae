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

    pub async fn merge_import_artist_identity_conflict(
        &self,
        content_hash: &str,
        surviving_artist_id: &str,
    ) -> Result<(), LibraryError> {
        self.database
            .merge_import_artist_identity_conflict(content_hash, surviving_artist_id)
            .await?;
        Ok(())
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
    /// Every exact Discogs and MusicBrainz match is considered across both the
    /// committed library and rows this import is waiting to insert. Pending
    /// rows are folded into one committed match when there is one, or into the
    /// first pending row otherwise. Two different committed matches are the
    /// recoverable identity conflict surfaced to the import pane.
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
            let existing = self
                .find_existing_artist_for_import(artist, &external_id_updates)
                .await?;
            let pending_indices = pending_artist_indices(&inserts, artist);
            let actual_id = match existing {
                Some(existing) => {
                    let id = existing.id.clone();
                    let mut merged = existing;
                    let pending_ids = pending_indices
                        .iter()
                        .map(|index| inserts[*index].id.clone())
                        .collect::<std::collections::HashSet<_>>();
                    for index in &pending_indices {
                        merge_artist_metadata(&mut merged, &inserts[*index])?;
                    }
                    merge_artist_metadata(&mut merged, artist)?;
                    if !pending_ids.is_empty() {
                        inserts.retain(|pending| !pending_ids.contains(&pending.id));
                        for resolved_id in &mut ids {
                            if pending_ids.contains(resolved_id) {
                                resolved_id.clone_from(&id);
                            }
                        }
                    }
                    stage_artist_update(&mut external_id_updates, id.clone(), merged);
                    id
                }
                None => match pending_indices.first().copied() {
                    Some(survivor_index) => {
                        let survivor_id = inserts[survivor_index].id.clone();
                        let absorbed_ids = pending_indices
                            .iter()
                            .skip(1)
                            .map(|index| inserts[*index].id.clone())
                            .collect::<std::collections::HashSet<_>>();
                        let absorbed = pending_indices
                            .iter()
                            .skip(1)
                            .map(|index| inserts[*index].clone())
                            .collect::<Vec<_>>();
                        for pending in &absorbed {
                            merge_artist_metadata(&mut inserts[survivor_index], pending)?;
                        }
                        merge_artist_metadata(&mut inserts[survivor_index], artist)?;
                        if !absorbed_ids.is_empty() {
                            inserts.retain(|pending| !absorbed_ids.contains(&pending.id));
                            for resolved_id in &mut ids {
                                if absorbed_ids.contains(resolved_id) {
                                    resolved_id.clone_from(&survivor_id);
                                }
                            }
                        }
                        survivor_id
                    }
                    None => {
                        inserts.push(artist.clone());
                        artist.id.clone()
                    }
                },
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
        let mut ids = vec![None; assignments.len()];
        let mut new_positions = Vec::new();
        let mut new_artists = Vec::new();
        for (position, assignment) in assignments.iter().enumerate() {
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
                    ids[position] = Some(artist.artist_id.clone());
                }
                crate::import::ArtistAssignment::New { seed } => {
                    new_positions.push(position);
                    new_artists.push(DbArtist {
                        id: self.ids.new_id(),
                        name: seed.name.clone(),
                        sort_name: seed.sort_name.clone(),
                        discogs_artist_id: seed.discogs_artist_id.clone(),
                        musicbrainz_artist_id: seed.musicbrainz_artist_id.clone(),
                        created_at: now,
                    });
                }
            }
        }
        let resolved_new = self.resolve_artists_for_import(&new_artists).await?;
        for (position, artist_id) in new_positions.into_iter().zip(&resolved_new.ids) {
            ids[position] = Some(artist_id.clone());
        }
        Ok(ResolvedImportArtists {
            ids: ids
                .into_iter()
                .map(|id| id.expect("every artist assignment was resolved"))
                .collect(),
            inserts: resolved_new.inserts,
            external_id_updates: resolved_new.external_id_updates,
        })
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
        staged_updates: &[(String, DbArtist)],
    ) -> Result<Option<DbArtist>, LibraryError> {
        // Match any known Various Artists ID across sources, so Discogs "Various"
        // merges with MusicBrainz "Various Artists".
        if artist.is_various_artists() {
            let va = &crate::db::VARIOUS_ARTISTS;
            if let Some(existing) = self.database.get_artist_by_discogs_id(va.discogs).await? {
                return Ok(Some(
                    staged_artist_by_id(staged_updates, &existing.id).unwrap_or(existing),
                ));
            }
            if let Some(existing) = self.database.get_artist_by_mb_id(va.musicbrainz).await? {
                return Ok(Some(
                    staged_artist_by_id(staged_updates, &existing.id).unwrap_or(existing),
                ));
            }
        }

        let by_discogs = match artist.discogs_artist_id.as_deref() {
            Some(id) => exact_committed_artist(
                "Discogs",
                &artist.name,
                id,
                self.database.get_artist_by_discogs_id(id).await?,
                staged_updates,
                |artist| artist.discogs_artist_id.as_deref(),
            )?,
            None => None,
        };
        let by_musicbrainz = match artist.musicbrainz_artist_id.as_deref() {
            Some(id) => exact_committed_artist(
                "MusicBrainz",
                &artist.name,
                id,
                self.database.get_artist_by_mb_id(id).await?,
                staged_updates,
                |artist| artist.musicbrainz_artist_id.as_deref(),
            )?,
            None => None,
        };
        matching_artist(artist, by_discogs, by_musicbrainz)
    }
}

fn staged_artist_by_id(staged_updates: &[(String, DbArtist)], artist_id: &str) -> Option<DbArtist> {
    staged_updates
        .iter()
        .find(|(id, _)| id == artist_id)
        .map(|(_, artist)| artist.clone())
}

fn exact_committed_artist(
    source: &str,
    incoming_name: &str,
    source_id: &str,
    stored: Option<DbArtist>,
    staged_updates: &[(String, DbArtist)],
    external_id: impl for<'a> Fn(&'a DbArtist) -> Option<&'a str>,
) -> Result<Option<DbArtist>, LibraryError> {
    let mut staged_matches = staged_updates
        .iter()
        .map(|(_, artist)| artist)
        .filter(|artist| external_id(artist) == Some(source_id));
    let staged = staged_matches.next().cloned();
    if staged_matches.next().is_some() {
        return Err(LibraryError::Import(format!(
            "artist '{incoming_name}' has a {source} source ID staged for multiple library artists"
        )));
    }
    match (stored, staged) {
        (Some(stored), Some(staged)) if stored.id != staged.id => {
            Err(LibraryError::Import(format!(
                "artist '{incoming_name}' has a {source} source ID belonging to multiple library artists"
            )))
        }
        (Some(_), Some(staged)) => Ok(Some(staged)),
        (Some(stored), None) => Ok(Some(stored)),
        (None, staged) => Ok(staged),
    }
}

fn stage_artist_update(
    staged_updates: &mut Vec<(String, DbArtist)>,
    artist_id: String,
    artist: DbArtist,
) {
    if let Some((_, staged)) = staged_updates
        .iter_mut()
        .find(|(staged_id, _)| *staged_id == artist_id)
    {
        *staged = artist;
    } else {
        staged_updates.push((artist_id, artist));
    }
}

fn pending_artist_indices(artists: &[DbArtist], incoming: &DbArtist) -> Vec<usize> {
    artists
        .iter()
        .enumerate()
        .filter_map(|(index, artist)| {
            let discogs_matches = incoming.discogs_artist_id.is_some()
                && incoming.discogs_artist_id == artist.discogs_artist_id;
            let musicbrainz_matches = incoming.musicbrainz_artist_id.is_some()
                && incoming.musicbrainz_artist_id == artist.musicbrainz_artist_id;
            (discogs_matches || musicbrainz_matches).then_some(index)
        })
        .collect()
}

fn matching_artist(
    incoming: &DbArtist,
    by_discogs: Option<DbArtist>,
    by_musicbrainz: Option<DbArtist>,
) -> Result<Option<DbArtist>, LibraryError> {
    let matched = match (by_discogs, by_musicbrainz) {
        (Some(discogs), Some(musicbrainz)) if discogs.id != musicbrainz.id => {
            let discogs_artist_id = incoming.discogs_artist_id.clone().ok_or_else(|| {
                LibraryError::Import(format!(
                    "artist '{}' matched Discogs without a Discogs source ID",
                    incoming.name
                ))
            })?;
            let musicbrainz_artist_id =
                incoming.musicbrainz_artist_id.clone().ok_or_else(|| {
                    LibraryError::Import(format!(
                        "artist '{}' matched MusicBrainz without a MusicBrainz source ID",
                        incoming.name
                    ))
                })?;
            return Err(crate::import::ArtistIdentityConflict {
                incoming_artist_name: incoming.name.clone(),
                discogs_artist_id,
                musicbrainz_artist_id,
                discogs_artist: discogs.into(),
                musicbrainz_artist: musicbrainz.into(),
            }
            .into());
        }
        (Some(artist), _) | (_, Some(artist)) => Some(artist),
        (None, None) => None,
    };
    if let Some(existing) = matched.as_ref() {
        if source_id_conflicts(
            existing.discogs_artist_id.as_deref(),
            incoming.discogs_artist_id.as_deref(),
        ) || source_id_conflicts(
            existing.musicbrainz_artist_id.as_deref(),
            incoming.musicbrainz_artist_id.as_deref(),
        ) {
            return Err(LibraryError::Import(format!(
                "artist '{}' has conflicting source IDs",
                incoming.name
            )));
        }
    }
    Ok(matched)
}

fn merge_artist_metadata(target: &mut DbArtist, incoming: &DbArtist) -> Result<(), LibraryError> {
    if source_id_conflicts(
        target.discogs_artist_id.as_deref(),
        incoming.discogs_artist_id.as_deref(),
    ) || source_id_conflicts(
        target.musicbrainz_artist_id.as_deref(),
        incoming.musicbrainz_artist_id.as_deref(),
    ) {
        return Err(LibraryError::Import(format!(
            "artist '{}' has conflicting source IDs",
            incoming.name
        )));
    }
    if target.discogs_artist_id.is_none() {
        target
            .discogs_artist_id
            .clone_from(&incoming.discogs_artist_id);
    }
    if target.musicbrainz_artist_id.is_none() {
        target
            .musicbrainz_artist_id
            .clone_from(&incoming.musicbrainz_artist_id);
    }
    if target.sort_name.is_none() {
        target.sort_name.clone_from(&incoming.sort_name);
    }
    Ok(())
}

fn source_id_conflicts(existing: Option<&str>, incoming: Option<&str>) -> bool {
    matches!((existing, incoming), (Some(a), Some(b)) if a != b)
}

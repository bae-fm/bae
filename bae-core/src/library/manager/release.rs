//! Release domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    pub async fn get_release_by_id(
        &self,
        release_id: &str,
    ) -> Result<Option<DbRelease>, LibraryError> {
        Ok(self.database.find_release_by_id(release_id).await?)
    }

    /// Whether a release whose stored content hash equals `hash` is in the
    /// library. The import watcher stamps each scanned candidate with this so an
    /// already-imported folder surfaces under the "Added" tab even after a
    /// restart (it matches by file structure, not by name).
    pub async fn is_content_hash_imported(&self, hash: &str) -> Result<bool, LibraryError> {
        Ok(self.database.is_content_hash_imported(hash).await?)
    }

    pub async fn get_tracks_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbTrack>, LibraryError> {
        Ok(self.database.get_tracks_for_release(release_id).await?)
    }

    /// Find an existing album the new import should attach to.
    ///
    /// Two-pass identity dedup against `release_identities`:
    ///
    /// 1. **Per-pressing rejection.** If any release already in the library
    ///    carries an identity row matching one of the new release's
    ///    `(source, source_release_id)` pairs (Exact identities only —
    ///    Approximate skips this), that's a duplicate import. Surface the
    ///    existing album's title so the user sees what they already have.
    ///
    /// 2. **Cross-source merge.** If any release in the library carries an
    ///    identity row matching one of the new release's
    ///    `(source, source_group_id)` pairs, return that release's
    ///    `album_id` so the new release attaches to the same album.
    ///    Identities can pair across sources — an MB-rooted import that
    ///    carried a cross-link Discogs row will be reachable from a later
    ///    Discogs-rooted import of the same master.
    ///
    /// Empty `identities` (Unknown) skips both lookups — Unknown imports
    /// always get a fresh album.
    pub async fn find_existing_album_for_import(
        &self,
        identities: &[crate::import::ReleaseIdentity],
    ) -> Result<Option<String>, crate::import::ImportError> {
        self.find_existing_album_for_import_excluding(identities, &[])
            .await
    }

    pub(crate) async fn find_existing_album_for_import_excluding(
        &self,
        identities: &[crate::import::ReleaseIdentity],
        excluded_release_ids: &[String],
    ) -> Result<Option<String>, crate::import::ImportError> {
        if identities.is_empty() {
            return Ok(None);
        }

        // 1. Per-pressing rejection: any Exact identity matching a row
        //    already in `release_identities`.
        if let Some(existing) = self
            .database
            .find_album_by_identity_release_excluding(identities, excluded_release_ids)
            .await
            .map_err(|e| crate::import::ImportError::Db(LibraryError::Database(e)))?
        {
            return Err(crate::import::ImportError::AlreadyInLibrary {
                album_title: existing.title,
            });
        }

        // 2. Cross-source merge: any group identity matching a row already
        //    in `release_identities`.
        let album_id = self
            .database
            .find_album_by_identity_group_excluding_many(identities, excluded_release_ids)
            .await
            .map_err(|e| crate::import::ImportError::Db(LibraryError::Database(e)))?;

        Ok(album_id)
    }

    /// Re-run the seeding projection from `metadata_source` /
    /// `metadata_source_release_id`, returning the projected
    /// `ReleaseUserEdit`. Read-only: no DB writes happen here. The caller
    /// (the editor) populates its form with the returned values; the
    /// user then re-edits or saves via `apply_release_metadata_user_edit`.
    ///
    /// Source dispatch:
    ///
    /// - `MusicBrainz` / `Discogs` — pull cached `release_metadata` rows
    ///   for the release and re-project per the same rules import uses.
    ///   The Exact-vs-Approximate decision comes from the matching
    ///   `release_identities` row's `source_release_id`: present = Exact
    ///   (full pressing data), NULL = Approximate (album-group fields
    ///   only; pressing fields cleared).
    /// - `FileTags` — re-read embedded tags from the release's local
    ///   audio files via `map_file_tags_to_db`. Errors out if the files
    ///   aren't reachable on disk (cloud-only without a local copy).
    ///
    /// Identity rows and the `metadata_source` columns are not touched —
    /// reset replays from the existing pointer rather than changing it.
    /// Identity changes go through `set_identity`.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn reset_metadata_to_source(
        &self,
        release_id: &str,
    ) -> Result<crate::import::ReleaseUserEdit, LibraryError> {
        use crate::db::ReleaseMetadataSource;
        use crate::import::{parsed_album_to_user_edit, MetadataSource};

        let release = self
            .database
            .find_release_by_id(release_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("Release '{release_id}' not found")))?;

        let identities = self.database.get_release_identities(release_id).await?;

        let parsed =
            match release.metadata_source {
                ReleaseMetadataSource::MusicBrainz => {
                    let source_release_id = release
                        .metadata_source_release_id
                        .as_deref()
                        .ok_or_else(|| {
                            LibraryError::Import(
                            "metadata_source = 'musicbrainz' but metadata_source_release_id is NULL"
                                .to_string(),
                        )
                        })?;
                    project_musicbrainz_from_cache(
                        &self.database,
                        release_id,
                        source_release_id,
                        self.clock.as_ref(),
                        self.ids.as_ref(),
                    )
                    .await?
                }
                ReleaseMetadataSource::Discogs => {
                    let source_release_id = release
                        .metadata_source_release_id
                        .as_deref()
                        .ok_or_else(|| {
                            LibraryError::Import(
                            "metadata_source = 'discogs' but metadata_source_release_id is NULL"
                                .to_string(),
                        )
                        })?;
                    project_discogs_from_cache(
                        &self.database,
                        release_id,
                        source_release_id,
                        self.clock.as_ref(),
                        self.ids.as_ref(),
                    )
                    .await?
                }
                ReleaseMetadataSource::FileTags => {
                    project_file_tags(
                        &self.database,
                        &release,
                        self.clock.clone(),
                        self.ids.clone(),
                    )
                    .await?
                }
            };

        // Approximate clearing. The matching identity row drives the
        // Exact-vs-Approximate decision per source. file_tags has no
        // identity row to inspect — its pressing fields come straight
        // from the tags and stay as projected.
        let approximate = match release.metadata_source {
            ReleaseMetadataSource::MusicBrainz => identities
                .iter()
                .find(|id| id.source == MetadataSource::MusicBrainz)
                .is_some_and(|id| id.source_release_id.is_none()),
            ReleaseMetadataSource::Discogs => identities
                .iter()
                .find(|id| id.source == MetadataSource::Discogs)
                .is_some_and(|id| id.source_release_id.is_none()),
            ReleaseMetadataSource::FileTags => false,
        };
        let mut user_edit = parsed_album_to_user_edit(&parsed);
        if approximate {
            user_edit.pressing = crate::import::PressingEdit::blank();
        }
        Ok(user_edit)
    }

    /// Re-identify commit. Translates the user's `IdentityChoice` from
    /// the re-identify result list into a fully cross-linked identity vec
    /// plus metadata pointer, then calls `set_identity`. Mirrors the
    /// import commit pipeline so a re-identified release lands with the
    /// same identity-row shape an initial import would produce.
    ///
    /// - **Exact / Approximate** — fetches the picked release through
    ///   `prepare_release` (which composes MB↔Discogs cross-linking via
    ///   `commit_mb_release` / `commit_discogs_release`) and projects the
    ///   mapper's identity vec via `apply_identity_choice`. The
    ///   `metadata_pointer` points at the picked release. The fetched
    ///   `metadata_pairs` flow into `set_identity` so the cached source
    ///   payload aligns with the new pointer — reset-to-source can
    ///   replay the seed without divergence. Track count is checked
    ///   against the release's existing track row count; a mismatch
    ///   errors before the identity write so a 12-track release can't
    ///   replace a 10-track rip.
    /// - **Unknown** — empty identities, `metadata_source = file_tags`,
    ///   `metadata_source_release_id = NULL`, no cached payload. Always
    ///   lands the release on a fresh album. The old source's
    ///   album/release/track rows are then reseeded from the local file
    ///   tags in the same call — projecting through the now-`FileTags`
    ///   pointer via [`Self::reset_metadata_to_source`] and writing the
    ///   result with [`Self::apply_release_metadata_user_edit`] — so the
    ///   release stops displaying the prior source's metadata. A
    ///   tag-sparse rip reseeds to blank-but-editable title/artist rather
    ///   than erroring.
    ///
    /// For **Exact / Approximate** the album/release/track row data is not
    /// touched: the identity pointer flips, but the existing rows stay as
    /// the user last had them.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn re_identify_release(
        &self,
        release_id: &str,
        identity_choice: crate::import::IdentityChoice,
    ) -> Result<(), LibraryError> {
        use crate::import::{IdentityChoice, MetadataPointer};

        let (new_identities, metadata_pointer, metadata_pairs) = match &identity_choice {
            IdentityChoice::Exact { release_ref } | IdentityChoice::Approximate { release_ref } => {
                let prepared = crate::import::service::prepare_release(self, release_ref).await?;

                // Source pressing track count must match the local
                // release's row count. The folder-import path enforces
                // the same invariant via prefetch's `track_count_mismatch`
                // flag (which disables the commit button); re-identify
                // bypasses prefetch — the user picks a row directly —
                // so the check belongs here at commit time.
                let existing_track_count = self
                    .database
                    .get_tracks_for_release(release_id)
                    .await?
                    .len();
                let new_track_count = prepared.parsed.tracks.len();
                if existing_track_count != new_track_count {
                    return Err(LibraryError::Import(format!(
                        "Track count mismatch: release has {existing_track_count} tracks, \
                         picked release has {new_track_count}"
                    )));
                }

                let identities = crate::import::service::apply_identity_choice(
                    &prepared.parsed.identities,
                    &identity_choice,
                );
                let pointer = MetadataPointer::External {
                    source: release_ref.source,
                    release_id: release_ref.id.clone(),
                };
                (identities, pointer, prepared.metadata_pairs)
            }
            IdentityChoice::Unknown => (Vec::new(), MetadataPointer::FileTags, Vec::new()),
        };

        self.set_identity(
            release_id,
            new_identities,
            metadata_pointer,
            &metadata_pairs,
        )
        .await?;

        // Unknown flips the pointer to FileTags but leaves the old
        // source's rows in place — they would still display the prior
        // (e.g. MusicBrainz) metadata. Reseed atomically here: project
        // through the now-FileTags pointer and write the result. A
        // tag-sparse rip projects to a blank-but-editable title/artist,
        // which `apply_release_metadata_user_edit` accepts (it rejects
        // only a zero-length artist list, not a blank-named artist).
        if matches!(identity_choice, IdentityChoice::Unknown) {
            let edit = self.reset_metadata_to_source(release_id).await?;
            self.apply_release_metadata_user_edit(release_id, &edit)
                .await?;
        }

        Ok(())
    }

    /// Insert album, release, and tracks into database in a transaction.
    /// Production imports go through `finalize_import_atomic`; this is only
    /// a test helper for seeding a full album/release/tracks in one call.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_album_with_release_and_tracks(
        &self,
        album: &DbAlbum,
        release: &DbRelease,
        tracks: &[DbTrack],
        metadata: &[crate::db::DbReleaseMetadata],
        track_artists: &[crate::db::DbTrackArtist],
    ) -> Result<(), LibraryError> {
        self.database
            .insert_album_with_release_and_tracks(album, release, tracks, metadata, track_artists)
            .await?;
        Ok(())
    }

    /// Insert a release and tracks into an existing album. Only a test
    /// helper for seeding a second release onto an already-inserted album.
    #[cfg(test)]
    pub async fn insert_release_with_tracks(
        &self,
        release: &DbRelease,
        tracks: &[DbTrack],
        metadata: &[crate::db::DbReleaseMetadata],
        track_artists: &[crate::db::DbTrackArtist],
    ) -> Result<(), LibraryError> {
        self.database
            .insert_release_with_tracks(release, tracks, metadata, track_artists)
            .await?;
        Ok(())
    }

    /// Load the album id, release, album, and existing tracks for a release
    /// being edited — the shared prelude of `release_edit_seed` and
    /// `apply_release_metadata_user_edit`.
    async fn load_release_for_edit(
        &self,
        release_id: &str,
    ) -> Result<(String, DbRelease, DbAlbum, Vec<DbTrack>), LibraryError> {
        let album_id = self
            .database
            .find_album_id_for_release(release_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("Release '{release_id}' not found")))?;
        let release = self
            .database
            .find_release_by_id(release_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("Release '{release_id}' not found")))?;
        let album = self
            .database
            .find_album_by_id(&album_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("Album '{album_id}' not found")))?;
        let existing_tracks = self.database.get_tracks_for_release(release_id).await?;
        Ok((album_id, release, album, existing_tracks))
    }

    /// Seed the edit form for an existing library release from its current
    /// metadata — the read counterpart to `apply_release_metadata_user_edit`.
    /// Reads the album title and artists, the release pressing fields, and the
    /// per-track titles/sides/numbers/artists, projects them into a wire
    /// `ReleaseUserEdit` describing the current state, then renders that into
    /// the raw editor form via `RawReleaseEdit::from_user_edit`. A track with
    /// no artist rows of its own seeds an empty artist field ("shares the album
    /// artist"); the album artists seed the album artist field.
    pub async fn release_edit_seed(
        &self,
        release_id: &str,
    ) -> Result<crate::import::RawReleaseEdit, LibraryError> {
        let (album_id, release, album, existing_tracks) =
            self.load_release_for_edit(release_id).await?;

        let album_artist_names: Vec<String> = self
            .database
            .get_artists_for_album(&album_id)
            .await?
            .into_iter()
            .map(|a| a.name)
            .collect();

        let mut tracks = Vec::with_capacity(existing_tracks.len());
        for track in &existing_tracks {
            // Empty when the track has no artist rows of its own — the wire edit
            // reads that as "shares the album artist", matching how
            // `apply_release_metadata_user_edit` writes it back.
            let artist_names = self
                .database
                .get_artists_for_track(&track.id)
                .await?
                .into_iter()
                .map(|a| a.name)
                .collect();
            tracks.push(crate::import::TrackUserEdit {
                title: track.title.clone(),
                side: track.side,
                track_number: track.track_number,
                artist_names,
            });
        }

        let edit = crate::import::ReleaseUserEdit {
            album_title: album.title,
            album_artist_names,
            pressing: crate::import::PressingEdit {
                year: release.pressing.year,
                format: release.pressing.format,
                label: release.pressing.label,
                catalog_number: release.pressing.catalog_number,
                country: release.pressing.country,
                barcode: release.pressing.barcode,
            },
            tracks,
        };

        Ok(crate::import::RawReleaseEdit::from_user_edit(
            edit, release_id,
        ))
    }

    /// Apply a user-supplied metadata edit to an existing release: album
    /// title and artists, release pressing fields, and per-track titles,
    /// sides, track numbers, and artists. Resolves artist names against the
    /// library (creating rows for new names), writes the album/release/track
    /// rows and replaces the `album_artists` / `track_artists` junctions, then
    /// emits an `AlbumUpdated` event.
    ///
    /// Track edits align positionally with the release's existing tracks (the
    /// edit can't add or remove tracks — `tracks.len()` must equal the
    /// release's track count). Album artists and per-track artists are
    /// positional lists — the order in `album_artist_names` /
    /// `tracks[i].artist_names` becomes the `position` column on the
    /// `album_artists` / `track_artists` rows.
    ///
    /// `release_metadata` rows, `release_identities`, and the `metadata_source`
    /// columns are deliberately not touched. Identity is orthogonal to
    /// metadata; the cached source payload stays put.
    pub async fn apply_release_metadata_user_edit(
        &self,
        release_id: &str,
        edit: &crate::import::ReleaseUserEdit,
    ) -> Result<(), LibraryError> {
        use crate::db::{DbAlbumArtist, DbArtist, DbTrackArtist};

        if edit.album_artist_names.is_empty() {
            return Err(LibraryError::Import(
                "Album must have at least one artist".to_string(),
            ));
        }

        let (album_id, release, album, existing_tracks) =
            self.load_release_for_edit(release_id).await?;
        if existing_tracks.len() != edit.tracks.len() {
            return Err(LibraryError::Import(format!(
                "Track count mismatch: release has {} tracks, edit supplies {}",
                existing_tracks.len(),
                edit.tracks.len()
            )));
        }

        // Collect every distinct artist name the edit references. The album
        // artists always appear; track-level artists only when the user
        // supplied any (an empty `artist_names` means "same as album artist",
        // no per-track row).
        let mut name_order: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut push_name = |name: &str| {
            let key = name.to_lowercase();
            if seen.insert(key) {
                name_order.push(name.to_string());
            }
        };
        for name in &edit.album_artist_names {
            push_name(name);
        }
        for t in &edit.tracks {
            for name in &t.artist_names {
                push_name(name);
            }
        }

        let now = self.clock.now();
        let parsed_artists: Vec<DbArtist> = name_order
            .iter()
            .map(|name| DbArtist {
                id: self.ids.new_id(),
                name: name.clone(),
                sort_name: None,
                discogs_artist_id: None,
                musicbrainz_artist_id: None,
                created_at: now,
            })
            .collect();

        let resolved_artists = self.resolve_artists_for_import(&parsed_artists).await?;
        let name_to_id: HashMap<String, String> = name_order
            .iter()
            .zip(resolved_artists.ids.iter())
            .map(|(name, id)| (name.to_lowercase(), id.clone()))
            .collect();

        let lookup_artist_id = |name: &str| -> Result<String, LibraryError> {
            name_to_id
                .get(&name.to_lowercase())
                .cloned()
                .ok_or_else(|| {
                    LibraryError::Import(format!("Artist '{name}' missing from resolved map"))
                })
        };

        // The `album.artist_id` FK is the primary album artist; additional
        // artists go in the `album_artists` junction with position >= 1
        // (mirrors the convention in {discogs,musicbrainz}_mapper.rs).
        // `get_artists_for_album` UNIONs the FK row in at sort_key = -1, so
        // including the primary in the junction too would duplicate it.
        let primary_album_artist_id = lookup_artist_id(&edit.album_artist_names[0])?;

        let updated_album = DbAlbum {
            title: edit.album_title.clone(),
            artist_id: primary_album_artist_id,
            ..album.clone()
        };

        let updated_release = DbRelease {
            pressing: Pressing {
                year: edit.pressing.year,
                format: edit.pressing.format.clone(),
                label: edit.pressing.label.clone(),
                catalog_number: edit.pressing.catalog_number.clone(),
                country: edit.pressing.country.clone(),
                barcode: edit.pressing.barcode.clone(),
            },
            ..release.clone()
        };

        let track_updates: Vec<(String, DbTrack)> = existing_tracks
            .iter()
            .zip(edit.tracks.iter())
            .map(|(existing, t)| {
                let updated = DbTrack {
                    title: t.title.clone(),
                    side: t.side,
                    track_number: t.track_number,
                    ..existing.clone()
                };
                (existing.id.clone(), updated)
            })
            .collect();

        let mut album_artists: Vec<DbAlbumArtist> = Vec::new();
        for (i, name) in edit.album_artist_names.iter().enumerate().skip(1) {
            let artist_id = lookup_artist_id(name)?;
            album_artists.push(DbAlbumArtist::new(
                &album_id,
                &artist_id,
                i as i32,
                self.ids.new_id(),
                now,
            ));
        }

        // Track artists have no FK on `tracks` — every artist (primary or
        // additional) goes in `track_artists` with positional ordering.
        let mut track_artists: Vec<DbTrackArtist> = Vec::new();
        for (existing, t) in existing_tracks.iter().zip(edit.tracks.iter()) {
            for (i, name) in t.artist_names.iter().enumerate() {
                let artist_id = lookup_artist_id(name)?;
                track_artists.push(DbTrackArtist::new(
                    &existing.id,
                    &artist_id,
                    i as i32,
                    self.ids.new_id(),
                    now,
                ));
            }
        }

        self.database
            .update_release_metadata_user_edit(
                &album_id,
                release_id,
                &updated_album,
                &updated_release,
                &track_updates,
                &resolved_artists.inserts,
                &resolved_artists.external_id_updates,
                &album_artists,
                &track_artists,
            )
            .await?;

        self.emit_album_updated(&album_id).await;

        Ok(())
    }

    /// The storage summary for a single release, or `None` if it doesn't exist.
    /// The download queue reads this at enqueue time for the release's title /
    /// file count / total size and to skip an already-pinned release.
    pub async fn find_release_storage_summary(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseStorageSummary>, LibraryError> {
        let Some(raw) = self
            .database
            .find_release_storage_summary(release_id)
            .await?
        else {
            return Ok(None);
        };
        let has_cloud_home = self.has_cloud_home();
        let pinned = self.release_pinned(raw.any_file_id.as_deref()).await?;
        Ok(Some(ReleaseStorageSummary::from_raw(
            raw,
            has_cloud_home,
            pinned,
        )))
    }

    /// Resolved release detail for the album-detail view. Composes a
    /// `ReleaseSummary` with tracks/files/gallery loaded by SQL joins,
    /// then derives the release's position in its album so
    /// `display_name` can be computed without the caller supplying an
    /// index. Returns `Ok(None)` when the release doesn't exist.
    pub async fn find_release_detail(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseDetail>, LibraryError> {
        let Some((raw, album_artists, release_index)) = self
            .database
            .find_release_detail_context(release_id)
            .await?
        else {
            return Ok(None);
        };
        let has_cloud_home = self.has_cloud_home();
        let pinned = self
            .release_pinned(raw.files.first().map(|f| f.id.as_str()))
            .await?;
        let cover = self.cover_ref(release_id).await?;
        let ctx = ReleaseResolveCtx {
            has_cloud_home,
            pinned,
            cover,
            transfer_action: self.current_transfer_action(release_id),
        };
        Ok(Some(ReleaseDetail::from_raw(
            raw,
            &album_artists,
            release_index,
            &ctx,
        )))
    }

    /// Get all releases for a specific album
    pub async fn get_releases_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<DbRelease>, LibraryError> {
        Ok(self.database.get_releases_for_album(album_id).await?)
    }

    pub async fn check_releases_in_library(
        &self,
        checks: &[crate::db::LibraryCheck],
    ) -> Result<Vec<crate::db::LibraryStatus>, LibraryError> {
        Ok(self.database.check_releases_in_library(checks).await?)
    }

    /// Get all files for a specific release
    ///
    /// Files belong to releases (not albums or tracks). This includes both:
    /// - Audio files (linked to tracks via db_track_position)
    /// - Metadata files (cover art, CUE sheets, etc.)
    pub async fn get_files_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbFile>, LibraryError> {
        Ok(self.database.get_files_for_release(release_id).await?)
    }

    /// Get album ID for a release
    pub async fn get_album_id_for_release(&self, release_id: &str) -> Result<String, LibraryError> {
        let album_id = self
            .database
            .find_album_id_for_release(release_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping("Release not found".to_string()))?;
        Ok(album_id)
    }

    /// Set an album's cover release (which release provides the cover art)
    pub async fn set_album_primary_release(
        &self,
        album_id: &str,
        primary_release_id: &str,
    ) -> Result<(), LibraryError> {
        self.database
            .set_album_primary_release(album_id, primary_release_id)
            .await?;

        self.emit_album_updated(album_id).await;

        Ok(())
    }

    /// Delete a release and its associated data. The database rows are removed
    /// in one cleanup-aware transaction; if this was the last release for the
    /// album, the album is removed too. coven evicts the blobs named by the
    /// delete plan after the transaction commits.
    pub async fn delete_release(&self, release_id: &str) -> Result<(), LibraryError> {
        let release = self
            .database
            .find_release_by_id(release_id)
            .await?
            .ok_or_else(|| {
                LibraryError::TrackMapping(format!("Release not found: {release_id}"))
            })?;
        let album_id = release.album_id.clone();

        // Collect track IDs before deletion for playback cleanup
        let track_ids: Vec<String> = self
            .get_tracks(release_id)
            .await?
            .into_iter()
            .map(|t| t.id)
            .collect();

        let delete_plan = self.release_delete_plan(&release).await?;
        let album_deleted = self
            .database
            .delete_release_with_cleanup(release_id, &album_id, delete_plan.db_cleanup)
            .await?;
        self.emit_outbox_changed().await;
        self.evict_delete_blobs(delete_plan.evict_blobs).await;

        if !track_ids.is_empty() {
            self.emit(LibraryEvent::TracksDeleted { track_ids });
        }

        if album_deleted {
            // This release was the album's last; it's the only child to drop.
            self.emit_album_removed(&album_id, vec![release_id.to_string()]);
        } else {
            self.emit_album_updated(&album_id).await;
            self.emit_release_removed(&album_id, release_id).await;
        }

        Ok(())
    }

    /// Remove any releases whose stored content hash equals `hash`, via
    /// [`delete_release`](Self::delete_release) per match. Re-imports do not use
    /// this destructive helper; they prepare replacement plans and commit the
    /// prior-release delete inside the finalize transaction. Only a test
    /// helper for triggering that removal directly.
    #[cfg(test)]
    pub async fn delete_releases_with_content_hash(&self, hash: &str) -> Result<(), LibraryError> {
        for release_id in self.database.release_ids_for_content_hash(hash).await? {
            self.delete_release(&release_id).await?;
        }
        Ok(())
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn import_replacement_plans_for_content_hash(
        &self,
        hash: &str,
    ) -> Result<Vec<ImportReplacementPlan>, LibraryError> {
        let mut plans = Vec::new();
        for release_id in self.database.release_ids_for_content_hash(hash).await? {
            let release = self
                .database
                .find_release_by_id(&release_id)
                .await?
                .ok_or_else(|| {
                    LibraryError::TrackMapping(format!("Release not found: {release_id}"))
                })?;
            let track_ids: Vec<String> = self
                .get_tracks(&release_id)
                .await?
                .into_iter()
                .map(|t| t.id)
                .collect();
            let delete_plan = self.release_delete_plan(&release).await?;
            plans.push(ImportReplacementPlan {
                db_delete: crate::db::ImportReplacementDelete {
                    release_id,
                    album_id: release.album_id,
                    cleanup: delete_plan.db_cleanup,
                },
                evict_blobs: delete_plan.evict_blobs,
                track_ids,
            });
        }
        Ok(plans)
    }
}

/// Project cached MusicBrainz `release_metadata` rows back into a
/// `ParsedAlbum`. Replays what `commit_mb_release` did at import,
/// minus the network calls — uses whatever the importer archived in
/// `release_metadata` (the MB release JSON, optional cross-linked
/// Discogs release JSON).
#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn project_musicbrainz_from_cache(
    database: &Database,
    release_id: &str,
    source_release_id: &str,
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<crate::import::ParsedAlbum, LibraryError> {
    let pairs = database.get_release_metadata_by_source(release_id).await?;
    let mb_json = pairs.get("musicbrainz").ok_or_else(|| {
        LibraryError::Import(format!(
            "no cached MusicBrainz payload for release '{release_id}' (source release {source_release_id})"
        ))
    })?;
    let response: crate::musicbrainz::MbReleaseResponse =
        serde_json::from_str(mb_json).map_err(|e| {
            LibraryError::Import(format!("failed to parse cached MusicBrainz JSON: {e}"))
        })?;

    // The cached payload may belong to an earlier pressing if `set_identity`
    // redirected `metadata_source_release_id` without re-fetching. Refuse to
    // project stale data — caller must re-fetch (e.g. via Re-identify) first.
    if response.id != source_release_id {
        return Err(LibraryError::Import(format!(
            "cached MusicBrainz payload (release '{}') doesn't match current pointer '{}'; re-fetch via Re-identify first",
            response.id, source_release_id
        )));
    }

    let discogs_release = match pairs.get("discogs") {
        Some(json) => Some(
            crate::discogs::client::parse_discogs_release_json(json).map_err(|e| {
                LibraryError::Import(format!(
                    "failed to parse cached Discogs cross-ref JSON: {e}"
                ))
            })?,
        ),
        None => None,
    };

    crate::import::musicbrainz_mapper::map_mb_response_to_db(
        &response,
        None,
        discogs_release,
        clock,
        ids,
    )
    .map_err(LibraryError::from)
}

/// Project cached Discogs `release_metadata` rows back into a
/// `ParsedAlbum`. Replays the import-time projection from the archived
/// raw JSON (Discogs release + optional master + optional MB cross-ref).
#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn project_discogs_from_cache(
    database: &Database,
    release_id: &str,
    source_release_id: &str,
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<crate::import::ParsedAlbum, LibraryError> {
    let pairs = database.get_release_metadata_by_source(release_id).await?;
    let discogs_json = pairs.get("discogs").ok_or_else(|| {
        LibraryError::Import(format!(
            "no cached Discogs payload for release '{release_id}' (source release {source_release_id})"
        ))
    })?;
    let release = crate::discogs::client::parse_discogs_release_json(discogs_json)
        .map_err(|e| LibraryError::Import(format!("failed to parse cached Discogs JSON: {e}")))?;

    // The cached payload may belong to an earlier pressing if `set_identity`
    // redirected `metadata_source_release_id` without re-fetching. Refuse to
    // project stale data — caller must re-fetch (e.g. via Re-identify) first.
    if release.id != source_release_id {
        return Err(LibraryError::Import(format!(
            "cached Discogs payload (release '{}') doesn't match current pointer '{}'; re-fetch via Re-identify first",
            release.id, source_release_id
        )));
    }

    let master_year = match pairs.get("discogs_master") {
        Some(json) => crate::discogs::client::parse_discogs_master_year(json).map_err(|e| {
            LibraryError::Import(format!("failed to parse cached Discogs master JSON: {e}"))
        })?,
        None => release.year,
    };

    let mb_xref = match pairs.get("musicbrainz") {
        Some(json) => Some(
            serde_json::from_str::<crate::musicbrainz::MbReleaseResponse>(json).map_err(|e| {
                LibraryError::Import(format!(
                    "failed to parse cached MusicBrainz cross-ref JSON: {e}"
                ))
            })?,
        ),
        None => None,
    };

    crate::import::discogs_mapper::map_discogs_to_db(
        &release,
        master_year,
        mb_xref.as_ref(),
        clock,
        ids,
    )
    .map_err(LibraryError::from)
}

/// Project the embedded tags of a release's local audio files into a
/// `ParsedAlbum`. Mirrors the Unknown import path's call to
/// `map_file_tags_to_db`. Errors out if any audio file is unreachable on
/// disk (cloud-only release without a local copy).
#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn project_file_tags(
    database: &Database,
    release: &DbRelease,
    clock: ClockRef,
    ids: IdRef,
) -> Result<crate::import::ParsedAlbum, LibraryError> {
    let files = database.get_files_for_release(&release.id).await?;
    let mut audio_paths = Vec::new();
    for file in &files {
        if !file.content_type.is_audio() {
            continue;
        }
        // The file's bytes must be the user's own file in place (a Local
        // user-provided blob coven holds an external ref for); a Remote release
        // has no on-disk original to re-read tags from.
        let path = database
            .external_blob(&file.id)
            .await?
            .map(|ext| ext.path)
            .ok_or_else(|| {
            LibraryError::Import(format!(
                "audio file '{}' is remote — make the release local before resetting from file tags",
                file.original_filename
            ))
        })?;
        audio_paths.push(path);
    }
    if audio_paths.is_empty() {
        return Err(LibraryError::Import(format!(
            "release '{}' has no audio files to read tags from",
            release.id
        )));
    }
    // Album-title fallback when no file carries an ALBUM tag: the folder the
    // release was originally imported from.
    let folder_name = release.source_folder_name.clone();
    tokio::task::spawn_blocking(move || {
        crate::import::file_tag_mapper::map_file_tags_to_db(
            &audio_paths,
            folder_name.as_deref(),
            clock.as_ref(),
            ids.as_ref(),
        )
    })
    .await
    .map_err(|e| LibraryError::Import(format!("file-tag mapping task failed: {e}")))?
    .map_err(LibraryError::from)
}

/// The cover [`ImageRef`] for one release from its `covers` row's `_updated_at`,
/// or `None` when it has no cover row. Free function so the manager's `cover_ref`
/// and the observer's `find_release_detail_with` share one construction.
pub(crate) async fn cover_ref_for(
    database: &Database,
    release_id: &str,
) -> Result<Option<ImageRef>, LibraryError> {
    Ok(database
        .cover_version(release_id)
        .await?
        .map(|version| ImageRef {
            id: release_id.to_string(),
            version,
            image_type: LibraryImageType::Cover,
        }))
}

/// Free-function variant of `LibraryManager::find_release_detail`.
///
/// Used by the manager and by the upload observer (which holds the same
/// `Database` and a `CovenHandle` so it can emit `ReleaseUpdated` events for a
/// release whose `local_path` just got cleared at the end of an upload run,
/// without owning a manager). The pin-state is answered through `handle`, the
/// same door the manager uses. `has_cloud_home` is supplied by the caller; the
/// observer fires inside a running sync cycle so it can pass `true`.
pub(crate) async fn find_release_detail_with(
    database: &Database,
    handle: &CovenHandle,
    has_cloud_home: bool,
    release_id: &str,
) -> Result<Option<ReleaseDetail>, LibraryError> {
    let Some((raw, album_artists, release_index)) =
        database.find_release_detail_context(release_id).await?
    else {
        return Ok(None);
    };
    let pinned = match raw.files.first() {
        Some(file) => release_file_pinned(handle, &file.id).await?,
        None => false,
    };
    let cover = cover_ref_for(database, release_id).await?;
    let ctx = ReleaseResolveCtx {
        has_cloud_home,
        pinned,
        cover,
        transfer_action: None,
    };
    Ok(Some(ReleaseDetail::from_raw(
        raw,
        &album_artists,
        release_index,
        &ctx,
    )))
}

impl LibraryManager {
    /// The full cloud object key a release file's blob lives at, derived through
    /// coven for the configured home's scheme (`Hashed` → `{ns}/{ab}/{cd}/{id}`,
    /// `Plain` → `{ns}/{cloud_path}`). Used by the bae-side delete path, which
    /// stays bae's responsibility (the transitions are coven's).
    pub(super) fn release_file_cloud_key(&self, file: &DbFile) -> Result<String, LibraryError> {
        self.handle
            .blob_cloud_key(&Self::release_file_blob_ref(file))
            .map_err(|e| LibraryError::Storage(format!("cloud key for file {}: {e}", file.id)))
    }
}

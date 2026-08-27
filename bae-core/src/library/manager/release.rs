//! Release domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn register_release_external_refs_for_test(
        &self,
        release_id: &str,
        source_dir: &str,
    ) -> Result<(), LibraryError> {
        self.database
            .register_release_external_refs_for_test(release_id, source_dir)
            .await?;
        Ok(())
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_audio_format_with_segments_for_test(
        &self,
        audio_format: &DbAudioFormat,
        segments: &[DbAudioSegment],
    ) -> Result<(), LibraryError> {
        self.database
            .insert_audio_format_with_segments_for_test(audio_format, segments)
            .await?;
        Ok(())
    }

    /// Test-only: seed a single file row. Production inserts files only as part of
    /// an import or edit transaction.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn add_file(&self, file: &DbFile) -> Result<(), LibraryError> {
        self.database.insert_file(file).await?;
        Ok(())
    }

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
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn is_content_hash_imported(&self, hash: &str) -> Result<bool, LibraryError> {
        Ok(self.database.is_content_hash_imported(hash).await?)
    }

    pub async fn get_tracks_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbTrack>, LibraryError> {
        Ok(self.database.get_tracks_for_release(release_id).await?)
    }

    /// The existing album a new import should attach to, from a two-pass identity
    /// dedup against `release_identities`:
    ///
    /// 1. **Per-pressing rejection.** A release in the library carrying an identity
    ///    row that matches one of the new release's `(source, source_release_id)`
    ///    pairs means this is a duplicate import. Surface that album's title so
    ///    the user sees what they already have.
    /// 2. **Cross-source merge.** A release carrying an identity row matching one of
    ///    the new release's `(source, source_group_id)` pairs gives up its
    ///    `album_id`, so the new release attaches to the same album. Identities pair
    ///    across sources, so an MB-rooted import that carried a cross-link Discogs
    ///    row is reachable from a later Discogs-rooted import of the same master.
    ///
    /// Empty `identities` (Unknown) skips both lookups — an Unknown import always
    /// gets a fresh album.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn find_existing_album_for_import(
        &self,
        identities: &[crate::import::ReleaseIdentity],
    ) -> Result<Option<String>, crate::import::ImportError> {
        self.find_existing_album_for_import_excluding(identities, &[])
            .await
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn find_existing_album_for_import_excluding(
        &self,
        identities: &[crate::import::ReleaseIdentity],
        excluded_release_ids: &[String],
    ) -> Result<Option<String>, crate::import::ImportError> {
        if identities.is_empty() {
            return Ok(None);
        }

        // Per-pressing rejection: an Exact identity matching a `release_identities`
        // row already in the library.
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

        // Cross-source merge: a group identity matching a row already there.
        let album_id = self
            .database
            .find_album_by_identity_group_excluding(identities, excluded_release_ids)
            .await
            .map_err(|e| crate::import::ImportError::Db(LibraryError::Database(e)))?;

        Ok(album_id)
    }

    /// Re-run the seeding projection from `metadata_source` /
    /// `metadata_source_release_id` and return the projected `ReleaseUserEdit`.
    /// Read-only — the editor populates its form from the result, and the user
    /// re-edits or saves through `apply_release_metadata_user_edit`.
    ///
    /// - `MusicBrainz` / `Discogs` — re-project the archived provider documents
    ///   under the same rules import uses, from the pressing the release's
    ///   identity row names.
    /// - `FileTags` — re-read the embedded tags from the release's local audio
    ///   files. Errors if they aren't reachable on disk (cloud-only, no local copy).
    ///
    /// Identity rows and the `metadata_source` columns are untouched: reset replays
    /// from the existing pointer rather than changing it. Identity changes go
    /// through `set_identity`.
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

        // Which external source seeded this release, or `None` for the one
        // pointer that names no source at all.
        let external_source = match release.metadata_source {
            ReleaseMetadataSource::MusicBrainz => Some(MetadataSource::MusicBrainz),
            ReleaseMetadataSource::Discogs => Some(MetadataSource::Discogs),
            ReleaseMetadataSource::FileTags => None,
            ReleaseMetadataSource::Manual => {
                return Err(LibraryError::Import(format!(
                    "release '{release_id}' was entered manually and has no metadata source"
                )))
            }
        };

        let parsed = match external_source {
            Some(source) => {
                // The pointer names which source release seeded this one, and
                // the documents are keyed by exactly that — so what is read back
                // cannot belong to a pressing the release was pointed away from.
                let source_release_id =
                    release
                        .metadata_source_release_id
                        .as_deref()
                        .ok_or_else(|| {
                            LibraryError::Import(format!(
                                "metadata_source = '{}' but metadata_source_release_id is NULL",
                                source.as_str()
                            ))
                        })?;
                let payloads = crate::import::payloads::load(
                    &self.database,
                    &crate::import::MetadataRef::new(source_release_id, source),
                )
                .await?
                .ok_or_else(|| {
                    LibraryError::Import(format!(
                        "no archived {} payload for release '{release_id}' (source release {source_release_id})",
                        source.as_str()
                    ))
                })?;
                payloads.parsed(self.clock.as_ref(), self.ids.as_ref())?
            }
            None => {
                project_file_tags(
                    &self.database,
                    &release,
                    self.clock.clone(),
                    self.ids.clone(),
                )
                .await?
            }
        };

        Ok(parsed_album_to_user_edit(&parsed))
    }

    /// Re-identify commit: translate the user's `ReleaseReseed` into a fully
    /// cross-linked identity vec plus metadata pointer, then `set_identity`. Mirrors
    /// the import commit pipeline, so a re-identified release lands with the same
    /// identity-row shape an initial import would produce.
    ///
    /// - **Release** — resolve the picked release's documents through
    ///   `prepare_release` (which composes the MB↔Discogs cross-linking, fetching
    ///   and storing when nothing has yet) and project the mapper's identity vec
    ///   as it stands. Those documents are keyed by the picked
    ///   release, which is what the new pointer names, so reset-to-source replays
    ///   the same seed. The picked release's track count is checked against the existing
    ///   track rows, and a mismatch errors before the identity write — a 12-track
    ///   release can't replace a 10-track rip. Album/release/track row data is not
    ///   touched: the identity pointer flips, the rows stay as the user last had them.
    /// - **Unknown** — empty identities, `metadata_source = file_tags`,
    ///   `metadata_source_release_id = NULL`; the release always
    ///   lands on a fresh album. The old source's album/release/track rows would
    ///   still show its metadata, so the same call reseeds them from the local file
    ///   tags, projecting through the now-`FileTags` pointer with
    ///   [`Self::reset_metadata_to_source`] and writing the result with
    ///   [`Self::apply_release_metadata_user_edit`]. A tag-sparse rip reseeds to a
    ///   blank-but-editable title/artist rather than erroring.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn re_identify_release(
        &self,
        release_id: &str,
        identity_choice: crate::import::ReleaseReseed,
    ) -> Result<(), LibraryError> {
        use crate::import::{MetadataSeed, ReleaseReseed};

        let (new_identities, metadata_pointer) = match &identity_choice {
            ReleaseReseed::ExternalRelease { release_ref } => {
                let payloads = crate::import::service::prepare_release(
                    self,
                    release_ref,
                    crate::util::rate_limiter::CallPriority::Interactive,
                )
                .await?;
                let parsed = payloads.parsed(self.clock.as_ref(), self.ids.as_ref())?;

                // The source pressing's track count must match the local
                // release's row count. Re-identify only re-points the identity;
                // it never re-binds audio, so a source with a different number
                // of tracks has nothing to re-point half the rows at. A folder
                // import has no such constraint — it maps its own audio into
                // track slots and a count disagreement is a slot, not a
                // refusal — so this check is re-identify's alone.
                let existing_track_count = self
                    .database
                    .get_tracks_for_release(release_id)
                    .await?
                    .len();
                let new_track_count = parsed.tracks.len();
                if existing_track_count != new_track_count {
                    return Err(LibraryError::Import(format!(
                        "Track count mismatch: release has {existing_track_count} tracks, \
                         picked release has {new_track_count}"
                    )));
                }

                let pointer = MetadataSeed::ExternalRelease {
                    source: release_ref.source,
                    release_id: release_ref.id.clone(),
                };
                (parsed.identities, pointer)
            }
            ReleaseReseed::FileTags => (Vec::new(), MetadataSeed::FileTags),
        };

        self.set_identity(release_id, new_identities, metadata_pointer)
            .await?;

        // Unknown flips the pointer to FileTags but leaves the old source's rows
        // in place, still showing the prior metadata. Reseed them here by projecting
        // through the now-FileTags pointer. A tag-sparse rip projects to a
        // blank-but-editable title/artist — the prompt the user answers in the
        // editor — so this writes through the ungated path. The blank is not a user
        // edit, and the user-edit gate would reject it.
        if matches!(identity_choice, ReleaseReseed::FileTags) {
            let edit = self.reset_metadata_to_source(release_id).await?;
            self.write_release_metadata(release_id, &edit).await?;
        }

        Ok(())
    }

    /// Test-only: seed a full album/release/tracks in one call. Production imports
    /// go through `finalize_import_atomic`.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_album_with_release_and_tracks(
        &self,
        album: &DbAlbum,
        release: &DbRelease,
        tracks: &[DbTrack],
        track_artists: &[crate::db::DbTrackArtist],
    ) -> Result<(), LibraryError> {
        self.database
            .insert_album_with_release_and_tracks(album, release, tracks, track_artists)
            .await?;
        Ok(())
    }

    /// Test-only: seed a second release onto an already-inserted album.
    #[cfg(test)]
    pub async fn insert_release_with_tracks(
        &self,
        release: &DbRelease,
        tracks: &[DbTrack],
        track_artists: &[crate::db::DbTrackArtist],
    ) -> Result<(), LibraryError> {
        self.database
            .insert_release_with_tracks(release, tracks, track_artists)
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

        let album_artist_assignments: Vec<crate::import::ArtistAssignment> = self
            .database
            .get_artists_for_album(&album_id)
            .await?
            .into_iter()
            .map(|artist| crate::import::ArtistAssignment::Existing {
                artist_id: artist.id,
            })
            .collect();

        let mut tracks = Vec::with_capacity(existing_tracks.len());
        for track in &existing_tracks {
            // Empty when the track has no artist rows of its own — the wire edit
            // reads that as "shares the album artist", matching how
            // `apply_release_metadata_user_edit` writes it back.
            let artists: Vec<crate::import::ArtistAssignment> = self
                .database
                .get_artists_for_track(&track.id)
                .await?
                .into_iter()
                .map(|artist| crate::import::ArtistAssignment::Existing {
                    artist_id: artist.id,
                })
                .collect();
            tracks.push(crate::import::TrackUserEdit {
                title: track.title.clone(),
                side: track.side,
                track_number: track.track_number,
                artist_assignments: if artists.is_empty() {
                    crate::import::TrackArtistAssignments::AlbumArtists
                } else {
                    crate::import::TrackArtistAssignments::Explicit(artists)
                },
                // Re-projecting a release's metadata never re-binds its files;
                // the audio each track already points at stays as it is.
                file: None,
            });
        }

        let edit = crate::import::ReleaseUserEdit {
            album_title: album.title,
            album_artist_assignments,
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

    /// Apply a user-supplied metadata edit to an existing release.
    ///
    /// Every surface a user edits through reaches the write here — the desktop
    /// editor and MCP's `release_metadata_update`. The desktop shapes its form
    /// first ([`crate::import::RawReleaseEdit::shape`]); MCP hands over
    /// a wire edit built field-for-field. So the edit is normalized and validated
    /// here rather than at any one caller: a blank album title or an artist-less
    /// album is rejected, and surrounding whitespace is trimmed, no matter which
    /// surface it came from.
    ///
    /// Writes only what `write_release_metadata` writes; see it for the row-level
    /// contract.
    pub async fn apply_release_metadata_user_edit(
        &self,
        release_id: &str,
        edit: &crate::import::ReleaseUserEdit,
    ) -> Result<(), LibraryError> {
        let edit = edit.clone().normalized();
        edit.validate()?;
        self.write_release_metadata(release_id, &edit).await
    }

    /// Write a release's metadata rows: album title and artists, release pressing
    /// fields, and per-track titles, sides, track numbers, and artists. Resolves
    /// explicit library IDs and exact external IDs, creates new name-only artists,
    /// and writes the
    /// album/release/track rows and replaces the `album_artists` /
    /// `track_artists` junctions in one commit.
    ///
    /// Track edits align positionally with the release's existing tracks (the
    /// edit can't add or remove tracks — `tracks.len()` must equal the
    /// release's track count). Album artists and per-track artists are
    /// positional lists — the order in `album_artist_assignments` and each
    /// track's explicit assignments becomes the `position` column on the
    /// `album_artists` / `track_artists` rows.
    ///
    /// archived provider documents, `release_identities`, and the `metadata_source`
    /// columns are deliberately not touched. Identity is orthogonal to
    /// metadata; the cached source payload stays put.
    ///
    /// Ungated on purpose: a release reseeded from sparse file tags carries a
    /// blank-but-editable title and artist, which the user fills in the editor.
    /// A *user's* edit is held to [`crate::import::ReleaseUserEdit::validate`] —
    /// it arrives through [`Self::apply_release_metadata_user_edit`].
    async fn write_release_metadata(
        &self,
        release_id: &str,
        edit: &crate::import::ReleaseUserEdit,
    ) -> Result<(), LibraryError> {
        use crate::db::{DbAlbumArtist, DbTrackArtist};

        let (album_id, release, album, existing_tracks) =
            self.load_release_for_edit(release_id).await?;
        if existing_tracks.len() != edit.tracks.len() {
            return Err(LibraryError::Import(format!(
                "Track count mismatch: release has {} tracks, edit supplies {}",
                existing_tracks.len(),
                edit.tracks.len()
            )));
        }

        let mut assignments = edit.album_artist_assignments.clone();
        let album_assignment_count = assignments.len();
        let mut track_ranges = Vec::with_capacity(edit.tracks.len());
        for track in &edit.tracks {
            let start = assignments.len();
            if let crate::import::TrackArtistAssignments::Explicit(track_assignments) =
                &track.artist_assignments
            {
                assignments.extend(track_assignments.iter().cloned());
            }
            track_ranges.push(start..assignments.len());
        }
        let resolved_artists = self.resolve_artist_assignments(&assignments).await?;
        let album_artist_ids = &resolved_artists.ids[..album_assignment_count];
        let now = self.clock.now();

        // The `album.artist_id` FK is the primary album artist; additional
        // artists go in the `album_artists` junction with position >= 1
        // (mirrors the convention in {discogs,musicbrainz}_mapper.rs).
        // `get_artists_for_album` UNIONs the FK row in at sort_key = -1, so
        // including the primary in the junction too would duplicate it.
        let primary_album_artist_id = album_artist_ids.first().ok_or_else(|| {
            LibraryError::Internal(format!(
                "release {release_id} metadata carries no album artist"
            ))
        })?;

        let updated_album = DbAlbum {
            title: edit.album_title.clone(),
            artist_id: primary_album_artist_id.clone(),
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
        for (i, artist_id) in album_artist_ids.iter().enumerate().skip(1) {
            album_artists.push(DbAlbumArtist::new(
                &album_id,
                artist_id,
                i as i32,
                self.ids.new_id(),
                now,
            ));
        }

        // Track artists have no FK on `tracks` — every artist (primary or
        // additional) goes in `track_artists` with positional ordering.
        let mut track_artists: Vec<DbTrackArtist> = Vec::new();
        for ((existing, track), range) in existing_tracks
            .iter()
            .zip(edit.tracks.iter())
            .zip(track_ranges)
        {
            if matches!(
                track.artist_assignments,
                crate::import::TrackArtistAssignments::AlbumArtists
            ) {
                continue;
            }
            for (i, artist_id) in resolved_artists.ids[range].iter().enumerate() {
                track_artists.push(DbTrackArtist::new(
                    &existing.id,
                    artist_id,
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

        Ok(())
    }

    /// The storage summary for one release, or `None` if it doesn't exist. The
    /// download queue reads it at enqueue time for the title / file count / total
    /// size, and to skip an already-pinned release.
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

    pub(crate) fn subscribe_release_detail(
        &self,
        release_id: &str,
    ) -> coven::LiveQuery<crate::db::ReleaseDetailProjection> {
        self.database.subscribe_release_detail(release_id)
    }

    pub(crate) async fn resolve_release_detail_projection(
        &self,
        release_id: &str,
        projection: crate::db::ReleaseDetailProjection,
    ) -> Result<Option<ReleaseDetail>, LibraryError> {
        let Some(crate::db::ReleaseDetailContext {
            detail,
            album_artists,
            release_index,
            is_compilation,
        }) = projection.context
        else {
            return Ok(None);
        };
        let cover = projection
            .cover_versions
            .get(release_id)
            .map(|version| ImageRef {
                id: release_id.to_string(),
                version: version.clone(),
                image_type: LibraryImageType::Cover,
            });
        self.resolve_release_detail_context(
            release_id,
            detail,
            album_artists,
            release_index,
            is_compilation,
            cover,
        )
        .await
        .map(Some)
    }

    async fn resolve_release_detail_context(
        &self,
        release_id: &str,
        raw: crate::db::DbReleaseDetail,
        album_artists: Vec<DbArtist>,
        release_index: usize,
        is_compilation: bool,
        cover: Option<ImageRef>,
    ) -> Result<ReleaseDetail, LibraryError> {
        let has_cloud_home = self.has_cloud_home();
        let pinned = self
            .release_pinned(raw.files.first().map(|file| file.id.as_str()))
            .await?;
        let ctx = ReleaseResolveCtx {
            has_cloud_home,
            pinned,
            cover,
            transfer_action: self.current_transfer_action(release_id),
            is_compilation,
        };
        let (detail, orphans) = ReleaseDetail::from_raw(raw, &album_artists, release_index, &ctx);
        self.report_audio_format_orphans(orphans);
        Ok(detail)
    }

    /// Resolved release detail for the album-detail view: a `ReleaseSummary` plus
    /// the tracks/files/gallery its SQL joins load, and the release's position
    /// within its album, so `display_name` needs no index from the caller. `None`
    /// when the release doesn't exist.
    pub async fn find_release_detail(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseDetail>, LibraryError> {
        let Some(crate::db::ReleaseDetailContext {
            detail: raw,
            album_artists,
            release_index,
            is_compilation,
        }) = self
            .database
            .find_release_detail_context(release_id)
            .await?
        else {
            return Ok(None);
        };
        let cover = self.cover_ref(release_id).await?;
        self.resolve_release_detail_context(
            release_id,
            raw,
            album_artists,
            release_index,
            is_compilation,
            cover,
        )
        .await
        .map(Some)
    }

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

    pub(crate) fn subscribe_release_library_status(
        &self,
        check: crate::db::LibraryCheck,
    ) -> coven::LiveQuery<crate::db::LibraryStatus> {
        self.database.subscribe_release_library_status(check)
    }

    /// Every file of a release — audio files, and the metadata files (cover art,
    /// CUE sheets) that no track owns. Files belong to releases, not to albums or
    /// tracks; a track reaches its audio file through its `audio_segments`.
    pub async fn get_files_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbFile>, LibraryError> {
        Ok(self.database.get_files_for_release(release_id).await?)
    }

    pub async fn get_album_id_for_release(&self, release_id: &str) -> Result<String, LibraryError> {
        let album_id = self
            .database
            .find_album_id_for_release(release_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping("Release not found".to_string()))?;
        Ok(album_id)
    }

    /// Set which of an album's releases provides its cover art.
    pub async fn set_album_primary_release(
        &self,
        album_id: &str,
        primary_release_id: &str,
    ) -> Result<(), LibraryError> {
        self.database
            .set_album_primary_release(album_id, primary_release_id)
            .await?;

        Ok(())
    }

    /// Delete a release and its data. The rows go in one cleanup-aware transaction,
    /// taking the album with them if this was its last release; coven evicts the
    /// blobs named by the delete plan once that transaction commits.
    pub async fn delete_release(&self, release_id: &str) -> Result<(), LibraryError> {
        let release = self
            .database
            .find_release_by_id(release_id)
            .await?
            .ok_or_else(|| {
                LibraryError::TrackMapping(format!("Release not found: {release_id}"))
            })?;
        let album_id = release.album_id.clone();

        // Read the track ids before the delete cascades them away — playback needs
        // them to clear the queue.
        let track_ids: Vec<String> = self
            .get_tracks_for_release(release_id)
            .await?
            .into_iter()
            .map(|t| t.id)
            .collect();

        let delete_plan = self.release_delete_plan(&release).await?;
        // The delete records the unwind itself, in the transaction that removes
        // the rows: coven marks the transition cancelling and the drain takes
        // any object already in the cloud back out. Cancelling here first would
        // only make the delete wait for a provider it does not need — deleting
        // is local work, and the unwind waits visibly like any other transfer.
        self.database
            .delete_release_with_cleanup(release_id, &album_id, delete_plan.db_cleanup)
            .await?;
        self.emit_outbox_changed().await;
        self.evict_delete_blobs(delete_plan.evict_blobs).await;

        if !track_ids.is_empty() {
            self.emit(LibraryEvent::TracksDeleted { track_ids });
        }

        Ok(())
    }

    /// Test-only: remove every release whose stored content hash equals `hash`, one
    /// [`delete_release`](Self::delete_release) per match. A re-import does NOT use
    /// this destructive path — it prepares replacement plans and commits the
    /// prior-release delete inside the finalize transaction.
    #[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
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
                .get_tracks_for_release(&release_id)
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

/// Project the embedded tags of a release's local audio files into a `ParsedAlbum`,
/// as the Unknown import path does. Errors if any audio file is unreachable on disk
/// (a cloud-only release with no local copy).
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

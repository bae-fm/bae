//! Database domain operations for [`LibraryManager`].

use super::resolve::*;
use super::*;

/// Translate a UI-facing `StorageSort` to the DB-layer sort criterion.
fn to_db_storage_sort(sort: &StorageSort) -> DbStorageSortCriterion {
    DbStorageSortCriterion {
        field: match sort.field {
            StorageSortField::AlbumTitle => DbStorageSortField::AlbumTitle,
            StorageSortField::ArtistNames => DbStorageSortField::ArtistNames,
            StorageSortField::Format => DbStorageSortField::Format,
            StorageSortField::FileCount => DbStorageSortField::FileCount,
            StorageSortField::TotalSize => DbStorageSortField::TotalSize,
        },
        direction: match sort.direction {
            StorageSortDirection::Ascending => DbSortDirection::Ascending,
            StorageSortDirection::Descending => DbSortDirection::Descending,
        },
    }
}

/// Translate a UI-facing `StorageFilter` to the DB-layer filter.
fn to_db_storage_filter(filter: StorageFilter) -> DbStorageFilter {
    match filter {
        StorageFilter::All => DbStorageFilter::All,
        StorageFilter::Remote => DbStorageFilter::Remote,
        StorageFilter::Local => DbStorageFilter::Local,
        StorageFilter::Uploading => DbStorageFilter::Uploading,
    }
}

/// Per-source agreement check: do `new_identities` fit alongside
/// `other_release_identities` (the identity rows of every *other*
/// release in the candidate album)?
///
/// Two releases can share an album as long as they don't disagree on
/// any source they both claim. `new_id.source == other.source` requires
/// matching `source_group_id`; differing sources are independent.
fn identities_fit_album(
    new_identities: &[crate::import::ReleaseIdentity],
    other_release_identities: &[Vec<crate::import::ReleaseIdentity>],
) -> bool {
    for new_id in new_identities {
        for other_release in other_release_identities {
            for existing in other_release {
                if existing.source == new_id.source
                    && existing.source_group_id != new_id.source_group_id
                {
                    return false;
                }
            }
        }
    }
    true
}

/// Project `MetadataPointer` to the two `releases` columns it sets:
/// `metadata_source` (always present) and `metadata_source_release_id`
/// (NULL when source is `file_tags`).
fn metadata_pointer_to_columns(
    pointer: crate::import::MetadataPointer,
) -> (crate::db::ReleaseMetadataSource, Option<String>) {
    use crate::db::ReleaseMetadataSource;
    use crate::import::{MetadataPointer, MetadataSource};
    match pointer {
        MetadataPointer::External { source, release_id } => {
            let column_source = match source {
                MetadataSource::MusicBrainz => ReleaseMetadataSource::MusicBrainz,
                MetadataSource::Discogs => ReleaseMetadataSource::Discogs,
            };
            (column_source, Some(release_id))
        }
        MetadataPointer::FileTags => (ReleaseMetadataSource::FileTags, None),
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
    .map_err(LibraryError::Import)
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
    .map_err(LibraryError::Import)
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
    .map_err(LibraryError::Import)
}

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

    /// Count outbox upload entries still pending for a release's files.
    /// Zero means the cloud copy is confirmed durable. Used by the unpin
    /// guard in `make_release_local` to refuse a transition mid-upload — the
    /// UI side of "no actions mid-upload" reads the `OutboxSnapshot.per_release`
    /// map instead.
    pub async fn count_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> Result<i64, LibraryError> {
        Ok(self
            .database
            .count_pending_uploads_for_release(release_id)
            .await?)
    }

    /// Seed an upload outbox row + refresh the snapshot. coven owns enqueueing in
    /// `make_remote`, so this is only a test helper for exercising the
    /// outbox-snapshot / drain machinery directly.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn add_cloud_outbox_upload(
        &self,
        file_id: &str,
        cloud_key: &str,
        source_path: Option<&str>,
        retain_pinned: bool,
    ) -> Result<(), LibraryError> {
        self.database
            .add_cloud_outbox_upload(file_id, cloud_key, source_path, retain_pinned)
            .await?;
        self.emit_outbox_changed().await;
        Ok(())
    }

    /// The full cloud object key a release file's blob lives at, derived through
    /// coven for the configured home's scheme (`Hashed` → `{ns}/{ab}/{cd}/{id}`,
    /// `Plain` → `{ns}/{cloud_path}`). Used by the bae-side delete path, which
    /// stays bae's responsibility (the transitions are coven's).
    pub(super) fn release_file_cloud_key(&self, file: &DbFile) -> Result<String, LibraryError> {
        self.handle
            .blob_cloud_key(&Self::release_file_blob_ref(file))
            .map_err(|e| LibraryError::Storage(format!("cloud key for file {}: {e}", file.id)))
    }

    /// The full cloud object key a cover blob lives at (namespace `covers`),
    /// derived through coven for the configured home's scheme. Used by the
    /// bae-side cover delete path.
    fn cover_cloud_key(
        &self,
        release_id: &str,
        cloud_path: Option<&str>,
    ) -> Result<String, LibraryError> {
        self.handle
            .blob_cloud_key(&Self::image_blob_ref(
                crate::sync::COVERS_NAMESPACE,
                release_id,
                cloud_path.map(str::to_string),
            ))
            .map_err(|e| LibraryError::Storage(format!("cloud key for cover {release_id}: {e}")))
    }

    /// Retry failed uploads now: clear their backoff so the next cycle picks
    /// them up immediately, then kick the sync loop.
    pub async fn retry_outbox_now(&self) -> Result<(), LibraryError> {
        self.database.reset_cloud_outbox_backoff().await?;
        self.trigger_sync();
        self.emit_outbox_changed().await;
        Ok(())
    }

    /// Cancel one queued outbox entry by id. Removes the queue row only; the
    /// local file is untouched, so the release just stops syncing this entry.
    pub async fn cancel_outbox_item(&self, id: i64) -> Result<(), LibraryError> {
        self.database.remove_cloud_outbox_entry(id).await?;
        self.emit_outbox_changed().await;
        Ok(())
    }

    /// Stop uploading a release that's mid-make-Remote and keep it Local.
    ///
    /// coven owns the cancel: it clears the durable make-Remote intent and the
    /// release's pending upload rows, and tombstones any blob that already reached
    /// the cloud, in one transaction. The gate never flips, so the release stays
    /// Local — its files are still the external refs coven holds, untouched.
    pub async fn cancel_release_upload(&self, release_id: &str) -> Result<(), LibraryError> {
        self.coven_cancel_make_remote(release_id).await?;
        self.emit_outbox_changed().await;
        // Refresh the release row (it no longer reads as "uploading"). A
        // best-effort UI nudge — the cancel itself already succeeded above.
        match self.get_release_by_id(release_id).await {
            Ok(Some(release)) => {
                self.emit_release_updated(&release.album_id, release_id)
                    .await
            }
            Ok(None) => {
                warn!("cancel_release_upload: release {release_id} missing; skipped UI refresh")
            }
            Err(e) => {
                warn!("cancel_release_upload: loading release {release_id} for refresh failed: {e}")
            }
        }
        Ok(())
    }

    /// Drive coven's upload drain once through the handle's connected sync
    /// manager, for tests that connected an injected cloud home via
    /// [`connect_test_cloud_home`](Self::connect_test_cloud_home). Returns the
    /// number of blobs uploaded. Production drains from the running sync loop, so
    /// this stays out of release builds.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn drain_uploads_for_test(&self) -> Result<usize, String> {
        self.handle
            .drain_uploads()
            .await
            .map(|outcome| outcome.uploaded)
    }

    pub async fn get_tracks_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbTrack>, LibraryError> {
        Ok(self.database.get_tracks_for_release(release_id).await?)
    }

    /// All `release_identities` rows for a release. Empty for Unknown.
    pub async fn get_release_identities(
        &self,
        release_id: &str,
    ) -> Result<Vec<crate::import::ReleaseIdentity>, LibraryError> {
        Ok(self.database.get_release_identities(release_id).await?)
    }

    /// Insert identity rows for an existing release.
    pub async fn insert_release_identities(
        &self,
        release_id: &str,
        identities: &[crate::import::ReleaseIdentity],
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .insert_release_identities(release_id, identities)
            .await?)
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
    ) -> Result<Option<String>, String> {
        if identities.is_empty() {
            return Ok(None);
        }

        // 1. Per-pressing rejection: any Exact identity matching a row
        //    already in `release_identities`.
        if let Some(existing) = self
            .database
            .find_album_by_identity_release(identities)
            .await
            .map_err(|e| format!("Database error: {e}"))?
        {
            return Err(format!(
                "This release is already in your library as \"{}\"",
                existing.title,
            ));
        }

        // 2. Cross-source merge: any group identity matching a row already
        //    in `release_identities`.
        let album_id = self
            .database
            .find_album_by_identity_group(identities)
            .await
            .map_err(|e| format!("Database error: {e}"))?;

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

    /// Replace a release's identity rows, metadata-source pointer, and
    /// cached source payload in one shot, moving the release between
    /// albums when the new identity shape doesn't fit the current one.
    ///
    /// `new_identities` may be empty (Unknown), or carry one or more
    /// `(source, source_group_id, source_release_id)` rows that the
    /// caller has already cross-linked. `metadata_pointer` updates the
    /// `metadata_source` / `metadata_source_release_id` columns; a later
    /// re-projection reads these to replay the seed.
    ///
    /// `metadata_pairs` is the freshly-fetched cached payload that
    /// pairs with `metadata_pointer`. Pass an empty slice for Unknown
    /// (no source payload to cache); for Exact/Approximate pass the
    /// `metadata_pairs` returned alongside the parsed release. The
    /// cache replacement is atomic with the identity / pointer write —
    /// there's no in-between state where a re-projection would observe a
    /// stale payload pointing at the prior source.
    ///
    /// **Album side effects.** Empty `new_identities` always moves the
    /// release to a fresh album holding only it. Otherwise, target
    /// resolution prefers a cross-source merge: if any *other* release
    /// in the library has an identity row matching one of
    /// `new_identities` on `(source, source_group_id)`, that release's
    /// album is the destination (the per-source agreement invariant
    /// makes the candidate unique). With no merge candidate the release
    /// stays in its current album when no sibling disagrees on any
    /// shared source, or moves to a fresh album when one does. Vacated
    /// albums with no remaining releases are deleted.
    ///
    /// **Album/release/track row data is not touched.** Pressing fields,
    /// album fields, and tracks stay as-is. Only `release_metadata`
    /// cache rows are replaced. Caller decides whether to also reseed
    /// the metadata.
    ///
    /// Emits one of `AlbumAdded` / `AlbumUpdated` for the destination
    /// album, plus `AlbumRemoved` or `AlbumUpdated` for the vacated
    /// source album when the release actually moved.
    pub async fn set_identity(
        &self,
        release_id: &str,
        new_identities: Vec<crate::import::ReleaseIdentity>,
        metadata_pointer: crate::import::MetadataPointer,
        metadata_pairs: &[(String, String)],
    ) -> Result<(), LibraryError> {
        use crate::db::DbReleaseMetadata;

        let current_album_id = self
            .database
            .find_album_id_for_release(release_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("Release '{release_id}' not found")))?;

        let target = self
            .resolve_identity_target_album(release_id, &current_album_id, &new_identities)
            .await?;

        let (new_metadata_source, new_metadata_source_release_id) =
            metadata_pointer_to_columns(metadata_pointer);

        let now = self.clock.now();
        let new_metadata: Vec<DbReleaseMetadata> = metadata_pairs
            .iter()
            .map(|(source, json)| {
                DbReleaseMetadata::new(release_id, source, json.clone(), self.ids.new_id(), now)
            })
            .collect();

        // The atomic call handles all source-album bookkeeping inside
        // its transaction (empty-check, primary_release_id repair,
        // album_artists copy) plus the `release_metadata` cache
        // replacement. Empty/repair decisions live there to avoid
        // TOCTOU between a separate read and the write.
        let outcome = self
            .database
            .set_identity_atomic(
                release_id,
                &new_identities,
                new_metadata_source,
                new_metadata_source_release_id.as_deref(),
                &current_album_id,
                &target.album_id,
                target.new_album.as_ref(),
                &new_metadata,
            )
            .await?;

        let release_moved = target.album_id != current_album_id;

        // Event emission. The destination album event is fat: AlbumAdded
        // when we created it just now, AlbumUpdated otherwise (its
        // release set changed). The source-album event covers the move
        // itself: AlbumRemoved when the vacated album is now empty,
        // AlbumUpdated when it still has releases.
        if target.new_album.is_some() {
            self.emit_album_added(&target.album_id).await;
        } else {
            self.emit_album_updated(&target.album_id).await;
        }
        if release_moved {
            if outcome.source_album_deleted {
                // The release moved to the destination album; the destination
                // event above already re-homed it. No child releases remain
                // under the vacated source album.
                self.emit_album_removed(&current_album_id, Vec::new());
            } else {
                self.emit_album_updated(&current_album_id).await;
            }
        }

        Ok(())
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
                let prepared = crate::import::service::prepare_release(self, release_ref)
                    .await
                    .map_err(LibraryError::Import)?;

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

    /// Pick the album the release should land in after a `set_identity`.
    /// See `set_identity` for the policy. Lookup order:
    ///
    /// 1. **Cross-source merge first.** If any other release in the
    ///    library carries an identity row matching one of `new_identities`
    ///    on `(source, source_group_id)`, that release's album is the
    ///    target — the per-source agreement invariant guarantees a
    ///    cross-merging album is unique. Even if the current album
    ///    would also fit, the merge candidate wins because two
    ///    different albums cannot both legitimately claim the same
    ///    group.
    /// 2. **Stay in current** when no merge candidate exists and the
    ///    current album's other releases don't disagree with
    ///    `new_identities` on any shared source.
    /// 3. **Fresh album** otherwise.
    async fn resolve_identity_target_album(
        &self,
        release_id: &str,
        current_album_id: &str,
        new_identities: &[crate::import::ReleaseIdentity],
    ) -> Result<IdentityTargetAlbum, LibraryError> {
        // Unknown — always a fresh album holding only this release.
        if new_identities.is_empty() {
            let new_album = self.fresh_album_for_release(current_album_id).await?;
            return Ok(IdentityTargetAlbum {
                album_id: new_album.id.clone(),
                new_album: Some(new_album),
            });
        }

        // Cross-source merge: any album already holding a release that
        // matches the new identity on at least one source.
        // `find_album_by_identity_group_excluding` ignores rows belonging
        // to `release_id` so the lookup never matches against the very
        // identities we're about to overwrite.
        if let Some(candidate_album_id) = self
            .database
            .find_album_by_identity_group_excluding(new_identities, release_id)
            .await?
        {
            return Ok(IdentityTargetAlbum {
                album_id: candidate_album_id,
                new_album: None,
            });
        }

        // No merge candidate. Stay in the current album if its other
        // releases don't disagree with the new identity on any shared
        // source. An album whose only release is this one trivially
        // agrees.
        let other_identities_in_current = self
            .other_release_identities_for_album(current_album_id, release_id)
            .await?;
        if identities_fit_album(new_identities, &other_identities_in_current) {
            return Ok(IdentityTargetAlbum {
                album_id: current_album_id.to_string(),
                new_album: None,
            });
        }

        // Doesn't fit anywhere. Spin up a fresh album.
        let new_album = self.fresh_album_for_release(current_album_id).await?;
        Ok(IdentityTargetAlbum {
            album_id: new_album.id.clone(),
            new_album: Some(new_album),
        })
    }

    /// Identity rows for every release in an album except `exclude_release_id`.
    /// Each inner Vec is one release's identity rows.
    async fn other_release_identities_for_album(
        &self,
        album_id: &str,
        exclude_release_id: &str,
    ) -> Result<Vec<Vec<crate::import::ReleaseIdentity>>, LibraryError> {
        let releases = self.database.get_releases_for_album(album_id).await?;
        let mut all = Vec::with_capacity(releases.len());
        for release in releases {
            if release.id == exclude_release_id {
                continue;
            }
            let ids = self.database.get_release_identities(&release.id).await?;
            all.push(ids);
        }
        Ok(all)
    }

    /// Build a fresh album row that mirrors `seed_album_id`'s metadata.
    /// Used when `set_identity` needs a brand-new album for the release —
    /// metadata isn't touched by `set_identity`, so the new album reflects
    /// what the release already had. Caller can reseed the metadata.
    async fn fresh_album_for_release(&self, seed_album_id: &str) -> Result<DbAlbum, LibraryError> {
        let source = self
            .database
            .find_album_by_id(seed_album_id)
            .await?
            .ok_or_else(|| {
                LibraryError::Import(format!("Source album '{seed_album_id}' not found"))
            })?;
        let now = self.clock.now();
        Ok(DbAlbum {
            id: self.ids.new_id(),
            title: source.title,
            artist_id: source.artist_id,
            year: source.year,
            // The new album holds only this release; let the move pick
            // up `primary_release_id` lazily via the existing fallback
            // ("first release in the album") rather than hard-coding it
            // here.
            primary_release_id: None,
            is_compilation: source.is_compilation,
            created_at: now,
        })
    }

    /// Insert album, release, and tracks into database in a transaction
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

        let resolved_ids = self.find_or_create_artists(&parsed_artists).await?;
        let name_to_id: HashMap<String, String> = name_order
            .iter()
            .zip(resolved_ids.iter())
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
                &album_artists,
                &track_artists,
            )
            .await?;

        self.emit_album_updated(&album_id).await;

        Ok(())
    }

    /// Add a file to the library
    pub async fn add_file(&self, file: &DbFile) -> Result<(), LibraryError> {
        self.database.insert_file(file).await?;
        Ok(())
    }

    /// Atomically insert all import data in a single transaction.
    /// Nothing is in the DB yet (except the import record and artists).
    /// The release either exists complete or doesn't exist at all.
    ///
    /// Track rows are read straight off `tracks_to_files` — each `TrackFile`
    /// owns the `DbTrack` (with its populated `duration_ms`) that gets
    /// inserted. There is no parallel list of tracks or durations.
    #[allow(clippy::too_many_arguments)]
    pub async fn finalize_import_atomic(
        &self,
        album: Option<&DbAlbum>,
        release: &DbRelease,
        tracks_to_files: &[crate::import::TrackFile],
        metadata: &[crate::db::DbReleaseMetadata],
        track_artists: &[crate::db::DbTrackArtist],
        album_artists: &[crate::db::DbAlbumArtist],
        files: &[DbFile],
        audio_formats: &[DbAudioFormat],
        library_image: Option<(&DbLibraryImage, &[u8])>,
        primary_release_id: Option<(&str, &str)>,
        import_id: &str,
        identities: &[crate::import::ReleaseIdentity],
        local_path: &str,
    ) -> Result<(), LibraryError> {
        // The home's storage mode decides the blob layout (opaque hashed-by-id vs.
        // browsable readable paths); the manager owns config, so it reads the mode
        // here rather than threading it from the importer.
        let storage = self.config_handle.config().cloud_home.storage;
        self.database
            .finalize_import_atomic(
                album,
                release,
                tracks_to_files,
                metadata,
                track_artists,
                album_artists,
                files,
                audio_formats,
                library_image,
                primary_release_id,
                import_id,
                identities,
                local_path,
                storage,
            )
            .await?;
        Ok(())
    }

    /// Get all albums in the library, sorted by the given criteria.
    ///
    /// Pass an empty slice for default sort (newest first).
    pub async fn get_albums(
        &self,
        sort: &[crate::db::AlbumSortCriterion],
    ) -> Result<Vec<DbAlbum>, LibraryError> {
        Ok(self.database.get_albums(sort).await?)
    }

    /// Get a page of albums for lazy loading.
    pub async fn get_album_page(
        &self,
        sort: &[crate::db::AlbumSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<AlbumSummary>, LibraryError> {
        let raws = self.database.get_album_page(sort, offset, limit).await?;
        let release_ids: Vec<String> = raws
            .iter()
            .flat_map(|r| r.release_ids.iter().cloned())
            .collect();
        let covers = self.cover_refs(&release_ids).await?;
        Ok(raws
            .into_iter()
            .map(|raw| resolve_album_summary(raw, |rid| covers.get(rid).cloned()))
            .collect())
    }

    /// Count total albums.
    pub async fn get_album_count(&self) -> Result<u64, LibraryError> {
        Ok(self.database.get_album_count().await?)
    }

    pub async fn get_release_storage_summaries(
        &self,
    ) -> Result<Vec<ReleaseStorageSummary>, LibraryError> {
        let raws = self.database.get_release_storage_summaries().await?;
        let has_cloud_home = self.has_cloud_home();
        let mut out = Vec::with_capacity(raws.len());
        for raw in raws {
            let pinned = self.release_pinned(raw.any_file_id.as_deref()).await?;
            out.push(resolve_release_storage_summary(raw, has_cloud_home, pinned));
        }
        Ok(out)
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
        Ok(Some(resolve_release_storage_summary(
            raw,
            has_cloud_home,
            pinned,
        )))
    }

    /// Get album by ID
    pub async fn get_album_by_id(&self, album_id: &str) -> Result<Option<DbAlbum>, LibraryError> {
        Ok(self.database.find_album_by_id(album_id).await?)
    }
    pub async fn find_album_detail(
        &self,
        album_id: &str,
    ) -> Result<Option<AlbumDetail>, LibraryError> {
        let Some(raw) = self.database.find_album_detail(album_id).await? else {
            return Ok(None);
        };
        Ok(Some(self.resolve_album_detail(raw).await?))
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
        let Some(raw) = self.database.find_release_detail(release_id).await? else {
            return Ok(None);
        };
        let has_cloud_home = self.has_cloud_home();
        let album_id = raw.release.album_id.clone();
        let album_artists = self.database.get_artists_for_album(&album_id).await?;
        let releases = self.database.get_releases_for_album(&album_id).await?;
        let release_index = releases
            .iter()
            .position(|r| r.id == release_id)
            .expect("release belongs to its album");
        let pinned = self
            .release_pinned(raw.files.first().map(|f| f.id.as_str()))
            .await?;
        let cover = self.cover_ref(release_id).await?;
        Ok(Some(resolve_release(
            raw,
            &album_artists,
            release_index,
            has_cloud_home,
            pinned,
            cover,
        )))
    }

    /// One page of the Storage Manager list. Rows are returned pre-sorted
    /// and pre-filtered; `total_count` in the returned `StoragePage`
    /// reflects the filtered subset (not the full library).
    pub async fn get_storage_page(
        &self,
        sort: &StorageSort,
        filter: StorageFilter,
        offset: u64,
        limit: u64,
    ) -> Result<StoragePage, LibraryError> {
        let db_sort = to_db_storage_sort(sort);
        let db_filter = to_db_storage_filter(filter);

        let raw_rows = self
            .database
            .get_storage_page(&db_sort, db_filter, offset, limit)
            .await?;
        let total_count = self.database.get_storage_count(db_filter).await?;

        let has_cloud_home = self.has_cloud_home();
        // The cover resolver serves both halves of each row — the release's own id
        // and the album's primary release id — so gather both for the batch lookup.
        let cover_ids: Vec<String> = raw_rows
            .iter()
            .flat_map(|r| {
                [r.release.id.clone()]
                    .into_iter()
                    .chain(r.album.primary_release_id.clone())
                    .chain(r.album.release_ids.iter().cloned())
            })
            .collect();
        let covers = self.cover_refs(&cover_ids).await?;
        let mut rows = Vec::with_capacity(raw_rows.len());
        for raw in raw_rows {
            let pinned = self
                .release_pinned(raw.release.any_file_id.as_deref())
                .await?;
            rows.push(resolve_storage_row(raw, has_cloud_home, pinned, |rid| {
                covers.get(rid).cloned()
            }));
        }
        Ok(StoragePage { rows, total_count })
    }

    /// Count storage rows matching `filter`. Matches `get_storage_page`'s
    /// `total_count` for the same filter.
    pub async fn get_storage_count(&self, filter: StorageFilter) -> Result<u64, LibraryError> {
        let db_filter = to_db_storage_filter(filter);
        Ok(self.database.get_storage_count(db_filter).await?)
    }

    /// Get all releases for a specific album
    pub async fn get_releases_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<DbRelease>, LibraryError> {
        Ok(self.database.get_releases_for_album(album_id).await?)
    }
    /// Get tracks for a specific release
    pub async fn get_tracks(&self, release_id: &str) -> Result<Vec<DbTrack>, LibraryError> {
        Ok(self.database.get_tracks_for_release(release_id).await?)
    }
    /// Get ordered track IDs for a release. Use this when the caller only
    /// needs IDs (queue building, repeat-album rebuild) — avoids pulling
    /// full `DbTrack` rows.
    pub async fn get_track_ids(&self, release_id: &str) -> Result<Vec<String>, LibraryError> {
        Ok(self.database.get_track_ids_for_release(release_id).await?)
    }
    /// Every track id in the library, in a deterministic base order. Used to
    /// materialize a `ContextSource::Library` context (shuffle library, and the
    /// `Context`-repeat re-derive of a library context).
    pub async fn get_all_track_ids(&self) -> Result<Vec<String>, LibraryError> {
        Ok(self.database.get_all_track_ids().await?)
    }
    /// Return the play context for a track: its release id, the release's full
    /// track order, and the track's index within it. Used by the playback
    /// service to build the queue around a freshly selected track without
    /// chaining library calls.
    pub async fn get_play_context(&self, track_id: &str) -> Result<PlayContext, LibraryError> {
        let track = self
            .database
            .find_track_by_id(track_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping(format!("Track not found: {}", track_id)))?;
        let release_id = track.release_id;
        let track_ids = self.database.get_track_ids_for_release(&release_id).await?;
        let index = track_ids
            .iter()
            .position(|id| id == track_id)
            .ok_or_else(|| {
                LibraryError::TrackMapping(format!(
                    "Track {} not present in its release {}",
                    track_id, release_id
                ))
            })?;
        Ok(PlayContext {
            release_id,
            track_ids,
            index,
        })
    }

    /// Return the subset of `ids` that still exist in the tracks table.
    /// Used by playback restore to validate a persisted queue in a single
    /// query instead of one round-trip per track.
    pub async fn filter_existing_track_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<String>, LibraryError> {
        Ok(self.database.filter_existing_track_ids(ids).await?)
    }

    /// Resolve a list of IDs (which may be album IDs or track IDs) into track IDs.
    /// Album IDs are expanded to the track IDs of the album's primary release —
    /// the user's chosen release when set, otherwise the earliest-imported one
    /// (the fallback `primary_release_id` already encodes).
    pub async fn resolve_to_track_ids(&self, ids: &[String]) -> Result<Vec<String>, LibraryError> {
        let mut track_ids = Vec::new();
        for id in ids {
            if let Some(detail) = self.find_album_detail(id).await? {
                if let Some(release) = detail
                    .releases
                    .iter()
                    .find(|r| r.summary.id == detail.primary_release_id)
                {
                    track_ids.extend(release.tracks.iter().map(|t| t.id.clone()));
                }
            } else {
                track_ids.push(id.clone());
            }
        }
        Ok(track_ids)
    }
    pub async fn get_queue_items(
        &self,
        entries: &[QueueEntry],
    ) -> Result<Vec<QueueItem>, LibraryError> {
        Ok(self.database.get_queue_items(entries).await?)
    }
    pub async fn is_source_folder_name_imported(&self, name: &str) -> Result<bool, LibraryError> {
        Ok(self.database.is_source_folder_name_imported(name).await?)
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

    /// Bytes of one gallery slot, dispatching the read on its [`GallerySource`]
    /// so no caller picks the byte source itself: a `Cover` is read by image id
    /// (`read_image_blob`), a `ReleaseFile` by file id (`load_gallery_image`).
    /// The gallery carries a cover slot only when a cover exists, so a `Cover`
    /// with no bytes here is exceptional and surfaces rather than being masked.
    pub async fn read_gallery_bytes(
        &self,
        release_id: &str,
        source: &GallerySource,
    ) -> Result<Vec<u8>, LibraryError> {
        match source {
            GallerySource::Cover(image) => {
                self.read_image_blob(&image.id).await?.ok_or_else(|| {
                    LibraryError::Storage(format!("gallery cover image {} has no bytes", image.id))
                })
            }
            GallerySource::ReleaseFile { file_id } => {
                self.load_gallery_image(release_id, file_id).await
            }
        }
    }

    /// Bytes of one of a release's image files, read from the local copy when it
    /// exists here and otherwise downloaded from the release's cloud home (and
    /// decrypted). The `ReleaseFile` arm of [`read_gallery_bytes`](Self::read_gallery_bytes).
    pub async fn load_gallery_image(
        &self,
        release_id: &str,
        file_id: &str,
    ) -> Result<Vec<u8>, LibraryError> {
        let file = self
            .get_files_for_release(release_id)
            .await?
            .into_iter()
            .find(|f| f.id == file_id)
            .ok_or_else(|| {
                LibraryError::Import(format!(
                    "Image file {file_id} is not part of release {release_id}"
                ))
            })?;
        crate::storage::local::transfer::read_release_file_bytes(&file, self)
            .await
            .map_err(|e| LibraryError::Import(e.to_string()))
    }
    /// Get a specific file by ID
    ///
    /// Used during streaming to retrieve the file record after looking up
    /// the track→file relationship via db_track_position.
    pub async fn get_file_by_id(&self, file_id: &str) -> Result<Option<DbFile>, LibraryError> {
        Ok(self.database.find_file_by_id(file_id).await?)
    }
    /// Get audio format for a track
    pub async fn get_audio_format_by_track_id(
        &self,
        track_id: &str,
    ) -> Result<Option<DbAudioFormat>, LibraryError> {
        Ok(self
            .database
            .find_audio_format_by_track_id(track_id)
            .await?)
    }

    /// Resolve a track's audio into a `ResolvedTrackAudio` with its sample window
    /// resolved and all raw `Db*` fields hidden.
    pub async fn resolve_track_audio(
        &self,
        track_id: &str,
    ) -> Result<ResolvedTrackAudio, LibraryError> {
        let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;
        Ok(ResolvedTrackAudio::from_meta(&meta))
    }

    /// Resolve display metadata (artist names, album, cover) for a track at
    /// playback-preparation time. Done here so `PlaybackService` never sees
    /// `DbTrack`.
    pub async fn get_playback_track_info(
        &self,
        track_id: &str,
    ) -> Result<crate::playback::PlaybackTrackInfo, LibraryError> {
        let track = self
            .database
            .find_track_by_id(track_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping(format!("Track not found: {}", track_id)))?;
        let release = self.database.get_release_for_track(&track).await?;
        playback_info_from_track_release(&self.database, &track, &release).await
    }

    /// Resolve both the audio aggregate and the display metadata for a track in
    /// a single pass — avoids the `resolve_track_audio` + `get_playback_track_info`
    /// double-fetch of `DbTrack`/`DbRelease` at playback prep time.
    pub(crate) async fn resolve_track_audio_and_info(
        &self,
        track_id: &str,
    ) -> Result<(ResolvedTrackAudio, crate::playback::PlaybackTrackInfo), LibraryError> {
        let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;
        let audio = ResolvedTrackAudio::from_meta(&meta);
        let info =
            playback_info_from_track_release(&self.database, &meta.track, &meta.release).await?;
        Ok((audio, info))
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
    /// Insert an artist
    pub async fn insert_artist(&self, artist: &DbArtist) -> Result<(), LibraryError> {
        self.database.insert_artist(artist).await?;
        Ok(())
    }
    /// Get artist by Discogs ID (for deduplication)
    pub async fn get_artist_by_discogs_id(
        &self,
        discogs_artist_id: &str,
    ) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self
            .database
            .get_artist_by_discogs_id(discogs_artist_id)
            .await?)
    }

    /// Get artist by MusicBrainz ID (for deduplication)
    pub async fn get_artist_by_mb_id(&self, mb_id: &str) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self.database.get_artist_by_mb_id(mb_id).await?)
    }

    /// Get artist by name (case-insensitive, first match)
    pub async fn get_artist_by_name(&self, name: &str) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self.database.get_artist_by_name(name).await?)
    }

    /// Fill in NULL external IDs on an existing artist (never overwrites)
    pub async fn update_artist_external_ids(
        &self,
        id: &str,
        discogs_id: Option<&str>,
        mb_id: Option<&str>,
        sort_name: Option<&str>,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .update_artist_external_ids(id, discogs_id, mb_id, sort_name)
            .await?)
    }

    /// Insert album-artist relationship
    pub async fn insert_album_artist(
        &self,
        album_artist: &DbAlbumArtist,
    ) -> Result<(), LibraryError> {
        self.database.insert_album_artist(album_artist).await?;
        Ok(())
    }
    /// Insert track-artist relationship
    pub async fn insert_track_artist(
        &self,
        track_artist: &DbTrackArtist,
    ) -> Result<(), LibraryError> {
        self.database.insert_track_artist(track_artist).await?;
        Ok(())
    }
    /// Get artists for an album
    pub async fn get_artists_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<DbArtist>, LibraryError> {
        Ok(self.database.get_artists_for_album(album_id).await?)
    }
    /// Get artists for a track
    pub async fn get_artists_for_track(
        &self,
        track_id: &str,
    ) -> Result<Vec<DbArtist>, LibraryError> {
        Ok(self.database.get_artists_for_track(track_id).await?)
    }
    /// Get artist by ID
    pub async fn get_artist_by_id(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self.database.find_artist_by_id(artist_id).await?)
    }

    /// Resolve each parsed artist to an existing DB row or insert a new one.
    ///
    /// Returns the DB artist ID for each input in the same order, so callers can
    /// zip with `artists` to build a parsed-ID -> DB-ID map.
    ///
    /// Lookup chain: Various Artists alias (cross-source), `discogs_artist_id`,
    /// `musicbrainz_artist_id`, name (case-insensitive) with source-ID conflict
    /// check, then insert. On a match, any new source IDs are accumulated onto
    /// the existing row via COALESCE.
    pub async fn find_or_create_artists(
        &self,
        artists: &[DbArtist],
    ) -> Result<Vec<String>, LibraryError> {
        let mut resolved = Vec::with_capacity(artists.len());

        for artist in artists {
            // 0. Various Artists: match any known VA ID across sources so that
            //    e.g. Discogs "Various" (ID 194) merges with MB "Various Artists".
            let existing = if artist.is_various_artists() {
                let va = &crate::db::VARIOUS_ARTISTS;
                let by_discogs = self.database.get_artist_by_discogs_id(va.discogs).await?;

                if by_discogs.is_some() {
                    by_discogs
                } else {
                    self.database.get_artist_by_mb_id(va.musicbrainz).await?
                }
            } else {
                None
            };

            // 1. Try discogs_artist_id
            let existing = if existing.is_some() {
                existing
            } else if let Some(ref discogs_id) = artist.discogs_artist_id {
                self.database.get_artist_by_discogs_id(discogs_id).await?
            } else {
                None
            };

            // 2. Try musicbrainz_artist_id
            let existing = match existing {
                Some(e) => Some(e),
                None => {
                    if let Some(ref mb_id) = artist.musicbrainz_artist_id {
                        self.database.get_artist_by_mb_id(mb_id).await?
                    } else {
                        None
                    }
                }
            };

            // 3. Try name (case-insensitive) with conflict check
            let existing = match existing {
                Some(e) => Some(e),
                None => {
                    let name_match = self.database.get_artist_by_name(&artist.name).await?;

                    match name_match {
                        Some(ref matched) => {
                            let discogs_conflict =
                                match (&matched.discogs_artist_id, &artist.discogs_artist_id) {
                                    (Some(a), Some(b)) => a != b,
                                    _ => false,
                                };
                            let mb_conflict = match (
                                &matched.musicbrainz_artist_id,
                                &artist.musicbrainz_artist_id,
                            ) {
                                (Some(a), Some(b)) => a != b,
                                _ => false,
                            };

                            if discogs_conflict || mb_conflict {
                                debug!(
                                    "Name match for '{}' has conflicting source IDs, inserting new artist",
                                    artist.name
                                );
                                None
                            } else {
                                name_match
                            }
                        }
                        None => None,
                    }
                }
            };

            let actual_id = if let Some(existing_artist) = existing {
                self.database
                    .update_artist_external_ids(
                        &existing_artist.id,
                        artist.discogs_artist_id.as_deref(),
                        artist.musicbrainz_artist_id.as_deref(),
                        artist.sort_name.as_deref(),
                    )
                    .await?;
                existing_artist.id
            } else {
                self.database.insert_artist(artist).await?;
                artist.id.clone()
            };

            resolved.push(actual_id);
        }

        Ok(resolved)
    }

    /// Search across albums and tracks.
    pub async fn search_library(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<SearchResults, LibraryError> {
        let raw = self.database.search_library(query, limit).await?;
        let primary_ids: Vec<String> = raw
            .albums
            .iter()
            .filter_map(|a| a.primary_release_id.clone())
            .collect();
        let covers = self.cover_refs(&primary_ids).await?;
        Ok(resolve_search_results(raw, &covers))
    }

    /// Upsert a library image record
    pub async fn upsert_library_image(&self, image: &DbLibraryImage) -> Result<(), LibraryError> {
        self.database.upsert_library_image(image).await?;
        Ok(())
    }

    /// The readable `cloud_path` for an artist image under the current home:
    /// `None` (hashed-by-id) on an opaque home, `Some({artist}/artist.{ext})`
    /// on a browsable one. The manager owns config, so it reads the storage mode.
    pub fn artist_image_cloud_path(
        &self,
        artist_id: &str,
        content_type: &crate::util::content_type::ContentType,
    ) -> Option<String> {
        let storage = self.config_handle.config().cloud_home.storage;
        self.database
            .artist_image_cloud_path_for_storage(storage, artist_id, content_type)
    }

    /// Get a library image by ID and type
    pub async fn get_library_image(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<DbLibraryImage>, LibraryError> {
        Ok(self.database.find_library_image(id, image_type).await?)
    }

    /// Delete a library image by ID and type
    pub async fn delete_library_image(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<(), LibraryError> {
        self.database.delete_library_image(id, image_type).await?;
        Ok(())
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

    /// Change the cover art for an album's release.
    ///
    /// `ReleaseImage`: reads an image file already in the library (by file ID),
    /// copies it to the images dir, and records it as the cover.
    /// `RemoteCover`: downloads cover art from a URL, writes it, records it.
    pub async fn change_cover(
        &self,
        album_id: &str,
        release_id: &str,
        selection: CoverSelection,
    ) -> Result<(), LibraryError> {
        let (bytes, content_type, source, source_url) = match selection {
            CoverSelection::ReleaseImage { file_id } => {
                let file = self
                    .get_file_by_id(&file_id)
                    .await?
                    .ok_or_else(|| LibraryError::Import(format!("File '{file_id}' not found")))?;

                // Read the chosen release image file through coven's locality-aware
                // read (the user's own file when Local, the cache/cloud when Remote).
                let bytes = self.read_release_blob(&file).await?;
                let source_url = format!("release://{}", file.original_filename);
                (
                    bytes,
                    file.content_type.clone(),
                    "local".to_string(),
                    Some(source_url),
                )
            }
            CoverSelection::RemoteCover { url, source } => {
                let (bytes, content_type) =
                    crate::import::cover_art::download_cover_art_bytes(&url)
                        .await
                        .map_err(|e| {
                            LibraryError::Import(format!("Failed to download cover: {e}"))
                        })?;

                (bytes, content_type, source.as_str().to_string(), Some(url))
            }
        };

        // Record the cover blob and row in one coven write. The cover's `id` IS
        // the release id. Under a
        // browsable home the cover blob lands at a readable
        // `{artist}/{album}/cover.{ext}` key, computed + stored here; an opaque
        // home leaves `cloud_path` NULL (hashed-by-id).
        let now = self.clock.now();
        let storage = self.config_handle.config().cloud_home.storage;
        let cloud_path = self
            .database
            .cover_cloud_path_for_storage(storage, release_id, &content_type)
            .await?;
        let library_image = DbLibraryImage {
            id: release_id.to_string(),
            image_type: LibraryImageType::Cover,
            content_type,
            file_size: bytes.len() as i64,
            width: None,
            height: None,
            source,
            source_url,
            cloud_path,
            created_at: now,
        };
        self.store_library_image_blob(&library_image, &bytes)
            .await?;

        // Don't touch primary_release_id here — "change cover" updates
        // the image on this release; "set primary release" is a separate
        // user action. Let the event emit so UIs refresh.
        self.emit_album_updated(album_id).await;

        Ok(())
    }

    /// Queue files for a release for deletion (local + cloud).
    ///
    /// Skips local releases -- those are the user's original files. For
    /// remote releases:
    /// - Queues local file deletion if this device pins them
    /// - Adds cloud outbox delete entries for each file
    /// - Cancels any pending uploads for the same files
    async fn queue_release_files_for_deletion(&self, release_id: &str) {
        let release = match self.database.find_release_by_id(release_id).await {
            Ok(Some(r)) => r,
            _ => return,
        };

        let files = match self.get_files_for_release(release_id).await {
            Ok(files) => files,
            Err(e) => {
                warn!("Failed to get files for release {}: {}", release_id, e);
                return;
            }
        };

        if release.remote {
            // Remote: tombstone the cloud blobs and drop coven's cache copies.
            self.queue_storage_deletions(&files).await;
        } else {
            // Local: the files are the user's own files in place — never delete
            // them. Just clear coven's external refs so no orphan ref outlives the
            // release row.
            for file in &files {
                if let Err(e) = self.database.clear_external_blob(&file.id).await {
                    warn!(
                        "Failed to clear external ref for {} on delete: {e}",
                        file.id
                    );
                }
            }
        }
    }

    /// Tombstone every file's cloud blob (cancelling any pending upload first) and
    /// drop coven's local cache copies, for a Remote release that is being deleted.
    ///
    /// SAFETY: the cloud copies are the only ones, so this is only safe when the
    /// release is genuinely being removed. Its sole caller is
    /// `queue_release_files_for_deletion` (the delete path); make-Local
    /// tombstoning is coven's (it enqueues the deletes inside `make_local`'s
    /// atomic commit). `files` are precomputed by the caller, so the cloud keys are
    /// correct.
    pub(crate) async fn queue_storage_deletions(&self, files: &[DbFile]) {
        // Queue cloud outbox deletes and cancel pending uploads. The delete key
        // must match the key the blob was uploaded under, derived through coven
        // for the home's scheme (the row's readable `cloud_path` on a browsable
        // home, the hashed-by-id default on an opaque one).
        for file in files {
            let cloud_key = match self.release_file_cloud_key(file) {
                Ok(key) => key,
                Err(e) => {
                    warn!("Failed to derive delete key for {}: {e}", file.id);
                    continue;
                }
            };

            // Cancel any pending upload for this file
            if let Err(e) = self
                .database
                .remove_cloud_outbox_uploads_for_key(&cloud_key)
                .await
            {
                warn!("Failed to cancel outbox upload for {}: {e}", cloud_key);
            }

            // Queue cloud delete
            if let Err(e) = self.database.add_cloud_outbox_delete(&cloud_key).await {
                warn!("Failed to add outbox delete for {}: {e}", cloud_key);
            }
        }

        // Drop coven's local cache copies (both pinned and evictable folders) so a
        // deleted release leaks nothing on disk. The release is Remote here, so its
        // blobs are cache copies, not external refs. Dropping the on-device cache
        // for a deleted blob is bae's delete-path responsibility. Best-effort: each
        // drop logs and continues so a cleanup hiccup never aborts the delete.
        for file in files {
            if let Err(e) = self
                .handle
                .evict_blob(&Self::release_file_blob_ref(file))
                .await
            {
                warn!(
                    "Failed to drop on-device copies of {} during deletion: {e}",
                    file.id
                );
            }
        }

        self.emit_outbox_changed().await;
    }

    /// Clean up a release's cover blob when the release is deleted. The `covers`
    /// row itself is cascade-deleted with the release (its FK to `releases`), and
    /// that DELETE changeset replicates the removal — and on peers coven's
    /// apply-side cache drop removes their cover copy. This handles the owner's
    /// blob bytes: a Remote release's cover is in the cloud + cache (tombstone the
    /// cloud blob + drop the cache copy), a Local release's cover is in coven's
    /// local store (drop it). Best-effort: each step logs and continues so a
    /// cleanup hiccup never aborts the delete.
    async fn queue_release_cover_for_deletion(&self, release_id: &str, was_remote: bool) {
        let cover = match self
            .database
            .find_library_image(release_id, &LibraryImageType::Cover)
            .await
        {
            Ok(Some(cover)) => cover,
            // No cover: nothing to clean up.
            Ok(None) => return,
            Err(e) => {
                warn!("Failed to look up cover for release {release_id}: {e}");
                return;
            }
        };

        if was_remote {
            // Remote: tombstone the cloud cover blob (its on-device cache copy is
            // dropped below, alongside the Local case).
            match self.cover_cloud_key(release_id, cover.cloud_path.as_deref()) {
                Ok(cloud_key) => {
                    if let Err(e) = self.database.add_cloud_outbox_delete(&cloud_key).await {
                        warn!("Failed to enqueue cover blob delete for {release_id}: {e}");
                    }
                }
                Err(e) => warn!("Failed to derive cover blob key for {release_id}: {e}"),
            }
        }
        // Drop every on-device copy of the cover blob — a Remote release's cache
        // copy or a Local release's local-store copy (it lived in at most one).
        if let Err(e) = self
            .handle
            .evict_blob(&Self::image_blob_ref(
                crate::sync::COVERS_NAMESPACE,
                release_id,
                cover.cloud_path.clone(),
            ))
            .await
        {
            warn!("Failed to drop on-device cover copies for {release_id}: {e}");
        }

        self.emit_outbox_changed().await;
    }

    /// Delete a release and its associated data
    ///
    /// This will:
    /// 1. Queue files for deferred deletion via the pending deletions manifest
    /// 2. Delete the release from database (cascades to tracks, files, etc.)
    /// 3. If this was the last release for the album, also delete the album
    ///
    /// File cleanup happens asynchronously via the cleanup service, which retries
    /// on failure. This prevents orphaned cloud objects when deletion fails.
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

        // Queue files for deferred deletion before removing DB records
        self.queue_release_files_for_deletion(release_id).await;
        self.queue_release_cover_for_deletion(release_id, release.remote)
            .await;

        self.database.delete_release(release_id).await?;
        let remaining_releases = self.get_releases_for_album(&album_id).await?;
        let album_deleted = remaining_releases.is_empty();
        if album_deleted {
            self.database.delete_album(&album_id).await?;
        } else if let Some(album) = self.database.find_album_by_id(&album_id).await? {
            if album.primary_release_id.as_deref() == Some(release_id) {
                self.database.clear_album_primary_release(&album_id).await?;
            }
        }

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

        // Drain the local `storage/` copies this release queued for deletion.
        // Matches delete_album/unpin/unmanage; without it a single-release
        // delete of a pinned remote release leaks its remote copies on disk.
        self.spawn_cleanup();

        Ok(())
    }

    /// Remove any releases whose stored content hash equals `hash` — full
    /// remote-file cleanup, primary-release reassignment, album cascade, and
    /// removal events, via [`delete_release`](Self::delete_release) per match.
    /// The import worker calls this before inserting a re-import of the same
    /// folder tree, so the re-import overwrites the prior release(s) instead of
    /// duplicating them.
    pub async fn delete_releases_with_content_hash(&self, hash: &str) -> Result<(), LibraryError> {
        for release_id in self.database.release_ids_for_content_hash(hash).await? {
            self.delete_release(&release_id).await?;
        }
        Ok(())
    }

    /// Delete an album and all its associated data
    ///
    /// This will:
    /// 1. Get all releases for the album
    /// 2. Queue files for deferred deletion via the pending deletions manifest
    /// 3. Delete the album from database (cascades to releases and all related data)
    ///
    /// File cleanup happens asynchronously via the cleanup service, which retries
    /// on failure. This prevents orphaned cloud objects when deletion fails.
    pub async fn delete_album(&self, album_id: &str) -> Result<(), LibraryError> {
        let releases = self.get_releases_for_album(album_id).await?;

        // Collect track IDs from all releases before deletion for playback cleanup
        let mut all_track_ids = Vec::new();
        for release in &releases {
            if let Ok(tracks) = self.get_tracks(&release.id).await {
                all_track_ids.extend(tracks.into_iter().map(|t| t.id));
            }
            self.queue_release_files_for_deletion(&release.id).await;
            self.queue_release_cover_for_deletion(&release.id, release.remote)
                .await;
        }

        self.database.delete_album(album_id).await?;

        if !all_track_ids.is_empty() {
            self.emit(LibraryEvent::TracksDeleted {
                track_ids: all_track_ids,
            });
        }

        self.emit_album_removed(album_id, releases.iter().map(|r| r.id.clone()).collect());

        self.spawn_cleanup();

        Ok(())
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn export_release(
        &self,
        release_id: &str,
        target_dir: &Path,
    ) -> Result<(), LibraryError> {
        ExportService::export_release(release_id, target_dir, self)
            .await
            .map_err(LibraryError::Import)
    }

    /// Assemble everything `ExportService::export_track` needs for a
    /// track in one pass: source audio bytes, tag fields, cover image path,
    /// neighbour counts, and the raw audio-format aggregate for decoding.
    /// Cloud-only tracks download + decrypt here — export never requires a
    /// local copy.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn get_export_track_plan(
        &self,
        track_id: &str,
    ) -> Result<ExportTrackPlan, LibraryError> {
        let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;

        let audio_bytes =
            crate::storage::local::transfer::read_release_file_bytes(&meta.audio_file, self)
                .await
                .map_err(|e| {
                    LibraryError::TrackMapping(format!(
                        "Couldn't read audio for track {track_id}: {e}"
                    ))
                })?;

        let album = self.database.get_album_for_release(&meta.release).await?;

        let album_artists = self.database.get_artists_for_album(&album.id).await?;
        let artist = join_artist_names(&album_artists);

        let release_tracks = self
            .database
            .get_tracks_for_release(&meta.release.id)
            .await?;
        let total_tracks = release_tracks.len();
        let has_multiple_sides = release_tracks
            .iter()
            .map(|t| t.side)
            .collect::<std::collections::HashSet<_>>()
            .len()
            > 1;
        let disc = if has_multiple_sides {
            Some(meta.track.side)
        } else {
            None
        };

        let year = meta.release.pressing.year.or(album.year);

        let cover_image_bytes = match album.primary_release_id.as_deref() {
            Some(rid) => self.read_image_blob(rid).await?,
            None => None,
        };

        let is_digital =
            crate::util::format::is_digital_format(meta.release.pressing.format.as_deref());

        let tags = ExportTags {
            title: meta.track.title.clone(),
            artist,
            album: album.title,
            year,
            disc,
        };

        let track_number = meta.track.track_number;

        Ok(ExportTrackPlan {
            audio_bytes,
            tags,
            cover_image_bytes,
            track_number,
            total_tracks,
            is_digital,
            audio_meta: meta,
        })
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn export_track(
        &self,
        track_id: &str,
        output_path: &Path,
        format: crate::library::ExportFormat,
    ) -> Result<(), LibraryError> {
        let plan = self.get_export_track_plan(track_id).await?;
        ExportService::export_track(plan, output_path, format)
            .await
            .map_err(LibraryError::Import)
    }
    /// Insert a new import operation record
    pub async fn insert_import(&self, import: &DbImport) -> Result<(), LibraryError> {
        Ok(self.database.insert_import(import).await?)
    }
    /// Update the status of an import operation
    pub async fn update_import_status(
        &self,
        id: &str,
        status: ImportOperationStatus,
    ) -> Result<(), LibraryError> {
        Ok(self.database.update_import_status(id, status).await?)
    }
    /// Record an error for an import operation
    pub async fn update_import_error(&self, id: &str, error: &str) -> Result<(), LibraryError> {
        Ok(self.database.update_import_error(id, error).await?)
    }
    /// Get all active (non-complete, non-failed) imports
    pub async fn get_active_imports(&self) -> Result<Vec<DbImport>, LibraryError> {
        Ok(self.database.get_active_imports().await?)
    }

    /// Delete an import record (used by UI to dismiss stuck imports)
    pub async fn delete_import(&self, id: &str) -> Result<(), LibraryError> {
        Ok(self.database.delete_import(id).await?)
    }
}

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

mod album;
mod artist;
mod export;
mod identity;
mod image;
mod import;
mod release;
mod storage;
mod track;

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
}

//! Identity domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// All `release_identities` rows for a release. Empty for Unknown.
    pub async fn get_release_identities(
        &self,
        release_id: &str,
    ) -> Result<Vec<crate::import::ReleaseIdentity>, LibraryError> {
        Ok(self.database.get_release_identities(release_id).await?)
    }

    /// Test-only: seed identity rows directly. Production writes them only through
    /// `finalize_import_atomic` or `set_identity`.
    #[cfg(test)]
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

    /// Replace a release's identity rows, metadata-source pointer, and cached source
    /// payload in one shot, moving the release between albums when the new identity
    /// doesn't fit its current one.
    ///
    /// `new_identities` is empty (Unknown), or carries the already-cross-linked
    /// `(source, source_group_id, source_release_id)` rows. `metadata_pointer` sets
    /// the `metadata_source` / `metadata_source_release_id` columns a later
    /// re-projection reads to replay the seed, and `metadata_pairs` is the
    /// freshly-fetched payload that pairs with it — empty for Unknown. The cache
    /// replacement is atomic with the identity/pointer write, so a re-projection can
    /// never see a stale payload pointing at the prior source.
    ///
    /// **Album side effects.** Empty `new_identities` always moves the release to a
    /// fresh album holding only it. Otherwise a cross-source merge wins: if any
    /// *other* release in the library has an identity row matching one of
    /// `new_identities` on `(source, source_group_id)`, that release's album is the
    /// destination (per-source agreement makes the candidate unique). With no merge
    /// candidate the release stays put if no sibling disagrees on a shared source,
    /// and moves to a fresh album if one does. A vacated album with no releases left
    /// is deleted.
    ///
    /// **Album/release/track row data is not touched** — pressing fields, album
    /// fields, and tracks stay as they are, and only `release_metadata` cache rows
    /// are replaced. The caller decides whether to reseed the metadata.
    ///
    /// Emits `AlbumAdded` or `AlbumUpdated` for the destination album, plus
    /// `AlbumRemoved` or `AlbumUpdated` for the vacated source album if the release
    /// actually moved.
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

        // The atomic call does all source-album bookkeeping inside its transaction
        // (empty-check, primary_release_id repair, album_artists copy) plus the
        // `release_metadata` cache replacement — those decisions live there so a
        // separate read and write can't race.
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

        // The destination event is AlbumAdded when we just created the album,
        // AlbumUpdated otherwise (its release set changed). The source event covers
        // the move: AlbumRemoved when the vacated album is now empty, AlbumUpdated
        // when releases remain.
        if target.new_album.is_some() {
            self.emit_album_added(&target.album_id).await;
        } else {
            self.emit_album_updated(&target.album_id).await;
        }
        if release_moved {
            if outcome.source_album_deleted {
                // No child releases remain under the vacated album, and the
                // destination event above already re-homed the moved one.
                self.emit_album_removed(&current_album_id, Vec::new());
            } else {
                self.emit_album_updated(&current_album_id).await;
            }
        }

        Ok(())
    }

    /// Pick the album a release lands in after a `set_identity` (policy in
    /// `set_identity`). In order:
    ///
    /// 1. **Cross-source merge.** If another release in the library carries an
    ///    identity row matching one of `new_identities` on `(source,
    ///    source_group_id)`, its album is the target — per-source agreement makes
    ///    that album unique. It wins even when the current album would also fit,
    ///    since two albums cannot both legitimately claim one group.
    /// 2. **Stay put** when there is no merge candidate and the current album's
    ///    other releases don't disagree with `new_identities` on a shared source.
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

        // Any album already holding a release that matches the new identity on at
        // least one source. The lookup excludes `release_id`'s own rows, so it never
        // matches against the identities we are about to overwrite.
        if let Some(candidate_album_id) = self
            .database
            .find_album_by_identity_group_excluding(new_identities, &[release_id.to_string()])
            .await?
        {
            return Ok(IdentityTargetAlbum {
                album_id: candidate_album_id,
                new_album: None,
            });
        }

        // No merge candidate: stay put if the album's other releases don't disagree
        // on a shared source. An album whose only release is this one agrees
        // trivially.
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

    /// The identity rows of every release in an album but `exclude_release_id`, one
    /// inner Vec per release.
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

    /// A fresh album row mirroring `seed_album_id`'s metadata, for when
    /// `set_identity` needs a brand-new album. `set_identity` doesn't touch
    /// metadata, so the new album reflects what the release already had; the caller
    /// can reseed it.
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
            // The new album holds only this release, so leave `primary_release_id`
            // to the "first release in the album" fallback rather than hard-coding
            // it here.
            primary_release_id: None,
            is_compilation: source.is_compilation,
            created_at: now,
        })
    }
}

/// Per-source agreement: do `new_identities` fit alongside the identity rows of
/// every *other* release in the candidate album? Two releases can share an album as
/// long as they don't disagree on a source they both claim — a shared source
/// requires a matching `source_group_id`; different sources are independent.
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

/// Project a `MetadataPointer` onto the two `releases` columns it sets:
/// `metadata_source`, always present, and `metadata_source_release_id`, NULL when
/// the source is `file_tags`.
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

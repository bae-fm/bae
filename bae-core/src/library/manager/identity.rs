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

    /// Insert identity rows for an existing release. Production writes
    /// identities only through `finalize_import_atomic` or `set_identity`;
    /// only a test helper for seeding rows directly.
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

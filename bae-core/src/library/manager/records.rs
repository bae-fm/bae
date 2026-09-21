//! Release-record operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Test-only: seed records directly. Production writes them only through
    /// `finalize_import_atomic` or `set_records`.
    #[cfg(test)]
    pub async fn insert_release_records(
        &self,
        release_id: &str,
        records: &[crate::import::ReleaseRecord],
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .insert_release_records(release_id, records)
            .await?)
    }

    /// Replace a release's records and where its draft was read from in one
    /// shot, moving the release between albums when the new records don't fit
    /// its current one.
    ///
    /// `new_records` is empty (File Tags), or carries the already-cross-linked
    /// rows, one per catalog. Nothing about the archived provider documents
    /// changes: they are keyed by the catalog's release, so re-pointing at
    /// another one already reads other rows, and the rows this release used may
    /// be another candidate's.
    ///
    /// **Album side effects.** Empty `new_records` always moves the release to
    /// a fresh album holding only it. Otherwise a cross-catalog merge wins: if
    /// any *other* release in the library has a record matching one of
    /// `new_records` on `(catalog, album key)`, that release's album is the
    /// destination (per-catalog agreement makes the candidate unique). With no
    /// merge candidate the release stays put if no sibling disagrees on a
    /// shared catalog, and moves to a fresh album if one does. A vacated album
    /// with no releases left is deleted.
    ///
    /// **Album/release/track row data is not touched** — pressing fields, album
    /// fields, and tracks stay as they are. The caller decides whether to reseed
    /// the metadata.
    ///
    /// The atomic commit wakes subscriptions for the destination and any vacated
    /// source album.
    pub async fn set_records(
        &self,
        release_id: &str,
        new_records: Vec<crate::import::ReleaseRecord>,
        draft_from_tags: bool,
    ) -> Result<(), LibraryError> {
        let current_album_id = self
            .database
            .find_album_id_for_release(release_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("Release '{release_id}' not found")))?;

        let target = self
            .resolve_records_target_album(release_id, &current_album_id, &new_records)
            .await?;

        // The atomic call does all source-album bookkeeping inside its transaction
        // (empty-check, primary_release_id repair, album_artists copy) — those
        // decisions live there so a separate read and write can't race.
        self.database
            .set_records_atomic(
                release_id,
                &new_records,
                draft_from_tags,
                &current_album_id,
                &target.album_id,
                target.new_album.as_ref(),
            )
            .await?;

        Ok(())
    }

    /// Pick the album a release lands in after a `set_records` (policy in
    /// `set_records`). In order:
    ///
    /// 1. **Cross-catalog merge.** If another release in the library carries a
    ///    record matching one of `new_records` on `(catalog, album key)`, its
    ///    album is the target — per-catalog agreement makes that album unique.
    ///    It wins even when the current album would also fit, since two albums
    ///    cannot both legitimately claim one group.
    /// 2. **Stay put** when there is no merge candidate and the current album's
    ///    other releases don't disagree with `new_records` on a shared catalog.
    /// 3. **Fresh album** otherwise.
    async fn resolve_records_target_album(
        &self,
        release_id: &str,
        current_album_id: &str,
        new_records: &[crate::import::ReleaseRecord],
    ) -> Result<RecordsTargetAlbum, LibraryError> {
        // No catalog describes it — always a fresh album holding only this
        // release.
        if new_records.is_empty() {
            let new_album = self.fresh_album_for_release(current_album_id).await?;
            return Ok(RecordsTargetAlbum {
                album_id: new_album.id.clone(),
                new_album: Some(new_album),
            });
        }

        // Any album already holding a release that matches the new records on at
        // least one catalog. The lookup excludes `release_id`'s own rows, so it
        // never matches against the records we are about to overwrite.
        if let Some(candidate_album_id) = self
            .database
            .find_album_by_record_group_excluding(new_records, &[release_id.to_string()])
            .await?
        {
            return Ok(RecordsTargetAlbum {
                album_id: candidate_album_id,
                new_album: None,
            });
        }

        // No merge candidate: stay put if the album's other releases don't disagree
        // on a shared catalog. An album whose only release is this one agrees
        // trivially.
        let other_records_in_current = self
            .other_release_records_for_album(current_album_id, release_id)
            .await?;
        if records_fit_album(new_records, &other_records_in_current) {
            return Ok(RecordsTargetAlbum {
                album_id: current_album_id.to_string(),
                new_album: None,
            });
        }

        // Doesn't fit anywhere. Spin up a fresh album.
        let new_album = self.fresh_album_for_release(current_album_id).await?;
        Ok(RecordsTargetAlbum {
            album_id: new_album.id.clone(),
            new_album: Some(new_album),
        })
    }

    /// The records of every release in an album but `exclude_release_id`, one
    /// inner Vec per release.
    async fn other_release_records_for_album(
        &self,
        album_id: &str,
        exclude_release_id: &str,
    ) -> Result<Vec<Vec<crate::import::ReleaseRecord>>, LibraryError> {
        let releases = self.database.get_releases_for_album(album_id).await?;
        let mut all = Vec::with_capacity(releases.len());
        for release in releases {
            if release.id == exclude_release_id {
                continue;
            }
            let records = self.database.get_release_records(&release.id).await?;
            all.push(records);
        }
        Ok(all)
    }

    /// A fresh album row mirroring `seed_album_id`'s metadata, for when
    /// `set_records` needs a brand-new album. `set_records` doesn't touch
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

/// Per-catalog agreement: do `new_records` fit alongside the records of every
/// *other* release in the candidate album? Two releases can share an album as
/// long as they don't disagree on a catalog they both name — a shared catalog
/// requires a matching group; different catalogs are independent.
fn records_fit_album(
    new_records: &[crate::import::ReleaseRecord],
    other_release_records: &[Vec<crate::import::ReleaseRecord>],
) -> bool {
    for new_album in new_records
        .iter()
        .filter_map(crate::import::ReleaseRecord::album_ref)
    {
        for other_release in other_release_records {
            for existing in other_release
                .iter()
                .filter_map(crate::import::ReleaseRecord::album_ref)
            {
                if existing.catalog == new_album.catalog && existing.key != new_album.key {
                    return false;
                }
            }
        }
    }
    true
}

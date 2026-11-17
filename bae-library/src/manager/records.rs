//! Release-record operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Test-only: seed records directly. Production writes them only through
    /// `finalize_import_atomic` or `set_records`.
    #[cfg(test)]
    pub async fn insert_release_records(
        &self,
        release_id: &str,
        records: &[bae_catalogs::records::ReleaseRecord],
    ) -> Result<(), LibraryError> {
        Ok(self.records.insert_release_records(release_id, records).await?)
    }

    /// Replace a release's records in one shot, moving the release between
    /// albums when the new records don't fit its current one.
    ///
    /// `new_records` is empty (no catalog describes it), or carries the
    /// already-cross-linked rows, one per catalog, the applied one first. Nothing about the stored catalog releases
    /// changes: they are keyed by the catalog's release, so re-pointing at
    /// another one already reads other rows, and the rows this release used may
    /// be another candidate's.
    ///
    /// **Album side effects.** Empty `new_records` always moves the release to
    /// a fresh album holding only it. Otherwise a cross-catalog merge wins: if
    /// any *other* release in the library has a record matching one of
    /// `new_records` on `(catalog, album key)`, that release's album is the
    /// destination (per-catalog agreement makes the candidate unique). With no
    /// merge candidate, a release alone in its album moves to its group's own
    /// album (`bae_records::identity::album_id_for_records`); one with siblings stays put
    /// if no sibling disagrees on a shared catalog, and moves to its group's
    /// album (or a fresh one, with no group) if one does. A vacated album is
    /// deleted with its last release.
    ///
    /// **Album/release/track row data is not touched** — pressing fields, album
    /// fields, and tracks stay as they are. The caller decides whether to reseed
    /// the metadata.
    ///
    /// The atomic commit wakes subscriptions for the destination and any vacated
    /// source album.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn set_records(
        &self,
        release_id: &str,
        new_records: Vec<bae_catalogs::records::ReleaseRecord>,
    ) -> Result<(), LibraryError> {
        let change = self.records_change(release_id, new_records).await?;
        // The atomic call does all source-album bookkeeping inside its transaction
        // (empty-check, the moved release's order, album_artists copy) — those
        // decisions live there so a separate read and write can't race.
        self.records.set_records_atomic(change).await?;
        Ok(())
    }

    /// The write that gives a release `new_records`: the records, and the
    /// album they file it under (policy in [`Self::set_records`]), as
    /// [`bae_records::records::release_records::records_target_album_sql`]
    /// names it. A new album takes the id of the group the records name,
    /// else a fresh one, and the current album's fields.
    pub(super) async fn records_change(
        &self,
        release_id: &str,
        new_records: Vec<bae_catalogs::records::ReleaseRecord>,
    ) -> Result<bae_records::RecordsChange, LibraryError> {
        let target =
            self.records.records_target_album(release_id, &new_records).await?.ok_or_else(
                || LibraryError::release_missing(release_id, super::ReleaseWork::Edit),
            )?;
        let (target_album_id, new_album) = match target.target_album_id {
            Some(album_id) => (album_id, None),
            None => {
                let fresh = self.fresh_album_for_release(&target.current_album_id).await?;
                let album = match bae_records::identity::album_id_for_records(&new_records) {
                    Some(group_album_id) => DbAlbum { id: group_album_id, ..fresh },
                    None => fresh,
                };
                (album.id.clone(), Some(album))
            }
        };
        Ok(bae_records::RecordsChange {
            release_id: release_id.to_string(),
            records: new_records,
            current_album_id: target.current_album_id,
            target_album_id,
            new_album,
        })
    }

    /// A fresh album row mirroring `seed_album_id`'s metadata, for when
    /// `set_records` needs a brand-new album. `set_records` doesn't touch
    /// metadata, so the new album reflects what the release already had; the caller
    /// can reseed it.
    pub(super) async fn fresh_album_for_release(
        &self,
        seed_album_id: &str,
    ) -> Result<DbAlbum, LibraryError> {
        let source =
            self.records.find_album_by_id(seed_album_id).await?.ok_or_else(|| {
                LibraryError::AlbumMissing { album_id: seed_album_id.to_string() }
            })?;
        let now = bae_store::utc_now(self.clock.as_ref());
        Ok(DbAlbum {
            id: self.ids.new_id().to_string(),
            title: source.title,
            year: source.year,
            is_compilation: source.is_compilation,
            created_at: now,
        })
    }
}

#[cfg(test)]
#[path = "records_tests.rs"]
mod tests;

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

    /// [`Self::records_change_filed`] for `release_id` as the library holds
    /// it now.
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) async fn records_change(
        &self,
        release_id: &str,
        new_records: Vec<bae_catalogs::records::ReleaseRecord>,
    ) -> Result<bae_records::RecordsChange, LibraryError> {
        let filed =
            self.records.records_target_album(release_id, &new_records).await?.ok_or_else(
                || LibraryError::release_missing(release_id, super::ReleaseWork::Edit),
            )?;
        let current =
            self.records.find_album_by_id(&filed.current_album_id).await?.ok_or_else(|| {
                LibraryError::AlbumMissing { album_id: filed.current_album_id.clone() }
            })?;
        Ok(self.records_change_filed(release_id, new_records, filed, &current))
    }

    /// The write that gives a release `new_records`: the records, and the
    /// album they file it under (policy in [`Self::set_records`]), which
    /// `filed` names as
    /// [`bae_records::records::fragments::FiledByRecords`] does. A new album
    /// takes the id of the group the records name, else a fresh one, and the
    /// fields of `current`, the album the release is in.
    pub(super) fn records_change_filed(
        &self,
        release_id: &str,
        new_records: Vec<bae_catalogs::records::ReleaseRecord>,
        filed: bae_records::records::release_records::RecordsTargetAlbum,
        current: &DbAlbum,
    ) -> bae_records::RecordsChange {
        let (target_album_id, new_album) = match filed.target_album_id {
            Some(album_id) => (album_id, None),
            None => {
                let fresh = self.fresh_album_like(current);
                let album = match bae_records::identity::album_id_for_records(&new_records) {
                    Some(group_album_id) => DbAlbum { id: group_album_id, ..fresh },
                    None => fresh,
                };
                (album.id.clone(), Some(album))
            }
        };
        bae_records::RecordsChange {
            release_id: release_id.to_string(),
            records: new_records,
            current_album_id: filed.current_album_id,
            target_album_id,
            new_album,
        }
    }

    /// A fresh album row with `album`'s metadata, for a release that moves
    /// into a new album. Moving a release changes no metadata, so the new
    /// album states what the release already had.
    pub(super) fn fresh_album_like(&self, album: &DbAlbum) -> DbAlbum {
        DbAlbum {
            id: self.ids.new_id().to_string(),
            created_at: bae_store::utc_now(self.clock.as_ref()),
            ..album.clone()
        }
    }
}

#[cfg(test)]
#[path = "records_tests.rs"]
mod tests;

//! Album domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Test-only. Production reads albums through the library views'
    /// `find_album_detail` / `get_album_page`, never as a bare row.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_album_by_id(&self, album_id: &str) -> Result<Option<DbAlbum>, LibraryError> {
        Ok(self.database.find_album_by_id(album_id).await?)
    }

    /// Delete an album: every release it holds goes, with what each owns, in
    /// one transaction, as [`Self::delete_release`] deletes one. Each coven
    /// file of theirs goes with its last reference; the album row stays,
    /// empty, for the reason in `vacate_album_on`. An album holding no
    /// release is not in the library, and deleting it is refused.
    pub async fn delete_album(&self, album_id: &str) -> Result<(), LibraryError> {
        let releases = self.get_releases_for_album(album_id).await?;
        if releases.is_empty() {
            return Err(LibraryError::Refused(format!("Album not found: {album_id}")));
        }

        // Read every release's track ids before the delete cascades them away —
        // the player stops a deleted track it plays or has loaded next. The
        // queue's rows leave in the delete's own transaction.
        let mut all_track_ids = Vec::new();
        let mut deletions = Vec::new();
        for release in &releases {
            let tracks = self.get_tracks_for_release(&release.id).await?;
            all_track_ids.extend(tracks.into_iter().map(|t| t.id));
            // Each plan names the rows and coven files it was read against;
            // the write refuses a release that changed since.
            deletions.push(self.database.plan_release_deletion(&release.id).await?);
        }

        self.database.delete_album_with_cleanup(album_id, deletions).await?;

        if !all_track_ids.is_empty() {
            self.tracks_deleted(all_track_ids);
        }

        Ok(())
    }
}

//! Whole albums written in one transaction, for the library fixture a debug
//! UI test opens the app on.

use super::*;

/// One album as a fixture writes it: the album, its one credit, its one
/// release, and that release's tracks.
pub(crate) struct SeededAlbum {
    pub(crate) album: DbAlbum,
    pub(crate) credit: DbAlbumArtist,
    pub(crate) release: DbRelease,
    pub(crate) tracks: Vec<DbTrack>,
}

impl Database {
    /// Insert `artists`, then every album with its credit, release, and
    /// tracks, in one transaction: the library holds all of them or none.
    pub(crate) async fn insert_seeded_albums(
        &self,
        artists: Vec<DbArtist>,
        albums: Vec<SeededAlbum>,
    ) -> Result<(), DbError> {
        self.call_sql(move |sql| {
            let tx = &sql;
            // One HLC stamp for every synced row this transaction writes.
            let reg = sql.stamp();
            for artist in &artists {
                insert_artist_row(tx, artist, &reg)?;
            }
            for seeded in &albums {
                insert_album_row(tx, &seeded.album, &reg)?;
                insert_album_artist_row(tx, &seeded.credit, &reg)?;
                insert_release_row(tx, &seeded.release, &reg)?;
                for track in &seeded.tracks {
                    insert_track_row(tx, track, &reg)?;
                }
            }
            Ok(())
        })
        .await
    }
}

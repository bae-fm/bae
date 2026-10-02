//! Whole albums written in one transaction, for the library fixture a debug
//! UI test opens the app on.

use super::*;

/// One album as a fixture writes it: the album, the artists it credits after
/// its own, its one release with that release's tracks, and the cover the
/// release shows with the bytes it stores.
pub(crate) struct SeededAlbum {
    pub(crate) album: DbAlbum,
    pub(crate) credits: Vec<DbAlbumArtist>,
    pub(crate) release: DbRelease,
    pub(crate) tracks: Vec<DbTrack>,
    pub(crate) cover: Option<(DbLibraryImage, Vec<u8>)>,
}

impl Database {
    /// Insert `artists`, then every album with its credits, release, tracks
    /// and cover, in one transaction: the library holds all of them or none.
    pub(crate) async fn insert_seeded_albums(
        &self,
        artists: Vec<DbArtist>,
        albums: Vec<SeededAlbum>,
    ) -> Result<(), DbError> {
        let cover_blobs: Vec<(String, String, Vec<u8>)> = albums
            .iter()
            .filter_map(|seeded| seeded.cover.as_ref())
            .map(|(image, bytes)| {
                (
                    image.image_type.namespace().to_string(),
                    image.blob_id.clone(),
                    bytes.clone(),
                )
            })
            .collect();
        self.inner
            .handle
            .write_with_blobs(
                move |batch| {
                    for (namespace, id, bytes) in cover_blobs {
                        batch.put_blob(namespace, id, bytes);
                    }
                    Ok(())
                },
                move |sql| {
                    let tx = &sql;
                    // One HLC stamp for every synced row this transaction writes.
                    let reg = sql.stamp();
                    for artist in &artists {
                        insert_artist_row(tx, artist, &reg)?;
                    }
                    for seeded in &albums {
                        insert_album_row(tx, &seeded.album, &reg)?;
                        for credit in &seeded.credits {
                            insert_album_artist_row(tx, credit, &reg)?;
                        }
                        insert_release_row(tx, &seeded.release, &reg)?;
                        for track in &seeded.tracks {
                            insert_track_row(tx, track, &reg)?;
                        }
                        if let Some((image, _)) = &seeded.cover {
                            upsert_library_image_row(tx, image, &reg)?;
                        }
                    }
                    Ok(())
                },
            )
            .await
            .map(|_| ())
            .map_err(Self::coven_error)
    }
}

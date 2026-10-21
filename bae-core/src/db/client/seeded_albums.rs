//! Whole albums written in one transaction, for the library fixture a debug
//! UI test opens the app on.

use super::*;

/// One album as a fixture writes it: the album, the artists it credits after
/// its own, its one release with that release's tracks, the cover the
/// release shows with the bytes it stores, and the audio its tracks play.
pub(crate) struct SeededAlbum {
    pub(crate) album: DbAlbum,
    pub(crate) credits: Vec<DbAlbumArtist>,
    pub(crate) release: DbRelease,
    pub(crate) tracks: Vec<DbTrack>,
    pub(crate) cover: Option<(DbLibraryImage, Vec<u8>)>,
    /// `None` for an album whose tracks play nothing.
    pub(crate) audio: Option<SeededAudio>,
}

/// The audio of a [`SeededAlbum`]'s tracks: the release's files, each the
/// user's own file prepared as an import registers it, and how each track's
/// audio is laid out in them.
pub(crate) struct SeededAudio {
    pub(crate) files: Vec<(DbFile, coven::PreparedUserFile)>,
    pub(crate) audio_formats: Vec<DbAudioFormat>,
    pub(crate) audio_segments: Vec<DbAudioSegment>,
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
                    image.id.clone(),
                    bytes.clone(),
                )
            })
            .collect();
        self.write_with_files(
            move |batch| {
                for (namespace, id, bytes) in cover_blobs {
                    batch.put_file(namespace, id, bytes);
                }
                Ok(())
            },
            move |sql| {
                let tx = &sql;
                for artist in &artists {
                    insert_artist_row(tx, artist)?;
                }
                for seeded in &albums {
                    insert_album_row(tx, &seeded.album)?;
                    for credit in &seeded.credits {
                        insert_album_artist_row(tx, credit)?;
                    }
                    insert_release_row(tx, &seeded.release)?;
                    for track in &seeded.tracks {
                        insert_track_row(tx, track)?;
                    }
                    if let Some((image, _)) = &seeded.cover {
                        upsert_library_image_row(tx, image)?;
                    }
                }
                // The files go in after every release, each registered as
                // the user's own file as an import registers it, then the
                // layouts reading them.
                for audio in albums.into_iter().filter_map(|seeded| seeded.audio) {
                    for (file, blob) in audio.files {
                        insert_external_file_row(tx, &file, blob)?;
                    }
                    for format in &audio.audio_formats {
                        insert_audio_format_row(tx, format)?;
                    }
                    for segment in &audio.audio_segments {
                        insert_audio_segment_row(tx, segment)?;
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

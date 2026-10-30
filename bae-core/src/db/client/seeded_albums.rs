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
    /// The release's one medium, which its tracks sit on.
    pub(crate) medium: DbReleaseMedium,
    pub(crate) tracks: Vec<DbTrack>,
    pub(crate) cover: Option<(DbLibraryImage, Vec<u8>)>,
    /// `None` for an album whose tracks play nothing.
    pub(crate) audio: Option<SeededAudio>,
}

/// The audio of a [`SeededAlbum`]'s tracks: the release's files, each the
/// user's own file as an import records it and stages it with coven, and how
/// each track's
/// audio is laid out in them.
pub(crate) struct SeededAudio {
    pub(crate) files: Vec<(DbFile, coven::UserFile, coven::PreparedUserFile)>,
    pub(crate) audio_formats: Vec<DbAudioFormat>,
    pub(crate) audio_segments: Vec<DbAudioSegment>,
}

impl Database {
    /// Insert `artists`, then every album with its credits, release, tracks
    /// and cover, in one transaction: the library holds all of them or none.
    pub(crate) async fn insert_seeded_albums(
        &self,
        artists: Vec<DbArtist>,
        mut albums: Vec<SeededAlbum>,
    ) -> Result<(), DbError> {
        // Each cover's bytes, then each release file's original, are staged
        // as coven files in album order, which the rows then reference.
        let cover_files: Vec<(&'static str, Vec<u8>)> = albums
            .iter()
            .filter_map(|seeded| seeded.cover.as_ref())
            .map(|(image, bytes)| (image.image_type.namespace(), bytes.clone()))
            .collect();
        let mut originals = Vec::new();
        let mut audios = Vec::new();
        for seeded in &mut albums {
            if let Some(audio) = seeded.audio.take() {
                let mut rows = Vec::with_capacity(audio.files.len());
                for (file, original, prepared) in audio.files {
                    originals.push(prepared);
                    rows.push((file, original));
                }
                audios.push((rows, audio.audio_formats, audio.audio_segments));
            }
        }
        self.write_with_files(
            move |batch| {
                let covers: Vec<coven::FileId> = cover_files
                    .into_iter()
                    .map(|(namespace, bytes)| batch.put_file(namespace, bytes, coven::Place::Cloud))
                    .collect();
                let files: Vec<coven::FileId> = originals
                    .into_iter()
                    .map(|prepared| {
                        batch.put_user_file(
                            crate::sync::RELEASE_FILES_NAMESPACE,
                            coven::UserSource::Cloud(prepared),
                        )
                    })
                    .collect();
                Ok((covers, files))
            },
            move |sql, (covers, files)| {
                let tx = &sql;
                let mut covers = covers.into_iter();
                let mut files = files.into_iter();
                let staged = |file: Option<coven::FileId>| {
                    file.ok_or_else(|| DbError::Message("a seeded file has no coven file".into()))
                };
                for artist in &artists {
                    insert_artist_row(tx, artist)?;
                }
                for seeded in &albums {
                    insert_album_row(tx, &seeded.album)?;
                    for credit in &seeded.credits {
                        insert_album_artist_row(tx, credit)?;
                    }
                    insert_release_row(tx, &seeded.release)?;
                    insert_release_medium_row(tx, &seeded.medium)?;
                    for track in &seeded.tracks {
                        insert_track_row(tx, track)?;
                    }
                    if let Some((image, _)) = &seeded.cover {
                        upsert_library_image_row(tx, image, &staged(covers.next())?)?;
                    }
                }
                // The files go in after every release, each referencing the
                // user's own file as an import stages it, then the layouts
                // reading them.
                for (rows, audio_formats, audio_segments) in &audios {
                    for (file, original) in rows {
                        insert_external_file_row(tx, file, original, staged(files.next())?)?;
                    }
                    for format in audio_formats {
                        insert_audio_format_row(tx, format)?;
                    }
                    for segment in audio_segments {
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

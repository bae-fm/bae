//! Mapping bae's library types onto the Subsonic response DTOs.
//!
//! A Subsonic album is a bae *release* (a specific pressing), so an album's
//! tracks, cover, and audio all come from one release row. These helpers turn a
//! release into an `AlbumID3` and a track into a `Child`, resolving the audio
//! format, artists, and backing file each `Child` requires.

use bae_core::db::{DbFile, DbRelease, DbTrack, LibraryImageType, ReleaseMetadataSource};
use bae_core::library::{AppServices, LibraryError};

use crate::error::SubError;
use crate::model::{album_wire_id, artist_wire_id, track_wire_id, AlbumId3, ArtistId3, Child};

/// A library error is opaque to a Subsonic client — surface it as a generic
/// error (code 0) carrying the message.
pub(crate) fn lib_err(error: LibraryError) -> SubError {
    SubError::generic(error.to_string())
}

/// The release's MusicBrainz id, when its metadata was seeded from MusicBrainz.
/// Used as the `musicBrainzId` of both the album (a release MBID) and its songs.
fn release_mb_id(release: &DbRelease) -> Option<String> {
    match release.metadata_source {
        ReleaseMetadataSource::MusicBrainz => release.metadata_source_release_id.clone(),
        ReleaseMetadataSource::Discogs | ReleaseMetadataSource::FileTags => None,
    }
}

/// Whether this release has stored cover art. Only then is a `coverArt` id
/// advertised, so a client never fetches art that resolves to "not found".
async fn has_cover(services: &AppServices, release_id: &str) -> Result<bool, SubError> {
    Ok(services
        .get_library_image(release_id, &LibraryImageType::Cover)
        .await
        .map_err(lib_err)?
        .is_some())
}

/// Sum of track durations in whole seconds.
fn total_duration_secs(tracks: &[DbTrack]) -> i64 {
    tracks
        .iter()
        .map(|t| t.duration_ms.unwrap_or(0))
        .sum::<i64>()
        / 1000
}

/// The `AlbumID3` for a release: its album's title/artist, this release's year
/// and cover, and the release's song count and total duration.
pub(crate) async fn release_album_id3(
    services: &AppServices,
    release_id: &str,
) -> Result<AlbumId3, SubError> {
    let release = services
        .get_release_by_id(release_id)
        .await
        .map_err(lib_err)?
        .ok_or_else(SubError::not_found)?;
    release_album_id3_with(services, &release).await
}

/// [`release_album_id3`] when the caller already holds the release row.
pub(crate) async fn release_album_id3_with(
    services: &AppServices,
    release: &DbRelease,
) -> Result<AlbumId3, SubError> {
    let album = services
        .find_album_detail(&release.album_id)
        .await
        .map_err(lib_err)?
        .ok_or_else(SubError::not_found)?;
    let tracks = services
        .get_tracks_for_release(&release.id)
        .await
        .map_err(lib_err)?;
    let cover_art = has_cover(services, &release.id)
        .await?
        .then(|| album_wire_id(&release.id));

    Ok(AlbumId3 {
        id: album_wire_id(&release.id),
        name: album.album.title.clone(),
        artist: Some(album.artist_names.clone()),
        artist_id: Some(artist_wire_id(&album.album.artist_id)),
        cover_art,
        song_count: tracks.len() as i64,
        duration: total_duration_secs(&tracks),
        created: Some(release.created_at.to_rfc3339()),
        year: release.pressing.year.or(album.album.year),
        music_brainz_id: release_mb_id(release),
    })
}

/// The audio-derived `Child` fields, shared by the album-song and search-song
/// builders so the two can't disagree on how a track's format maps to
/// `bitDepth`/`samplingRate`/`channelCount`, its original type/suffix, or size.
struct AudioFields {
    release_id: String,
    bit_depth: i64,
    sampling_rate: i64,
    channel_count: i64,
    content_type: String,
    suffix: Option<String>,
    size: Option<i64>,
    duration_secs: Option<i64>,
}

/// Resolve a track's audio and its backing file into the `Child` fields those
/// determine. `files` is the release's file rows when the caller already holds
/// them (whole-album builds); otherwise they are fetched.
async fn resolve_audio_fields(
    services: &AppServices,
    track_id: &str,
    files: Option<&[DbFile]>,
) -> Result<AudioFields, SubError> {
    let audio = services
        .resolve_track_audio(track_id)
        .await
        .map_err(lib_err)?;

    let owned_files;
    let files = match files {
        Some(files) => files,
        None => {
            owned_files = services
                .get_files_for_release(&audio.release_id)
                .await
                .map_err(lib_err)?;
            &owned_files
        }
    };
    // Every track resolves at least one segment, so its first segment's file is
    // the one a raw stream would serve — the source of the original type/suffix.
    let backing = audio
        .segments
        .first()
        .and_then(|segment| files.iter().find(|f| f.id == segment.file_id));

    Ok(AudioFields {
        release_id: audio.release_id.clone(),
        // `bits_per_sample` is `None` for a lossy codec, which has no fixed
        // sample depth; OpenSubsonic's required `bitDepth` reports 0 there.
        bit_depth: audio.bits_per_sample.map(i64::from).unwrap_or(0),
        sampling_rate: i64::from(audio.sample_rate),
        channel_count: i64::from(audio.channels),
        content_type: audio.content_type.as_str().to_string(),
        suffix: backing.map(|f| file_suffix(&f.original_filename)),
        size: backing.map(|f| f.file_size),
        duration_secs: audio.duration_ms.map(|ms| ms / 1000),
    })
}

/// The `Child` (song) for a track on `release`. `album_title`, the release's
/// files, and the cover flag are passed in so a whole-album build resolves them
/// once rather than per track.
pub(crate) async fn track_child(
    services: &AppServices,
    track: &DbTrack,
    release: &DbRelease,
    album_title: &str,
    files: &[DbFile],
    has_cover_art: bool,
) -> Result<Child, SubError> {
    let audio = resolve_audio_fields(services, &track.id, Some(files)).await?;

    let artists = services
        .get_artists_for_track(&track.id)
        .await
        .map_err(lib_err)?;
    let (artist_name, artist_id) = match artists.first() {
        Some(artist) => (
            Some(join_artist_names(&artists)),
            Some(artist_wire_id(&artist.id)),
        ),
        None => (None, None),
    };

    let cover_art = has_cover_art.then(|| album_wire_id(&release.id));

    Ok(Child {
        id: track_wire_id(&track.id),
        parent: Some(album_wire_id(&release.id)),
        title: track.title.clone(),
        album: Some(album_title.to_string()),
        artist: artist_name,
        track: track.track_number,
        year: release.pressing.year,
        cover_art,
        size: audio.size,
        content_type: Some(audio.content_type),
        suffix: audio.suffix,
        duration: audio.duration_secs,
        bit_rate: None,
        disc_number: Some(track.side),
        created: Some(track.created_at.to_rfc3339()),
        album_id: Some(album_wire_id(&release.id)),
        artist_id,
        bit_depth: audio.bit_depth,
        sampling_rate: audio.sampling_rate,
        channel_count: audio.channel_count,
        music_brainz_id: release_mb_id(release),
    })
}

/// The `Child` for a `search3` song hit, which carries only display strings.
/// The album (release) and audio format are resolved from the track id; the
/// per-track fields a search result lacks (track number, disc, created) are
/// omitted, which the spec allows.
pub(crate) async fn search_track_child(
    services: &AppServices,
    track_id: &str,
    title: &str,
    album_title: &str,
    artist_name: &str,
) -> Result<Child, SubError> {
    let audio = resolve_audio_fields(services, track_id, None).await?;
    let release = services
        .get_release_by_id(&audio.release_id)
        .await
        .map_err(lib_err)?
        .ok_or_else(SubError::not_found)?;
    let has_cover_art = has_cover(services, &audio.release_id).await?;

    Ok(Child {
        id: track_wire_id(track_id),
        parent: Some(album_wire_id(&audio.release_id)),
        title: title.to_string(),
        album: Some(album_title.to_string()),
        artist: Some(artist_name.to_string()),
        track: None,
        year: release.pressing.year,
        cover_art: has_cover_art.then(|| album_wire_id(&audio.release_id)),
        size: audio.size,
        content_type: Some(audio.content_type),
        suffix: audio.suffix,
        duration: audio.duration_secs,
        bit_rate: None,
        disc_number: None,
        created: None,
        album_id: Some(album_wire_id(&audio.release_id)),
        artist_id: None,
        bit_depth: audio.bit_depth,
        sampling_rate: audio.sampling_rate,
        channel_count: audio.channel_count,
        music_brainz_id: release_mb_id(&release),
    })
}

/// The number of releases (Subsonic albums) an artist has, summed over their
/// bae albums. `0` for an unknown artist.
pub(crate) async fn artist_release_count(
    services: &AppServices,
    artist_id: &str,
) -> Result<i64, SubError> {
    let detail = services
        .get_artist_detail(artist_id)
        .await
        .map_err(lib_err)?;
    Ok(detail
        .map(|d| d.albums.iter().map(|a| a.release_ids.len()).sum::<usize>())
        .unwrap_or(0) as i64)
}

/// The `ArtistID3` for an artist row, with a release-count album total.
pub(crate) fn artist_id3(
    id: &str,
    name: &str,
    album_count: i64,
    music_brainz_id: Option<String>,
    has_image: bool,
) -> ArtistId3 {
    ArtistId3 {
        id: artist_wire_id(id),
        name: name.to_string(),
        album_count,
        cover_art: has_image.then(|| artist_wire_id(id)),
        music_brainz_id,
    }
}

/// The file extension, lowercased, with no leading dot — Subsonic's `suffix`.
fn file_suffix(filename: &str) -> String {
    filename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Join artist display names the way bae does elsewhere: comma-separated.
fn join_artist_names(artists: &[bae_core::db::DbArtist]) -> String {
    artists
        .iter()
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

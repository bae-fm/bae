//! Mapping bae's library types onto the Subsonic response DTOs.
//!
//! A Subsonic album is a bae *release* (a specific pressing), so an album's
//! tracks, cover, and audio all come from one release row. These helpers turn a
//! release into an `AlbumID3` and a track into a `Child`, resolving the audio
//! format, artists, and backing file each `Child` requires.

use bae_catalogs::catalog::Catalog;
use bae_core::db::{DbFile, DbRelease, DbTrack};
use bae_core::library::{AppServices, LibraryError};

use crate::error::SubError;
use crate::id::SubId;
use crate::model::{
    album_wire_id, artist_wire_id, track_wire_id, AlbumId3, ArtistId3, Child, CreditedArtist,
};

/// A library error is opaque to a Subsonic client — surface it as a generic
/// error (code 0) carrying the message.
pub(crate) fn lib_err(error: LibraryError) -> SubError {
    SubError::generic(error.to_string())
}

/// The release's MusicBrainz id, when MusicBrainz describes it. Used as the
/// `musicBrainzId` of both the album (a release MBID) and its songs.
/// An album-only record identifies the group, not this pressing.
async fn release_mb_id(
    services: &AppServices,
    release_id: &str,
) -> Result<Option<String>, SubError> {
    Ok(services
        .get_release_records(release_id)
        .await
        .map_err(lib_err)?
        .iter()
        .filter_map(bae_core::import::ReleaseRecord::release_ref)
        .find(|release| release.catalog == Catalog::MusicBrainz)
        .map(|release| release.key.clone()))
}

/// The release's `coverArt` id, carrying its cover's version, when it has a
/// stored cover. Only then is a `coverArt` id advertised, so a client never
/// fetches art that resolves to "not found".
async fn release_cover_art(
    services: &AppServices,
    release_id: &str,
) -> Result<Option<String>, SubError> {
    Ok(services
        .cover_ref(release_id)
        .await
        .map_err(lib_err)?
        .map(|cover| SubId::Album(release_id.to_string()).cover_art_id(&cover.version)))
}

/// Sum of track durations in whole seconds.
fn total_duration_secs(tracks: &[DbTrack]) -> i64 {
    whole_seconds(tracks.iter().map(|t| t.duration_ms).sum())
}

/// A length in milliseconds as the whole seconds Subsonic states. Any `u64`
/// count of milliseconds, divided by 1000, fits in an `i64`.
fn whole_seconds(duration_ms: u64) -> i64 {
    i64::try_from(duration_ms / 1000).expect("u64::MAX / 1000 is below i64::MAX")
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
    let cover_art = release_cover_art(services, &release.id).await?;

    Ok(AlbumId3 {
        id: album_wire_id(&release.id),
        name: album.album.title.clone(),
        artist: Some(album.artist_names.clone()),
        artist_id: album
            .artists
            .first()
            .map(|credit| artist_wire_id(&credit.artist.id)),
        artists: CreditedArtist::of(&album.artists),
        cover_art,
        song_count: tracks.len() as i64,
        duration: total_duration_secs(&tracks),
        created: Some(release.created_at.to_rfc3339()),
        year: release.pressing.year.or(album.album.year),
        music_brainz_id: release_mb_id(services, &release.id).await?,
    })
}

/// The audio-derived `Child` fields, shared by the album-song and search-song
/// builders so the two can't disagree on how a track's format maps to
/// `bitDepth`/`samplingRate`/`channelCount`, or the type/suffix/size of what
/// `stream` serves it as unconverted.
struct AudioFields {
    release_id: String,
    bit_depth: i64,
    sampling_rate: i64,
    channel_count: i64,
    content_type: String,
    suffix: Option<String>,
    size: Option<i64>,
    duration_secs: i64,
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
    // What `stream` serves unconverted: a whole-file track's file, as stored;
    // any other track's stream as WAV, whose length is not known until it
    // ends.
    let (content_type, suffix, size) = match audio.whole_file() {
        Some(segment) => {
            let file = files.iter().find(|f| f.id == segment.file_id);
            (
                audio.content_type.as_str().to_string(),
                file.map(|f| file_suffix(&f.original_filename)),
                file.map(|f| f.file_size),
            )
        }
        None => {
            let format = audio.wav_stream_format();
            (
                format.content_type().to_string(),
                Some(format.extension().to_string()),
                None,
            )
        }
    };

    Ok(AudioFields {
        release_id: audio.release_id.clone(),
        // `bits_per_sample` is `None` for a lossy codec, which has no fixed
        // sample depth; OpenSubsonic's required `bitDepth` reports 0 there.
        bit_depth: audio.bits_per_sample.map(i64::from).unwrap_or(0),
        sampling_rate: i64::from(audio.sample_rate.get()),
        channel_count: i64::from(audio.channels.get()),
        content_type,
        suffix,
        size,
        duration_secs: whole_seconds(audio.duration_ms),
    })
}

/// The `Child` (song) for a track on `release`. `album_title`, the release's
/// files and mediums, and its `coverArt` id are passed in so a whole-album
/// build resolves them once rather than per track. Its disc is its medium's
/// position, stated only for a release of several mediums.
pub(crate) async fn track_child(
    services: &AppServices,
    track: &DbTrack,
    release: &DbRelease,
    album_title: &str,
    files: &[DbFile],
    mediums: &bae_core::db::DbReleaseMediums,
    cover_art: Option<String>,
) -> Result<Child, SubError> {
    let audio = resolve_audio_fields(services, &track.id, Some(files)).await?;

    let credits = shown_track_credits(services, &track.id, &release.album_id).await?;

    Ok(Child {
        id: track_wire_id(&track.id),
        parent: Some(album_wire_id(&release.id)),
        title: track.title.clone(),
        album: Some(album_title.to_string()),
        artist: Some(bae_core::credits::library_line(&credits)).filter(|line| !line.is_empty()),
        track: Some(track.track_number.get()),
        year: release.pressing.year,
        cover_art,
        size: audio.size,
        content_type: Some(audio.content_type),
        suffix: audio.suffix,
        duration: audio.duration_secs,
        bit_rate: None,
        disc_number: mediums.disc_number(track),
        created: Some(track.created_at.to_rfc3339()),
        album_id: Some(album_wire_id(&release.id)),
        artist_id: credits
            .first()
            .map(|credit| artist_wire_id(&credit.artist.id)),
        artists: CreditedArtist::of(&credits),
        bit_depth: audio.bit_depth,
        sampling_rate: audio.sampling_rate,
        channel_count: audio.channel_count,
        music_brainz_id: release_mb_id(services, &release.id).await?,
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
    let cover_art = release_cover_art(services, &audio.release_id).await?;
    let credits = shown_track_credits(services, track_id, &release.album_id).await?;

    Ok(Child {
        id: track_wire_id(track_id),
        parent: Some(album_wire_id(&audio.release_id)),
        title: title.to_string(),
        album: Some(album_title.to_string()),
        artist: Some(artist_name.to_string()),
        track: None,
        year: release.pressing.year,
        cover_art,
        size: audio.size,
        content_type: Some(audio.content_type),
        suffix: audio.suffix,
        duration: audio.duration_secs,
        bit_rate: None,
        disc_number: None,
        created: None,
        album_id: Some(album_wire_id(&audio.release_id)),
        artist_id: credits
            .first()
            .map(|credit| artist_wire_id(&credit.artist.id)),
        artists: CreditedArtist::of(&credits),
        bit_depth: audio.bit_depth,
        sampling_rate: audio.sampling_rate,
        channel_count: audio.channel_count,
        music_brainz_id: release_mb_id(services, &audio.release_id).await?,
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

/// The `ArtistID3` for an artist row, with a release-count album total. Its
/// `coverArt` names the artist's own image, at that image's version.
pub(crate) fn artist_id3(
    id: &str,
    name: &str,
    album_count: i64,
    music_brainz_id: Option<String>,
    image: Option<&bae_core::album_detail::ImageRef>,
) -> ArtistId3 {
    ArtistId3 {
        id: artist_wire_id(id),
        name: name.to_string(),
        album_count,
        cover_art: image.map(|image| SubId::Artist(id.to_string()).cover_art_id(&image.version)),
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

/// The credits a song shows: the track's own, else its album's.
async fn shown_track_credits(
    services: &AppServices,
    track_id: &str,
    album_id: &str,
) -> Result<Vec<bae_core::credits::LibraryCredit>, SubError> {
    let own = services
        .get_artists_for_track(track_id)
        .await
        .map_err(lib_err)?;
    if !own.is_empty() {
        return Ok(own);
    }
    services
        .get_artists_for_album(album_id)
        .await
        .map_err(lib_err)
}

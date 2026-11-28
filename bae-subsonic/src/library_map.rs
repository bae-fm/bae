//! Mapping bae's library types onto the Subsonic response DTOs.
//!
//! A Subsonic album is a bae *release* (a specific pressing), so an album's
//! songs, cover and audio all come from one release. Each endpoint reads what
//! it answers in one read, and these helpers turn what it read into an
//! `AlbumID3`, a `Child` or an `ArtistID3`.

use bae_audio::StreamEncodeFormat;
use bae_library::LibraryError;
use bae_records::records::served::{DbNumberedServedTrack, DbServedRelease, DbServedTrack};
use bae_util::timestamp::StoredTimestamp as _;
use bae_views::served::DbServedArtist;

use crate::error::SubError;
use crate::id::SubId;
use crate::model::{
    album_wire_id, artist_wire_id, track_wire_id, AlbumId3, ArtistId3, Child, CreditedArtist,
};

/// A stream bae decodes is served as uncompressed WAV: PCM at the depth the
/// audio was stored at, `bits_per_sample` (16, 24 or 32 bits), or 16 for a
/// lossy codec, which has no stored depth and decodes to nothing finer.
pub(crate) fn wav_stream_format(
    bits_per_sample: Option<std::num::NonZeroU32>,
) -> StreamEncodeFormat {
    let bits_per_sample = match bits_per_sample.map(std::num::NonZeroU32::get) {
        Some(17..=24) => 24,
        Some(25..) => 32,
        Some(_) | None => 16,
    };
    StreamEncodeFormat::PcmWav { bits_per_sample }
}

/// A library error is opaque to a Subsonic client — surface it as a generic
/// error (code 0) carrying the message.
pub(crate) fn lib_err(error: LibraryError) -> SubError {
    SubError::generic(error.to_string())
}

/// A length in milliseconds as the whole seconds Subsonic states. Any `u64`
/// count of milliseconds, divided by 1000, fits in an `i64`.
fn whole_seconds(duration_ms: u64) -> i64 {
    i64::try_from(duration_ms / 1000).expect("u64::MAX / 1000 is below i64::MAX")
}

/// A count as the `i64` Subsonic states. No library holds `i64::MAX` rows.
fn wire_count(count: u64) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

/// The `AlbumID3` for a release: its album's title and credits, this
/// release's year and cover, and the release's song count and total
/// duration. A `coverArt` id, carrying the cover's version, is advertised
/// only for a release with a stored cover, so a client never fetches art that
/// resolves to "not found".
pub(crate) fn album_id3(release: &DbServedRelease) -> AlbumId3 {
    AlbumId3 {
        id: album_wire_id(&release.id),
        name: release.album_title.clone(),
        artist: Some(bae_records::credits::library_line(&release.credits).plain_text()),
        artist_id: release.credits.first().map(|credit| artist_wire_id(&credit.artist.id)),
        artists: CreditedArtist::of(&release.credits),
        cover_art: release
            .cover
            .as_ref()
            .map(|cover| SubId::Album(release.id.clone()).cover_art_id(&cover.version)),
        song_count: wire_count(release.song_count),
        duration: whole_seconds(release.duration_ms),
        created: Some(release.created_at.to_rfc3339()),
        year: release.year,
        music_brainz_id: release.musicbrainz_id.clone(),
    }
}

/// What `stream` serves a track as, unconverted: the file that plays it
/// whole, as stored; any other track's stream as WAV, whose length is not
/// known until it ends. Its content type, suffix and size.
fn served_file(track: &DbServedTrack) -> (String, Option<String>, Option<u64>) {
    match &track.whole_file {
        Some(file) => (
            file.container.http_mime().to_string(),
            bae_records::stored_name::file_extension(&file.name),
            Some(file.size),
        ),
        None => {
            let format = wav_stream_format(track.bits_per_sample);
            (format.container().http_mime().to_string(), Some(format.extension().to_string()), None)
        }
    }
}

/// The `Child` (song) for `track` on `release`, whose `AlbumID3` is `album`.
/// The album-song and search-song builders share it so the two can't
/// disagree on how a track's format maps to `bitDepth`, `samplingRate` and
/// `channelCount`, or the type, suffix and size of what `stream` serves.
fn song(track: &DbServedTrack, release: &DbServedRelease, album: &AlbumId3) -> Child {
    let (content_type, suffix, size) = served_file(track);
    Child {
        id: track_wire_id(&track.id),
        parent: Some(album_wire_id(&release.id)),
        title: track.title.clone(),
        album: Some(album.name.clone()),
        artist: Some(bae_records::credits::library_line(&track.credits).plain_text()),
        track: None,
        year: album.year,
        cover_art: album.cover_art.clone(),
        size,
        content_type: Some(content_type),
        suffix,
        duration: whole_seconds(track.duration_ms),
        bit_rate: None,
        disc_number: None,
        created: None,
        album_id: Some(album_wire_id(&release.id)),
        artist_id: track.credits.first().map(|credit| artist_wire_id(&credit.artist.id)),
        artists: CreditedArtist::of(&track.credits),
        // `bits_per_sample` is `None` for a lossy codec, which has no fixed
        // sample depth; OpenSubsonic's required `bitDepth` reports 0 there.
        bit_depth: track.bits_per_sample.map(|bits| i64::from(bits.get())).unwrap_or(0),
        sampling_rate: i64::from(track.sample_rate.get()),
        channel_count: i64::from(track.channels.get()),
        music_brainz_id: release.musicbrainz_id.clone(),
    }
}

/// The `Child` (song) for a numbered track on `release`, whose `AlbumID3` is
/// `album`. Its number counts its medium's tracks across the medium's sides,
/// and its disc is its medium's position, stated only for a release of
/// several mediums. A track that credits no one states no `artist`.
pub(crate) fn track_child(
    numbered: &DbNumberedServedTrack,
    release: &DbServedRelease,
    album: &AlbumId3,
) -> Child {
    let mut child = song(&numbered.track, release, album);
    child.artist = child.artist.filter(|line| !line.is_empty());
    child.track = Some(numbered.number);
    child.disc_number = numbered.disc;
    child.created = Some(numbered.track.created_at.to_rfc3339());
    child
}

/// The `Child` for a `search3` song hit. The fields a search result omits
/// (track number, disc, created) are left out, which the spec allows.
pub(crate) fn search_track_child(track: &DbServedTrack, release: &DbServedRelease) -> Child {
    song(track, release, &album_id3(release))
}

/// The `ArtistID3` for an artist, whose album total counts its releases (a
/// Subsonic album is a release). Its `coverArt` names the artist's own image,
/// at that image's version.
pub(crate) fn artist_id3(served: &DbServedArtist) -> ArtistId3 {
    let summary = &served.artist;
    let artist = &summary.artist;
    ArtistId3 {
        id: artist_wire_id(&artist.id),
        name: artist.name.clone(),
        album_count: wire_count(served.release_count),
        cover_art: summary
            .image
            .as_ref()
            .map(|image| SubId::Artist(artist.id.clone()).cover_art_id(&image.version)),
        music_brainz_id: artist.musicbrainz_artist_id.clone(),
    }
}

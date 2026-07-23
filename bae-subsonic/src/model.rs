//! Subsonic/OpenSubsonic response objects as wire DTOs.
//!
//! Each type here mirrors a Subsonic response schema (`ArtistID3`, `AlbumID3`,
//! `Child`, …) and knows how to render itself into the shared [`Element`] model,
//! from which the envelope emits XML or JSON. The endpoint modules fill these
//! from bae's library types; the field set and names come from the OpenSubsonic
//! spec (opensubsonic.netlify.app).

use crate::envelope::Element;
use crate::id::SubId;

/// `ArtistID3` — an artist in the tag-based browsing tree.
pub(crate) struct ArtistId3 {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) album_count: i64,
    pub(crate) cover_art: Option<String>,
    pub(crate) music_brainz_id: Option<String>,
}

impl ArtistId3 {
    fn to_named(&self, name: &'static str) -> Element {
        Element::new(name)
            .attr("id", self.id.clone())
            .attr("name", self.name.clone())
            .attr("albumCount", self.album_count)
            .opt_attr("coverArt", self.cover_art.clone())
            .opt_attr("musicBrainzId", self.music_brainz_id.clone())
    }

    /// As an `<artist>` element (its element name in indexes and search).
    pub(crate) fn to_element(&self) -> Element {
        self.to_named("artist")
    }
}

/// `AlbumID3` — an album (a bae release) in the tag-based browsing tree.
pub(crate) struct AlbumId3 {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) artist: Option<String>,
    pub(crate) artist_id: Option<String>,
    pub(crate) cover_art: Option<String>,
    pub(crate) song_count: i64,
    /// Total playing time in whole seconds.
    pub(crate) duration: i64,
    pub(crate) created: Option<String>,
    pub(crate) year: Option<i32>,
    pub(crate) music_brainz_id: Option<String>,
}

impl AlbumId3 {
    fn base(&self, name: &'static str) -> Element {
        Element::new(name)
            .attr("id", self.id.clone())
            .attr("name", self.name.clone())
            .opt_attr("artist", self.artist.clone())
            .opt_attr("artistId", self.artist_id.clone())
            .opt_attr("coverArt", self.cover_art.clone())
            .attr("songCount", self.song_count)
            .attr("duration", self.duration)
            .opt_attr("created", self.created.clone())
            .opt_attr("year", self.year.map(i64::from))
            .opt_attr("musicBrainzId", self.music_brainz_id.clone())
    }

    /// As an `<album>` element, for list and search responses.
    pub(crate) fn to_element(&self) -> Element {
        self.base("album")
    }

    /// As an `<album>` element with its songs nested — the `getAlbum` payload.
    pub(crate) fn with_songs(&self, songs: Vec<Child>) -> Element {
        self.base("album")
            .children(songs.into_iter().map(|song| song.to_element()))
    }
}

/// `Child` — a song (Subsonic's media-item shape). OpenSubsonic makes
/// `bitDepth`, `samplingRate`, and `channelCount` required; a lossy track,
/// which has no fixed sample depth, reports `bitDepth=0` (the accepted
/// "not applicable" value).
pub(crate) struct Child {
    pub(crate) id: String,
    /// The album (release) this song belongs to.
    pub(crate) parent: Option<String>,
    pub(crate) title: String,
    pub(crate) album: Option<String>,
    pub(crate) artist: Option<String>,
    pub(crate) track: Option<i32>,
    pub(crate) year: Option<i32>,
    pub(crate) cover_art: Option<String>,
    pub(crate) size: Option<i64>,
    /// MIME type of the *original* file (Subsonic convention — a transcode's
    /// real type reaches the client through the response `Content-Type`).
    pub(crate) content_type: Option<String>,
    pub(crate) suffix: Option<String>,
    /// Playing time in whole seconds.
    pub(crate) duration: Option<i64>,
    pub(crate) bit_rate: Option<i64>,
    pub(crate) disc_number: Option<i32>,
    pub(crate) created: Option<String>,
    pub(crate) album_id: Option<String>,
    pub(crate) artist_id: Option<String>,
    pub(crate) bit_depth: i64,
    pub(crate) sampling_rate: i64,
    pub(crate) channel_count: i64,
    pub(crate) music_brainz_id: Option<String>,
}

impl Child {
    /// As a `<song>` element (its element name in album and search responses).
    pub(crate) fn to_element(&self) -> Element {
        Element::new("song")
            .attr("id", self.id.clone())
            .opt_attr("parent", self.parent.clone())
            .attr("isDir", false)
            .attr("title", self.title.clone())
            .opt_attr("album", self.album.clone())
            .opt_attr("artist", self.artist.clone())
            .opt_attr("track", self.track.map(i64::from))
            .opt_attr("year", self.year.map(i64::from))
            .opt_attr("coverArt", self.cover_art.clone())
            .opt_attr("size", self.size)
            .opt_attr("contentType", self.content_type.clone())
            .opt_attr("suffix", self.suffix.clone())
            .opt_attr("duration", self.duration)
            .opt_attr("bitRate", self.bit_rate)
            .opt_attr("discNumber", self.disc_number.map(i64::from))
            .opt_attr("created", self.created.clone())
            .opt_attr("albumId", self.album_id.clone())
            .opt_attr("artistId", self.artist_id.clone())
            .attr("type", "music")
            .attr("bitDepth", self.bit_depth)
            .attr("samplingRate", self.sampling_rate)
            .attr("channelCount", self.channel_count)
            .opt_attr("musicBrainzId", self.music_brainz_id.clone())
    }
}

/// The wire id for a release-as-album.
pub(crate) fn album_wire_id(release_id: &str) -> String {
    SubId::Album(release_id.to_string()).encode()
}

/// The wire id for an artist.
pub(crate) fn artist_wire_id(artist_id: &str) -> String {
    SubId::Artist(artist_id.to_string()).encode()
}

/// The wire id for a track-as-song.
pub(crate) fn track_wire_id(track_id: &str) -> String {
    SubId::Track(track_id.to_string()).encode()
}

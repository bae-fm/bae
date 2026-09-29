//! Namespaced Subsonic ids.
//!
//! bae's own ids are bare UUIDs with no type tag, but Subsonic ids flow across
//! endpoints — `search3` returns artists, albums, and songs together, and
//! `getCoverArt?id=` accepts any of them. So the wire id carries its kind as a
//! short prefix, and any handler resolves an id to the entity it names:
//!
//! ```text
//! ar-<artist_uuid>    artist -> DbArtist
//! al-<release_uuid>   album  -> DbRelease   (a Subsonic album is a bae release)
//! tr-<track_uuid>     song   -> DbTrack
//! ```
//!
//! A `coverArt` id is the wire id of the image's subject — an album's cover or
//! an artist's image — followed by `~` and the image's content version, so the
//! id changes whenever the image does and a client caching art by id fetches
//! it again. `getCoverArt` also takes a plain album, song, or artist id, which
//! some clients send.
//!
//! A malformed id, or one of the wrong kind for the endpoint that received it,
//! is a Subsonic "not found" (error 70).

use crate::error::SubError;

const ARTIST_PREFIX: &str = "ar-";
const ALBUM_PREFIX: &str = "al-";
const TRACK_PREFIX: &str = "tr-";
const VERSION_SEPARATOR: char = '~';

/// A parsed Subsonic id and the kind of entity it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SubId {
    Artist(String),
    Album(String),
    Track(String),
}

impl SubId {
    /// The wire form: the kind prefix followed by the bare bae id.
    pub(crate) fn encode(&self) -> String {
        match self {
            SubId::Artist(id) => format!("{ARTIST_PREFIX}{id}"),
            SubId::Album(id) => format!("{ALBUM_PREFIX}{id}"),
            SubId::Track(id) => format!("{TRACK_PREFIX}{id}"),
        }
    }

    /// Parse a wire id into its kind and bare id. An id with no known prefix, or
    /// with an empty body, is "not found" — the same answer a real-but-absent id
    /// gets, so a client can't tell a typo from a deleted row.
    pub(crate) fn parse(raw: &str) -> Result<SubId, SubError> {
        let parsed = if let Some(rest) = raw.strip_prefix(ARTIST_PREFIX) {
            SubId::Artist(rest.to_string())
        } else if let Some(rest) = raw.strip_prefix(ALBUM_PREFIX) {
            SubId::Album(rest.to_string())
        } else if let Some(rest) = raw.strip_prefix(TRACK_PREFIX) {
            SubId::Track(rest.to_string())
        } else {
            return Err(SubError::not_found());
        };
        if parsed.bare().is_empty() {
            return Err(SubError::not_found());
        }
        Ok(parsed)
    }

    /// The `coverArt` id for this subject's image at `version`.
    pub(crate) fn cover_art_id(&self, version: &str) -> String {
        format!("{}{VERSION_SEPARATOR}{version}", self.encode())
    }

    /// The subject a `getCoverArt` id names: a `coverArt` id, whose version only
    /// distinguishes one image from the next and reads whatever image the
    /// subject has now, or a plain id.
    pub(crate) fn parse_cover_art(raw: &str) -> Result<SubId, SubError> {
        match raw.split_once(VERSION_SEPARATOR) {
            Some((_, "")) => Err(SubError::not_found()),
            Some((subject, _version)) => SubId::parse(subject),
            None => SubId::parse(raw),
        }
    }

    /// The bare bae id, prefix stripped.
    pub(crate) fn bare(&self) -> &str {
        match self {
            SubId::Artist(id) | SubId::Album(id) | SubId::Track(id) => id,
        }
    }

    /// The bare artist id, or "not found" if this id names a different kind. The
    /// wrong-kind case is the same error as an unknown id: an endpoint expecting
    /// an artist can't act on an album id.
    pub(crate) fn expect_artist(&self) -> Result<&str, SubError> {
        match self {
            SubId::Artist(id) => Ok(id),
            _ => Err(SubError::not_found()),
        }
    }

    /// The bare album (= release) id, or "not found" for a different kind.
    pub(crate) fn expect_album(&self) -> Result<&str, SubError> {
        match self {
            SubId::Album(id) => Ok(id),
            _ => Err(SubError::not_found()),
        }
    }

    /// The bare track id, or "not found" for a different kind.
    pub(crate) fn expect_track(&self) -> Result<&str, SubError> {
        match self {
            SubId::Track(id) => Ok(id),
            _ => Err(SubError::not_found()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_each_kind() {
        for id in [
            SubId::Artist("a1".to_string()),
            SubId::Album("r1".to_string()),
            SubId::Track("t1".to_string()),
        ] {
            assert_eq!(SubId::parse(&id.encode()).unwrap(), id);
        }
    }

    #[test]
    fn a_prefixless_id_is_not_found() {
        assert_eq!(SubId::parse("plain-uuid").unwrap_err().code, 70);
        assert_eq!(SubId::parse("").unwrap_err().code, 70);
        assert_eq!(SubId::parse("ar-").unwrap_err().code, 70);
    }

    #[test]
    fn wrong_kind_is_not_found() {
        let album = SubId::parse("al-r1").unwrap();
        assert_eq!(album.expect_artist().unwrap_err().code, 70);
        assert_eq!(album.expect_track().unwrap_err().code, 70);
        assert_eq!(album.expect_album().unwrap(), "r1");
    }

    #[test]
    fn cover_art_ids_carry_their_version_and_parse_to_their_subject() {
        let album = SubId::Album("r1".to_string());
        let id = album.cover_art_id("blob-2");
        assert_eq!(id, "al-r1~blob-2");
        assert_eq!(SubId::parse_cover_art(&id).unwrap(), album);
        assert_eq!(SubId::parse_cover_art("al-r1").unwrap(), album);
        assert_eq!(SubId::parse_cover_art("al-r1~").unwrap_err().code, 70);
    }
}

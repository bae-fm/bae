//! A track as playback shows it: the now-playing surfaces, the queue's rows
//! and the system media controls. [`DisplayedTrack`] is the fragment every
//! read of that display spreads.

use super::*;

/// A track as the now-playing bar, the queue, and the system media controls
/// show it, read from the library's current state.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackDisplay {
    pub title: String,
    /// The artists the track credits, or its album's when it credits none.
    pub artist_line: bae_types::credits::CreditLine,
    pub album_id: String,
    /// The release the track is on, which may be any of its album's releases.
    pub release_id: String,
    pub album_title: String,
    /// The track's own release's cover, versioned, so new bytes replace the
    /// copy a UI decoded; `None` when that release has no cover.
    pub cover_image: Option<crate::ImageRef>,
}

// The release a track is on as the track's display names it, as one JSON
// object: its album, and the version of its own cover, not the album's primary
// release's.
sqlive_compose::fragment!(
    ShownRelease on tracks {
        struct ShownReleaseDocument {
            album_id: String,
            album_title: String,
            cover_version: Option<String>
        }
        shown_release: json<ShownReleaseDocument>
    },
    r#"
    SELECT json_object(
               'album_id', album.id,
               'album_title', album.title,
               'cover_version', (SELECT cover.version
                                 FROM @crate::records::files::CoverVersion AS cover
                                 WHERE cover.id = release.id)) AS shown_release
    FROM releases AS release
    JOIN albums AS album ON album.id = release.album_id
    WHERE release.id = @parent.release_id
"#
);

// Each track as playback shows it: its id as `display_track_id`, its length,
// its title, its release, the credits it shows and the release as it shows it.
// The fragment reads `tracks` alone, so that SQLite flattens it where a read
// puts it on the right of a LEFT JOIN: a subquery that is itself a join would
// be built there for every track in the library.
sqlive_compose::fragment!(
    DisplayedTrack { duration_ms: u64 },
    r#"
    SELECT track.id AS display_track_id, track.duration_ms, track.title, track.release_id,
           @crate::records::credit_fragments::TrackCredits(parent: track) AS credits,
           @crate::records::track_display::ShownRelease(parent: track) AS release
    FROM tracks AS track
"#
);

impl DisplayedTrack {
    /// The track's id.
    pub fn track_id(&self) -> &str {
        &self.display_track_id
    }

    /// How long the track plays, in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        self.duration_ms
    }

    /// The track's display. A track's release and its credits are always
    /// there to read, so a missing one fails.
    pub fn into_display(self) -> Result<TrackDisplay, DbError> {
        let missing = |what: &str| {
            DbError::unreadable(
                format!("the {what} of track {}", self.display_track_id),
                None::<String>,
            )
        };
        let shown = self.release.ok_or_else(|| missing("release"))?.shown_release;
        let credits = self.credits.ok_or_else(|| missing("credits"))?;
        Ok(TrackDisplay {
            title: self.title,
            artist_line: super::read::credit_line_of_json(&credits.credits)?,
            // A release's cover image is keyed by the release's id.
            cover_image: crate::cover_ref(&self.release_id, shown.cover_version),
            album_id: shown.album_id,
            release_id: self.release_id,
            album_title: shown.album_title,
        })
    }
}

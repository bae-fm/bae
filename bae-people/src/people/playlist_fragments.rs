//! What the people domain offers to the playlist screens: a playlist as it is
//! listed, a playlist's entries in order, and whether the person using this
//! device can make a playlist.

use crate::collections::{PlaylistCover, PlaylistEntryRow, PlaylistSummary, COVER_GRID};
use crate::people::{PersonRef, Sharing};
use bae_records::cover_ref;
use bae_store::DbError;
use bae_types::count_label::CountLabel;

// Each playlist as it is listed: its owner, whether this device's person made
// it, how many entries it has, how long their tracks play, and the covers of
// the first four different releases its entries are on, in entry order.
sqlive_compose::fragment!(
    ListedPlaylist {
        audience: coven::Audience,
        yours: bool,
        track_count: u64,
        total_duration_ms: u64,
        struct PlaylistCoverDocument { release_id: String, version: Option<String> }
        covers: json<Vec<PlaylistCoverDocument>>,
    },
    r#"
    SELECT pl.id AS playlist_id, pl.name AS playlist_name, pl.audience, owner.*,
           COALESCE(pl.person_id = (SELECT own.member FROM coven_members AS own
                                    WHERE own.is_self AND NOT own.removed), 0) AS yours,
           (SELECT COUNT(*) FROM playlist_entries AS counted
            WHERE counted.playlist_id = pl.id) AS track_count,
           (SELECT COALESCE(SUM(timed.duration_ms), 0)
            FROM playlist_entries AS played
            JOIN @bae_records::records::spread_fragments::TrackRow AS timed
              ON timed.track_id = played.track_id
            WHERE played.playlist_id = pl.id) AS total_duration_ms,
           (SELECT json_group_array(json_object(
                       'release_id', first.release_id, 'version', first.version)
                       ORDER BY first.first_position)
            FROM (SELECT held.release_id, cover.version,
                         MIN(entry.position) AS first_position
                  FROM playlist_entries AS entry
                  JOIN @bae_records::records::spread_fragments::TrackRow AS held
                    ON held.track_id = entry.track_id
                  LEFT JOIN @bae_records::records::files::CoverVersion AS cover
                         ON cover.id = held.release_id
                  WHERE entry.playlist_id = pl.id
                  GROUP BY held.release_id
                  ORDER BY first_position
                  LIMIT 4) AS first) AS covers
    FROM playlists AS pl
    JOIN @crate::people::fragments::Profile AS owner ON owner.id = pl.person_id
"#
);

// The entries of the playlist `playlist` as its page lists them, as one JSON
// array in order, each with its track, the track's album, its release's
// cover, its number from 1, and whether an entry comes before it and after
// it. `playlist_entries_position` serves the lookup and the numbering's order.
sqlive_compose::fragment!(
    PagedEntries(playlist: Option<String>) {
        struct PlaylistEntryDocument {
            position_id: String,
            track_id: String,
            title: String,
            artist_line: bae_records::records::read::CreditListLine,
            album_id: String,
            album_title: String,
            release_id: String,
            duration_ms: u64,
            cover_version: Option<String>,
            number: u64,
            can_move_up: bool,
            can_move_down: bool
        }
        entries: json<Vec<PlaylistEntryDocument>>,
    },
    r#"
    SELECT json_group_array(json_object(
               'position_id', listed.position_id, 'track_id', listed.track_id,
               'title', track.title, 'artist_line', json(shown.credits),
               'album_id', filed.album_id, 'album_title', album.title,
               'release_id', track.release_id, 'duration_ms', track.duration_ms,
               'cover_version', cover.version, 'number', listed.number,
               'can_move_up', json(iif(listed.can_move_up, 'true', 'false')),
               'can_move_down', json(iif(listed.can_move_down, 'true', 'false')))
               ORDER BY listed.number) AS entries
    FROM (SELECT entry.position_id, entry.track_id,
                 ROW_NUMBER() OVER entries AS number,
                 LAG(entry.position_id) OVER entries IS NOT NULL AS can_move_up,
                 LEAD(entry.position_id) OVER entries IS NOT NULL AS can_move_down
          FROM playlist_entries AS entry
          WHERE entry.playlist_id = :playlist
          WINDOW entries AS (ORDER BY entry.position)) AS listed
    JOIN @bae_records::records::spread_fragments::TrackRow AS track
      ON track.track_id = listed.track_id
    JOIN @bae_records::records::credit_fragments::ShownTrackCredits AS shown
      ON shown.track_id = listed.track_id
    JOIN @bae_records::records::fragments::ReleaseAlbum AS filed
      ON filed.release_id = track.release_id
    JOIN @bae_records::records::fragments::AlbumTitle AS album
      ON album.album_id = filed.album_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS cover
           ON cover.id = track.release_id
"#
);

// Whether the person using this device has a profile, which a playlist needs
// to have an owner.
sqlive_compose::fragment!(
    OwnProfileExists { has_profile: bool },
    r#"
    SELECT COUNT(*) > 0 AS has_profile
    FROM coven_members AS viewing
    JOIN people AS profiled ON profiled.id = viewing.member
    WHERE viewing.is_self AND NOT viewing.removed
"#
);

impl OwnProfileExists {
    /// Whether the person has a profile.
    pub fn has_profile(&self) -> bool {
        self.has_profile
    }
}

/// The UUID `text` that `what` holds.
fn stored_uuid(what: &str, text: &str) -> Result<uuid::Uuid, DbError> {
    uuid::Uuid::parse_str(text).map_err(|_error| DbError::unreadable(what, text))
}

impl ListedPlaylist {
    /// The playlist as its views list it.
    pub fn into_summary(self) -> Result<PlaylistSummary, DbError> {
        let owner = self.owner.into_profile();
        let mut covers =
            self.covers.into_iter().map(|cover| cover_ref(&cover.release_id, cover.version));
        let cover = match <[_; COVER_GRID]>::try_from(covers.by_ref().collect::<Vec<_>>()) {
            Ok(grid) => PlaylistCover::Grid(grid),
            Err(fewer) => PlaylistCover::Single(fewer.into_iter().next().flatten()),
        };
        Ok(PlaylistSummary {
            id: stored_uuid("a playlist's id", &self.playlist_id)?,
            name: self.playlist_name,
            sharing: Sharing::of_audience(self.audience),
            owner: PersonRef {
                person: owner.id,
                name: owner.name,
                avatar_version: owner.avatar.map(|avatar| avatar.version),
            },
            yours: self.yours,
            track_count: CountLabel {
                key: "core.library.track_count".to_string(),
                count: self.track_count,
            },
            total_duration_ms: self.total_duration_ms,
            cover,
        })
    }
}

impl PagedEntries {
    /// The entries as the playlist's page lists them, in order.
    pub fn into_rows(self) -> Result<Vec<PlaylistEntryRow>, DbError> {
        self.entries
            .into_iter()
            .map(|entry| {
                Ok(PlaylistEntryRow {
                    position_id: stored_uuid("a playlist entry's id", &entry.position_id)?,
                    cover: cover_ref(&entry.release_id, entry.cover_version),
                    track_id: entry.track_id,
                    title: entry.title,
                    artist_line: entry.artist_line.into_line(),
                    album_id: entry.album_id,
                    album_title: entry.album_title,
                    release_id: entry.release_id,
                    duration_ms: entry.duration_ms,
                    number: entry.number,
                    can_move_up: entry.can_move_up,
                    can_move_down: entry.can_move_down,
                })
            })
            .collect()
    }
}

//! The rows one write lays down for a whole release: an import's commit, or
//! an edit that applies a catalog's release over a library one.

use super::*;

/// The rows of the works a release's write lays down.
#[derive(Debug, Clone)]
pub struct WorkGraphRows {
    pub works: Vec<DbWork>,
    pub work_artists: Vec<DbWorkArtist>,
    pub work_parts: Vec<DbWorkPart>,
    pub track_works: Vec<DbTrackWork>,
}

/// The rows a release's write lays down in `albums`, `releases`, `tracks`
/// and the tables that link them. Its artists are credits the write resolves
/// to library artists when it commits. The release's records are the link's
/// to state, not these rows', so they reach the write alongside.
#[derive(Debug, Clone)]
pub struct ReleaseRows {
    pub album: DbAlbum,
    pub release: DbRelease,
    /// The release's mediums, in order; each track sits on one.
    pub mediums: Vec<DbReleaseMedium>,
    pub tracks: Vec<TrackRow>,
    pub artists: Vec<DbArtist>,
    pub album_artists: Vec<DbAlbumArtist>,
    pub track_artists: Vec<DbTrackArtist>,
    pub work_graph: WorkGraphRows,
    pub release_artist_roles: Vec<DbReleaseArtistRole>,
    pub track_artist_roles: Vec<DbTrackArtistRole>,
}

/// A track row before its audio is bound: every column but the length, which
/// only the audio it plays can measure. Measuring it makes the [`DbTrack`],
/// so no track row exists without its length.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackRow {
    pub id: String,
    pub release_id: String,
    pub title: String,
    pub medium_id: String,
    pub side: std::num::NonZeroU8,
    pub track_number: std::num::NonZeroU32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl TrackRow {
    /// The row, its audio measuring `duration_ms`.
    pub fn measured(self, duration_ms: u64) -> DbTrack {
        DbTrack {
            id: self.id,
            release_id: self.release_id,
            title: self.title,
            medium_id: self.medium_id,
            side: self.side,
            track_number: self.track_number,
            duration_ms,
            created_at: self.created_at,
        }
    }
}

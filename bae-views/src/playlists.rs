//! The playlists the screens read live: the list of every playlist this
//! device holds, the Add to playlist menu's targets, and one playlist's page
//! with its entries (§5.3).

use super::*;
use bae_people::people::Sharing;
use bae_records::records::files::cover_version_sql;
use bae_types::count_label::CountLabel;

/// How many different releases' covers a playlist's picture shows when it
/// has that many.
const COVER_GRID: usize = 4;

/// The picture a playlist is drawn with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaylistCover {
    /// One cover: the first entry's release's when the playlist's entries
    /// are on fewer than four different releases. `None` draws the
    /// missing-art tile, as a playlist with no entries or a first release
    /// with no cover does.
    Single(Option<ImageRef>),
    /// The covers of the first four different releases the entries are on,
    /// in entry order, drawn as a 2×2 square; a release with no cover is
    /// `None` in its place.
    Grid([Option<ImageRef>; COVER_GRID]),
}

/// Who made a playlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistOwner {
    pub person: coven::MemberId,
    pub name: String,
    /// The version of the person's avatar; `None` without one.
    pub avatar_version: Option<String>,
}

/// A playlist as the Playlists view and the Add to playlist menu list it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistSummary {
    pub id: uuid::Uuid,
    pub name: String,
    pub sharing: Sharing,
    pub owner: PlaylistOwner,
    /// Whether this device's person made it.
    pub yours: bool,
    /// "{count} tracks" (`core.library.track_count`).
    pub track_count: CountLabel,
    /// The entries' total playing time; zero when there are none.
    pub total_duration_ms: u64,
    pub cover: PlaylistCover,
}

/// The playlists this device holds, as the views that list them read them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistListing {
    /// Every playlist this device holds, the person's own and the household
    /// ones others made, by name then id.
    pub listed: Vec<PlaylistSummary>,
    /// The person's own playlists, by name then id: where Add to playlist
    /// can put a track.
    pub add_targets: Vec<PlaylistSummary>,
    /// How many playlists this device holds: the length of `listed`.
    pub total: u64,
    /// Whether the person has a profile, which a playlist needs to have an
    /// owner.
    pub can_create: bool,
}

/// One entry of a playlist's page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistEntryRow {
    pub position_id: uuid::Uuid,
    pub track_id: String,
    pub title: String,
    /// The track's credit line: its own credits, else its album's (§2.3).
    pub artist_line: bae_types::credits::CreditLine,
    pub album_id: String,
    pub album_title: String,
    pub release_id: String,
    pub duration_ms: u64,
    /// The cover of the release the track is on.
    pub cover: Option<ImageRef>,
}

/// A playlist's page: the playlist and its entries in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistDetail {
    pub summary: PlaylistSummary,
    pub entries: Vec<PlaylistEntryRow>,
}

/// The playlists, with the columns [`playlist_row`] reads, over `playlists pl`
/// and its owner's profile, `narrowing` narrowing them, by name in the
/// device's language, then id. Each playlist's summary comes with it: its
/// entries counted and summed, the covers of its first releases (`cover_grid`,
/// when it has a grid's worth of different releases) and the cover of its
/// first entry's release.
pub(crate) fn playlists_sql(narrowing: &str) -> String {
    format!(
        "SELECT pl.id, pl.name, pl.audience, pl.person_id, owner.name, {avatar}, \
                COALESCE(pl.person_id = {own}, 0) AS yours, \
                (SELECT COUNT(*) FROM playlist_entries AS e \
                 WHERE e.playlist_id = pl.id) AS track_count, \
                (SELECT COALESCE(SUM(t.duration_ms), 0) FROM playlist_entries AS e \
                 JOIN tracks AS t ON t.id = e.track_id \
                 WHERE e.playlist_id = pl.id) AS total_duration_ms, \
                (SELECT CASE WHEN COUNT(*) = {COVER_GRID} THEN json_group_array( \
                        json_object('release_id', release_id, 'cover_version', cover) \
                        ORDER BY first_position) END \
                 FROM (SELECT t.release_id AS release_id, {cover} AS cover, \
                              MIN(e.position) AS first_position \
                       FROM playlist_entries AS e JOIN tracks AS t ON t.id = e.track_id \
                       WHERE e.playlist_id = pl.id GROUP BY t.release_id \
                       ORDER BY first_position LIMIT {COVER_GRID})) AS cover_grid, \
                (SELECT json_object('release_id', t.release_id, 'cover_version', {cover}) \
                 FROM playlist_entries AS e JOIN tracks AS t ON t.id = e.track_id \
                 WHERE e.playlist_id = pl.id ORDER BY e.position LIMIT 1) AS first_cover \
         FROM playlists AS pl JOIN people AS owner ON owner.id = pl.person_id{narrowing} \
         ORDER BY {name_key}, pl.id",
        name_key = crate::name_sort::sort_key_sql("pl.name"),
        avatar = bae_store::file_version_sql("owner.avatar_id"),
        own = OWN_MEMBER,
        cover = cover_version_sql("t.release_id"),
    )
}

/// This device's member.
const OWN_MEMBER: &str = "(SELECT member FROM coven_members WHERE is_self AND NOT removed)";

/// One playlist's entries as its page lists them, in order.
pub(crate) fn playlist_entries_sql() -> String {
    format!(
        "SELECT e.position_id, e.track_id, t.title, {credits} AS artist_line, \
                r.album_id, a.title, t.release_id, t.duration_ms, {cover} \
         FROM playlist_entries AS e JOIN tracks AS t ON t.id = e.track_id \
              JOIN releases AS r ON r.id = t.release_id JOIN albums AS a ON a.id = r.album_id \
         WHERE e.playlist_id = ?1 ORDER BY e.position",
        credits = track_credits_json_sql("t"),
        cover = cover_version_sql("t.release_id"),
    )
}

/// How many playlists [`playlists_sql`] lists.
pub(crate) const PLAYLIST_TOTAL_SQL: &str = "SELECT COUNT(*) FROM playlists AS pl \
     JOIN people AS owner ON owner.id = pl.person_id";

/// Whether the person using this device has a profile.
pub(crate) const CAN_CREATE_SQL: &str = "SELECT EXISTS (SELECT 1 FROM coven_members AS m \
     JOIN people AS p ON p.id = m.member WHERE m.is_self AND NOT m.removed)";

fn uuid_column(row: &Row, index: usize) -> rusqlite::Result<uuid::Uuid> {
    let value: String = row.get(index)?;
    uuid::Uuid::parse_str(&value).map_err(|error| column_conversion_error(row, index, error))
}

/// The covers a playlist's summary names, as JSON objects of a release and
/// its cover's version.
fn cover_of(object: &serde_json::Value) -> Option<ImageRef> {
    let release = object["release_id"].as_str()?;
    cover_ref(release, object["cover_version"].as_str().map(str::to_string))
}

fn json_column(row: &Row, index: usize) -> rusqlite::Result<Option<serde_json::Value>> {
    row.get::<_, Option<String>>(index)?
        .map(|text| {
            serde_json::from_str(&text).map_err(|error| column_conversion_error(row, index, error))
        })
        .transpose()
}

/// A playlist as [`playlists_sql`] reads it.
fn playlist_row(row: &Row) -> rusqlite::Result<PlaylistSummary> {
    let cover = match json_column(row, 9)? {
        Some(grid) => {
            let mut covers = grid.as_array().into_iter().flatten().map(cover_of);
            PlaylistCover::Grid(std::array::from_fn(|_| covers.next().flatten()))
        }
        None => PlaylistCover::Single(json_column(row, 10)?.as_ref().and_then(cover_of)),
    };
    Ok(PlaylistSummary {
        id: uuid_column(row, 0)?,
        name: row.get(1)?,
        sharing: Sharing::of_audience(row.get(2)?),
        owner: PlaylistOwner {
            person: coven::MemberId(uuid_column(row, 3)?),
            name: row.get(4)?,
            avatar_version: row.get(5)?,
        },
        yours: row.get(6)?,
        track_count: CountLabel { key: "core.library.track_count".to_string(), count: row.get(7)? },
        total_duration_ms: row.get(8)?,
        cover,
    })
}

fn playlist_listing_on(sql: &SqlReadContext<'_>) -> Result<PlaylistListing, DbError> {
    Ok(PlaylistListing {
        listed: sql.query(&playlists_sql(""), [], playlist_row)?,
        add_targets: sql.query(
            &playlists_sql(&format!(" WHERE pl.person_id = {OWN_MEMBER}")),
            [],
            playlist_row,
        )?,
        total: sql.query_row(PLAYLIST_TOTAL_SQL, [], |row| row.get(0))?,
        can_create: sql.query_row(CAN_CREATE_SQL, [], |row| row.get(0))?,
    })
}

fn playlist_detail_on(
    sql: &SqlReadContext<'_>,
    id: &Option<String>,
) -> Result<Option<PlaylistDetail>, DbError> {
    let Some(id) = id else {
        return Ok(None);
    };
    let Some(summary) = sql.query(&playlists_sql(" WHERE pl.id = ?1"), [id], playlist_row)?.pop()
    else {
        return Ok(None);
    };
    let entries = sql.query(&playlist_entries_sql(), [id], |row| {
        Ok(PlaylistEntryRow {
            position_id: uuid_column(row, 0)?,
            track_id: row.get(1)?,
            title: row.get(2)?,
            artist_line: credit_line_column(row, "artist_line")?,
            album_id: row.get(4)?,
            album_title: row.get(5)?,
            release_id: row.get(6)?,
            duration_ms: row.get(7)?,
            cover: cover_ref(&row.get::<_, String>(6)?, row.get(8)?),
        })
    })?;
    Ok(Some(PlaylistDetail { summary, entries }))
}

impl LibraryViews {
    /// Every playlist this device holds, live.
    pub fn subscribe_playlists(&self) -> coven::LiveQuery<PlaylistListing, bae_store::DbError> {
        self.store.subscribe(|sql| playlist_listing_on(&sql))
    }

    /// Follow the playlist `initial` names, or none; the page moves to
    /// another playlist through the request handle. `None` once the
    /// playlist is gone.
    pub fn subscribe_playlist_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, Option<PlaylistDetail>, bae_store::DbError>
    {
        self.store.subscribe_reconfigurable(initial, |id, sql| playlist_detail_on(&sql, id))
    }
}

#[cfg(test)]
#[path = "playlists_tests.rs"]
mod tests;

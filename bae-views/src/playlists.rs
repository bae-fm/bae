//! The playlists the screens read live: the list of every playlist this
//! device holds, the Add to playlist menu's targets, and one playlist's page
//! with its entries (§5.3).

use super::*;
use bae_people::people::Sharing;
use bae_records::records::files::cover_version_sql;
use bae_types::count_label::CountLabel;
use std::collections::HashMap;

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

/// The playlists, with the columns [`summary_of`] reads, over `playlists pl`
/// and its owner's profile, `narrowing` narrowing them, by name in the
/// device's language, then id.
pub(crate) fn playlists_sql(narrowing: &str) -> String {
    format!(
        "SELECT pl.id, pl.name, pl.audience, pl.person_id, owner.name, {avatar}, \
                COALESCE(pl.person_id = {own}, 0) AS yours \
         FROM playlists AS pl JOIN people AS owner ON owner.id = pl.person_id{narrowing} \
         ORDER BY {name_key}, pl.id",
        name_key = crate::name_sort::sort_key_sql("pl.name"),
        avatar = bae_store::file_version_sql("owner.avatar_id"),
        own = OWN_MEMBER,
    )
}

/// This device's member.
const OWN_MEMBER: &str = "(SELECT member FROM coven_members WHERE is_self AND NOT removed)";

/// What each entry of every playlist adds to its playlist's summary, in
/// each playlist's order; `playlist_entries_position` serves the order.
pub(crate) fn entry_stats_sql() -> String {
    format!(
        "SELECT e.playlist_id, t.release_id, t.duration_ms, {cover} \
         FROM playlist_entries AS e JOIN tracks AS t ON t.id = e.track_id \
         ORDER BY e.playlist_id, e.position",
        cover = cover_version_sql("t.release_id"),
    )
}

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

/// Whether the person using this device has a profile.
pub(crate) const CAN_CREATE_SQL: &str = "SELECT EXISTS (SELECT 1 FROM coven_members AS m \
     JOIN people AS p ON p.id = m.member WHERE m.is_self AND NOT m.removed)";

/// The entries of one playlist, as far as its summary shows them.
#[derive(Default)]
struct Stats {
    tracks: u32,
    duration_ms: u64,
    /// The different releases the entries are on, in entry order, up to a
    /// grid's worth, each with its cover.
    releases: Vec<(String, Option<ImageRef>)>,
    /// The first entry's release's cover.
    first: Option<Option<ImageRef>>,
}

impl Stats {
    fn add(&mut self, release_id: String, duration_ms: u64, cover_version: Option<String>) {
        self.tracks += 1;
        self.duration_ms += duration_ms;
        let cover = cover_ref(&release_id, cover_version);
        if self.first.is_none() {
            self.first = Some(cover.clone());
        }
        if self.releases.len() < COVER_GRID
            && !self.releases.iter().any(|(release, _)| *release == release_id)
        {
            self.releases.push((release_id, cover));
        }
    }

    fn cover(self) -> PlaylistCover {
        if self.releases.len() < COVER_GRID {
            return PlaylistCover::Single(self.first.flatten());
        }
        let mut covers = self.releases.into_iter().map(|(_, cover)| cover);
        PlaylistCover::Grid(std::array::from_fn(|_| covers.next().flatten()))
    }
}

fn uuid_column(row: &Row, index: usize) -> rusqlite::Result<uuid::Uuid> {
    let value: String = row.get(index)?;
    uuid::Uuid::parse_str(&value).map_err(|error| column_conversion_error(row, index, error))
}

/// A playlist without its entries' part.
struct PlaylistRow {
    id: uuid::Uuid,
    name: String,
    sharing: Sharing,
    owner: PlaylistOwner,
    yours: bool,
}

fn playlist_row(row: &Row) -> rusqlite::Result<PlaylistRow> {
    Ok(PlaylistRow {
        id: uuid_column(row, 0)?,
        name: row.get(1)?,
        sharing: Sharing::of_audience(row.get(2)?),
        owner: PlaylistOwner {
            person: coven::MemberId(uuid_column(row, 3)?),
            name: row.get(4)?,
            avatar_version: row.get(5)?,
        },
        yours: row.get(6)?,
    })
}

impl PlaylistRow {
    fn summarized(self, stats: Stats) -> PlaylistSummary {
        PlaylistSummary {
            id: self.id,
            name: self.name,
            sharing: self.sharing,
            owner: self.owner,
            yours: self.yours,
            track_count: CountLabel {
                key: "core.library.track_count".to_string(),
                count: stats.tracks,
            },
            total_duration_ms: stats.duration_ms,
            cover: stats.cover(),
        }
    }
}

fn playlist_listing_on(sql: &SqlReadContext<'_>) -> Result<PlaylistListing, DbError> {
    let rows = sql.query(&playlists_sql(""), [], playlist_row)?;
    let mut stats: HashMap<uuid::Uuid, Stats> = HashMap::new();
    for (playlist, release, duration_ms, cover_version) in
        sql.query(&entry_stats_sql(), [], |row| {
            Ok((
                uuid_column(row, 0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u64>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?
    {
        stats.entry(playlist).or_default().add(release, duration_ms, cover_version);
    }
    let listed: Vec<PlaylistSummary> = rows
        .into_iter()
        .map(|row| {
            let stats = stats.remove(&row.id).unwrap_or_default();
            row.summarized(stats)
        })
        .collect();
    let add_targets = listed.iter().filter(|playlist| playlist.yours).cloned().collect();
    Ok(PlaylistListing {
        listed,
        add_targets,
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
    let Some(row) = sql.query(&playlists_sql(" WHERE pl.id = ?1"), [id], playlist_row)?.pop()
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
    let mut stats = Stats::default();
    for entry in &entries {
        stats.add(
            entry.release_id.clone(),
            entry.duration_ms,
            entry.cover.as_ref().map(|cover| cover.version.clone()),
        );
    }
    Ok(Some(PlaylistDetail { summary: row.summarized(stats), entries }))
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

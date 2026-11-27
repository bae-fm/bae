//! The playlists the screens read live: the list of every playlist this
//! device holds, the Add to playlist menu's targets, and one playlist's page
//! with its entries (§5.3).

use super::*;
pub use bae_people::collections::{PlaylistCover, PlaylistEntryRow, PlaylistSummary};
use bae_people::people::playlist_fragments::ListedPlaylist;

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
    /// How many of them are the person's own: the length of `add_targets`.
    pub add_target_count: u64,
    /// Whether the person has a profile, which a playlist needs to have an
    /// owner.
    pub can_create: bool,
}

/// A playlist's page: the playlist and its entries in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistDetail {
    pub summary: PlaylistSummary,
    pub entries: Vec<PlaylistEntryRow>,
    /// Whether the playlist holds no entries, so its start is the one place
    /// its page takes dropped tracks.
    pub empty: bool,
}

// Every playlist this device holds, by name in the device's language, then id,
// each row beside how many there are. The read reads `name_sort`, so coven
// reruns it when the device's language changes.
sqlive_compose::read!(
    PlaylistList { total: u64 },
    r#"
    SELECT playlist.*, COUNT(*) OVER () AS total
    FROM @bae_people::people::playlist_fragments::ListedPlaylist AS playlist
    JOIN @bae_records::records::name_sort::NameSortLanguage AS sort ON 1
    ORDER BY bae_name_key(playlist.playlist_name, sort.language), playlist.playlist_id
"#
);

// The person's own playlists, where Add to playlist can put a track, in the
// order [`PlaylistList`] lists them, each row beside how many there are.
sqlive_compose::read!(
    AddTargets { total: u64 },
    r#"
    SELECT playlist.*, COUNT(*) OVER () AS total
    FROM @bae_people::people::playlist_fragments::ListedPlaylist AS playlist
    JOIN @bae_records::records::name_sort::NameSortLanguage AS sort ON 1
    WHERE playlist.yours
    ORDER BY bae_name_key(playlist.playlist_name, sort.language), playlist.playlist_id
"#
);

// Whether the person can make a playlist.
sqlive_compose::read!(
    CanCreatePlaylist,
    r#"
    SELECT viewer.*
    FROM @bae_people::people::playlist_fragments::OwnProfileExists AS viewer
"#
);

// The playlist `playlist` with its entries; no row when the library holds no
// such playlist, or `playlist` names none.
sqlive_compose::read!(
    PlaylistPage(playlist: Option<String>),
    r#"
    SELECT playlist.*, paged.*
    FROM @bae_people::people::playlist_fragments::ListedPlaylist AS playlist
    JOIN @bae_people::people::playlist_fragments::PagedEntries(playlist: :playlist) AS paged
      ON 1
    WHERE playlist.playlist_id = :playlist
"#
);

/// The playlists of a read's rows, each given beside the count of the read's
/// rows, with that count: zero for a read with no row.
fn counted_playlists(
    rows: impl IntoIterator<Item = (u64, ListedPlaylist)>,
) -> Result<(Vec<PlaylistSummary>, u64), DbError> {
    let mut count = 0;
    let playlists = rows
        .into_iter()
        .map(|(total, playlist)| {
            count = total;
            playlist.into_summary()
        })
        .collect::<Result<_, _>>()?;
    Ok((playlists, count))
}

fn playlist_listing_on(sql: &SqlReadContext<'_>) -> Result<PlaylistListing, DbError> {
    let listed = bae_store::statement_rows(sql, &PlaylistListParams {})?;
    let (listed, total) =
        counted_playlists(listed.into_iter().map(|row| (row.total, row.playlist)))?;
    let targets = bae_store::statement_rows(sql, &AddTargetsParams {})?;
    let (add_targets, add_target_count) =
        counted_playlists(targets.into_iter().map(|row| (row.total, row.playlist)))?;
    let can_create =
        bae_store::statement_row(sql, &CanCreatePlaylistParams {})?.viewer.has_profile();
    Ok(PlaylistListing { listed, add_targets, total, add_target_count, can_create })
}

fn playlist_detail_on(
    sql: &SqlReadContext<'_>,
    id: &Option<String>,
) -> Result<Option<PlaylistDetail>, DbError> {
    let statement = PlaylistPageParams { playlist: id.clone() };
    bae_store::statement_optional(sql, &statement)?
        .map(|row| {
            let summary = row.playlist.into_summary()?;
            let empty = summary.track_count.count == 0;
            Ok(PlaylistDetail { summary, entries: row.paged.into_rows()?, empty })
        })
        .transpose()
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

//! The playlists the screens read live: the list of every playlist this
//! device holds, the Add to playlist menu's targets, and one playlist's page
//! with its entries (§5.3).

use super::*;
pub use bae_people::collections::{PlaylistCover, PlaylistEntryRow, PlaylistSummary};

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

// The playlists this device holds, by name in the device's language, then id,
// twice: every one, then the person's own, where Add to playlist can put a
// track (`add_target`). Each row is beside how many its list holds. The read
// reads `name_sort`, so coven reruns it when the device's language changes.
sqlive_compose::read!(
    writer PlaylistLists { add_target: bool, total: u64 },
    r#"
    SELECT playlist.*, lists.value IS 1 AS add_target,
           COUNT(*) OVER (PARTITION BY lists.value) AS total
    FROM @bae_people::people::playlist_fragments::ListedPlaylist AS playlist
    JOIN json_each('[0, 1]') AS lists ON lists.value = 0 OR playlist.yours
    JOIN @bae_records::records::name_sort::NameSortLanguage AS sort ON 1
    ORDER BY lists.value, bae_name_key(playlist.playlist_name, sort.language),
             playlist.playlist_id
"#
);

// Whether the person can make a playlist.
sqlive_compose::read!(
    writer CanCreatePlaylist,
    r#"
    SELECT viewer.*
    FROM @bae_people::people::playlist_fragments::OwnProfileExists AS viewer
"#
);

// The playlist `playlist` with its entries; no row when the library holds no
// such playlist, or `playlist` names none.
sqlive_compose::read!(
    writer PlaylistPage(playlist: Option<String>),
    r#"
    SELECT playlist.*, paged.*
    FROM @bae_people::people::playlist_fragments::ListedPlaylist AS playlist
    JOIN @bae_people::people::playlist_fragments::PagedEntries(playlist: :playlist) AS paged
      ON 1
    WHERE playlist.playlist_id = :playlist
"#
);

/// The playlists this device holds as [`PlaylistLists`] and
/// [`CanCreatePlaylist`] return them.
fn playlist_listing_of(
    (lists, viewer): (Vec<PlaylistListsRow>, Vec<CanCreatePlaylistRow>),
) -> Result<PlaylistListing, DbError> {
    let can_create = exactly_one::<CanCreatePlaylistParams, _>(viewer)?.viewer.has_profile();
    let (mut listed, mut add_targets) = (Vec::new(), Vec::new());
    let (mut total, mut add_target_count) = (0, 0);
    for row in lists {
        let summary = row.playlist.into_summary()?;
        match row.add_target {
            false => {
                total = row.total;
                listed.push(summary);
            }
            true => {
                add_target_count = row.total;
                add_targets.push(summary);
            }
        }
    }
    Ok(PlaylistListing { listed, add_targets, total, add_target_count, can_create })
}

/// A playlist's page as [`PlaylistPage`] returns it.
fn playlist_detail_of(row: Option<PlaylistPageRow>) -> Result<Option<PlaylistDetail>, DbError> {
    row.map(|row| {
        let summary = row.playlist.into_summary()?;
        let empty = summary.track_count.count == 0;
        Ok(PlaylistDetail { summary, entries: row.paged.into_rows()?, empty })
    })
    .transpose()
}

impl LibraryViews {
    /// Every playlist this device holds, live.
    pub fn subscribe_playlists(&self) -> coven::LiveQuery<PlaylistListing, bae_store::DbError> {
        let reads = coven::Both(PlaylistListsParams {}, CanCreatePlaylistParams {});
        self.store.subscribe(reads).process(playlist_listing_of)
    }

    /// Follow the playlist `initial` names, or none; the page moves to
    /// another playlist through the request handle. `None` once the
    /// playlist is gone.
    pub fn subscribe_playlist_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, Option<PlaylistDetail>, bae_store::DbError>
    {
        self.store
            .subscribe_reconfigurable(initial, |id| Ok(PlaylistPageParams { playlist: id.clone() }))
            .process(|_, rows| playlist_detail_of(at_most_one::<PlaylistPageParams, _>(rows)?))
    }
}

#[cfg(test)]
#[path = "playlists_tests.rs"]
mod tests;

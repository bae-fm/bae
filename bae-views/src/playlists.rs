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

// Every playlist this device holds, by name in the device's language, then id,
// each row beside whether the person can make one; one row with no playlist
// when there is none. The read reads `name_sort`, so coven reruns it when the
// device's language changes.
sqlive_compose::read!(
    PlaylistList,
    r#"
    SELECT viewer.*, playlist.*
    FROM @bae_people::people::playlist_fragments::OwnProfileExists AS viewer
    LEFT JOIN @bae_people::people::playlist_fragments::ListedPlaylist AS playlist ON 1
    JOIN @bae_records::records::name_sort::NameSortLanguage AS sort ON 1
    ORDER BY bae_name_key(playlist.playlist_name, sort.language), playlist.playlist_id
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

fn playlist_listing_on(sql: &SqlReadContext<'_>) -> Result<PlaylistListing, DbError> {
    let mut can_create = false;
    let mut listed = Vec::new();
    for row in bae_store::statement_rows(sql, &PlaylistListParams {})? {
        can_create = row.viewer.has_profile();
        if let Some(playlist) = row.playlist {
            listed.push(playlist.into_summary()?);
        }
    }
    let add_targets: Vec<PlaylistSummary> =
        listed.iter().filter(|playlist| playlist.yours).cloned().collect();
    Ok(PlaylistListing {
        total: listed.len() as u64,
        add_target_count: add_targets.len() as u64,
        listed,
        add_targets,
        can_create,
    })
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

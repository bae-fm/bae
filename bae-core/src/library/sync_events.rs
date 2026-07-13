//! Map sync-applied row changes to granular library events.
//!
//! coven surfaces each applied changeset as a list of [`RowChange`]s. This
//! collapses them into deduplicated album/release events: album-level events
//! trump release-level for the same album, and track/junction changes escalate
//! to an album-level update (the album payload carries its releases, tracks, and
//! artists).
use std::collections::{HashMap, HashSet};

use coven::{ChangeOp, RowChange};

/// An album-level event to emit after processing a changeset.
#[derive(Debug, Clone)]
pub enum AlbumChangeEvent {
    Added(String),
    Updated(String),
    Removed {
        album_id: String,
        /// Ids of the album's child releases, gathered from the changeset's
        /// (otherwise suppressed) per-release deletes so the removal event is
        /// self-contained — consumers drop the album and exactly these releases
        /// without re-deriving the child set from their own state.
        release_ids: Vec<String>,
    },
}

/// A release-level event to emit after processing a changeset.
#[derive(Debug, Clone)]
pub enum ReleaseChangeEvent {
    Added {
        album_id: String,
        release_id: String,
    },
    Updated {
        album_id: String,
        release_id: String,
    },
    Removed {
        album_id: String,
        release_id: String,
    },
}

/// Collected entity changes from one or more changesets.
#[derive(Debug, Clone, Default)]
pub struct ChangesetEntityChanges {
    pub album_events: Vec<AlbumChangeEvent>,
    pub release_events: Vec<ReleaseChangeEvent>,
    /// Track IDs whose release wasn't in the changeset. The caller resolves these to
    /// album ids with a DB query and adds album-level update events for them.
    pub unresolved_track_ids: Vec<String>,
    /// Release IDs whose release row wasn't in the changeset — a cover changed on
    /// its own. The caller resolves these to album ids with a DB query, the same way
    /// as [`Self::unresolved_track_ids`].
    pub unresolved_release_ids: Vec<String>,
}

/// Raw per-table change info collected from a changeset's row changes.
#[derive(Debug, Default)]
struct RawChanges {
    /// album_id → operation
    albums: HashMap<String, ChangeOp>,
    /// release_id → (album_id, operation)
    releases: HashMap<String, (String, ChangeOp)>,
    /// track_id → (release_id, operation) — used to resolve album_id
    tracks: HashMap<String, (String, ChangeOp)>,
    /// album_id set from album_artists junction changes
    artist_junction_albums: HashSet<String>,
    /// track_id set from track_artists junction changes (need DB to resolve to album_id)
    track_artist_track_ids: HashSet<String>,
    /// release_id set from `covers` changes — the cover's PK IS its release id
    /// (may need the DB to resolve to album_id)
    cover_release_ids: HashSet<String>,
}

/// Collect affected entity IDs per table from coven's row changes.
///
/// `RowChange` already chooses the right value per op (new for inserts, old for
/// deletes, old-else-new for updates so PKs/FKs are always present), matching
/// what these lookups need.
///
/// Column layouts (matching the DB schema):
/// - `albums`: col 0 = id (PK)
/// - `releases`: col 0 = id (PK), col 1 = album_id (FK)
/// - `tracks`: col 0 = id (PK), col 1 = release_id (FK)
/// - `album_artists`: col 1 = album_id
/// - `track_artists`: col 1 = track_id
/// - `covers`: col 0 = id (PK), which IS the release id
fn collect_raw_changes(changes: &[RowChange]) -> (RawChanges, u32) {
    let mut raw = RawChanges::default();
    // A changeset row that's missing its FK is a malformed changeset — dropped
    // here and counted so the caller (which holds the diagnostics sink) can ship
    // the anomaly.
    let mut missing_fk = 0u32;

    for change in changes {
        let op = change.op;
        match change.table.as_str() {
            "albums" => {
                if let Some(id) = change.pk() {
                    raw.albums.insert(id.to_string(), op);
                }
            }
            "releases" => {
                if let Some(id) = change.pk() {
                    if let Some(album_id) = change.col(1) {
                        raw.releases
                            .insert(id.to_string(), (album_id.to_string(), op));
                    } else {
                        tracing::warn!("Changeset release {id} missing album_id FK, skipping");
                        missing_fk += 1;
                    }
                }
            }
            "tracks" => {
                if let Some(id) = change.pk() {
                    if let Some(release_id) = change.col(1) {
                        raw.tracks
                            .insert(id.to_string(), (release_id.to_string(), op));
                    } else {
                        tracing::warn!("Changeset track {id} missing release_id FK, skipping");
                        missing_fk += 1;
                    }
                }
            }
            "album_artists" => {
                if let Some(album_id) = change.col(1) {
                    raw.artist_junction_albums.insert(album_id.to_string());
                }
            }
            "track_artists" => {
                if let Some(track_id) = change.col(1) {
                    raw.track_artist_track_ids.insert(track_id.to_string());
                }
            }
            // A cover is keyed by its release id. `change_cover` writes the `covers`
            // row alone, so this is the one row a peer's changeset carries for it —
            // and the album payload is what ships the release's cover ref (and its
            // version, the UI's art cache key), so it escalates to an album update.
            "covers" => {
                if let Some(release_id) = change.pk() {
                    raw.cover_release_ids.insert(release_id.to_string());
                }
            }
            // `artists` and `artist_images` need no event of their own: neither is
            // ever written on its own. An artist image is only ever inserted by an
            // import, inside the same transaction as its artist's album and release,
            // whose album event already refreshes every surface that reads an artist
            // image ref (the artist and composer lists). A standalone
            // change-artist-image action would have to appear here.
            _ => {}
        }
    }

    (raw, missing_fk)
}

/// Resolve raw per-table changes into deduplicated per-album events:
///
/// - An album-level event trumps any release-level event for the same album.
/// - A removed album carries the ids of its suppressed child releases, so no
///   consumer has to re-derive the child set.
/// - A track change collapses into an album-level event, since the release payload
///   carries its tracks.
/// - An `album_artists` / `track_artists` change produces an album-level update,
///   since the album payload carries its artists.
/// - A `covers` change collapses into an album-level update, since the album payload
///   carries each release's cover ref — including its version, which is the UI's art
///   cache key.
fn resolve_changes(raw: RawChanges) -> ChangesetEntityChanges {
    let mut result = ChangesetEntityChanges::default();

    // Album-level changes trump release-level ones for the same album.
    let mut album_level_ids: HashSet<String> = HashSet::new();

    for (album_id, op) in &raw.albums {
        album_level_ids.insert(album_id.clone());
        match op {
            ChangeOp::Insert => result
                .album_events
                .push(AlbumChangeEvent::Added(album_id.clone())),
            ChangeOp::Update => result
                .album_events
                .push(AlbumChangeEvent::Updated(album_id.clone())),
            ChangeOp::Delete => {
                // A deleted album cascades to its releases. Those release deletes
                // are in `raw.releases` but get suppressed below, so carry their ids
                // here to keep the removal event self-contained.
                let release_ids = raw
                    .releases
                    .iter()
                    .filter(|(_, (aid, _))| aid == album_id)
                    .map(|(rid, _)| rid.clone())
                    .collect();
                result.album_events.push(AlbumChangeEvent::Removed {
                    album_id: album_id.clone(),
                    release_ids,
                });
            }
        }
    }

    for (release_id, (album_id, op)) in &raw.releases {
        if album_level_ids.contains(album_id) {
            continue;
        }
        match op {
            ChangeOp::Insert => result.release_events.push(ReleaseChangeEvent::Added {
                album_id: album_id.clone(),
                release_id: release_id.clone(),
            }),
            ChangeOp::Update => result.release_events.push(ReleaseChangeEvent::Updated {
                album_id: album_id.clone(),
                release_id: release_id.clone(),
            }),
            ChangeOp::Delete => result.release_events.push(ReleaseChangeEvent::Removed {
                album_id: album_id.clone(),
                release_id: release_id.clone(),
            }),
        }
    }

    // A track change means its release's payload changed, so it escalates to an
    // album-level update.
    for (track_id, (release_id, _op)) in &raw.tracks {
        if let Some((album_id, _)) = raw.releases.get(release_id) {
            if !album_level_ids.contains(album_id) {
                album_level_ids.insert(album_id.clone());
                result
                    .album_events
                    .push(AlbumChangeEvent::Updated(album_id.clone()));
            }
        } else {
            // The release isn't in the changeset; the caller resolves it from the DB.
            result.unresolved_track_ids.push(track_id.clone());
        }
    }

    for album_id in &raw.artist_junction_albums {
        if !album_level_ids.contains(album_id) {
            album_level_ids.insert(album_id.clone());
            result
                .album_events
                .push(AlbumChangeEvent::Updated(album_id.clone()));
        }
    }

    // A cover change means its release's payload changed, so it escalates to an
    // album-level update — the same event a local `change_cover` emits.
    for release_id in &raw.cover_release_ids {
        if let Some((album_id, _)) = raw.releases.get(release_id) {
            if !album_level_ids.contains(album_id) {
                album_level_ids.insert(album_id.clone());
                result
                    .album_events
                    .push(AlbumChangeEvent::Updated(album_id.clone()));
            }
        } else {
            // The release isn't in the changeset; the caller resolves it from the DB.
            result.unresolved_release_ids.push(release_id.clone());
        }
    }

    // Resolve each track_artists row's track to an album through the changeset's own
    // tracks/releases, or leave it for the caller's DB lookup.
    for track_id in &raw.track_artist_track_ids {
        if let Some((release_id, _)) = raw.tracks.get(track_id) {
            if let Some((album_id, _)) = raw.releases.get(release_id) {
                if !album_level_ids.contains(album_id) {
                    album_level_ids.insert(album_id.clone());
                    result
                        .album_events
                        .push(AlbumChangeEvent::Updated(album_id.clone()));
                }
            } else {
                result.unresolved_track_ids.push(track_id.clone());
            }
        } else {
            result.unresolved_track_ids.push(track_id.clone());
        }
    }

    result
}

/// Resolve coven row changes into the deduplicated entity changes to emit, plus
/// the count of changeset rows dropped for a missing FK (a malformed changeset
/// the caller ships as an anomaly).
pub fn changes_from_row_changes(changes: &[RowChange]) -> (ChangesetEntityChanges, u32) {
    let (raw, missing_fk) = collect_raw_changes(changes);
    (resolve_changes(raw), missing_fk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_changeset_walker_dedupes_per_album() {
        // Column layouts match the synced schema: albums col0=id; releases col0=id,
        // col1=album_id; tracks col0=id, col1=release_id.
        fn insert(table: &str, cols: &[&str]) -> RowChange {
            RowChange {
                table: table.to_string(),
                op: ChangeOp::Insert,
                columns: cols.iter().map(|c| Some(c.to_string())).collect(),
            }
        }

        // An album plus its release and two tracks, inserted together. The album
        // event subsumes the release and track changes: one album event, no release
        // events.
        let (changes, _missing_fk) = changes_from_row_changes(&[
            insert("albums", &["alb-1"]),
            insert("releases", &["rel-1", "alb-1"]),
            insert("tracks", &["trk-1", "rel-1"]),
            insert("tracks", &["trk-2", "rel-1"]),
        ]);

        assert_eq!(
            changes.album_events.len(),
            1,
            "expected one deduped album event, got {}",
            changes.album_events.len()
        );
        assert_eq!(changes.release_events.len(), 0);
    }

    /// `change_cover` writes the `covers` row alone, so that is the whole changeset a
    /// peer sends. It must still reach the album — the album payload is what carries
    /// the release's cover ref, and its version is the UI's art cache key.
    #[test]
    fn test_sync_changeset_lone_cover_change_resolves_through_the_db() {
        fn update(table: &str, cols: &[&str]) -> RowChange {
            RowChange {
                table: table.to_string(),
                op: ChangeOp::Update,
                columns: cols.iter().map(|c| Some(c.to_string())).collect(),
            }
        }

        // The cover's PK is its release id, and the release row isn't in the
        // changeset — the caller resolves it to an album with a DB query.
        let (changes, _missing_fk) = changes_from_row_changes(&[update("covers", &["rel-1"])]);
        assert!(changes.album_events.is_empty());
        assert!(changes.release_events.is_empty());
        assert_eq!(changes.unresolved_release_ids, vec!["rel-1".to_string()]);
    }

    /// When the changeset carries the release too, the cover resolves to its album
    /// from the changeset alone — no DB lookup — and escalates to exactly one album
    /// update, however many covers it carries. (The release's own event still rides
    /// along, the same way a track change escalates to the album while its release
    /// keeps its event; consumers re-read by primary key either way.)
    #[test]
    fn test_sync_changeset_cover_with_its_release_resolves_without_the_db() {
        fn update(table: &str, cols: &[&str]) -> RowChange {
            RowChange {
                table: table.to_string(),
                op: ChangeOp::Update,
                columns: cols.iter().map(|c| Some(c.to_string())).collect(),
            }
        }

        let (changes, _missing_fk) = changes_from_row_changes(&[
            update("releases", &["rel-1", "alb-1"]),
            update("releases", &["rel-2", "alb-1"]),
            update("covers", &["rel-1"]),
            update("covers", &["rel-2"]),
        ]);

        assert!(changes.unresolved_release_ids.is_empty());
        assert_eq!(
            changes.album_events.len(),
            1,
            "both covers collapse into one update for their shared album, got {:?}",
            changes.album_events
        );
        assert!(matches!(
            &changes.album_events[0],
            AlbumChangeEvent::Updated(album_id) if album_id == "alb-1"
        ));
    }

    #[test]
    fn test_sync_changeset_album_delete_carries_child_release_ids() {
        fn delete(table: &str, cols: &[&str]) -> RowChange {
            RowChange {
                table: table.to_string(),
                op: ChangeOp::Delete,
                columns: cols.iter().map(|c| Some(c.to_string())).collect(),
            }
        }

        // Deleting an album cascades to its releases. The per-release deletes are
        // suppressed (album-level trumps release-level), but their ids must ride
        // along on the album removal so consumers can drop the children without
        // re-deriving the set from their own state.
        let (changes, _missing_fk) = changes_from_row_changes(&[
            delete("albums", &["alb-1"]),
            delete("releases", &["rel-1", "alb-1"]),
            delete("releases", &["rel-2", "alb-1"]),
        ]);

        assert_eq!(changes.album_events.len(), 1);
        assert_eq!(changes.release_events.len(), 0);
        match &changes.album_events[0] {
            AlbumChangeEvent::Removed {
                album_id,
                release_ids,
            } => {
                assert_eq!(album_id, "alb-1");
                let mut got = release_ids.clone();
                got.sort();
                assert_eq!(got, vec!["rel-1".to_string(), "rel-2".to_string()]);
            }
            other => panic!("expected AlbumChangeEvent::Removed, got {other:?}"),
        }
    }
}

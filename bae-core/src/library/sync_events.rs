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
    /// Track IDs whose release wasn't in the changeset — caller must resolve
    /// these to album_ids via DB query and add album-level update events.
    pub unresolved_track_ids: Vec<String>,
}

impl ChangesetEntityChanges {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Merge another set of changes into this one.
    pub fn merge(&mut self, other: ChangesetEntityChanges) {
        self.album_events.extend(other.album_events);
        self.release_events.extend(other.release_events);
        self.unresolved_track_ids.extend(other.unresolved_track_ids);
    }

    pub fn is_empty(&self) -> bool {
        self.album_events.is_empty()
            && self.release_events.is_empty()
            && self.unresolved_track_ids.is_empty()
    }
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
fn collect_raw_changes(changes: &[RowChange]) -> RawChanges {
    let mut raw = RawChanges::default();

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
                    }
                }
            }
            "album_artists" => {
                // col 1 = album_id
                if let Some(album_id) = change.col(1) {
                    raw.artist_junction_albums.insert(album_id.to_string());
                }
            }
            "track_artists" => {
                // col 1 = track_id
                if let Some(track_id) = change.col(1) {
                    raw.track_artist_track_ids.insert(track_id.to_string());
                }
            }
            _ => {} // Ignore other tables (artists, covers, artist_images, etc.)
        }
    }

    raw
}

/// Resolve raw per-table changes into deduplicated per-album events.
///
/// Rules:
/// - Album-level events trump release-level events for the same album_id.
/// - A removed album carries the ids of its suppressed child releases, so the
///   removal event is self-contained (no consumer re-derives the child set).
/// - Track changes get collapsed into album-level events (the release payload
///   carries its tracks).
/// - Junction table changes (album_artists, track_artists) produce album-level
///   update events (the album payload carries its artists).
fn resolve_changes(raw: RawChanges) -> ChangesetEntityChanges {
    let mut result = ChangesetEntityChanges::default();

    // Collect album_ids that have album-level changes (these trump release-level)
    let mut album_level_ids: HashSet<String> = HashSet::new();

    // 1. Album table changes → album-level events
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
                // A deleted album cascades to its releases; those release
                // deletes are in `raw.releases` but get suppressed below
                // (album-level trumps release-level). Carry their ids so the
                // removal is self-contained.
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

    // 2. Release table changes → release-level OR escalate to album-level
    for (release_id, (album_id, op)) in &raw.releases {
        if album_level_ids.contains(album_id) {
            // Already covered by album-level event
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

    // 3. Track table changes → escalate to album-level update
    //    (tracks are nested in releases; track changes mean the release payload changed)
    for (track_id, (release_id, _op)) in &raw.tracks {
        // Resolve release_id → album_id through the releases map
        if let Some((album_id, _)) = raw.releases.get(release_id) {
            if !album_level_ids.contains(album_id) {
                album_level_ids.insert(album_id.clone());
                result
                    .album_events
                    .push(AlbumChangeEvent::Updated(album_id.clone()));
            }
        } else {
            // Release not in changeset — caller must resolve via DB query.
            result.unresolved_track_ids.push(track_id.clone());
        }
    }

    // 4. Junction table changes → album-level update events
    for album_id in &raw.artist_junction_albums {
        if !album_level_ids.contains(album_id) {
            album_level_ids.insert(album_id.clone());
            result
                .album_events
                .push(AlbumChangeEvent::Updated(album_id.clone()));
        }
    }

    // 4b. track_artists junction → resolve track_id to album_id via tracks/releases
    //     in the changeset, or mark as unresolved for DB query.
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
            // Track not in changeset at all — needs DB resolution.
            result.unresolved_track_ids.push(track_id.clone());
        }
    }

    result
}

/// Resolve coven row changes into deduplicated entity changes to emit.
pub fn changes_from_row_changes(changes: &[RowChange]) -> ChangesetEntityChanges {
    resolve_changes(collect_raw_changes(changes))
}

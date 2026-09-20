//! Shared release intermediate representation and the single assembler.
//!
//! All three import mappers (file tags, MusicBrainz, Discogs) parse their source
//! into a [`ReleaseIr`] — an ordered, pre-id, pre-dedup description of what the
//! source said — and hand it to [`assemble_parsed_album`], which owns artist-pool
//! dedup, per-side numbering, junction-row emission, and DB-row construction.
//! The IR is the boundary between "what the source claims" and "which rows we
//! mint": every [`ArtistRef`] is an unresolved reference the assembler resolves
//! against one artist pool.
//!
//! Per-track event order is observable output — [`ParsedAlbum::artists`]
//! insertion order, junction-row order, and id-mint order all follow it — so the
//! mappers preserve each source's exact discovery order when building the IR.

use crate::db::{
    DbAlbum, DbAlbumArtist, DbArtist, DbRelease, DbReleaseArtistRole, DbTrack, DbTrackArtist,
    DbTrackArtistRole, DbTrackWork, DbWork, DbWorkArtist, DbWorkPart, Pressing,
};
use crate::import::types::{Catalog, MetadataProvenance};
use crate::import::{ParsedAlbum, ParsedWorkGraph};
use chrono::{DateTime, Utc};
use coven::{Clock, IdProvider};
use std::collections::{HashMap, HashSet};
use tracing::debug;

/// A reference to an artist as a source credits it, before any DB id exists.
pub(crate) struct ArtistRef {
    pub name: String,
    pub sort_name: Option<String>,
    pub musicbrainz_artist_id: Option<String>,
    pub discogs_artist_id: Option<String>,
}

/// Read a positive number from a provider position: 7, A7, or 2-7.
pub(crate) fn position_number(position: &str) -> Option<i32> {
    let number = match position.split_once('-') {
        Some((disc, number)) if disc.bytes().all(|byte| byte.is_ascii_digit()) => number,
        _ if position
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic) =>
        {
            &position[1..]
        }
        _ => position,
    };
    number.parse::<i32>().ok().filter(|number| *number > 0)
}

pub(crate) enum PartDirection {
    Forward,
    Backward,
}

/// A work and its sub-graph, validated: every event carries its payload. The
/// source→IR mapper drops malformed relations (and logs them), so the assembler
/// never has to.
pub(crate) struct WorkNode {
    /// The MusicBrainz work this node describes. Identifies the work within the
    /// graph and across imports; the assembler mints the `works` row id
    /// separately, because an MBID is often a version-3 UUID the sync layer
    /// refuses on a synced row.
    pub musicbrainz_work_id: String,
    pub title: String,
    pub disambiguation: Option<String>,
    pub work_type: Option<String>,
    /// Composer credits and part relations, in source relation order.
    pub events: Vec<WorkEvent>,
}

pub(crate) enum WorkEvent {
    Composer(ArtistRef),
    /// "parts" relation: `Forward` names a child of this work, `Backward` names
    /// its parent.
    Part {
        direction: PartDirection,
        work: WorkGraphRef,
    },
}

/// A reference to a work, from a track performance or a parent work. Each work
/// is reachable from many places; the first reference in a release carries the
/// expanded sub-graph (`Expanded`), and every later reference to the same work
/// carries only its id (`AlreadyExpanded`). The distinction is explicit so a
/// relation-less work (an `Expanded` node with no events) is never confused with
/// a repeat reference.
pub(crate) enum WorkGraphRef {
    Expanded(WorkNode),
    AlreadyExpanded { musicbrainz_work_id: String },
}

/// One track-scoped event, in source order. Order determines artist-pool
/// insertion order, junction-row order, and id-mint order.
pub(crate) enum TrackEvent {
    /// Display credit → `track_artists` row at `position`.
    Credit { position: i32, artist: ArtistRef },
    /// Role credit → `track_artist_roles` row.
    Role {
        position: i32,
        artist: ArtistRef,
        source: Catalog,
        source_credit: Option<String>,
    },
    /// Work performance → work graph rows + a `track_works` row (deduped per
    /// track on work id).
    Work {
        position: i32,
        source: Catalog,
        work: WorkGraphRef,
    },
}

pub(crate) struct TrackIr {
    pub title: String,
    pub side: Option<i32>,
    pub number: Option<i32>,
    /// Raw source position ("A1", "1-2", MusicBrainz track number); lands in
    /// `DbTrack.discogs_position`.
    pub source_position: Option<String>,
    pub events: Vec<TrackEvent>,
}

/// Release-level role credit (Discogs extraartists composers).
pub(crate) struct ReleaseRole {
    pub position: i32,
    pub artist: ArtistRef,
    pub source: Catalog,
    pub source_credit: Option<String>,
}

/// Which artists get `album_artists` junction rows (beyond the primary).
pub(crate) enum AlbumArtistScope {
    /// Only the release's own positional credits (MusicBrainz / Discogs).
    ReleaseCredits,
    /// Every artist in the final pool — a divergent per-track artist also
    /// becomes an album artist (file-tag / CUE seeds).
    FullPool,
}

pub(crate) struct ReleaseIr {
    pub album_title: String,
    /// The album's primary artist (`ParsedAlbum.artists[0]`, `DbAlbum.artist_id`).
    pub primary_artist: ArtistRef,
    /// Remaining release-level credits in order; pushed to the pool without
    /// dedup, positions 1.. in `album_artists`.
    pub additional_artists: Vec<ArtistRef>,
    pub album_year: Option<i32>,
    pub is_compilation: bool,
    pub pressing: Pressing,
    pub metadata_provenance: Option<MetadataProvenance>,
    pub album_artist_scope: AlbumArtistScope,
    pub release_roles: Vec<ReleaseRole>,
    pub tracks: Vec<TrackIr>,
}

/// Mint a `DbArtist` from an [`ArtistRef`] and append it to the pool, returning
/// its new id. No dedup — the caller decides whether to look first.
fn push_artist(
    artists: &mut Vec<DbArtist>,
    artist_ref: &ArtistRef,
    ids: &dyn IdProvider,
    now: DateTime<Utc>,
) -> String {
    let artist = DbArtist {
        id: ids.new_id(),
        name: artist_ref.name.clone(),
        sort_name: artist_ref.sort_name.clone(),
        discogs_artist_id: artist_ref.discogs_artist_id.clone(),
        musicbrainz_artist_id: artist_ref.musicbrainz_artist_id.clone(),
        created_at: now,
    };
    let id = artist.id.clone();
    artists.push(artist);
    id
}

/// Resolve an [`ArtistRef`] against the pool, minting a new artist on a miss.
///
/// Match rule, in order:
///   - ref has a musicbrainz id → an existing artist with the same musicbrainz
///     id;
///   - else ref has a discogs id → an existing artist with the same discogs id;
///   - else (no source ids) → a case-insensitive name match. For
///     MusicBrainz provenance the match is restricted to existing
///     artists that also lack a musicbrainz id (an id-less credit never merges
///     into an id-bearing artist); Discogs / FileTags match any artist by name.
fn find_or_push_artist(
    artists: &mut Vec<DbArtist>,
    artist_ref: &ArtistRef,
    source: Option<Catalog>,
    ids: &dyn IdProvider,
    now: DateTime<Utc>,
) -> String {
    let existing = artists.iter().find(|artist| {
        if let Some(mb_id) = artist_ref.musicbrainz_artist_id.as_ref() {
            artist.musicbrainz_artist_id.as_ref() == Some(mb_id)
        } else if let Some(discogs_id) = artist_ref.discogs_artist_id.as_ref() {
            artist.discogs_artist_id.as_ref() == Some(discogs_id)
        } else {
            let name_matches = artist.name.eq_ignore_ascii_case(&artist_ref.name);
            match source {
                // An id-less MusicBrainz credit only merges into an artist that
                // also lacks a musicbrainz id.
                Some(Catalog::MusicBrainz) => {
                    name_matches && artist.musicbrainz_artist_id.is_none()
                }
                Some(Catalog::Discogs) | None => name_matches,
                Some(other) => {
                    unreachable!("no {} credit reaches a mapper", other.as_str())
                }
            }
        }
    });

    if let Some(existing) = existing {
        return existing.id.clone();
    }
    push_artist(artists, artist_ref, ids, now)
}

pub(crate) fn album_artist_links(
    album_id: &str,
    artists: &[DbArtist],
    ids: &dyn IdProvider,
    now: DateTime<Utc>,
) -> Vec<DbAlbumArtist> {
    artists
        .iter()
        .enumerate()
        .skip(1)
        .map(|(position, artist)| {
            DbAlbumArtist::new(album_id, &artist.id, position as i32, ids.new_id(), now)
        })
        .collect()
}

/// Pools the assembler threads through the work-graph walk.
struct WorkPools<'a> {
    artists: &'a mut Vec<DbArtist>,
    works: &'a mut Vec<DbWork>,
    work_artists: &'a mut Vec<DbWorkArtist>,
    work_parts: &'a mut Vec<DbWorkPart>,
    /// MusicBrainz work id → the `works` row id minted for it. Also the
    /// "already emitted its row" set: a work is in here exactly once its row is
    /// in `works`.
    work_row_ids: &'a mut HashMap<String, String>,
    expanded_works: &'a mut HashSet<String>,
}

/// Link `child_work_id` under `parent_work_id`, unless the pair is already
/// linked. Both are minted row ids.
fn push_work_part(
    pools: &mut WorkPools,
    parent_work_id: &str,
    child_work_id: &str,
    source: Catalog,
    ids: &dyn IdProvider,
    now: DateTime<Utc>,
) {
    if pools
        .work_parts
        .iter()
        .any(|part| part.parent_work_id == parent_work_id && part.child_work_id == child_work_id)
    {
        return;
    }
    pools.work_parts.push(DbWorkPart::new(
        parent_work_id,
        child_work_id,
        pools.work_parts.len() as i32,
        source,
        ids.new_id(),
        now,
    ));
}

/// Fold a validated [`WorkNode`] into the work graph: mint the work row once,
/// link it to `parent` if given, then expand its events once. Returns the work's
/// minted row id. `work_row_ids` / `expanded_works` are release-global;
/// `work_artists` / `work_parts` positions are the running pool lengths.
fn push_work_graph(
    node: &WorkNode,
    parent: Option<&str>,
    source: Catalog,
    pools: &mut WorkPools,
    ids: &dyn IdProvider,
    now: DateTime<Utc>,
) -> String {
    let work_id = match pools.work_row_ids.get(&node.musicbrainz_work_id) {
        Some(work_id) => work_id.clone(),
        None => {
            let work_id = ids.new_id();
            pools
                .work_row_ids
                .insert(node.musicbrainz_work_id.clone(), work_id.clone());
            pools.works.push(DbWork {
                id: work_id.clone(),
                title: node.title.clone(),
                disambiguation: node.disambiguation.clone(),
                work_type: node.work_type.clone(),
                musicbrainz_work_id: node.musicbrainz_work_id.clone(),
                created_at: now,
            });
            work_id
        }
    };

    if let Some(parent_work_id) = parent {
        push_work_part(pools, parent_work_id, &work_id, source, ids, now);
    }

    if node.events.is_empty()
        || !pools
            .expanded_works
            .insert(node.musicbrainz_work_id.clone())
    {
        return work_id;
    }

    for event in &node.events {
        match event {
            WorkEvent::Composer(artist_ref) => {
                let artist_id = find_or_push_artist(
                    pools.artists,
                    artist_ref,
                    Some(Catalog::MusicBrainz),
                    ids,
                    now,
                );
                if !pools.work_artists.iter().any(|link| {
                    link.work_id == work_id && link.artist_id == artist_id && link.source == source
                }) {
                    pools.work_artists.push(DbWorkArtist::new(
                        &work_id,
                        &artist_id,
                        pools.work_artists.len() as i32,
                        source,
                        ids.new_id(),
                        now,
                    ));
                }
            }
            WorkEvent::Part { direction, work } => match direction {
                PartDirection::Backward => {
                    // `work` is this node's parent.
                    let parent_id = push_work_ref(work, None, source, pools, ids, now);
                    push_work_part(pools, &parent_id, &work_id, source, ids, now);
                }
                PartDirection::Forward => {
                    // `work` is a child of this node; `push_work_ref` creates the
                    // parent link.
                    push_work_ref(work, Some(&work_id), source, pools, ids, now);
                }
            },
        }
    }

    work_id
}

/// Resolve a [`WorkGraphRef`] into the graph, returning the work's minted row id.
/// `Expanded` walks the sub-graph (minting the work row and, if `parent` is
/// given, the parent→child link). `AlreadyExpanded` names a work whose row and
/// sub-graph a prior reference already emitted; only its `parent` link, if any,
/// still needs creating.
fn push_work_ref(
    work: &WorkGraphRef,
    parent: Option<&str>,
    source: Catalog,
    pools: &mut WorkPools,
    ids: &dyn IdProvider,
    now: DateTime<Utc>,
) -> String {
    match work {
        WorkGraphRef::Expanded(node) => push_work_graph(node, parent, source, pools, ids, now),
        WorkGraphRef::AlreadyExpanded {
            musicbrainz_work_id,
        } => {
            // The source→IR mapper only emits `AlreadyExpanded` for a work an
            // enclosing `Expanded` node already carried, and a node's row is
            // minted before its events are walked — so the row id is always here.
            let work_id = pools
                .work_row_ids
                .get(musicbrainz_work_id)
                .unwrap_or_else(|| {
                    panic!(
                        "work {musicbrainz_work_id} referenced before the reference that expanded it"
                    )
                })
                .clone();
            if let Some(parent_work_id) = parent {
                push_work_part(pools, parent_work_id, &work_id, source, ids, now);
            }
            work_id
        }
    }
}

/// The single place a [`ParsedAlbum`] is built. Owns artist dedup, ordered default
/// numbering, junction emission, and row construction. Infallible — all source
/// validation happens in the source→IR mappers.
pub(crate) fn assemble_parsed_album(
    ir: ReleaseIr,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> ParsedAlbum {
    let now = clock.now();
    let artist_source = match &ir.metadata_provenance {
        Some(MetadataProvenance::ExternalRelease { record, .. }) => Some(record.catalog),
        Some(MetadataProvenance::FileTags) | None => None,
    };

    // Release-level artists: primary then additional, minted in order, no dedup.
    let mut artists: Vec<DbArtist> = Vec::new();
    push_artist(&mut artists, &ir.primary_artist, ids, now);
    for artist_ref in &ir.additional_artists {
        push_artist(&mut artists, artist_ref, ids, now);
    }
    let release_artist_count = artists.len();

    let album = DbAlbum {
        id: ids.new_id(),
        title: ir.album_title,
        artist_id: artists[0].id.clone(),
        year: ir.album_year,
        primary_release_id: None,
        is_compilation: ir.is_compilation,
        created_at: now,
    };

    let release = DbRelease {
        id: ids.new_id(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: ir.pressing,
        draft_from_tags: matches!(ir.metadata_provenance, Some(MetadataProvenance::FileTags)),
        // Which lookup found the record is the candidate's answer, not this
        // assembly's; `run_import` stamps it onto the row it writes.
        identified_by: None,
        // Imports land local; the upload observer flips `remote` true once the
        // release's audio is durably in the cloud.
        remote: false,
        source_folder_name: None,
        content_hash: None,
        // Album loudness is measured by `loudness::measure_loudness` and stamped
        // onto the release row by `run_import`, not here.
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };

    let mut release_artist_roles: Vec<DbReleaseArtistRole> = Vec::new();
    for role in &ir.release_roles {
        let artist_id = find_or_push_artist(&mut artists, &role.artist, artist_source, ids, now);
        release_artist_roles.push(DbReleaseArtistRole::new(
            &release.id,
            &artist_id,
            role.position,
            role.source,
            role.source_credit.clone(),
            ids.new_id(),
            now,
        ));
    }

    let mut tracks: Vec<DbTrack> = Vec::with_capacity(ir.tracks.len());
    let mut track_artists: Vec<DbTrackArtist> = Vec::new();
    let mut track_artist_roles: Vec<DbTrackArtistRole> = Vec::new();
    let mut works: Vec<DbWork> = Vec::new();
    let mut work_artists: Vec<DbWorkArtist> = Vec::new();
    let mut work_parts: Vec<DbWorkPart> = Vec::new();
    let mut track_works: Vec<DbTrackWork> = Vec::new();
    let mut work_row_ids: HashMap<String, String> = HashMap::new();
    let mut expanded_works: HashSet<String> = HashSet::new();

    for (index, track_ir) in ir.tracks.iter().enumerate() {
        let track_number = Some(
            track_ir
                .number
                .unwrap_or_else(|| i32::try_from(index + 1).expect("track position fits i32")),
        );

        let db_track = DbTrack {
            id: ids.new_id(),
            release_id: release.id.clone(),
            title: track_ir.title.clone(),
            side: track_ir.side,
            track_number,
            duration_ms: None,
            discogs_position: track_ir.source_position.clone(),
            created_at: now,
        };

        // A recording can carry more than one performance relation to the same
        // work; track_works is uniquely keyed on (track_id, work_id), so link
        // each work to this track at most once.
        let mut track_work_ids: HashSet<String> = HashSet::new();

        for event in &track_ir.events {
            match event {
                TrackEvent::Credit { position, artist } => {
                    let artist_id =
                        find_or_push_artist(&mut artists, artist, artist_source, ids, now);
                    track_artists.push(DbTrackArtist::new(
                        &db_track.id,
                        &artist_id,
                        *position,
                        ids.new_id(),
                        now,
                    ));
                }
                TrackEvent::Role {
                    position,
                    artist,
                    source,
                    source_credit,
                } => {
                    let artist_id =
                        find_or_push_artist(&mut artists, artist, artist_source, ids, now);
                    track_artist_roles.push(DbTrackArtistRole::new(
                        &db_track.id,
                        &artist_id,
                        *position,
                        *source,
                        source_credit.clone(),
                        ids.new_id(),
                        now,
                    ));
                }
                TrackEvent::Work {
                    position,
                    source,
                    work,
                } => {
                    let work_id = {
                        let mut pools = WorkPools {
                            artists: &mut artists,
                            works: &mut works,
                            work_artists: &mut work_artists,
                            work_parts: &mut work_parts,
                            work_row_ids: &mut work_row_ids,
                            expanded_works: &mut expanded_works,
                        };
                        push_work_ref(work, None, *source, &mut pools, ids, now)
                    };
                    if !track_work_ids.insert(work_id.clone()) {
                        debug!(
                            track_id = %db_track.id,
                            work_id = %work_id,
                            "Skipping duplicate MusicBrainz recording performance relation to the same work"
                        );
                        continue;
                    }
                    track_works.push(DbTrackWork::new(
                        &db_track.id,
                        &work_id,
                        *position,
                        *source,
                        ids.new_id(),
                        now,
                    ));
                }
            }
        }

        tracks.push(db_track);
    }

    let album_artist_slice = match ir.album_artist_scope {
        AlbumArtistScope::ReleaseCredits => &artists[..release_artist_count],
        AlbumArtistScope::FullPool => &artists[..],
    };
    let album_artists = album_artist_links(&album.id, album_artist_slice, ids, now);

    ParsedAlbum {
        album,
        release,
        tracks,
        artists,
        album_artists,
        track_artists,
        work_graph: ParsedWorkGraph {
            works,
            work_artists,
            work_parts,
            track_works,
        },
        release_artist_roles,
        track_artist_roles,
    }
}

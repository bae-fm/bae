//! MusicBrainz release → stored tracklist, and stored tracklist → `ParsedAlbum`
//! and picker rows. The records a pick commits are extracted with the release
//! by `ReleasePayloads::extract`, not built here.
//!
//! MB → Discogs cross-link: MB releases carry url-rels that routinely include a
//! Discogs release URL, which is what the Discogs document alongside an MB one
//! was fetched from. The reverse (Discogs → MB) is less reliable — the Discogs
//! API exposes no MBID field — so it is resolved through MB's URL endpoint
//! instead.

use super::assemble::{
    assemble_parsed_album, AlbumArtistScope, ArtistRef, PartDirection, ReleaseIr, TrackEvent,
    TrackIr, WorkEvent, WorkGraphRef, WorkNode,
};
use super::ParsedAlbum;
use crate::db::{is_various_artists, Pressing};
use crate::import::medium_coverage::MediumCoverage;
use crate::import::{Catalog, ImportError, MetadataRef};
use crate::import::search::ReleaseTrack;
use crate::import::source_release::{
    ArtistCredit, EntryKind, PerformedWork, RoleCredit, SourceMedium, SourceRelease, SourceWork,
    SourceWorkEvent, TracklistEntry,
};
use crate::musicbrainz::{label_and_catno, MbArtistRef, MbRelation, MbReleaseResponse, MbTrack, MbWork};
use coven::Clock;
use coven::IdProvider;
use std::collections::HashSet;
use tracing::{debug, warn};

fn mb_relation_is(relation: &MbRelation, target_type: &str, relation_type: &str) -> bool {
    relation.target_type.as_deref() == Some(target_type)
        && relation.relation_type.as_deref() == Some(relation_type)
}

fn mb_artist_name(artist: &MbArtistRef, credit: Option<&str>) -> Option<String> {
    credit
        .filter(|c| !c.trim().is_empty())
        .map(str::to_string)
        .or_else(|| artist.name.clone())
}

fn mb_relation_is_composer(relation: &MbRelation) -> bool {
    relation.relation_type.as_deref() == Some("composer")
}

/// An [`ArtistRef`] for a MusicBrainz artist: `name` is the resolved credit
/// name; sort name and MB id come from the artist payload. Linked album
/// metadata may later supply another catalog identity for matching credits.
fn mb_artist_ref(name: String, artist: &MbArtistRef) -> ArtistRef {
    ArtistRef {
        name,
        sort_name: artist.sort_name.clone(),
        musicbrainz_artist_id: artist.id.clone(),
        discogs_artist_id: None,
    }
}

/// Read an `MbWork` as one reference states it, validating its relations:
/// malformed ones are dropped here, at the source boundary, so a stored work
/// only ever holds events that carry their payloads.
///
/// `reported` is release-scoped: a work reached from several tracks logs its
/// dropped relations the first time it is read, not once per reference.
fn source_work(work: &MbWork, reported: &mut HashSet<String>) -> SourceWork {
    let report = reported.insert(work.id.clone());
    let mut events = Vec::new();
    for relation in &work.relations {
        if relation.target_type.as_deref() == Some("artist") {
            if mb_relation_is_composer(relation) {
                let Some(artist_ref) = relation.artist.as_ref() else {
                    if report {
                        warn!(
                        work_id = %work.id,
                        relation_type = ?relation.relation_type,
                        "Skipping MusicBrainz work artist relation without artist payload"
                        );
                    }
                    continue;
                };
                let Some(name) = mb_artist_name(artist_ref, relation.target_credit.as_deref())
                else {
                    if report {
                        warn!(
                            work_id = %work.id,
                            musicbrainz_artist_id = ?artist_ref.id,
                            "Skipping MusicBrainz work artist relation with unresolved artist"
                        );
                    }
                    continue;
                };
                events.push(SourceWorkEvent::Composer(mb_artist_ref(name, artist_ref)));
            } else if report {
                debug!(
                    work_id = %work.id,
                    relation_type = ?relation.relation_type,
                    target_type = ?relation.target_type,
                    target_credit = ?relation.target_credit,
                    "Skipping MusicBrainz work artist relation with non-composer relation type"
                );
            }
        } else if mb_relation_is(relation, "work", "parts") {
            let Some(child_or_parent) = relation.work.as_ref() else {
                if report {
                    warn!(
                        work_id = %work.id,
                        relation_type = ?relation.relation_type,
                        "Skipping MusicBrainz work parts relation without work payload"
                    );
                }
                continue;
            };
            let direction = match relation.direction.as_deref() {
                Some("backward") => PartDirection::Backward,
                _ => PartDirection::Forward,
            };
            events.push(SourceWorkEvent::Part {
                direction,
                work: source_work(child_or_parent, reported),
            });
        }
    }
    SourceWork {
        musicbrainz_work_id: work.id.clone(),
        title: work.title.clone(),
        disambiguation: work.disambiguation.clone(),
        work_type: work.work_type.clone(),
        events,
    }
}

/// A stored work as the assembler takes it.
///
/// `converted` is release-scoped: the first reference to a work id returns an
/// `Expanded` node carrying its sub-graph; every later reference returns
/// `AlreadyExpanded`, so the assembler emits each work's row and sub-graph
/// once per release.
fn work_ref(work: &SourceWork, converted: &mut HashSet<String>) -> WorkGraphRef {
    if !converted.insert(work.musicbrainz_work_id.clone()) {
        return WorkGraphRef::AlreadyExpanded {
            musicbrainz_work_id: work.musicbrainz_work_id.clone(),
        };
    }
    WorkGraphRef::Expanded(WorkNode {
        musicbrainz_work_id: work.musicbrainz_work_id.clone(),
        title: work.title.clone(),
        disambiguation: work.disambiguation.clone(),
        work_type: work.work_type.clone(),
        events: work
            .events
            .iter()
            .map(|event| match event {
                SourceWorkEvent::Composer(artist) => WorkEvent::Composer(artist.clone()),
                SourceWorkEvent::Part { direction, work } => WorkEvent::Part {
                    direction: *direction,
                    work: work_ref(work, converted),
                },
            })
            .collect(),
    })
}

/// The pressing a MusicBrainz release describes: its own release date's year, its
/// first medium's format, its first label's name and catalog number, its country
/// and barcode.
///
/// The one MB → pressing projection. The committed release, the picker's detail,
/// and a search result all read it, so a pressing shown is the pressing stored.
pub(crate) fn pressing(response: &MbReleaseResponse) -> Pressing {
    let (label, catalog_number) = label_and_catno(&response.label_info);
    Pressing {
        year: super::parse_year(response.date.as_deref()),
        format: response.media.first().and_then(|m| m.format.clone()),
        label,
        catalog_number,
        country: response.country.clone(),
        barcode: response.barcode.clone(),
    }
}

/// A track's title: the recording's, else the track's own override. What a
/// stored tracklist keeps for the row.
fn stated_title(track: &MbTrack) -> Option<String> {
    track
        .recording
        .as_ref()
        .and_then(|r| r.title.as_deref())
        .or(track.title.as_deref())
        .filter(|title| !title.trim().is_empty())
        .map(str::to_string)
}

/// A stored track's title. Shared by the DB mapper and the picker's detail so
/// the picker and the committed rows can't show different titles.
///
/// Errors when the track carries no non-blank title: there is no title to
/// show, and an empty string in its place is a lie the user can't see
/// through.
fn track_title(release_id: &str, track: &TracklistEntry) -> Result<String, ImportError> {
    track.title.clone().ok_or_else(|| ImportError::SourceData {
        catalog: Catalog::MusicBrainz,
        detail: format!(
            "MusicBrainz release {} track {:?} has no track title",
            release_id, track.position
        ),
    })
}

/// Vinyl/cassette side assignment for one medium, shared by the DB mapper and
/// the picker's detail so the two never diverge.
pub(crate) struct MediumSides {
    /// Side offset (0-based, relative to the medium's lowest side letter) for
    /// each track, in track order.
    pub offsets: Vec<Option<u32>>,
    /// Number of sides this medium occupies; advances the running side base
    /// between media.
    pub side_span: u32,
}

/// Assign each track of a medium to a vinyl/cassette side.
///
/// Multi-side media (format contains "Vinyl" or "Cassette") derive the side
/// from the leading letter of the track number ("A1" -> offset 0, "B2" ->
/// offset 1), relative to the medium's lowest side letter — so a second medium
/// lettered C/D yields offsets 0/1, not 2/3. Single-side media put every track
/// on offset 0.
///
/// Tracks without a side letter retain an unknown side. An empty medium has
/// no playable tracks and is rejected.
pub(crate) fn medium_sides(
    release_id: &str,
    medium: &SourceMedium,
) -> Result<MediumSides, ImportError> {
    if medium.entries.is_empty() {
        return Err(ImportError::SourceData {
            catalog: Catalog::MusicBrainz,
            detail: format!(
                "MusicBrainz release {} has a medium with no tracks",
                release_id
            ),
        });
    }

    let is_multi_side = medium
        .format
        .as_deref()
        .is_some_and(|f| f.contains("Vinyl") || f.contains("Cassette"));

    if !is_multi_side {
        return Ok(MediumSides {
            offsets: vec![Some(0); medium.entries.len()],
            side_span: 1,
        });
    }

    let letters: Vec<_> = medium
        .entries
        .iter()
        .map(|track| {
            let position = track.position.as_deref()?;
            position
                .chars()
                .next()
                .filter(|letter| {
                    letter.is_ascii_alphabetic()
                        && position[1..].bytes().all(|byte| byte.is_ascii_digit())
                })
                .map(|letter| letter.to_ascii_uppercase() as u32)
        })
        .collect();
    let base = letters.iter().flatten().copied().min();
    let offsets: Vec<_> = letters
        .into_iter()
        .map(|letter| letter.zip(base).map(|(letter, base)| letter - base))
        .collect();
    let side_span = match offsets.iter().flatten().max() {
        Some(last) => last + 1,
        None => 0,
    };
    Ok(MediumSides { offsets, side_span })
}

/// Read source artist credits before linked documents fill absent album facts.
pub(crate) fn artist_credits(
    credits: &[crate::musicbrainz::MbArtistCredit],
    entity_id: &str,
) -> Result<Vec<ArtistRef>, ImportError> {
    let mut release_refs: Vec<ArtistRef> = Vec::new();
    for credit in credits {
        if let Some(artist_obj) = &credit.artist {
            let artist_name = mb_artist_name(artist_obj, Some(&credit.name)).ok_or_else(|| {
                ImportError::SourceData {
                    catalog: Catalog::MusicBrainz,
                    detail: format!(
                        "MusicBrainz release {} artist credit {:?} has no artist name",
                        entity_id, artist_obj.id
                    ),
                }
            })?;
            release_refs.push(ArtistRef {
                name: artist_name,
                sort_name: artist_obj.sort_name.clone(),
                musicbrainz_artist_id: artist_obj.id.clone(),
                discogs_artist_id: None,
            });
        }
    }
    if release_refs.is_empty() {
        if let Some(credit) = credits.first() {
            if !credit.name.trim().is_empty() {
                release_refs.push(ArtistRef {
                    name: credit.name.clone(),
                    sort_name: None,
                    musicbrainz_artist_id: None,
                    discogs_artist_id: None,
                });
            }
        }
    }
    Ok(release_refs)
}

pub(crate) fn metadata(
    response: &MbReleaseResponse,
) -> Result<super::release_metadata::ReleaseMetadata, ImportError> {
    let release_refs = artist_credits(&response.artist_credit, &response.id)?;
    let album_year = super::parse_year(
        response
            .release_group
            .as_ref()
            .and_then(|group| group.first_release_date.as_deref()),
    );
    Ok(super::release_metadata::ReleaseMetadata {
        album: super::release_metadata::AlbumMetadata {
            title: response.title.clone(),
            artists: release_refs,
            year: album_year,
        },
        pressing: pressing(response),
    })
}

/// A MusicBrainz release's mediums as bae keeps them: every medium with its
/// format and every track with its position, title, length, display credits,
/// composer credits and performed works.
pub(crate) fn mediums(response: &MbReleaseResponse) -> Vec<SourceMedium> {
    let mut reported = HashSet::new();
    response
        .media
        .iter()
        .map(|medium| SourceMedium {
            format: medium.format.clone(),
            entries: medium
                .tracks
                .iter()
                .map(|track| tracklist_entry(track, &mut reported))
                .collect(),
        })
        .collect()
}

fn tracklist_entry(track: &MbTrack, reported: &mut HashSet<String>) -> TracklistEntry {
    let title = stated_title(track);
    let credits = track
        .artist_credit
        .iter()
        .enumerate()
        .map(|(position, credit)| ArtistCredit {
            position: position as i32,
            credited_name: credit.name.clone(),
            artist: credit.artist.as_ref().and_then(|artist| {
                // A credit with no resolvable name (empty credit, no artist
                // payload name) is malformed sub-data: its artist is dropped
                // and the track kept rather than the whole release refused.
                let Some(name) = mb_artist_name(artist, Some(credit.name.as_str())) else {
                    warn!(
                        musicbrainz_artist_id = ?artist.id,
                        track_number = ?track.number,
                        track_title = ?title,
                        "Skipping MusicBrainz track artist credit with unresolvable artist name"
                    );
                    return None;
                };
                Some(mb_artist_ref(name, artist))
            }),
        })
        .collect();
    let mut roles = Vec::new();
    let mut works = Vec::new();
    if let Some(recording) = track.recording.as_ref() {
        for (relation_pos, relation) in recording.relations.iter().enumerate() {
            if mb_relation_is(relation, "work", "performance") {
                let Some(work) = relation.work.as_ref() else {
                    warn!(
                        track_title = ?title,
                        relation_type = ?relation.relation_type,
                        "Skipping MusicBrainz recording work relation without work payload"
                    );
                    continue;
                };
                works.push(PerformedWork {
                    position: relation_pos as i32,
                    work: source_work(work, reported),
                });
            } else if relation.target_type.as_deref() == Some("artist") {
                if mb_relation_is_composer(relation) {
                    let Some(artist_ref) = relation.artist.as_ref() else {
                        warn!(
                            track_title = ?title,
                            relation_type = ?relation.relation_type,
                            "Skipping MusicBrainz recording artist relation without artist payload"
                        );
                        continue;
                    };
                    let Some(name) = mb_artist_name(artist_ref, relation.target_credit.as_deref())
                    else {
                        warn!(
                            track_title = ?title,
                            musicbrainz_artist_id = ?artist_ref.id,
                            "Skipping MusicBrainz recording artist relation with unresolved artist"
                        );
                        continue;
                    };
                    roles.push(RoleCredit {
                        position: relation_pos as i32,
                        artist: mb_artist_ref(name, artist_ref),
                        role: relation.relation_type.clone(),
                    });
                } else {
                    debug!(
                        track_title = ?title,
                        relation_type = ?relation.relation_type,
                        target_type = ?relation.target_type,
                        target_credit = ?relation.target_credit,
                        "Skipping MusicBrainz recording artist relation with non-composer relation type"
                    );
                }
            }
        }
    }
    TracklistEntry {
        kind: EntryKind::Track,
        position: track.number.clone(),
        number: track.position,
        title,
        duration_ms: track.length,
        credits,
        roles,
        works,
        children: Vec::new(),
    }
}

/// The picker's tracklist for the covered mediums of a MusicBrainz release,
/// sides numbered from the first covered medium.
pub(crate) fn detail_tracks(
    release: &SourceRelease,
    coverage: &MediumCoverage,
) -> Result<Vec<ReleaseTrack>, ImportError> {
    let release_id = &release.release.key;
    let mut side_base: u32 = 0;
    let mut tracks = Vec::new();
    for medium in release.covered_mediums(coverage) {
        let sides = medium_sides(release_id, medium)?;
        for (track, &side_offset) in medium.entries.iter().zip(&sides.offsets) {
            tracks.push(ReleaseTrack {
                title: track_title(release_id, track)?,
                artist: track
                    .credits
                    .first()
                    .map(|credit| credit.credited_name.clone()),
                duration_ms: track.duration_ms,
                position: track.position.clone().unwrap_or_else(|| {
                    track
                        .number
                        .map(|number| number.to_string())
                        .unwrap_or_default()
                }),
                side: side_offset.map(|offset| side_base + offset + 1),
            });
        }
        side_base += sides.side_span;
    }
    Ok(tracks)
}

/// The album the covered mediums of a release describe: their tracks in
/// release order, sides numbered from the first covered medium.
pub(crate) fn map(
    release: &SourceRelease,
    coverage: &MediumCoverage,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<ParsedAlbum, ImportError> {
    let release_id = &release.release.key;
    let mut metadata = release.metadata.clone();
    let primary_artist = metadata
        .album
        .take_primary(Catalog::MusicBrainz, release_id)?;
    let is_compilation = is_various_artists(&primary_artist.name);

    // `side_base` advances per medium so side values never repeat across media.
    let mut tracks: Vec<TrackIr> = Vec::new();
    let mut side_base = 0i32;
    // Release-scoped: each work is expanded at most once, no matter how many
    // tracks reference it.
    let mut converted_works: HashSet<String> = HashSet::new();
    for medium in release.covered_mediums(coverage) {
        let sides = medium_sides(release_id, medium)?;

        for (track, &side_offset) in medium.entries.iter().zip(&sides.offsets) {
            let title = track_title(release_id, track)?;
            let side = side_offset.map(|offset| side_base + offset as i32 + 1);

            let mut events: Vec<TrackEvent> = track
                .credits
                .iter()
                .filter_map(|credit| {
                    Some(TrackEvent::Credit {
                        position: credit.position,
                        artist: credit.artist.clone()?,
                    })
                })
                .collect();
            // Roles and works interleave in the recording's relation order.
            enum Relation<'a> {
                Role(&'a RoleCredit),
                Work(&'a PerformedWork),
            }
            let mut relations: Vec<Relation<'_>> = track
                .roles
                .iter()
                .map(Relation::Role)
                .chain(track.works.iter().map(Relation::Work))
                .collect();
            relations.sort_by_key(|relation| match relation {
                Relation::Role(role) => role.position,
                Relation::Work(performed) => performed.position,
            });
            for relation in relations {
                events.push(match relation {
                    Relation::Role(role) => TrackEvent::Role {
                        position: role.position,
                        artist: role.artist.clone(),
                        source: Catalog::MusicBrainz,
                        source_credit: role.role.clone(),
                    },
                    Relation::Work(performed) => TrackEvent::Work {
                        position: performed.position,
                        source: Catalog::MusicBrainz,
                        work: work_ref(&performed.work, &mut converted_works),
                    },
                });
            }

            tracks.push(TrackIr {
                title,
                side,
                number: track
                    .position
                    .as_deref()
                    .and_then(super::assemble::position_number),
                source_position: track.position.clone(),
                events,
            });
        }

        side_base += sides.side_span as i32;
    }

    let ir = ReleaseIr {
        album_title: metadata.album.title,
        primary_artist,
        additional_artists: metadata.album.artists,
        album_year: metadata.album.year,
        is_compilation,
        pressing: metadata.pressing,
        metadata_provenance: Some(crate::import::MetadataProvenance::ExternalRelease {
            record: MetadataRef::new(Catalog::MusicBrainz, release_id.clone()),
            // As in `discogs_mapper`: one release's own claim.
            partners: Vec::new(),
        }),
        album_artist_scope: AlbumArtistScope::ReleaseCredits,
        release_roles: Vec::new(),
        tracks,
    };

    Ok(assemble_parsed_album(ir, clock, ids))
}

#[cfg(test)]
#[path = "musicbrainz_mapper_tests.rs"]
mod tests;

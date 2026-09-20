//! MusicBrainz release → `ParsedAlbum` mapping, plus the cross-link the response
//! carries. The records a pick commits are read off the archived documents by
//! `ReleasePayloads::records`, not built here.
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
use crate::import::{Catalog, ImportError, MetadataRef};
use crate::musicbrainz::{
    label_and_catno, MbArtistRef, MbMedium, MbRelation, MbReleaseResponse, MbTrack, MbWork,
};
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

/// Resolve an `MbWork` into a [`WorkGraphRef`], validating relations (malformed
/// ones are dropped and logged here, at the source→IR boundary, so the
/// assembler's walk never meets one).
///
/// `converted` is release-scoped: the first reference to a work id returns an
/// `Expanded` node carrying its walked sub-graph; every later reference returns
/// `AlreadyExpanded`, so each work's relations are walked and logged exactly
/// once per release and the assembler emits its row and sub-graph once.
fn mb_work_ref(work: &MbWork, converted: &mut HashSet<String>) -> WorkGraphRef {
    if !converted.insert(work.id.clone()) {
        return WorkGraphRef::AlreadyExpanded {
            musicbrainz_work_id: work.id.clone(),
        };
    }

    let mut events = Vec::new();
    for relation in &work.relations {
        if relation.target_type.as_deref() == Some("artist") {
            if mb_relation_is_composer(relation) {
                let Some(artist_ref) = relation.artist.as_ref() else {
                    warn!(
                        work_id = %work.id,
                        relation_type = ?relation.relation_type,
                        "Skipping MusicBrainz work artist relation without artist payload"
                    );
                    continue;
                };
                let Some(name) = mb_artist_name(artist_ref, relation.target_credit.as_deref())
                else {
                    warn!(
                        work_id = %work.id,
                        musicbrainz_artist_id = ?artist_ref.id,
                        "Skipping MusicBrainz work artist relation with unresolved artist"
                    );
                    continue;
                };
                events.push(WorkEvent::Composer(mb_artist_ref(name, artist_ref)));
            } else {
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
                warn!(
                    work_id = %work.id,
                    relation_type = ?relation.relation_type,
                    "Skipping MusicBrainz work parts relation without work payload"
                );
                continue;
            };
            let direction = match relation.direction.as_deref() {
                Some("backward") => PartDirection::Backward,
                _ => PartDirection::Forward,
            };
            events.push(WorkEvent::Part {
                direction,
                work: mb_work_ref(child_or_parent, converted),
            });
        }
    }

    WorkGraphRef::Expanded(WorkNode {
        musicbrainz_work_id: work.id.clone(),
        title: work.title.clone(),
        disambiguation: work.disambiguation.clone(),
        work_type: work.work_type.clone(),
        events,
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

/// A track's title: the recording's, else the track's own override. Shared by
/// the DB mapper and the UI-detail builder in `search.rs` so the picker and the
/// committed rows can't show different titles.
///
/// Errors when neither carries a non-blank title: there is no title to show, and
/// an empty string in its place is a lie the user can't see through.
pub(crate) fn track_title(release_id: &str, track: &MbTrack) -> Result<String, ImportError> {
    track
        .recording
        .as_ref()
        .and_then(|r| r.title.as_deref())
        .or(track.title.as_deref())
        .filter(|title| !title.trim().is_empty())
        .ok_or_else(|| ImportError::SourceData {
            catalog: Catalog::MusicBrainz,
            detail: format!(
                "MusicBrainz release {} track {:?} has no track title",
                release_id, track.number
            ),
        })
        .map(str::to_string)
}

/// Vinyl/cassette side assignment for one medium, shared by the DB mapper and
/// the UI-detail builder in `search.rs` so the two never diverge.
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
    medium: &MbMedium,
) -> Result<MediumSides, ImportError> {
    if medium.tracks.is_empty() {
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
            offsets: vec![Some(0); medium.tracks.len()],
            side_span: 1,
        });
    }

    let letters: Vec<_> = medium
        .tracks
        .iter()
        .map(|track| {
            let position = track.number.as_deref()?;
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

pub(crate) fn map_with_metadata(
    response: &MbReleaseResponse,
    mut metadata: super::release_metadata::ReleaseMetadata,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<ParsedAlbum, ImportError> {
    let primary_artist = metadata
        .album
        .take_primary(Catalog::MusicBrainz, &response.id)?;
    let is_compilation = is_various_artists(&primary_artist.name);

    // `side_base` advances per medium so side values never repeat across media.
    let mut tracks: Vec<TrackIr> = Vec::new();
    let mut side_base = 0i32;
    // Release-scoped: each work's relations are converted (and its skip lines
    // logged) at most once, no matter how many tracks reference it.
    let mut converted_works: HashSet<String> = HashSet::new();
    for medium in &response.media {
        let sides = medium_sides(&response.id, medium)?;

        for (track, &side_offset) in medium.tracks.iter().zip(&sides.offsets) {
            let title = track_title(&response.id, track)?;

            let side = side_offset.map(|offset| side_base + offset as i32 + 1);

            let mut events: Vec<TrackEvent> = Vec::new();

            for (credit_pos, credit) in track.artist_credit.iter().enumerate() {
                if let Some(artist_obj) = &credit.artist {
                    // A credit with no resolvable name (empty credit, no artist
                    // payload name) is malformed sub-data: skip it and keep the
                    // track rather than abort the whole import.
                    let Some(name) = mb_artist_name(artist_obj, Some(credit.name.as_str())) else {
                        warn!(
                            musicbrainz_artist_id = ?artist_obj.id,
                            track_number = ?track.number,
                            track_title = %title,
                            "Skipping MusicBrainz track artist credit with unresolvable artist name"
                        );
                        continue;
                    };
                    events.push(TrackEvent::Credit {
                        position: credit_pos as i32,
                        artist: mb_artist_ref(name, artist_obj),
                    });
                }
            }

            if let Some(recording) = track.recording.as_ref() {
                for (relation_pos, relation) in recording.relations.iter().enumerate() {
                    if mb_relation_is(relation, "work", "performance") {
                        let Some(work) = relation.work.as_ref() else {
                            warn!(
                                track_title = %title,
                                relation_type = ?relation.relation_type,
                                "Skipping MusicBrainz recording work relation without work payload"
                            );
                            continue;
                        };
                        events.push(TrackEvent::Work {
                            position: relation_pos as i32,
                            source: Catalog::MusicBrainz,
                            work: mb_work_ref(work, &mut converted_works),
                        });
                    } else if relation.target_type.as_deref() == Some("artist") {
                        if mb_relation_is_composer(relation) {
                            let Some(artist_ref) = relation.artist.as_ref() else {
                                warn!(
                                    track_title = %title,
                                    relation_type = ?relation.relation_type,
                                    "Skipping MusicBrainz recording artist relation without artist payload"
                                );
                                continue;
                            };
                            let Some(name) =
                                mb_artist_name(artist_ref, relation.target_credit.as_deref())
                            else {
                                warn!(
                                    track_title = %title,
                                    musicbrainz_artist_id = ?artist_ref.id,
                                    "Skipping MusicBrainz recording artist relation with unresolved artist"
                                );
                                continue;
                            };
                            events.push(TrackEvent::Role {
                                position: relation_pos as i32,
                                artist: mb_artist_ref(name, artist_ref),
                                source: Catalog::MusicBrainz,
                                source_credit: relation.relation_type.clone(),
                            });
                        } else {
                            debug!(
                                track_title = %title,
                                relation_type = ?relation.relation_type,
                                target_type = ?relation.target_type,
                                target_credit = ?relation.target_credit,
                                "Skipping MusicBrainz recording artist relation with non-composer relation type"
                            );
                        }
                    }
                }
            }

            tracks.push(TrackIr {
                title,
                side,
                number: track
                    .number
                    .as_deref()
                    .and_then(super::assemble::position_number),
                source_position: track.number.clone(),
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
            record: MetadataRef::new(Catalog::MusicBrainz, response.id.clone()),
            // As in `discogs_mapper`: one document's own claim.
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

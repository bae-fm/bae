//! MusicBrainz release → stored tracklist, and stored tracklist → the values
//! a draft holds and picker rows. The records a pick commits are extracted with the release
//! by `ReleasePayloads::extract`, not built here.
//!
//! MB → Discogs cross-link: MB releases carry url-rels that routinely include a
//! Discogs release URL, which is what the Discogs document alongside an MB one
//! was fetched from. The reverse (Discogs → MB) is less reliable — the Discogs
//! API exposes no MBID field — so it is resolved through MB's URL endpoint
//! instead.

use crate::import::medium_coverage::MediumCoverage;
use crate::import::release_values::{
    position_number, ReleaseValues, TrackCredits, TrackValues, WorkGraphBuilder,
};
use crate::import::source_release::{
    DisplayCredit, EntryKind, PartDirection, PerformedWork, RoleCredit, SourceMedium,
    SourceRelease, SourceWork, SourceWorkEvent, TracklistEntry,
};
use crate::import::{ArtistCredit, CreditedRole, ImportError, SourceCredit};
use bae_catalogs::catalog::Catalog;
use bae_catalogs::musicbrainz::{release_labels, MbArtistRef, MbRelation, MbReleaseResponse, MbTrack, MbWork};
use bae_types::pressing::{Medium, Pressing, StatedMedia};
use std::collections::HashSet;
use std::num::NonZeroU8;
use tracing::{debug, warn};

fn mb_relation_is(relation: &MbRelation, target_type: &str, relation_type: &str) -> bool {
    relation.target_type.as_deref() == Some(target_type)
        && relation.relation_type.as_deref() == Some(relation_type)
}

/// The name a role or composer credit goes by: the name the relation
/// credits, else the artist's own.
fn mb_artist_name(artist: &MbArtistRef, credit: Option<&str>) -> Option<String> {
    credit
        .filter(|c| !c.trim().is_empty())
        .map(str::to_string)
        .or_else(|| artist.name.clone())
}

/// A role or composer credit of a MusicBrainz artist: `name` is the resolved
/// credit name; sort name and MB id come from the artist payload.
fn mb_artist_ref(name: String, artist: &MbArtistRef) -> ArtistCredit {
    ArtistCredit {
        name,
        sort_name: artist.sort_name.clone(),
        musicbrainz_artist_id: artist.id.clone(),
        discogs_artist_id: None,
        credited_as: None,
    }
}

/// The artist a MusicBrainz credit names, by the artist's own name (else the
/// name the credit prints), with the printed name kept where it is another.
/// `None` where neither names anyone.
fn mb_credited_artist(artist: &MbArtistRef, printed: &str) -> Option<ArtistCredit> {
    let printed = Some(printed.trim()).filter(|printed| !printed.is_empty());
    let name = artist
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .or(printed)?
        .to_string();
    Some(ArtistCredit {
        credited_as: printed
            .filter(|printed| *printed != name)
            .map(str::to_string),
        name,
        sort_name: artist.sort_name.clone(),
        musicbrainz_artist_id: artist.id.clone(),
        discogs_artist_id: None,
    })
}

fn mb_relation_is_composer(relation: &MbRelation) -> bool {
    relation.relation_type.as_deref() == Some("composer")
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

/// The pressing a MusicBrainz release describes — its own release date's
/// year, every label's name and catalog number, its barcode, and what it
/// is: its country, every medium, its status and packaging — and its media
/// in the document's own shape.
///
/// The one MB → pressing projection. The committed release, the picker's detail,
/// and a search result all read it, so a pressing shown is the pressing stored.
pub(crate) fn pressing(response: &MbReleaseResponse) -> (Pressing, StatedMedia) {
    let (facts, media) = bae_catalogs::musicbrainz::pressing::read(bae_catalogs::musicbrainz::pressing::Stated {
        release_id: &response.id,
        country: response.country.as_deref(),
        status: response.status.as_deref(),
        packaging: response.packaging.as_deref(),
        media: response.media.iter().map(|medium| medium.format.as_deref()),
    });
    let pressing = Pressing {
        year: super::parse_year(response.date.as_deref()),
        labels: release_labels(&response.label_info),
        // MusicBrainz writes an empty barcode for a release it knows has none.
        barcodes: response
            .barcode
            .iter()
            .filter(|barcode| !barcode.trim().is_empty())
            .cloned()
            .collect(),
        facts,
    };
    (pressing, media)
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

/// A stored track's title.
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

/// Which side of one medium each of its tracks is on, and how many sides the
/// medium has.
pub(crate) struct MediumSides {
    /// One per track, in track order, counted from one.
    pub sides: Vec<NonZeroU8>,
    pub count: NonZeroU8,
}

/// The sides of one medium's tracks.
///
/// A medium played side by side (a record, a cassette) has as many sides as
/// its positions name letters, counted from its lowest letter — so a second
/// record lettered C/D has sides 1 and 2 of its own. A track whose position
/// names no letter is on the side of the track before it. Any other medium,
/// and a sided one whose positions name no letter, is one side.
///
/// An empty medium has no playable tracks and is rejected.
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
    let one_side = || MediumSides {
        sides: vec![NonZeroU8::MIN; medium.entries.len()],
        count: NonZeroU8::MIN,
    };
    if !medium.medium.is_some_and(Medium::is_sided) {
        return Ok(one_side());
    }
    let letters: Vec<Option<u8>> = medium
        .entries
        .iter()
        .map(|track| {
            let position = track.position.as_deref()?;
            let letter = *position.as_bytes().first()?;
            (letter.is_ascii_alphabetic()
                && position[1..].bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| letter.to_ascii_uppercase())
        })
        .collect();
    let Some(base) = letters.iter().flatten().copied().min() else {
        return Ok(one_side());
    };
    let mut sides = Vec::with_capacity(letters.len());
    let mut current = NonZeroU8::MIN;
    for letter in letters {
        if let Some(letter) = letter {
            current = crate::mediums::side_number(usize::from(letter - base));
        }
        sides.push(current);
    }
    let count = *sides.iter().max().expect("a medium with tracks has a side");
    Ok(MediumSides { sides, count })
}

/// A release's or track's MusicBrainz artist credit, place by place, with
/// what MusicBrainz prints after each. A credit naming no artist object keeps
/// its printed name as a credit of its own; the last credit's join is not
/// kept, since nothing follows it.
pub(crate) fn artist_credits(
    credits: &[bae_catalogs::musicbrainz::MbArtistCredit],
    entity_id: &str,
) -> Result<Vec<SourceCredit>, ImportError> {
    let mut stated: Vec<SourceCredit> = Vec::new();
    for credit in credits {
        let artist = match &credit.artist {
            Some(artist_obj) => {
                mb_credited_artist(artist_obj, &credit.name).ok_or_else(|| {
                    ImportError::SourceData {
                        catalog: Catalog::MusicBrainz,
                        detail: format!(
                            "MusicBrainz release {} artist credit {:?} has no artist name",
                            entity_id, artist_obj.id
                        ),
                    }
                })?
            }
            None if credit.name.trim().is_empty() => continue,
            None => ArtistCredit::named(credit.name.trim()),
        };
        stated.push(SourceCredit {
            credit: artist,
            join: crate::credits::stated_join(credit.joinphrase.as_deref()),
        });
    }
    crate::credits::drop_last_join(&mut stated, |credit| &mut credit.join);
    Ok(stated)
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
            first_year: album_year,
        },
        pressing: pressing(response).0,
    })
}

/// A MusicBrainz release's mediums as bae keeps them: every medium with the
/// carrier its format names and every track with its position, title, length, display credits,
/// composer credits and performed works.
pub(crate) fn mediums(response: &MbReleaseResponse) -> Vec<SourceMedium> {
    let mut reported = HashSet::new();
    response
        .media
        .iter()
        .map(|medium| SourceMedium {
            medium: bae_catalogs::musicbrainz::pressing::medium(&response.id, medium.format.as_deref()),
            disc_ids: medium.discs.iter().fold(Vec::new(), |mut held, disc| {
                if !held.contains(&disc.id) {
                    held.push(disc.id.clone());
                }
                held
            }),
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
        .map(|(position, credit)| DisplayCredit {
            position: position as i32,
            credited_name: credit.name.clone(),
            artist: credit.artist.as_ref().and_then(|artist| {
                // A credit with no resolvable name (empty credit, no artist
                // payload name) is malformed sub-data: its artist is dropped
                // and the track kept rather than the whole release refused.
                let credited = mb_credited_artist(artist, &credit.name);
                if credited.is_none() {
                    warn!(
                        musicbrainz_artist_id = ?artist.id,
                        track_number = ?track.number,
                        track_title = ?title,
                        "Skipping MusicBrainz track artist credit with unresolvable artist name"
                    );
                }
                credited
            }),
            join: crate::credits::stated_join(credit.joinphrase.as_deref()),
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

/// What the covered mediums of a release say the audio's tracks are: their
/// tracks in release order, sides numbered from the first covered medium,
/// each with its own credits, composers and works.
pub(crate) fn values(
    release: &SourceRelease,
    coverage: &MediumCoverage,
) -> Result<ReleaseValues, ImportError> {
    let release_id = &release.release.key;
    let metadata = release.metadata.clone();
    metadata.album.require_artists(Catalog::MusicBrainz, release_id)?;

    let mut tracks: Vec<TrackValues> = Vec::new();
    let mut mediums = Vec::new();
    let mut placer = crate::mediums::Placer::default();
    let mut works = WorkGraphBuilder::default();
    for (index, medium) in release.covered_mediums(coverage).enumerate() {
        let sides = medium_sides(release_id, medium)?;
        mediums.push(crate::mediums::ReleaseMedium {
            carrier: medium.medium,
            sides: sides.count,
        });
        let position = crate::mediums::medium_position(index);

        for (track, &side) in medium.entries.iter().zip(&sides.sides) {
            let credits = crate::import::source_release::stated_credits(&track.credits);
            tracks.push(TrackValues {
                title: track_title(release_id, track)?,
                place: placer.place(
                    position,
                    side,
                    track.position.as_deref().and_then(position_number),
                ),
                stated_length_ms: track.duration_ms,
                credits: if credits.is_empty() {
                    TrackCredits::AlbumArtists
                } else {
                    TrackCredits::Own(credits)
                },
                roles: track
                    .roles
                    .iter()
                    .map(|role| CreditedRole {
                        artist: role.artist.clone(),
                        source: Catalog::MusicBrainz,
                        role: role.role.clone(),
                    })
                    .collect(),
                works: track
                    .works
                    .iter()
                    .map(|performed| works.performed(&performed.work))
                    .fold(Vec::new(), |mut performed, work| {
                        // A recording can relate to one work more than once.
                        if !performed.contains(&work) {
                            performed.push(work);
                        }
                        performed
                    }),
            });
        }
    }

    Ok(ReleaseValues {
        album_title: metadata.album.title,
        album_credits: metadata.album.artists,
        album_year: metadata.album.year,
        pressing: metadata.pressing,
        mediums,
        release_roles: Vec::new(),
        works: works.finish(),
        tracks,
    })
}

#[cfg(test)]
#[path = "musicbrainz_mapper_tests.rs"]
mod tests;

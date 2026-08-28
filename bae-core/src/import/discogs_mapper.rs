use super::assemble::{
    assemble_parsed_album, AlbumArtistScope, ArtistRef, ReleaseIr, ReleaseRole, TrackEvent,
    TrackIr, TrackNumber,
};
use super::ParsedAlbum;
use crate::db::{is_various_artists, Pressing};
use crate::discogs::{DiscogsArtist, DiscogsRelease, DiscogsRoleArtist};
use crate::import::types::ReleaseIdentity;
use crate::import::{ImportError, MetadataSource};
use crate::musicbrainz::MbReleaseResponse;
use coven::Clock;
use coven::IdProvider;
use std::collections::HashSet;
use tracing::{debug, warn};

fn discogs_role_is_composer(role: &str) -> bool {
    let lowered = role
        .chars()
        .filter(|c| *c != '[' && *c != ']')
        .collect::<String>()
        .to_ascii_lowercase();
    let compact = lowered.replace(['-', '_'], " ");
    compact.contains("composed by")
        || compact.contains("written by")
        || compact.contains("music by")
        || compact.contains("composer")
}

/// An [`ArtistRef`] for a Discogs artist: the display name doubles as its own
/// sort name, and the Discogs artist id (when present) is what dedups the pool.
/// Discogs artists carry no MusicBrainz id.
fn discogs_artist_ref(name: String, discogs_artist_id: Option<String>) -> ArtistRef {
    ArtistRef {
        name: name.clone(),
        sort_name: Some(name),
        musicbrainz_artist_id: None,
        discogs_artist_id,
    }
}

/// An [`ArtistRef`] for a Discogs role credit. The display name is the
/// `credited_name`, falling back to the canonical name (logged) when absent.
fn discogs_role_artist_ref(credit: &DiscogsRoleArtist) -> ArtistRef {
    let name = match credit.credited_name.clone() {
        Some(name) => name,
        None => {
            warn!(
                discogs_artist_id = ?credit.id,
                artist_name = %credit.name,
                "Discogs role artist has no credited name; using canonical name"
            );
            credit.name.clone()
        }
    };
    discogs_artist_ref(name, credit.id.clone())
}

/// An [`ArtistRef`] for a Discogs display credit, keyed on its canonical name.
fn discogs_track_artist_ref(credit: &DiscogsArtist) -> ArtistRef {
    discogs_artist_ref(credit.name.clone(), Some(credit.id.clone()))
}

/// Map a Discogs release into database models (pure, no I/O).
///
/// `master_year` is the original release year from the Discogs master; the album
/// year falls back to the specific release's year when it's absent.
///
/// `mb_xref` is the MB release resolved through MB's URL endpoint
/// (`crate::musicbrainz::fetch_mb_xref`), when one was. It contributes a second
/// `ReleaseIdentity` row, so future MB-rooted imports of the same release group
/// attach to this album.
pub fn map_discogs_to_db(
    release: &DiscogsRelease,
    master_year: Option<u32>,
    mb_xref: Option<&MbReleaseResponse>,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<ParsedAlbum, ImportError> {
    // With no artists list, fall back to the artist half of the "Artist - Album"
    // title split.
    let mut release_refs: Vec<ArtistRef> = if release.artists.is_empty() {
        let artist_name = crate::discogs::split_title(&release.title)
            .and_then(|(artist, _)| artist)
            .ok_or_else(|| ImportError::SourceData {
                metadata_source: MetadataSource::Discogs,
                detail: format!(
                    "Discogs release {} has no release artist in artists list or title",
                    release.id
                ),
            })?
            .to_string();
        vec![ArtistRef {
            name: artist_name.clone(),
            sort_name: Some(artist_name),
            musicbrainz_artist_id: None,
            discogs_artist_id: None,
        }]
    } else {
        release
            .artists
            .iter()
            .map(discogs_track_artist_ref)
            .collect()
    };
    let primary_artist = release_refs.remove(0);

    let album_year = master_year
        .map(|y| y as i32)
        .or(release.year.map(|y| y as i32));
    let is_compilation = release
        .artists
        .first()
        .map(|a| is_various_artists(&a.name))
        .unwrap_or(false);
    let format = if release.format.is_empty() {
        None
    } else {
        Some(release.format.join(", "))
    };
    let pressing = Pressing {
        year: release.year.map(|y| y as i32),
        format,
        label: release.label.first().cloned(),
        catalog_number: release.catno.clone(),
        country: release.country.clone(),
        barcode: None,
    };

    // Positions come from source order, and non-composer roles are skipped — so
    // the positions keep holes.
    let mut release_roles: Vec<ReleaseRole> = Vec::new();
    if let Some(extraartists) = release.extraartists.as_ref() {
        for (position, credit) in extraartists.iter().enumerate() {
            if discogs_role_is_composer(&credit.role) {
                release_roles.push(ReleaseRole {
                    position: position as i32,
                    artist: discogs_role_artist_ref(credit),
                    source: MetadataSource::Discogs,
                    source_credit: Some(credit.role.clone()),
                });
            } else {
                debug!(
                    discogs_release_id = %release.id,
                    artist_name = %credit.name,
                    role = %credit.role,
                    "Skipping Discogs release-level extraartist with non-composer role"
                );
            }
        }
    }

    let processed = process_tracklist(&release.tracklist);
    let tracks: Vec<TrackIr> = processed
        .iter()
        .map(|pt| discogs_track_ir(release, pt))
        .collect();

    // Discogs's `master_id` is the group key, so a release without one stands on
    // its own and gets no Discogs identity row — the import still commits, just
    // without that source's identity. An `mb_xref` means MB back-links to this
    // Discogs release, contributing a second row so future MB-rooted imports of
    // the same release group attach to this album. Both rows are Exact.
    let mut identities: Vec<ReleaseIdentity> = release
        .master_id
        .as_ref()
        .map(|master_id| ReleaseIdentity {
            source: MetadataSource::Discogs,
            source_group_id: master_id.clone(),
            source_release_id: release.id.clone(),
        })
        .into_iter()
        .collect();
    if let Some(mb) = mb_xref {
        let rg = mb
            .release_group
            .as_ref()
            .ok_or_else(|| ImportError::SourceData {
                metadata_source: MetadataSource::MusicBrainz,
                detail: format!("MusicBrainz release {} missing release_group", mb.id),
            })?;
        identities.push(ReleaseIdentity {
            source: MetadataSource::MusicBrainz,
            source_group_id: rg.id.clone(),
            source_release_id: mb.id.clone(),
        });
    }

    let ir = ReleaseIr {
        album_title: release.title.clone(),
        primary_artist,
        additional_artists: release_refs,
        album_year,
        is_compilation,
        pressing,
        metadata_provenance: Some(crate::import::MetadataProvenance::ExternalRelease {
            source: MetadataSource::Discogs,
            release_id: release.id.clone(),
        }),
        album_artist_scope: AlbumArtistScope::ReleaseCredits,
        release_roles,
        tracks,
        identities,
    };

    Ok(assemble_parsed_album(ir, clock, ids))
}

/// Build one track's IR from a processed Discogs track. Role credits precede
/// display credits per source row (preserving the artist-pool discovery order);
/// display credits are deduped across a collapsed track's source rows by Discogs
/// artist id, first occurrence wins, with positions compacted `0..n`.
fn discogs_track_ir(release: &DiscogsRelease, pt: &ProcessedTrack) -> TrackIr {
    let mut events: Vec<TrackEvent> = Vec::new();
    let mut seen_credit_ids: HashSet<String> = HashSet::new();
    let mut credit_position = 0i32;

    for source_track_index in &pt.source_track_indices {
        let discogs_track = &release.tracklist[*source_track_index];
        match discogs_track.extraartists.as_ref() {
            Some(extraartists) => {
                for (role_position, credit) in extraartists.iter().enumerate() {
                    if discogs_role_is_composer(&credit.role) {
                        events.push(TrackEvent::Role {
                            position: role_position as i32,
                            artist: discogs_role_artist_ref(credit),
                            source: MetadataSource::Discogs,
                            source_credit: Some(credit.role.clone()),
                        });
                    } else {
                        debug!(
                            discogs_release_id = %release.id,
                            discogs_track_position = %discogs_track.position,
                            track_title = %discogs_track.title,
                            artist_name = %credit.name,
                            role = %credit.role,
                            "Skipping Discogs track-level extraartist with non-composer role"
                        );
                    }
                }
            }
            None => {
                debug!(
                    discogs_release_id = %release.id,
                    discogs_track_position = %discogs_track.position,
                    track_title = %discogs_track.title,
                    "Discogs track has no extraartists field; skipping per-track role credits"
                );
            }
        }

        for discogs_artist in &discogs_track.artists {
            if seen_credit_ids.insert(discogs_artist.id.clone()) {
                events.push(TrackEvent::Credit {
                    position: credit_position,
                    artist: discogs_track_artist_ref(discogs_artist),
                });
                credit_position += 1;
            }
        }
    }

    TrackIr {
        title: pt.title.clone(),
        side: pt.side,
        number: TrackNumber::PerSide,
        source_position: Some(pt.position.clone()),
        events,
    }
}

/// A processed track: heading/sub-track collapsing and index filtering applied,
/// with its side derived from the position. Per-side track numbers are assigned
/// downstream by the assembler's single numbering pass.
pub(crate) struct ProcessedTrack {
    pub title: String,
    pub position: String,
    /// Source tracklist indices. For collapsed sub-tracks, this contains all
    /// sub-track entries; for regular tracks, this contains the one source row.
    pub source_track_indices: Vec<usize>,
    pub side: i32,
}

/// Process a Discogs tracklist: filter index entries, collapse headings with
/// sub-tracks, and assign sides from the position format.
pub(crate) fn process_tracklist(tracklist: &[crate::discogs::DiscogsTrack]) -> Vec<ProcessedTrack> {
    use crate::discogs::DiscogsTrack;

    let filtered: Vec<(usize, &DiscogsTrack)> = tracklist
        .iter()
        .enumerate()
        .filter(|(_, track)| track.type_ != "index")
        .collect();

    struct CollapsedTrack {
        title: String,
        position: String,
        /// Source tracklist indices for artist lookups.
        source_track_indices: Vec<usize>,
    }

    let flush_accumulated_sub_tracks =
        |collapsed: &mut Vec<CollapsedTrack>,
         current_heading: Option<&str>,
         sub_tracks: &mut Vec<String>,
         sub_track_indices: &mut Vec<usize>,
         heading_position: &mut Option<String>| {
            if sub_tracks.is_empty() {
                return;
            }

            let heading = current_heading.expect("sub-tracks are accumulated under a heading");
            let title = format!("{}: {}", heading, sub_tracks.join(" \u{2013} "));
            collapsed.push(CollapsedTrack {
                title,
                position: heading_position
                    .take()
                    .expect("sub-tracks have a base heading position"),
                source_track_indices: std::mem::take(sub_track_indices),
            });
            sub_tracks.clear();
        };

    let mut collapsed: Vec<CollapsedTrack> = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut sub_tracks: Vec<String> = Vec::new();
    let mut sub_track_indices: Vec<usize> = Vec::new();
    let mut heading_position: Option<String> = None;

    for &(source_track_index, entry) in &filtered {
        if entry.type_ == "heading" {
            flush_accumulated_sub_tracks(
                &mut collapsed,
                current_heading.as_deref(),
                &mut sub_tracks,
                &mut sub_track_indices,
                &mut heading_position,
            );

            if entry.title == "-" {
                // A heading titled "-" clears the current heading.
                current_heading = None;
                heading_position = None;
            } else {
                current_heading = Some(entry.title.clone());
                heading_position = None;
            }
            continue;
        }

        if current_heading.is_some() && is_sub_track_position(&entry.position) {
            if heading_position.is_none() {
                heading_position = Some(extract_base_position(&entry.position));
            }
            sub_tracks.push(entry.title.clone());
            sub_track_indices.push(source_track_index);
        } else {
            flush_accumulated_sub_tracks(
                &mut collapsed,
                current_heading.as_deref(),
                &mut sub_tracks,
                &mut sub_track_indices,
                &mut heading_position,
            );
            current_heading = None;
            heading_position = None;

            collapsed.push(CollapsedTrack {
                title: entry.title.clone(),
                position: entry.position.clone(),
                source_track_indices: vec![source_track_index],
            });
        }
    }

    flush_accumulated_sub_tracks(
        &mut collapsed,
        current_heading.as_deref(),
        &mut sub_tracks,
        &mut sub_track_indices,
        &mut heading_position,
    );

    collapsed
        .into_iter()
        .map(|ct| {
            let side = parse_side_from_position(&ct.position);
            ProcessedTrack {
                title: ct.title,
                position: ct.position,
                source_track_indices: ct.source_track_indices,
                side,
            }
        })
        .collect()
}

/// Whether a position names a sub-track ("B1i", "B1ii", "B1iii"): a letter
/// prefix, then a number, then a roman-numeral or letter suffix.
fn is_sub_track_position(position: &str) -> bool {
    let bytes = position.as_bytes();
    if bytes.len() < 3 {
        return false;
    }
    if !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    let mut digit_end = 1;
    while digit_end < bytes.len() && bytes[digit_end].is_ascii_digit() {
        digit_end += 1;
    }
    // At least one digit, and a suffix after it.
    digit_end > 1 && digit_end < bytes.len()
}

/// Extract the base position from a sub-track position (e.g., "B1i" -> "B1").
fn extract_base_position(position: &str) -> String {
    let bytes = position.as_bytes();
    let mut end = 1;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    position[..end].to_string()
}

/// The side a Discogs position string names: for vinyl (`A1`, `B2`, `C1`) the
/// letter is the side (A=1, B=2, ...); for CD (`1-1`, `2-1`) the disc number is;
/// a plain number (`1`, `2`) is side 1.
pub fn parse_side_from_position(position: &str) -> i32 {
    if let Some(dash_idx) = position.find('-') {
        if let Ok(disc) = position[..dash_idx].parse::<i32>() {
            return disc;
        }
    }

    if let Some(first_char) = position.chars().next() {
        if first_char.is_ascii_alphabetic() {
            return (first_char.to_ascii_uppercase() as i32) - ('A' as i32) + 1;
        }
    }

    1
}

#[cfg(test)]
#[path = "discogs_mapper_tests.rs"]
mod tests;

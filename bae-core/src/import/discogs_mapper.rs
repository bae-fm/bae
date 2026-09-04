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

/// The identity a Discogs release states about itself.
///
/// The group is the release's master when Discogs filed it under one, and the
/// release's own id when it did not — a master-less release is its own group,
/// the same reading `ReleaseGroup::id` in `release_group.rs` gives a lone
/// release. Cross-source album merging matches on `(source, group)`, so a
/// release standing as its own group merges only with itself, which is what a
/// release Discogs never grouped should do. Emitting no row instead would drop
/// the claim the person made when they picked that release.
pub(super) fn discogs_identity(release: &DiscogsRelease) -> ReleaseIdentity {
    ReleaseIdentity {
        source: MetadataSource::Discogs,
        source_group_id: release
            .master_id
            .clone()
            .unwrap_or_else(|| release.id.clone()),
        source_release_id: release.id.clone(),
    }
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
    let tracks = process_tracklist(&release.tracklist);
    map_discogs_to_db_with_tracks(release, master_year, mb_xref, &tracks, clock, ids)
}

pub(crate) fn map_discogs_to_db_for_audio(
    release: &DiscogsRelease,
    master_year: Option<u32>,
    mb_xref: Option<&MbReleaseResponse>,
    audio_durations_ms: &[u64],
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<ParsedAlbum, ImportError> {
    let tracks = process_tracklist_for_audio(&release.tracklist, audio_durations_ms);
    map_discogs_to_db_with_tracks(release, master_year, mb_xref, &tracks, clock, ids)
}

fn map_discogs_to_db_with_tracks(
    release: &DiscogsRelease,
    master_year: Option<u32>,
    mb_xref: Option<&MbReleaseResponse>,
    processed: &[ProcessedTrack<'_>],
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

    let tracks: Vec<TrackIr> = processed
        .iter()
        .map(|pt| discogs_track_ir(release, pt))
        .collect();

    // Always a Discogs row: the release states its own identity whether or not
    // Discogs filed it under a master. An `mb_xref` means MB back-links to this
    // Discogs release, contributing a second row so future MB-rooted imports of
    // the same release group attach to this album. Both rows are Exact.
    let mut identities: Vec<ReleaseIdentity> = vec![discogs_identity(release)];
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
            // The mapper reads one document; what else the pick claimed is
            // the picker's to say, and reaches the library as identity rows.
            partners: Vec::new(),
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

    for discogs_track in &pt.source_tracks {
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

/// One playable track from a selected Discogs layout. The source entries stay
/// attached so credits survive both an expanded index and its collapsed form.
#[derive(Clone)]
pub(crate) struct ProcessedTrack<'a> {
    pub title: String,
    pub position: String,
    pub duration_ms: Option<u64>,
    pub source_tracks: Vec<&'a crate::discogs::DiscogsTrack>,
    pub side: i32,
}

#[derive(Clone)]
struct CandidateLayout<'a> {
    tracks: Vec<ProcessedTrack<'a>>,
    expanded_groups: usize,
}

/// The source's leaf tracks, independent of a particular folder.
pub(crate) fn process_tracklist(
    tracklist: &[crate::discogs::DiscogsTrack],
) -> Vec<ProcessedTrack<'_>> {
    select_tracklist(tracklist, TrackLayoutTarget::Expanded)
}

/// The source layout whose playable rows best fit the candidate's audio. Count
/// decides first; ordered per-track durations decide between equal-count
/// layouts; an unresolved tie keeps the more expanded source description.
pub(crate) fn process_tracklist_for_audio<'a>(
    tracklist: &'a [crate::discogs::DiscogsTrack],
    audio_durations_ms: &[u64],
) -> Vec<ProcessedTrack<'a>> {
    select_tracklist(tracklist, TrackLayoutTarget::Audio(audio_durations_ms))
}

enum TrackLayoutTarget<'a> {
    Expanded,
    Audio(&'a [u64]),
}

impl TrackLayoutTarget<'_> {
    fn audio_from(&self, offset: usize) -> &[u64] {
        match self {
            Self::Expanded => &[],
            Self::Audio(audio) if offset < audio.len() => &audio[offset..],
            Self::Audio(_) => &[],
        }
    }
}

enum LayoutEntry<'a> {
    Index(&'a crate::discogs::DiscogsTrack),
    Heading {
        heading: &'a crate::discogs::DiscogsTrack,
        children: &'a [crate::discogs::DiscogsTrack],
    },
    Track(&'a crate::discogs::DiscogsTrack),
}

impl<'a> LayoutEntry<'a> {
    fn options(
        &self,
        target: &TrackLayoutTarget<'_>,
        audio_offset: usize,
    ) -> Vec<CandidateLayout<'a>> {
        match self {
            Self::Index(index) => index_layouts(index, target, audio_offset),
            Self::Heading { heading, children } => heading_layouts(heading, children),
            Self::Track(track) => vec![fixed_track_layout(track)],
        }
    }
}

fn select_tracklist<'a>(
    tracklist: &'a [crate::discogs::DiscogsTrack],
    target: TrackLayoutTarget<'_>,
) -> Vec<ProcessedTrack<'a>> {
    let layouts = candidate_layouts(tracklist, &target, 0);
    layouts
        .into_values()
        .min_by(|left, right| compare_layouts(left, right, &target))
        .expect("Discogs layout generation always yields a candidate")
        .tracks
}

fn compare_layouts(
    left: &CandidateLayout<'_>,
    right: &CandidateLayout<'_>,
    target: &TrackLayoutTarget<'_>,
) -> std::cmp::Ordering {
    match target {
        TrackLayoutTarget::Expanded => right
            .tracks
            .len()
            .cmp(&left.tracks.len())
            .then_with(|| right.expanded_groups.cmp(&left.expanded_groups)),
        TrackLayoutTarget::Audio(audio) => left
            .tracks
            .len()
            .abs_diff(audio.len())
            .cmp(&right.tracks.len().abs_diff(audio.len()))
            .then_with(|| compare_duration_fit(&left.tracks, &right.tracks, audio))
            .then_with(|| right.expanded_groups.cmp(&left.expanded_groups)),
    }
}

fn compare_duration_fit(
    left: &[ProcessedTrack<'_>],
    right: &[ProcessedTrack<'_>],
    audio: &[u64],
) -> std::cmp::Ordering {
    let score = |tracks: &[ProcessedTrack<'_>]| {
        tracks
            .iter()
            .zip(audio)
            .filter_map(|(track, local)| Some(track.duration_ms?.abs_diff(*local)))
            .fold((0u64, 0u128), |(known, error), difference| {
                (
                    known + 1,
                    error
                        .checked_add(u128::from(difference))
                        .expect("Discogs duration differences fit u128"),
                )
            })
    };
    let (left_known, left_error) = score(left);
    let (right_known, right_error) = score(right);
    match (left_known, right_known) {
        (0, 0) => std::cmp::Ordering::Equal,
        (0, _) => std::cmp::Ordering::Greater,
        (_, 0) => std::cmp::Ordering::Less,
        _ => left_error
            .checked_mul(u128::from(right_known))
            .expect("Discogs average duration comparison fits u128")
            .cmp(
                &right_error
                    .checked_mul(u128::from(left_known))
                    .expect("Discogs average duration comparison fits u128"),
            )
            .then_with(|| right_known.cmp(&left_known)),
    }
}

fn candidate_layouts<'a>(
    entries: &'a [crate::discogs::DiscogsTrack],
    target: &TrackLayoutTarget<'_>,
    audio_offset: usize,
) -> std::collections::BTreeMap<usize, CandidateLayout<'a>> {
    let mut layouts = std::collections::BTreeMap::from([(
        0,
        CandidateLayout {
            tracks: Vec::new(),
            expanded_groups: 0,
        },
    )]);

    let mut entry_index = 0;
    while entry_index < entries.len() {
        let entry = &entries[entry_index];
        let (layout_entry, consumed) = if entry.type_ == "index" && !entry.sub_tracks.is_empty() {
            (LayoutEntry::Index(entry), 1)
        } else if entry.type_ == "heading" && entry.title != "-" {
            let mut end = entry_index + 1;
            while end < entries.len()
                && entries[end].type_ == "track"
                && is_sub_track_position(&entries[end].position)
            {
                end += 1;
            }
            if end == entry_index + 1 {
                entry_index += 1;
                continue;
            }
            (
                LayoutEntry::Heading {
                    heading: entry,
                    children: &entries[entry_index + 1..end],
                },
                end - entry_index,
            )
        } else if entry.type_ == "track" {
            (LayoutEntry::Track(entry), 1)
        } else {
            entry_index += 1;
            continue;
        };
        let mut combined = std::collections::BTreeMap::new();
        for prefix in layouts.values() {
            for option in layout_entry.options(target, audio_offset + prefix.tracks.len()) {
                let mut tracks = prefix.tracks.clone();
                tracks.extend(option.tracks);
                keep_better_layout(
                    &mut combined,
                    CandidateLayout {
                        tracks,
                        expanded_groups: prefix.expanded_groups + option.expanded_groups,
                    },
                    target,
                    audio_offset,
                );
            }
        }
        layouts = combined;
        entry_index += consumed;
    }
    layouts
}

fn fixed_track_layout(entry: &crate::discogs::DiscogsTrack) -> CandidateLayout<'_> {
    CandidateLayout {
        tracks: vec![ProcessedTrack {
            title: entry.title.clone(),
            position: entry.position.clone(),
            duration_ms: entry.duration.as_deref().and_then(parse_duration_to_ms),
            source_tracks: vec![entry],
            side: parse_side_from_position(&entry.position),
        }],
        expanded_groups: 0,
    }
}

fn heading_layouts<'a>(
    heading: &'a crate::discogs::DiscogsTrack,
    children: &'a [crate::discogs::DiscogsTrack],
) -> Vec<CandidateLayout<'a>> {
    let expanded = CandidateLayout {
        tracks: children
            .iter()
            .map(|child| ProcessedTrack {
                title: format!("{}: {}", heading.title, child.title),
                position: child.position.clone(),
                duration_ms: child.duration.as_deref().and_then(parse_duration_to_ms),
                source_tracks: vec![child],
                side: parse_side_from_position(&child.position),
            })
            .collect(),
        expanded_groups: 1,
    };
    let position = extract_base_position(&children[0].position);
    let sources: Vec<&crate::discogs::DiscogsTrack> = children.iter().collect();
    let collapsed = CandidateLayout {
        tracks: vec![ProcessedTrack {
            title: format!(
                "{}: {}",
                heading.title,
                children
                    .iter()
                    .map(|track| track.title.as_str())
                    .collect::<Vec<_>>()
                    .join(" \u{2013} ")
            ),
            side: parse_side_from_position(&position),
            position,
            duration_ms: sum_track_durations(&sources),
            source_tracks: sources,
        }],
        expanded_groups: 0,
    };
    vec![collapsed, expanded]
}

fn keep_better_layout<'a>(
    layouts: &mut std::collections::BTreeMap<usize, CandidateLayout<'a>>,
    candidate: CandidateLayout<'a>,
    target: &TrackLayoutTarget<'_>,
    audio_offset: usize,
) {
    let count = candidate.tracks.len();
    match layouts.get(&count) {
        Some(current)
            if compare_duration_fit(
                &candidate.tracks,
                &current.tracks,
                target.audio_from(audio_offset),
            )
            .then_with(|| current.expanded_groups.cmp(&candidate.expanded_groups))
                != std::cmp::Ordering::Less => {}
        _ => {
            layouts.insert(count, candidate);
        }
    }
}

fn index_layouts<'a>(
    index: &'a crate::discogs::DiscogsTrack,
    target: &TrackLayoutTarget<'_>,
    audio_offset: usize,
) -> Vec<CandidateLayout<'a>> {
    let child_layouts = candidate_layouts(&index.sub_tracks, target, audio_offset);
    let expanded = child_layouts
        .into_values()
        .map(|mut layout| {
            for track in &mut layout.tracks {
                track.title = format!("{}: {}", index.title, track.title);
                track.source_tracks.insert(0, index);
            }
            layout.expanded_groups += 1;
            layout
        })
        .collect::<Vec<_>>();
    let source_tracks = leaf_tracks(&index.sub_tracks);
    if source_tracks.is_empty() {
        return expanded;
    }
    let position = if index.position.is_empty() {
        source_tracks
            .first()
            .map(|track| extract_base_position(&track.position))
            .expect("a grouped Discogs index with playable leaves has a first leaf")
    } else {
        index.position.clone()
    };
    let duration_ms = index
        .duration
        .as_deref()
        .and_then(parse_duration_to_ms)
        .or_else(|| sum_track_durations(&source_tracks));
    let mut collapsed_sources = Vec::with_capacity(source_tracks.len() + 1);
    collapsed_sources.push(index);
    collapsed_sources.extend(source_tracks);
    let collapsed = CandidateLayout {
        tracks: vec![ProcessedTrack {
            title: index.title.clone(),
            side: parse_side_from_position(&position),
            position,
            duration_ms,
            source_tracks: collapsed_sources,
        }],
        expanded_groups: 0,
    };
    std::iter::once(collapsed).chain(expanded).collect()
}

fn leaf_tracks(entries: &[crate::discogs::DiscogsTrack]) -> Vec<&crate::discogs::DiscogsTrack> {
    entries
        .iter()
        .flat_map(|entry| {
            if entry.type_ == "index" && !entry.sub_tracks.is_empty() {
                leaf_tracks(&entry.sub_tracks)
            } else if entry.type_ == "track" {
                vec![entry]
            } else {
                Vec::new()
            }
        })
        .collect()
}

fn sum_track_durations(tracks: &[&crate::discogs::DiscogsTrack]) -> Option<u64> {
    tracks
        .iter()
        .map(|track| track.duration.as_deref().and_then(parse_duration_to_ms))
        .sum()
}

/// Whether a flat position names a sub-track (`B1ii` or `1b`).
fn is_sub_track_position(position: &str) -> bool {
    let bytes = position.as_bytes();
    let Some(digit_start) = bytes.iter().position(u8::is_ascii_digit) else {
        return false;
    };
    if !bytes[..digit_start].iter().all(u8::is_ascii_alphabetic) {
        return false;
    }
    let mut digit_end = digit_start;
    while digit_end < bytes.len() && bytes[digit_end].is_ascii_digit() {
        digit_end += 1;
    }
    digit_end > digit_start
        && digit_end < bytes.len()
        && bytes[digit_end..].iter().all(u8::is_ascii_alphabetic)
}

/// Extract the playable parent position (`B1ii` -> `B1`, `1b` -> `1`).
fn extract_base_position(position: &str) -> String {
    let bytes = position.as_bytes();
    let Some(mut end) = bytes.iter().position(u8::is_ascii_digit) else {
        return position.to_string();
    };
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    position[..end].to_string()
}

pub(crate) fn parse_duration_to_ms(duration: &str) -> Option<u64> {
    let parts: Vec<&str> = duration.split(':').collect();
    match parts.as_slice() {
        [minutes, seconds] => {
            Some((minutes.parse::<u64>().ok()? * 60 + seconds.parse::<u64>().ok()?) * 1_000)
        }
        [hours, minutes, seconds] => Some(
            (hours.parse::<u64>().ok()? * 3_600
                + minutes.parse::<u64>().ok()? * 60
                + seconds.parse::<u64>().ok()?)
                * 1_000,
        ),
        _ => None,
    }
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

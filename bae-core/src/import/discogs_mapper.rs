use super::assemble::{
    assemble_parsed_album, AlbumArtistScope, ArtistRef, ReleaseIr, ReleaseRole, TrackEvent, TrackIr,
};
use super::ParsedAlbum;
use crate::db::{is_various_artists, Pressing};
use crate::discogs::{DiscogsArtist, DiscogsRelease, DiscogsRoleArtist, DiscogsTrack};
use crate::import::medium_coverage::MediumCoverage;
use crate::import::search::ReleaseTrack;
use crate::import::source_release::{
    ArtistCredit, CatalogFacts, EntryKind, RoleCredit, SourceMedium, SourceRelease, TracklistEntry,
};
use crate::import::{Catalog, ImportError, MetadataRef};
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
pub(crate) fn discogs_track_artist_ref(credit: &DiscogsArtist) -> ArtistRef {
    discogs_artist_ref(credit.name.clone(), Some(credit.id.clone()))
}

/// Album and pressing facts from this release, independently of its tracklist.
pub(crate) fn metadata(release: &DiscogsRelease) -> super::release_metadata::ReleaseMetadata {
    // With no artists list, fall back to the artist half of the "Artist - Album"
    // title split.
    let release_refs: Vec<ArtistRef> = if release.artists.is_empty() {
        match crate::discogs::split_title(&release.title).and_then(|(artist, _)| artist) {
            Some(name) => vec![discogs_artist_ref(name.to_owned(), None)],
            None => Vec::new(),
        }
    } else {
        release
            .artists
            .iter()
            .map(discogs_track_artist_ref)
            .collect()
    };

    super::release_metadata::ReleaseMetadata {
        album: super::release_metadata::AlbumMetadata {
            title: release.title.clone(),
            artists: release_refs,
            year: None,
        },
        pressing: pressing(release),
    }
}

/// The pressing fields shared by detail and imported metadata.
pub(crate) fn pressing(release: &DiscogsRelease) -> Pressing {
    let format = if release.format.is_empty() {
        None
    } else {
        Some(release.format.join(", "))
    };
    Pressing {
        year: release.year.map(|y| y as i32),
        format,
        label: release.label.first().cloned(),
        catalog_number: release.catno.clone(),
        country: release.country.clone(),
        barcode: release.barcode.clone(),
    }
}

/// The composer credits a Discogs release states for itself rather than for
/// one track. Positions come from source order, and non-composer roles are
/// skipped — so the positions keep holes.
pub(crate) fn release_roles(release: &DiscogsRelease) -> Vec<RoleCredit> {
    let Some(extraartists) = release.extraartists.as_ref() else {
        return Vec::new();
    };
    extraartists
        .iter()
        .enumerate()
        .filter_map(|(position, credit)| {
            if discogs_role_is_composer(&credit.role) {
                Some(RoleCredit {
                    position: position as i32,
                    artist: discogs_role_artist_ref(credit),
                    role: Some(credit.role.clone()),
                })
            } else {
                debug!(
                    discogs_release_id = %release.id,
                    artist_name = %credit.name,
                    role = %credit.role,
                    "Skipping Discogs release-level extraartist with non-composer role"
                );
                None
            }
        })
        .collect()
}

/// A Discogs release's mediums as bae keeps them: the tracklist split into the
/// runs of rows each disc's positions name, every row with its sub-tracks.
pub(crate) fn mediums(release: &DiscogsRelease) -> Vec<SourceMedium> {
    medium_tracklists(&release.tracklist)
        .iter()
        .map(|rows| SourceMedium {
            format: None,
            entries: tracklist_entries(&release.id, rows),
        })
        .collect()
}

fn tracklist_entries(release_id: &str, rows: &[DiscogsTrack]) -> Vec<TracklistEntry> {
    rows.iter()
        .filter_map(|row| {
            let Some(kind) = EntryKind::parse(&row.type_) else {
                debug!(
                    discogs_release_id = %release_id,
                    discogs_track_position = %row.position,
                    row_type = %row.type_,
                    "Skipping Discogs tracklist row of an unknown type"
                );
                return None;
            };
            Some(tracklist_entry(release_id, row, kind))
        })
        .collect()
}

fn tracklist_entry(release_id: &str, row: &DiscogsTrack, kind: EntryKind) -> TracklistEntry {
    let roles = match row.extraartists.as_ref() {
        Some(extraartists) => extraartists
            .iter()
            .enumerate()
            .filter_map(|(position, credit)| {
                if discogs_role_is_composer(&credit.role) {
                    Some(RoleCredit {
                        position: position as i32,
                        artist: discogs_role_artist_ref(credit),
                        role: Some(credit.role.clone()),
                    })
                } else {
                    debug!(
                        discogs_release_id = %release_id,
                        discogs_track_position = %row.position,
                        track_title = %row.title,
                        artist_name = %credit.name,
                        role = %credit.role,
                        "Skipping Discogs track-level extraartist with non-composer role"
                    );
                    None
                }
            })
            .collect(),
        None => {
            debug!(
                discogs_release_id = %release_id,
                discogs_track_position = %row.position,
                track_title = %row.title,
                "Discogs track has no extraartists field; skipping per-track role credits"
            );
            Vec::new()
        }
    };
    TracklistEntry {
        kind,
        // Discogs prints an unstated position as the empty string.
        position: (!row.position.is_empty()).then(|| row.position.clone()),
        number: None,
        title: Some(row.title.clone()),
        duration_ms: row.duration.as_deref().and_then(parse_duration_to_ms),
        credits: row
            .artists
            .iter()
            .enumerate()
            .map(|(position, artist)| ArtistCredit {
                position: position as i32,
                credited_name: artist.name.clone(),
                artist: Some(discogs_track_artist_ref(artist)),
            })
            .collect(),
        roles,
        works: Vec::new(),
        children: tracklist_entries(release_id, &row.sub_tracks),
    }
}

/// The position a Discogs row prints, the empty string where it prints none.
fn position_of(entry: &TracklistEntry) -> &str {
    entry.position.as_deref().unwrap_or("")
}

/// The title a Discogs row prints; every Discogs row states one.
fn title_of(entry: &TracklistEntry) -> &str {
    entry
        .title
        .as_deref()
        .expect("a Discogs tracklist row states its title")
}

/// The release formats of a stored Discogs release.
fn formats(release: &SourceRelease) -> &[String] {
    match &release.catalog {
        CatalogFacts::Discogs { formats, .. } => formats,
        CatalogFacts::MusicBrainz { .. } => {
            unreachable!("a Discogs reading is asked only of a Discogs release")
        }
    }
}

/// The picker's tracklist for the covered mediums of a Discogs release, laid
/// out against the measured lengths.
pub(crate) fn detail_tracks(
    release: &SourceRelease,
    coverage: &MediumCoverage,
    audio_durations_ms: &[u64],
) -> Vec<ReleaseTrack> {
    let entries = release.covered_entries(coverage);
    process_tracklist(&entries, Some(audio_durations_ms))
        .iter()
        .map(|pt| ReleaseTrack {
            title: pt.title.clone(),
            artist: pt
                .source_tracks
                .iter()
                .find_map(|track| track.credits.first())
                .map(|credit| credit.credited_name.clone()),
            duration_ms: pt.duration_ms,
            position: pt.position.clone(),
            side: release_track_side(formats(release), pt).map(|side| side as u32),
        })
        .collect()
}

pub(crate) fn map(
    release: &SourceRelease,
    coverage: &MediumCoverage,
    audio_durations_ms: Option<&[u64]>,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<ParsedAlbum, ImportError> {
    let release_id = &release.release.key;
    let entries = release.covered_entries(coverage);
    let processed = process_tracklist(&entries, audio_durations_ms);
    let mut metadata = release.metadata.clone();
    let primary_artist = metadata.album.take_primary(Catalog::Discogs, release_id)?;
    let is_compilation = is_various_artists(&primary_artist.name);

    let release_roles = match &release.catalog {
        CatalogFacts::Discogs { release_roles, .. } => release_roles
            .iter()
            .map(|role| ReleaseRole {
                position: role.position,
                artist: role.artist.clone(),
                source: Catalog::Discogs,
                source_credit: role.role.clone(),
            })
            .collect(),
        CatalogFacts::MusicBrainz { .. } => {
            unreachable!("a Discogs reading is asked only of a Discogs release")
        }
    };

    let tracks: Vec<TrackIr> = processed
        .iter()
        .map(|pt| discogs_track_ir(formats(release), pt))
        .collect();

    let ir = ReleaseIr {
        album_title: metadata.album.title,
        primary_artist,
        additional_artists: metadata.album.artists,
        album_year: metadata.album.year,
        is_compilation,
        pressing: metadata.pressing,
        metadata_provenance: Some(crate::import::MetadataProvenance::ExternalRelease {
            record: MetadataRef::new(Catalog::Discogs, release_id.clone()),
            // The mapper reads one release; what else the pick claimed is
            // the picker's to say, and reaches the library as records.
            partners: Vec::new(),
        }),
        album_artist_scope: AlbumArtistScope::ReleaseCredits,
        release_roles,
        tracks,
    };

    Ok(assemble_parsed_album(ir, clock, ids))
}

/// Build one track's IR from a processed Discogs track. Role credits precede
/// display credits per source row (preserving the artist-pool discovery order);
/// display credits are deduped across a collapsed track's source rows by Discogs
/// artist id, first occurrence wins, with positions compacted `0..n`.
fn discogs_track_ir(formats: &[String], pt: &ProcessedTrack) -> TrackIr {
    let mut events: Vec<TrackEvent> = Vec::new();
    let mut seen_credit_ids: HashSet<Option<String>> = HashSet::new();
    let mut credit_position = 0i32;

    for discogs_track in &pt.source_tracks {
        for role in &discogs_track.roles {
            events.push(TrackEvent::Role {
                position: role.position,
                artist: role.artist.clone(),
                source: Catalog::Discogs,
                source_credit: role.role.clone(),
            });
        }

        for artist in discogs_track
            .credits
            .iter()
            .filter_map(|credit| credit.artist.as_ref())
        {
            if seen_credit_ids.insert(artist.discogs_artist_id.clone()) {
                events.push(TrackEvent::Credit {
                    position: credit_position,
                    artist: artist.clone(),
                });
                credit_position += 1;
            }
        }
    }

    TrackIr {
        title: pt.title.clone(),
        side: release_track_side(formats, pt),
        number: super::assemble::position_number(&pt.position),
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
    pub source_tracks: Vec<&'a TracklistEntry>,
    pub side: Option<i32>,
}

#[derive(Clone)]
struct CandidateLayout<'a> {
    tracks: Vec<ProcessedTrack<'a>>,
    expanded_groups: usize,
}

/// The source layout whose playable rows best fit `audio_durations_ms`: count
/// decides first, ordered per-track durations decide between equal-count
/// layouts, and an unresolved tie keeps the more expanded source description.
/// `None` — no folder to fit — takes the source's leaf tracks.
pub(crate) fn process_tracklist<'a>(
    tracklist: &'a [TracklistEntry],
    audio_durations_ms: Option<&[u64]>,
) -> Vec<ProcessedTrack<'a>> {
    let layouts = candidate_layouts(tracklist, audio_durations_ms, 0);
    layouts
        .into_values()
        .min_by(|left, right| compare_layouts(left, right, audio_durations_ms))
        .expect("Discogs layout generation always yields a candidate")
        .tracks
}

/// The durations still unmatched once a layout starts at `offset`.
fn audio_from(audio: Option<&[u64]>, offset: usize) -> &[u64] {
    match audio {
        Some(audio) if offset < audio.len() => &audio[offset..],
        _ => &[],
    }
}

enum LayoutEntry<'a> {
    Index(&'a TracklistEntry),
    Heading {
        heading: &'a TracklistEntry,
        children: &'a [TracklistEntry],
    },
    Track(&'a TracklistEntry),
}

impl<'a> LayoutEntry<'a> {
    fn options(&self, audio: Option<&[u64]>, audio_offset: usize) -> Vec<CandidateLayout<'a>> {
        match self {
            Self::Index(index) => index_layouts(index, audio, audio_offset),
            Self::Heading { heading, children } => heading_layouts(heading, children),
            Self::Track(track) => vec![fixed_track_layout(track)],
        }
    }
}

fn compare_layouts(
    left: &CandidateLayout<'_>,
    right: &CandidateLayout<'_>,
    audio: Option<&[u64]>,
) -> std::cmp::Ordering {
    match audio {
        None => right
            .tracks
            .len()
            .cmp(&left.tracks.len())
            .then_with(|| right.expanded_groups.cmp(&left.expanded_groups)),
        Some(audio) => left
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
    entries: &'a [TracklistEntry],
    audio: Option<&[u64]>,
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
        let (layout_entry, consumed) = if entry.kind == EntryKind::Index && !entry.children.is_empty() {
            (LayoutEntry::Index(entry), 1)
        } else if entry.kind == EntryKind::Heading && title_of(entry) != "-" {
            let mut end = entry_index + 1;
            while end < entries.len()
                && entries[end].kind == EntryKind::Track
                && is_sub_track_position(position_of(&entries[end]))
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
        } else if entry.kind == EntryKind::Track {
            (LayoutEntry::Track(entry), 1)
        } else {
            entry_index += 1;
            continue;
        };
        let mut combined = std::collections::BTreeMap::new();
        for prefix in layouts.values() {
            for option in layout_entry.options(audio, audio_offset + prefix.tracks.len()) {
                let mut tracks = prefix.tracks.clone();
                tracks.extend(option.tracks);
                keep_better_layout(
                    &mut combined,
                    CandidateLayout {
                        tracks,
                        expanded_groups: prefix.expanded_groups + option.expanded_groups,
                    },
                    audio,
                    audio_offset,
                );
            }
        }
        layouts = combined;
        entry_index += consumed;
    }
    layouts
}

fn fixed_track_layout(entry: &TracklistEntry) -> CandidateLayout<'_> {
    CandidateLayout {
        tracks: vec![ProcessedTrack {
            title: title_of(entry).to_string(),
            position: position_of(entry).to_string(),
            duration_ms: entry.duration_ms,
            source_tracks: vec![entry],
            side: parse_side_from_position(position_of(entry)),
        }],
        expanded_groups: 0,
    }
}

fn heading_layouts<'a>(
    heading: &'a TracklistEntry,
    children: &'a [TracklistEntry],
) -> Vec<CandidateLayout<'a>> {
    let expanded = CandidateLayout {
        tracks: children
            .iter()
            .map(|child| ProcessedTrack {
                title: format!("{}: {}", title_of(heading), title_of(child)),
                position: position_of(child).to_string(),
                duration_ms: child.duration_ms,
                source_tracks: vec![child],
                side: parse_side_from_position(position_of(child)),
            })
            .collect(),
        expanded_groups: 1,
    };
    let position = extract_base_position(position_of(&children[0]));
    let sources: Vec<&TracklistEntry> = children.iter().collect();
    let collapsed = CandidateLayout {
        tracks: vec![ProcessedTrack {
            title: format!(
                "{}: {}",
                title_of(heading),
                children
                    .iter()
                    .map(title_of)
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
    audio: Option<&[u64]>,
    audio_offset: usize,
) {
    let count = candidate.tracks.len();
    match layouts.get(&count) {
        Some(current)
            if compare_duration_fit(
                &candidate.tracks,
                &current.tracks,
                audio_from(audio, audio_offset),
            )
            .then_with(|| current.expanded_groups.cmp(&candidate.expanded_groups))
                != std::cmp::Ordering::Less => {}
        _ => {
            layouts.insert(count, candidate);
        }
    }
}

fn index_layouts<'a>(
    index: &'a TracklistEntry,
    audio: Option<&[u64]>,
    audio_offset: usize,
) -> Vec<CandidateLayout<'a>> {
    let child_layouts = candidate_layouts(&index.children, audio, audio_offset);
    let expanded = child_layouts
        .into_values()
        .map(|mut layout| {
            for track in &mut layout.tracks {
                track.title = format!("{}: {}", title_of(index), track.title);
                track.source_tracks.insert(0, index);
            }
            layout.expanded_groups += 1;
            layout
        })
        .collect::<Vec<_>>();
    let source_tracks = leaf_tracks(&index.children);
    if source_tracks.is_empty() {
        return expanded;
    }
    let position = if position_of(index).is_empty() {
        source_tracks
            .first()
            .map(|track| extract_base_position(position_of(track)))
            .expect("a grouped Discogs index with playable leaves has a first leaf")
    } else {
        position_of(index).to_string()
    };
    let duration_ms = index
        .duration_ms
        .or_else(|| sum_track_durations(&source_tracks));
    let mut collapsed_sources = Vec::with_capacity(source_tracks.len() + 1);
    collapsed_sources.push(index);
    collapsed_sources.extend(source_tracks);
    let collapsed = CandidateLayout {
        tracks: vec![ProcessedTrack {
            title: title_of(index).to_string(),
            side: parse_side_from_position(&position),
            position,
            duration_ms,
            source_tracks: collapsed_sources,
        }],
        expanded_groups: 0,
    };
    std::iter::once(collapsed).chain(expanded).collect()
}

fn leaf_tracks(entries: &[TracklistEntry]) -> Vec<&TracklistEntry> {
    entries
        .iter()
        .flat_map(|entry| {
            if entry.kind == EntryKind::Index && !entry.children.is_empty() {
                leaf_tracks(&entry.children)
            } else if entry.kind == EntryKind::Track {
                vec![entry]
            } else {
                Vec::new()
            }
        })
        .collect()
}

fn sum_track_durations(tracks: &[&TracklistEntry]) -> Option<u64> {
    tracks
        .iter()
        .map(|track| track.duration_ms)
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

/// A Discogs tracklist's mediums: the runs of rows whose positions name one
/// disc (`1-1`, `1-2`, then `2-1`). A tracklist that numbers no discs is one
/// medium, and a row naming no disc — a heading, an index — belongs with the
/// disc of the first numbered row after it.
pub(crate) fn medium_tracklists(
    tracklist: &[DiscogsTrack],
) -> Vec<Vec<DiscogsTrack>> {
    let disc_of = |track: &DiscogsTrack| -> Option<i32> {
        let (disc, _) = track.position.split_once('-')?;
        disc.parse::<i32>().ok().filter(|disc| *disc > 0)
    };
    if !tracklist.iter().any(|track| disc_of(track).is_some()) {
        return vec![tracklist.to_vec()];
    }
    let mut mediums: Vec<(i32, Vec<DiscogsTrack>)> = Vec::new();
    let mut unnumbered: Vec<DiscogsTrack> = Vec::new();
    for track in tracklist {
        match disc_of(track) {
            Some(disc) => {
                match mediums.last_mut() {
                    Some((current, rows)) if *current == disc => rows.append(&mut unnumbered),
                    _ => mediums.push((disc, std::mem::take(&mut unnumbered))),
                }
                mediums
                    .last_mut()
                    .expect("a numbered row has a medium")
                    .1
                    .push(track.clone());
            }
            None => unnumbered.push(track.clone()),
        }
    }
    mediums
        .last_mut()
        .expect("a numbered row has a medium")
        .1
        .append(&mut unnumbered);
    mediums.into_iter().map(|(_, rows)| rows).collect()
}

/// A known CD contains one side even when its track positions omit a disc.
pub(crate) fn release_track_side(formats: &[String], track: &ProcessedTrack) -> Option<i32> {
    track.side.or_else(|| {
        formats
            .iter()
            .any(|format| format.contains("CD"))
            .then_some(1)
    })
}

/// The side a Discogs position string names: for vinyl (`A1`, `B2`, `C1`) the
/// letter is the side (A=1, B=2, ...); for CD (`1-1`, `2-1`) the disc number is;
/// a plain number (`1`, `2`) leaves the side unknown.
pub fn parse_side_from_position(position: &str) -> Option<i32> {
    if let Some(dash_idx) = position.find('-') {
        if let Ok(disc) = position[..dash_idx].parse::<i32>() {
            if disc <= 0 {
                return None;
            }
            return Some(disc);
        }
    }

    if let Some(first_char) = position.chars().next() {
        if first_char.is_ascii_alphabetic()
            && position[1..].bytes().all(|byte| byte.is_ascii_digit())
        {
            return Some((first_char.to_ascii_uppercase() as i32) - ('A' as i32) + 1);
        }
    }

    None
}

#[cfg(test)]
#[path = "discogs_mapper_tests.rs"]
mod tests;

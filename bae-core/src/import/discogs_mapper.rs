use crate::import::medium_coverage::MediumCoverage;
use crate::import::release_values::{position_number, ReleaseValues, TrackCredits, TrackValues};
use crate::import::source_release::{
    CatalogFacts, DisplayCredit, EntryKind, RoleCredit, SourceMedium, SourceRelease, TracklistEntry,
};
use crate::import::{ArtistCredit, CreditedRole, ImportError};
use bae_catalogs::catalog::Catalog;
use bae_catalogs::discogs::{DiscogsArtist, DiscogsRelease, DiscogsRoleArtist, DiscogsTrack};
use bae_types::pressing::{Medium, Pressing, PressingFacts, ReleaseArea, StatedMedia};
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

/// The credit of a Discogs artist: the display name doubles as its own
/// sort name, and the Discogs artist id (when present) is what dedups the pool.
/// Discogs artists carry no MusicBrainz id.
fn discogs_artist_ref(name: String, discogs_artist_id: Option<String>) -> ArtistCredit {
    ArtistCredit {
        name: name.clone(),
        sort_name: Some(name),
        musicbrainz_artist_id: None,
        discogs_artist_id,
        credited_as: None,
    }
}

/// What a Discogs `join` prints between two credits: a comma as `", "`, any
/// other word or sign with a space either side (`&` → `" & "`). `None` where
/// Discogs states none.
fn discogs_join(join: Option<&str>) -> Option<String> {
    let join = join.map(str::trim).filter(|join| !join.is_empty())?;
    Some(if join == "," {
        ", ".to_string()
    } else {
        format!(" {join} ")
    })
}

/// A Discogs display credit list, place by place, with each artist's printed
/// name and the joins between them; the last credit's join is not kept.
pub(crate) fn discogs_credits(artists: &[DiscogsArtist]) -> Vec<crate::import::SourceCredit> {
    let mut stated: Vec<crate::import::SourceCredit> = artists
        .iter()
        .map(|artist| crate::import::SourceCredit {
            credit: discogs_track_artist_ref(artist),
            join: discogs_join(artist.join.as_deref()),
        })
        .collect();
    crate::credits::drop_last_join(&mut stated, |credit| &mut credit.join);
    stated
}

/// The credit of a Discogs role artist. The display name is the
/// `credited_name`, falling back to the canonical name (logged) when absent.
fn discogs_role_artist_ref(credit: &DiscogsRoleArtist) -> ArtistCredit {
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

/// The credit of a Discogs display artist, keyed on its canonical name, with
/// the name the release prints for it where that is another.
pub(crate) fn discogs_track_artist_ref(credit: &DiscogsArtist) -> ArtistCredit {
    ArtistCredit {
        credited_as: credit
            .anv
            .as_deref()
            .map(str::trim)
            .filter(|anv| !anv.is_empty() && *anv != credit.name)
            .map(str::to_string),
        ..discogs_artist_ref(credit.name.clone(), Some(credit.id.clone()))
    }
}

/// Album and pressing facts from this release, independently of its tracklist.
pub(crate) fn metadata(release: &DiscogsRelease) -> super::release_metadata::ReleaseMetadata {
    // With no artists list, fall back to the artist half of the "Artist - Album"
    // title split.
    let release_refs: Vec<crate::import::SourceCredit> = if release.artists.is_empty() {
        match bae_catalogs::discogs::split_title(&release.title).and_then(|(artist, _)| artist) {
            Some(name) => vec![crate::import::SourceCredit::plain(discogs_artist_ref(
                name.to_owned(),
                None,
            ))],
            None => Vec::new(),
        }
    } else {
        discogs_credits(&release.artists)
    };

    super::release_metadata::ReleaseMetadata {
        album: super::release_metadata::AlbumMetadata {
            title: release.title.clone(),
            artists: release_refs,
            year: None,
            first_year: None,
        },
        pressing: pressing(release).0,
    }
}

/// The pressing a Discogs release describes, shared by detail and imported
/// metadata, and its media in the release's own shape.
pub(crate) fn pressing(release: &DiscogsRelease) -> (Pressing, StatedMedia) {
    let formats = bae_catalogs::discogs::formats::read(&release.id, &release.formats);
    let pressing = Pressing {
        year: release.year.map(|y| y as i32),
        labels: release.labels.clone(),
        barcodes: release.barcodes.clone(),
        facts: PressingFacts {
            area: release
                .country
                .as_deref()
                .and_then(|name| area(&release.id, name)),
            media: formats.media.counts(),
            status: formats.status,
            packaging: formats.packaging,
            discogs_details: formats.details,
        },
    };
    (pressing, formats.media)
}

/// The area a Discogs release's `country` names. A name outside Discogs's
/// list is logged and left out: the list is what needs it.
pub(crate) fn area(release_id: &str, name: &str) -> Option<ReleaseArea> {
    let area = ReleaseArea::discogs(name);
    if area.is_none() {
        warn!(
            discogs_release_id = release_id,
            country = name,
            "a Discogs release country is outside Discogs's country list"
        );
    }
    area
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
/// mediums its positions name, each with the carrier the release's formats
/// give it (`media`), every row with its sub-tracks.
pub(crate) fn mediums(release: &DiscogsRelease, media: &StatedMedia) -> Vec<SourceMedium> {
    crate::import::discogs_mediums::split(&release.id, &release.tracklist, media)
        .into_iter()
        .map(|medium| SourceMedium {
            medium: medium.medium,
            disc_ids: Vec::new(),
            entries: tracklist_entries(&release.id, &medium.rows),
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
        credits: discogs_credits(&row.artists)
            .into_iter()
            .enumerate()
            .map(|(position, stated)| DisplayCredit {
                position: position as i32,
                credited_name: stated.credit.printed().to_string(),
                artist: Some(stated.credit),
                join: stated.join,
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

/// What the covered mediums of a Discogs release say the audio's tracks are,
/// its tracklist laid out against `audio_durations_ms`; with none, every
/// index and heading read out to its sub-tracks.
pub(crate) fn values(
    release: &SourceRelease,
    coverage: &MediumCoverage,
    audio_durations_ms: Option<&[u64]>,
) -> Result<ReleaseValues, ImportError> {
    let release_id = &release.release.key;
    let covered: Vec<&crate::import::source_release::SourceMedium> =
        release.covered_mediums(coverage).collect();
    let entries = release.covered_entries(coverage);
    let entry_mediums: Vec<usize> = covered
        .iter()
        .enumerate()
        .flat_map(|(index, medium)| std::iter::repeat_n(index, medium.entries.len()))
        .collect();
    let processed = process_tracklist(&entries, audio_durations_ms);
    let covered_layout = covered_layout(&covered, &entries, &entry_mediums, &processed);
    let metadata = release.metadata.clone();
    metadata
        .album
        .require_artists(Catalog::Discogs, release_id)?;

    let release_roles = match &release.catalog {
        CatalogFacts::Discogs { release_roles, .. } => release_roles
            .iter()
            .map(|role| CreditedRole {
                artist: role.artist.clone(),
                source: Catalog::Discogs,
                role: role.role.clone(),
            })
            .collect(),
        CatalogFacts::MusicBrainz { .. } => {
            unreachable!("a Discogs reading is asked only of a Discogs release")
        }
    };

    Ok(ReleaseValues {
        album_title: metadata.album.title,
        album_credits: metadata.album.artists,
        album_year: metadata.album.year,
        pressing: metadata.pressing,
        mediums: covered_layout.mediums,
        release_roles,
        works: Default::default(),
        tracks: processed
            .iter()
            .zip(covered_layout.places)
            .map(|(pt, place)| discogs_track_values(pt, place))
            .collect(),
    })
}

/// The mediums the processed tracks are on and each track's place: a track
/// is on the medium of the row it was read from, on the side its position's
/// letter names on a record or cassette (counted from the medium's lowest
/// letter; a track with no letter is on the side of the one before it), and
/// numbered by its position, else by its place on that side.
struct CoveredLayout {
    mediums: Vec<crate::mediums::ReleaseMedium>,
    places: Vec<crate::mediums::TrackPlace>,
}

fn covered_layout(
    covered: &[&crate::import::source_release::SourceMedium],
    entries: &[TracklistEntry],
    entry_mediums: &[usize],
    processed: &[ProcessedTrack<'_>],
) -> CoveredLayout {
    fn holds(entry: &TracklistEntry, target: &TracklistEntry) -> bool {
        std::ptr::eq(entry, target) || entry.children.iter().any(|child| holds(child, target))
    }
    let track_mediums: Vec<usize> = processed
        .iter()
        .map(|pt| {
            let first = pt
                .source_tracks
                .first()
                .expect("a processed Discogs track is read from a row");
            entries
                .iter()
                .zip(entry_mediums)
                .find(|(entry, _)| holds(entry, first))
                .map(|(_, medium)| *medium)
                .expect("a processed Discogs track is read from a covered row")
        })
        .collect();
    let letters: Vec<Option<u8>> = processed
        .iter()
        .map(|pt| {
            let letter = *pt.position.as_bytes().first()?;
            (letter.is_ascii_alphabetic()
                && pt.position[1..]
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_digit()))
            .then(|| letter.to_ascii_uppercase())
        })
        .collect();
    let mut mediums = Vec::with_capacity(covered.len());
    let mut sides = vec![std::num::NonZeroU8::MIN; processed.len()];
    for (index, medium) in covered.iter().enumerate() {
        let on_medium: Vec<usize> = (0..processed.len())
            .filter(|track| track_mediums[*track] == index)
            .collect();
        let sided = medium.medium.is_some_and(Medium::is_sided);
        let base = on_medium.iter().filter_map(|track| letters[*track]).min();
        let mut count = std::num::NonZeroU8::MIN;
        if let (true, Some(base)) = (sided, base) {
            let mut current = std::num::NonZeroU8::MIN;
            for track in &on_medium {
                if let Some(letter) = letters[*track] {
                    current = crate::mediums::side_number(usize::from(letter - base));
                }
                sides[*track] = current;
                count = count.max(current);
            }
        }
        mediums.push(crate::mediums::ReleaseMedium {
            carrier: medium.medium,
            sides: count,
        });
    }
    let mut placer = crate::mediums::Placer::default();
    let places = processed
        .iter()
        .enumerate()
        .map(|(track, pt)| {
            placer.place(
                crate::mediums::medium_position(track_mediums[track]),
                sides[track],
                position_number(&pt.position),
            )
        })
        .collect();
    CoveredLayout { mediums, places }
}

/// One track's values from a processed Discogs track at `place`. A collapsed
/// track holds every source row's composers, and its display artists once each
/// by Discogs artist id, first occurrence (and its join) winning.
fn discogs_track_values(pt: &ProcessedTrack, place: crate::mediums::TrackPlace) -> TrackValues {
    let mut roles = Vec::new();
    let mut credits: Vec<crate::import::SourceCredit> = Vec::new();
    let mut seen_credit_ids: HashSet<Option<String>> = HashSet::new();
    for discogs_track in &pt.source_tracks {
        roles.extend(discogs_track.roles.iter().map(|role| CreditedRole {
            artist: role.artist.clone(),
            source: Catalog::Discogs,
            role: role.role.clone(),
        }));
        for stated in crate::import::source_release::stated_credits(&discogs_track.credits) {
            if seen_credit_ids.insert(stated.credit.discogs_artist_id.clone()) {
                credits.push(stated);
            }
        }
    }
    crate::credits::drop_last_join(&mut credits, |credit| &mut credit.join);
    TrackValues {
        title: pt.title.clone(),
        place,
        stated_length_ms: pt.duration_ms,
        credits: if credits.is_empty() {
            TrackCredits::AlbumArtists
        } else {
            TrackCredits::Own(credits)
        },
        roles,
        works: Vec::new(),
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

/// Every number of tracks a layout of `entries` gives, each index and heading
/// either collapsed into one track or read out to its sub-tracks: what a
/// reading weighs a Discogs medium by. A medium that lists no playable row
/// gives zero alone.
pub(crate) fn layout_counts(entries: &[TracklistEntry]) -> std::collections::BTreeSet<usize> {
    candidate_layouts(entries, None, 0).into_keys().collect()
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
        let (layout_entry, consumed) =
            if entry.kind == EntryKind::Index && !entry.children.is_empty() {
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
    tracks.iter().map(|track| track.duration_ms).sum()
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

#[cfg(test)]
#[path = "discogs_mapper_tests.rs"]
mod tests;

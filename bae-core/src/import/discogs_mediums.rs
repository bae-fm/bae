//! A Discogs tracklist split into the mediums its positions name, each with
//! its carrier.
//!
//! Discogs lists one flat tracklist for a whole release and states the
//! release's media as format entries with quantities. The positions say
//! where one medium ends and the next begins:
//!
//! - `<n>-<m>` is track *m* of disc *n*;
//! - `<PREFIX><n>-<m>`, `<PREFIX>-<m>` and `<PREFIX><m>`, with a prefix from
//!   [`PREFIXES`], are on disc *n* of the carrier the prefix names, or on its
//!   only disc;
//! - `<L><m>`, `<L>` with one letter, are on side *L* of a record or a
//!   cassette;
//! - `<m>` alone, and anything else, names no medium.
//!
//! Each may carry sub-track letters (`1-10a`, `B1ii`). A row whose position
//! names no medium — a heading, an index with no position, a plain number —
//! belongs with the medium of the first row after it that names one, and rows
//! after the last such row with the last medium.

use bae_catalogs::discogs::DiscogsTrack;
use bae_types::pressing::{Medium, StatedMedia};
use tracing::debug;

/// The position prefixes Discogs editors write before a disc's track
/// numbers, each with the carrier it names.
const PREFIXES: &[(&str, Medium)] = &[
    ("CD", Medium::Cd),
    ("DVD", Medium::Dvd),
    ("BD", Medium::BluRay),
    ("SACD", Medium::Sacd),
    ("VHS", Medium::VideoTape),
    ("LD", Medium::LaserDisc),
];

/// One medium of a Discogs release: its carrier, `None` where the formats do
/// not say which, and its rows in tracklist order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MediumRows {
    pub(crate) medium: Option<Medium>,
    pub(crate) rows: Vec<DiscogsTrack>,
}

/// The mediums of a Discogs release, in tracklist order. `media` is what the
/// release's format entries state. A tracklist with no rows has no mediums;
/// one whose positions name no medium is one medium.
pub(crate) fn split(
    release_id: &str,
    tracklist: &[DiscogsTrack],
    media: &StatedMedia,
) -> Vec<MediumRows> {
    let runs = runs(tracklist);
    let carriers = carriers(release_id, &runs, media);
    runs.into_iter()
        .zip(carriers)
        .map(|(run, medium)| MediumRows {
            medium,
            rows: run.rows,
        })
        .collect()
}

/// What a row's position says about the medium it is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// Disc *n* of the release's numbered discs.
    Disc(u32),
    /// A disc of the carrier a prefix names: disc *n*, or its only one.
    Prefixed { medium: Medium, disc: Option<u32> },
    /// A side of a record or cassette, `0` for side A.
    Side(u8),
}

/// The run of rows one medium holds while the tracklist is split.
struct Run {
    kind: RunKind,
    rows: Vec<DiscogsTrack>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RunKind {
    Disc(u32),
    Prefixed { medium: Medium, disc: Option<u32> },
    /// A record or cassette and the sides its rows have named so far, in
    /// order: one or two.
    Sided(Vec<u8>),
    /// A tracklist whose positions name no medium.
    Unnumbered,
}

impl RunKind {
    fn of(place: Place) -> Self {
        match place {
            Place::Disc(disc) => Self::Disc(disc),
            Place::Prefixed { medium, disc } => Self::Prefixed { medium, disc },
            Place::Side(side) => Self::Sided(vec![side]),
        }
    }

    /// Takes a row at `place` into this run, or says it starts the next one.
    /// A record holds two sides, the second the letter after the first; a
    /// letter that does not follow, or a third side, is the next record — a
    /// box whose every record restarts at A splits per record.
    fn take(&mut self, place: Place) -> bool {
        match (self, place) {
            (Self::Disc(current), Place::Disc(disc)) => *current == disc,
            (
                Self::Prefixed { medium, disc },
                Place::Prefixed {
                    medium: row_medium,
                    disc: row_disc,
                },
            ) => *medium == row_medium && *disc == row_disc,
            (Self::Sided(sides), Place::Side(side)) => {
                let last = *sides.last().expect("a sided run has named a side");
                if side == last {
                    true
                } else if sides.len() == 1 && last.checked_add(1) == Some(side) {
                    sides.push(side);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

fn runs(tracklist: &[DiscogsTrack]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    let mut unplaced: Vec<DiscogsTrack> = Vec::new();
    for row in tracklist {
        let Some(place) = row_place(row) else {
            unplaced.push(row.clone());
            continue;
        };
        let continues = runs.last_mut().is_some_and(|run| run.kind.take(place));
        if !continues {
            runs.push(Run {
                kind: RunKind::of(place),
                rows: Vec::new(),
            });
        }
        let run = runs.last_mut().expect("a placed row has a run");
        run.rows.append(&mut unplaced);
        run.rows.push(row.clone());
    }
    match runs.last_mut() {
        Some(run) => run.rows.append(&mut unplaced),
        None if !unplaced.is_empty() => runs.push(Run {
            kind: RunKind::Unnumbered,
            rows: unplaced,
        }),
        None => {}
    }
    runs
}

/// Where a row is: its own position's place, or for an index that prints
/// none, where its first placed sub-track is.
fn row_place(row: &DiscogsTrack) -> Option<Place> {
    if row.position.is_empty() {
        return row.sub_tracks.iter().find_map(row_place);
    }
    let place = place(&row.position);
    // `<m>` alone is a track of whichever medium it is on; anything else
    // that names no medium is a position the grammar does not read.
    if place.is_none() && !is_track_number(&row.position) {
        debug!(
            discogs_track_position = %row.position,
            "a Discogs position names no medium; the row stays with its neighbours"
        );
    }
    place
}

/// The place a Discogs position names, per the grammar in the module
/// documentation.
fn place(position: &str) -> Option<Place> {
    let letters_end = position
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(position.len());
    let (letters, rest) = position.split_at(letters_end);
    match letters.len() {
        0 => {
            let (disc, track) = rest.split_once('-')?;
            if !is_track_number(track) {
                return None;
            }
            disc_number(disc).map(Place::Disc)
        }
        1 => (rest.is_empty() || is_track_number(rest))
            .then(|| Place::Side(letters.as_bytes()[0].to_ascii_uppercase() - b'A')),
        _ => {
            let medium = PREFIXES
                .iter()
                .find(|(prefix, _)| prefix.eq_ignore_ascii_case(letters))
                .map(|(_, medium)| *medium)?;
            let (disc, track) = match rest.split_once('-') {
                Some(("", track)) => (None, track),
                Some((disc, track)) => (Some(disc_number(disc)?), track),
                None => (None, rest),
            };
            is_track_number(track).then_some(Place::Prefixed { medium, disc })
        }
    }
}

/// A disc number: digits naming disc one or later.
fn disc_number(text: &str) -> Option<u32> {
    (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| text.parse::<u32>().ok())
        .flatten()
        .filter(|disc| *disc > 0)
}

/// A track number, with the sub-track letters Discogs may print after it.
fn is_track_number(text: &str) -> bool {
    let digits_end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    digits_end > 0 && text[digits_end..].bytes().all(|byte| byte.is_ascii_alphabetic())
}

/// Each run's carrier, from the release's format entries.
///
/// The entries name carriers with quantities; each unit is one medium. A
/// prefixed run is the carrier its prefix names and accounts for one unit of
/// it. Sided runs take the sided units in format order, and the other runs
/// the remaining units — every remaining unit when the release has no sided
/// run, since a record whose positions print no letters is still a record.
/// Where a group's count and its units' count differ, the group takes the
/// one carrier its units name, or none when they name several.
fn carriers(release_id: &str, runs: &[Run], media: &StatedMedia) -> Vec<Option<Medium>> {
    let mut units: Vec<Option<Medium>> = match media {
        StatedMedia::Undescribed => Vec::new(),
        StatedMedia::Formats(formats) => formats
            .iter()
            .flat_map(|format| {
                std::iter::repeat_n(format.medium, format.quantity as usize)
            })
            .collect(),
        StatedMedia::PerMedium(_) => {
            unreachable!("a Discogs release states its media as format entries")
        }
    };
    let mut carriers: Vec<Option<Medium>> = vec![None; runs.len()];
    for (index, run) in runs.iter().enumerate() {
        if let RunKind::Prefixed { medium, .. } = run.kind {
            carriers[index] = Some(medium);
            match units.iter().position(|unit| *unit == Some(medium)) {
                Some(unit) => {
                    units.remove(unit);
                }
                None => debug!(
                    discogs_release_id = release_id,
                    medium = medium.key(),
                    "a Discogs position prefix names a carrier the formats do not list"
                ),
            }
        }
    }
    let is_sided_unit = |unit: &Option<Medium>| unit.is_some_and(Medium::is_sided);
    let sided: Vec<usize> = positions(runs, |kind| matches!(kind, RunKind::Sided(_)));
    let unlettered: Vec<usize> = positions(runs, |kind| {
        matches!(kind, RunKind::Disc(_) | RunKind::Unnumbered)
    });
    let sided_units: Vec<Option<Medium>> = units.iter().copied().filter(is_sided_unit).collect();
    let unlettered_units: Vec<Option<Medium>> = if sided.is_empty() {
        units
    } else {
        units.into_iter().filter(|unit| !is_sided_unit(unit)).collect()
    };
    for (group, group_units) in [(sided, sided_units), (unlettered, unlettered_units)] {
        for (index, carrier) in group
            .iter()
            .zip(group_carriers(release_id, group.len(), &group_units))
        {
            carriers[*index] = carrier;
        }
    }
    carriers
}

fn positions(runs: &[Run], is: impl Fn(&RunKind) -> bool) -> Vec<usize> {
    runs.iter()
        .enumerate()
        .filter(|(_, run)| is(&run.kind))
        .map(|(index, _)| index)
        .collect()
}

/// The carriers of `count` runs from the units their group may take.
fn group_carriers(release_id: &str, count: usize, units: &[Option<Medium>]) -> Vec<Option<Medium>> {
    if count == units.len() {
        return units.to_vec();
    }
    let one = match units.split_first() {
        Some((first @ Some(_), rest)) if rest.iter().all(|unit| unit == first) => *first,
        _ => None,
    };
    if count > 0 && one.is_none() {
        debug!(
            discogs_release_id = release_id,
            mediums = count,
            format_units = units.len(),
            "a Discogs release's formats do not say which carrier each medium is"
        );
    }
    vec![one; count]
}

#[cfg(test)]
#[path = "discogs_mediums_tests.rs"]
mod tests;

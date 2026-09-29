//! Offered rows that would look the same on screen, kept once.
//!
//! Rows that show the same thing are the same answer as far as bae can tell:
//! listing each would put rows on screen no one can choose between in bae, so
//! one is kept and the rest are left out — not grouped under a count, which
//! would need explaining. Rows are compared only on one album's card — rows
//! sharing an album a catalog files them under. What counts is what a row
//! shows: its year, its labels and catalog numbers, where it was released and
//! what it is made of, the details beside those (packaging, status, format
//! descriptions), its badges and the note its Notes badge shows, whether it is
//! already in the library, and whether its document could not be read. Catalog ids, notes no
//! badge shows, entry dates and which catalog lists a row are not shown, so
//! they do not count.
//!
//! Rows that show the same thing state the same fields, so which one is kept
//! is decided by what else stands behind it: a row both catalogs list before
//! one only one lists, then the lowest catalog id.
//!
//! Leaving look-alikes out follows from what a row shows now. A row that
//! showed more of each entry's detail — what tells collector variants apart —
//! would compare that detail too.

use super::agreements::Agreements;
use crate::import::release_group::{Judgements, Pressing};
use crate::import::Catalog;
use crate::pressing::{LabelLine, PressingFacts};
use std::collections::{HashMap, HashSet};

type ReleaseKey = (Catalog, String);

/// `rows`, in their order, with each row that shows what an earlier one
/// shows left out, the one of them kept being the one that stands best.
pub(crate) fn keep_one_of_each_look(
    rows: Vec<Pressing>,
    judgements: &Judgements,
    notes: &HashMap<ReleaseKey, String>,
    in_library: &HashSet<ReleaseKey>,
) -> Vec<Pressing> {
    let mut kept: Vec<(Shown, Pressing)> = Vec::new();
    for row in rows {
        let shown = Shown::of(&row, judgements, notes, in_library);
        match kept.iter_mut().find(|(seen, _)| seen.looks_like(&shown)) {
            Some((_, standing)) => {
                if stands_before(&row, standing) {
                    *standing = row;
                }
            }
            None => kept.push((shown, row)),
        }
    }
    kept.into_iter().map(|(_, row)| row).collect()
}

/// Everything a row shows on screen, and the albums it sits under.
struct Shown {
    /// The albums its records' catalogs file them under. A record filed
    /// under none shares a card with no other row.
    albums: Vec<(Catalog, String)>,
    year: Option<i32>,
    labels: Vec<LabelLine>,
    facts: PressingFacts,
    /// The badges: the album's title and artist rank rows but are none.
    badges: Agreements,
    note: Option<String>,
    in_library: bool,
    unread: bool,
}

impl Shown {
    fn of(
        row: &Pressing,
        judgements: &Judgements,
        notes: &HashMap<ReleaseKey, String>,
        in_library: &HashSet<ReleaseKey>,
    ) -> Self {
        let key = |release: &crate::import::search::MetadataResult| {
            (release.source, release.release_id.clone())
        };
        Self {
            albums: row
                .releases
                .iter()
                .filter_map(|release| {
                    release
                        .source_group_id
                        .clone()
                        .map(|group| (release.source, group))
                })
                .collect(),
            year: row.lead().year,
            labels: row.label_lines(),
            facts: row.facts(),
            badges: Agreements {
                title: false,
                artist: false,
                ..row.agreements(judgements)
            },
            note: row
                .releases
                .iter()
                .find_map(|release| notes.get(&key(release)).cloned()),
            in_library: in_library.contains(&key(row.lead())),
            unread: row.document_failure().is_some(),
        }
    }
}

impl Shown {
    /// Whether a row showing `other` sits on the same card as this one and
    /// shows the same.
    fn looks_like(&self, other: &Shown) -> bool {
        self.albums.iter().any(|album| other.albums.contains(album))
            && self.year == other.year
            && self.labels == other.labels
            && self.facts == other.facts
            && self.badges == other.badges
            && self.note == other.note
            && self.in_library == other.in_library
            && self.unread == other.unread
    }
}

/// Whether `row` is kept over `other`, which shows the same: listed by more
/// catalogs, then the lower catalog id.
fn stands_before(row: &Pressing, other: &Pressing) -> bool {
    let catalogs = |row: &Pressing| {
        row.releases
            .iter()
            .map(|release| release.source)
            .collect::<HashSet<_>>()
            .len()
    };
    let id = |row: &Pressing| {
        let id = &row.lead().release_id;
        (id.parse::<u64>().ok(), id.clone())
    };
    match catalogs(row).cmp(&catalogs(other)) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => match (id(row), id(other)) {
            ((Some(ours), _), (Some(theirs), _)) => ours < theirs,
            ((_, ours), (_, theirs)) => ours < theirs,
        },
    }
}

#[cfg(test)]
#[path = "look_alike_tests.rs"]
mod tests;

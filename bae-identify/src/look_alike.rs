//! Offered rows that would look the same on screen, kept once.
//!
//! Rows that show the same thing are the same answer as far as bae can tell:
//! listing each would put rows on screen no one can choose between in bae, so
//! one is kept and the rest are left out — not grouped under a count, which
//! would need explaining. Rows are compared only on one album's card — rows
//! sharing an album a catalog files them under. What counts is what a row
//! shows: its year, its labels and catalog numbers, its barcodes, where it was
//! released and what it is made of, each as every record of the row states it
//! (a fact the records state differently is every record's value, with its
//! catalog), the details beside those (packaging, status, format descriptions)
//! of its lead record, its badges and the notes its records show, and whether
//! its document could not be read. Catalog ids, notes no badge shows, entry
//! dates and which catalog lists a row are not shown, so they do not count.
//! Whether a row is already in the library shows too, but it is read live
//! when the row shows, never stored with the run's rows: rows that look alike
//! otherwise are listed once whatever the library holds, so what a run
//! stores does not depend on it.
//!
//! Rows that show the same thing state the same fields, so which one is kept
//! is decided by what else stands behind it: a row both catalogs list before
//! one only one lists, then the lowest catalog id.
//!
//! Leaving look-alikes out follows from what a row shows now. A row that
//! showed more of each entry's detail — what tells collector variants apart —
//! would compare that detail too.

use super::agreements::Agreements;
use crate::release_group::{Judgements, Pressing, Statement, Statements};
use bae_catalogs::catalog::LookupCatalog;
use bae_types::pressing::{DiscogsDetail, Packaging, ReleaseStatus};
use std::collections::{HashMap, HashSet};

type ReleaseKey = (LookupCatalog, String);

/// `rows`, in their order, with each row that shows what an earlier one
/// shows left out, the one of them kept being the one that stands best.
pub(crate) fn keep_one_of_each_look(
    rows: Vec<Pressing>,
    judgements: &Judgements,
    notes: &HashMap<ReleaseKey, String>,
) -> Vec<Pressing> {
    let mut kept: Vec<(Shown, Pressing)> = Vec::new();
    for row in rows {
        let shown = Shown::of(&row, judgements, notes);
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
    albums: Vec<(LookupCatalog, String)>,
    /// Every fact the row's records state, as the row lists it: each
    /// record's year, catalog numbers, barcodes, country and format where
    /// they differ.
    stated: Statements,
    /// What the lead record states beyond those.
    status: Option<ReleaseStatus>,
    packaging: Option<Packaging>,
    discogs_details: Vec<DiscogsDetail>,
    /// The badges: the album's title and artist rank rows but are none.
    badges: Agreements,
    notes: Vec<Statement<String>>,
    unread: bool,
}

impl Shown {
    fn of(row: &Pressing, judgements: &Judgements, notes: &HashMap<ReleaseKey, String>) -> Self {
        let key = |release: &bae_catalogs::search::MetadataResult| {
            (release.source, release.release_id.clone())
        };
        Self {
            albums: row
                .releases
                .iter()
                .filter_map(|release| {
                    release.source_group_id.clone().map(|group| (release.source, group))
                })
                .collect(),
            stated: row.statements().clone(),
            status: row.lead().status,
            packaging: row.lead().packaging,
            discogs_details: row.lead().discogs_details.clone(),
            badges: Agreements { title: false, artist: false, ..row.agreements(judgements) },
            notes: row.noted(|release| notes.get(&key(release)).cloned()),
            unread: row.document_failure().is_some(),
        }
    }
}

impl Shown {
    /// Whether a row showing `other` sits on the same card as this one and
    /// shows the same.
    fn looks_like(&self, other: &Shown) -> bool {
        self.albums.iter().any(|album| other.albums.contains(album))
            && self.stated == other.stated
            && self.status == other.status
            && self.packaging == other.packaging
            && self.discogs_details == other.discogs_details
            && self.badges == other.badges
            && self.notes == other.notes
            && self.unread == other.unread
    }
}

/// Whether `row` is kept over `other`, which shows the same: listed by more
/// catalogs, then the lower catalog id.
fn stands_before(row: &Pressing, other: &Pressing) -> bool {
    let catalogs = |row: &Pressing| {
        row.releases.iter().map(|release| release.source).collect::<HashSet<_>>().len()
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

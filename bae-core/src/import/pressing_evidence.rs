//! What two records say about one physical pressing.
//!
//! The two catalogs describe the same object in their own words: a barcode
//! with or without spaces, a catalog number with or without a dash. The
//! country and the media arrive already read into bae's vocabulary
//! (`crate::pressing`). Each pressing fact is compared as the meaning behind
//! the spelling, and the answer for
//! each is one of three — the same, different, or nothing to say — never the
//! equality of two optional strings. [`PressingEvidence::support`] then says
//! whether the two records are candidates for one pressing and how well
//! supported that claim is.

use crate::barcode::comparison_key;
use crate::identify::label::stated;
use crate::import::search::MetadataResult;
use crate::import::types::{Catalog, MetadataRef};
use crate::pressing::{Medium, ReleaseArea, StatedMedia};
use crate::util::text::squash;
use tracing::debug;

/// What two records say about one pressing fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Comparison {
    Same,
    Different,
    /// Inconclusive: one side states nothing, or what both state cannot be
    /// told apart from a spelling difference.
    Unknown,
}

/// One record's pressing facts, read once into the form they are compared
/// in, so a record's spellings are resolved once however many records it is
/// compared against.
pub(crate) struct ComparedPressing<'a> {
    record: MetadataRef,
    links: &'a [MetadataRef],
    /// The usable barcodes' keys; a stated value that is not a code is left
    /// out here.
    barcodes: Vec<String>,
    catalog: Option<String>,
    year: Option<i32>,
    area: Option<ReleaseArea>,
    label: Option<String>,
    media: KnownMedia,
}

impl<'a> ComparedPressing<'a> {
    pub(crate) fn of(release: &'a MetadataResult) -> Self {
        Self {
            record: MetadataRef::new(release.source, release.release_id.clone()),
            links: &release.links,
            barcodes: release
                .barcodes
                .iter()
                .filter_map(|stated| barcode_key(release.source, &release.release_id, stated))
                .collect(),
            catalog: release.catalog_number.as_deref().and_then(catalog_key),
            year: release.year,
            area: release.area,
            label: release.label.as_deref().and_then(stated),
            media: KnownMedia::of(&release.media),
        }
    }
}

/// The key a stated barcode is compared by (see [`comparison_key`]), or
/// `None` when the value is not usable as one — which is logged, since a
/// catalog's barcode field holding something else is worth seeing.
fn barcode_key(source: Catalog, release_id: &str, stated: &str) -> Option<String> {
    comparison_key(stated)
        .inspect_err(|unusable| {
            debug!(
                %source,
                release_id,
                stated,
                ?unusable,
                "skipping a stated barcode that is no key to compare by"
            )
        })
        .ok()
}

/// A catalog number as it is compared: squashed, and only when that leaves a
/// number. `[none]` on MusicBrainz and `none` on Discogs state that the
/// release has no catalog number, which is no number to compare.
fn catalog_key(stated: &str) -> Option<String> {
    let key = squash(stated);
    (!key.is_empty() && key != "none").then_some(key)
}

/// The carriers a record names, and whether they are all of them.
struct KnownMedia {
    known: Vec<Medium>,
    /// Whether the carriers are the whole of what the pressing is made of.
    /// A MusicBrainz record lists its media, so it accounts for them all
    /// when every entry names a carrier; a Discogs record's format entries
    /// are every format the release has, so naming one carrier accounts for
    /// them all too.
    complete: bool,
}

impl KnownMedia {
    /// What a record's stated media say it is made of.
    fn of(media: &StatedMedia) -> Self {
        let mut known: Vec<Medium> = Vec::new();
        let mut note = |medium: Option<Medium>| {
            if let Some(medium) = medium {
                if !known.contains(&medium) {
                    known.push(medium);
                }
            }
            medium.is_some()
        };
        let complete = match media {
            StatedMedia::Undescribed => false,
            // A medium that names no carrier bae knows could be anything —
            // but every entry is read rather than stopped at the first of
            // those, because what the later ones name is still known.
            StatedMedia::PerMedium(entries) => {
                let mut complete = !entries.is_empty();
                for entry in entries {
                    complete &= note(*entry);
                }
                complete
            }
            // The entries are every format the release has, so one carrier
            // among them is an account of what the release is made of.
            StatedMedia::Formats(formats) => {
                let mut complete = false;
                for format in formats {
                    complete |= note(format.medium);
                }
                complete
            }
        };
        Self { known, complete }
    }

    fn contains_all_of(&self, other: &Self) -> bool {
        other.known.iter().all(|medium| self.known.contains(medium))
    }

    fn compare(&self, other: &Self) -> Comparison {
        // A record that accounts for everything it is made of is
        // contradicted by a carrier outside that account, however much the
        // other record leaves out.
        if (self.complete && !self.contains_all_of(other))
            || (other.complete && !other.contains_all_of(self))
        {
            return Comparison::Different;
        }
        if self.complete && other.complete {
            return Comparison::Same;
        }
        Comparison::Unknown
    }
}

/// `Same` or `Different` when both sides state the fact, `Unknown` when
/// either leaves it out.
fn compare_stated<T: PartialEq>(a: Option<T>, b: Option<T>) -> Comparison {
    match (a, b) {
        (Some(a), Some(b)) if a == b => Comparison::Same,
        (Some(_), Some(_)) => Comparison::Different,
        _ => Comparison::Unknown,
    }
}

/// What two records say about one pressing, fact by fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PressingEvidence {
    /// Both records are from one catalog. A catalog's editors keep two
    /// records apart because the pressings differ in something — a matrix, a
    /// plant, a sleeve variant — that these facts do not read, so two
    /// records of one catalog are never one pressing here.
    pub(crate) same_catalog: bool,
    /// One record's own document names the other as the same release.
    pub(crate) link: bool,
    pub(crate) barcode: Comparison,
    pub(crate) catalog: Comparison,
    pub(crate) year: Comparison,
    pub(crate) country: Comparison,
    pub(crate) label: Comparison,
    pub(crate) medium: Comparison,
}

impl PressingEvidence {
    pub(crate) fn between(a: &ComparedPressing<'_>, b: &ComparedPressing<'_>) -> Self {
        let barcode = if a.barcodes.iter().any(|key| b.barcodes.contains(key)) {
            Comparison::Same
        } else if !a.barcodes.is_empty() && !b.barcodes.is_empty() {
            Comparison::Different
        } else {
            Comparison::Unknown
        };
        // Never `Different`: the sources punctuate and abbreviate these
        // freely, so two numbers that squash apart may still be one.
        let catalog = match (&a.catalog, &b.catalog) {
            (Some(a), Some(b)) if a == b => Comparison::Same,
            _ => Comparison::Unknown,
        };
        // Two countries are the same or different. A region overlaps the
        // countries and the other regions it spans — "UK & Europe" holds the
        // United Kingdom and Europe both — so only the same region is a
        // match, and nothing involving a region is a contradiction.
        let country = match (a.area, b.area) {
            (Some(ReleaseArea::Country(a)), Some(ReleaseArea::Country(b))) => {
                compare_stated(Some(a), Some(b))
            }
            (Some(a @ ReleaseArea::Region(_)), Some(b)) if a == b => Comparison::Same,
            _ => Comparison::Unknown,
        };
        // Differently written names are inconclusive, not different labels.
        let label = match (&a.label, &b.label) {
            (Some(a), Some(b)) if a == b => Comparison::Same,
            _ => Comparison::Unknown,
        };
        Self {
            same_catalog: a.record.catalog == b.record.catalog,
            link: a.links.contains(&b.record) || b.links.contains(&a.record),
            barcode,
            catalog,
            year: compare_stated(a.year, b.year),
            country,
            label,
            medium: a.media.compare(&b.media),
        }
    }

    /// How well the claim that the two records name one pressing is
    /// supported, or `None` when they are not candidates for one.
    ///
    /// Only records from two different catalogs are candidates, and only on
    /// a link or a shared barcode. A catalog number is never enough: a label
    /// can keep one number on every reissue for decades, so a shared number
    /// with an unstated year joins an original to its represses. A different
    /// barcode, year, country or medium is a contradiction that removes an
    /// inferred candidate; a linked pair is stated rather than inferred, and
    /// stands. A shared catalog number still ranks candidates found another
    /// way.
    pub(crate) fn support(&self) -> Option<Support> {
        if self.same_catalog {
            return None;
        }
        let agreed = [self.year, self.country, self.label, self.medium]
            .iter()
            .filter(|comparison| **comparison == Comparison::Same)
            .count() as u8;
        let barcode = self.barcode == Comparison::Same;
        let catalog = self.catalog == Comparison::Same;
        if !(self.link || barcode) {
            return None;
        }
        let contradicted =
            [self.barcode, self.year, self.country, self.medium].contains(&Comparison::Different);
        if contradicted && !self.link {
            return None;
        }
        Some(Support {
            link: self.link,
            barcode,
            catalog,
            agreed,
        })
    }
}

/// How well supported the claim that two records name one pressing is,
/// ordered from what a document states down to what the facts agree on:
/// a link, then a shared barcode, then a shared catalog number, then how many
/// of the year, country, label and medium agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Support {
    link: bool,
    barcode: bool,
    catalog: bool,
    agreed: u8,
}

#[cfg(test)]
#[path = "pressing_evidence_tests.rs"]
mod tests;

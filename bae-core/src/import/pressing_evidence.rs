//! What two records say about one physical pressing.
//!
//! The two catalogs describe the same object in their own words: a barcode
//! with or without spaces, a catalog number with or without a dash, a country
//! as a code or a name, a medium with or without its qualifiers. Each pressing
//! fact is compared as the meaning behind the spelling, and the answer for
//! each is one of three — the same, different, or nothing to say — never the
//! equality of two optional strings. [`PressingEvidence::support`] then says
//! whether the two records are candidates for one pressing and how well
//! supported that claim is.

use crate::identify::country::named;
use crate::identify::label::stated;
use crate::import::medium::{Lookup, Medium};
use crate::import::search::{MetadataResult, StatedMedia};
use crate::import::types::{Catalog, MetadataRef};
use crate::signals::barcode::is_placeholder_code;
use crate::util::text::squash;
use tracing::{debug, warn};

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
pub(crate) struct PressingFacts<'a> {
    record: MetadataRef,
    links: &'a [MetadataRef],
    /// The usable barcodes' keys; a stated value that is not a code is left
    /// out here.
    barcodes: Vec<String>,
    catalog: Option<String>,
    year: Option<i32>,
    country: Option<Place>,
    label: Option<String>,
    media: KnownMedia,
}

impl<'a> PressingFacts<'a> {
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
            country: release.country.as_deref().and_then(Place::named),
            label: release.label.as_deref().and_then(stated),
            media: KnownMedia::of(release.source, &release.release_id, &release.media),
        }
    }
}

/// The key a stated barcode is compared by, or `None` when the value is not
/// usable as one.
///
/// A code is digits, with spaces and dashes between them and nothing else; a
/// value with letters or other punctuation is something else printed in the
/// barcode field. Fewer than eight digits is no UPC or EAN, and a run of one
/// digit is a placeholder. The key is the digits, with a twelve-digit UPC-A
/// written as the thirteen-digit EAN that prefixes a zero — the one
/// equivalence the two encodings define. No other length is rewritten, so an
/// eight-digit code and a thirteen-digit one never meet.
fn barcode_key(source: Catalog, release_id: &str, stated: &str) -> Option<String> {
    if !stated
        .chars()
        .all(|c| c.is_ascii_digit() || c == ' ' || c == '-')
    {
        debug!(
            %source,
            release_id,
            stated,
            "skipping a stated barcode that is not a code"
        );
        return None;
    }
    let digits: String = stated.chars().filter(char::is_ascii_digit).collect();
    if digits.len() < 8 {
        debug!(
            %source,
            release_id,
            stated,
            "skipping a stated barcode with too few digits for a code"
        );
        return None;
    }
    if is_placeholder_code(&digits) {
        debug!(
            %source,
            release_id,
            stated,
            "skipping a placeholder barcode"
        );
        return None;
    }
    Some(if digits.len() == 12 {
        format!("0{digits}")
    } else {
        digits
    })
}

/// A catalog number as it is compared: squashed, and only when that leaves a
/// number. `[none]` on MusicBrainz and `none` on Discogs state that the
/// release has no catalog number, which is no number to compare.
fn catalog_key(stated: &str) -> Option<String> {
    let key = squash(stated);
    (!key.is_empty() && key != "none").then_some(key)
}

/// Where a pressing was made, as far as the two catalogs' values can be
/// resolved: one country of the standard, or one of the regions MusicBrainz
/// writes outside it. A country and a region are not comparable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    Country(&'static str),
    Region(Region),
}

/// The regions MusicBrainz states where no one country applies, and the
/// names they are written out as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Region {
    Europe,
    Worldwide,
}

impl Place {
    fn named(value: &str) -> Option<Self> {
        if let Some(country) = named(value) {
            return Some(Self::Country(country.code));
        }
        let region = match squash(value).as_str() {
            "xe" | "europe" => Region::Europe,
            "xw" | "worldwide" => Region::Worldwide,
            _ => return None,
        };
        Some(Self::Region(region))
    }
}

/// The carriers a record names, and whether they are all of them.
struct KnownMedia {
    known: Vec<Medium>,
    /// Whether the carriers are the whole of what the pressing is made of.
    /// A MusicBrainz record lists its media, so it accounts for them all
    /// when every entry names a carrier; a Discogs record's `format` array
    /// lists every format the release has, so naming one carrier accounts
    /// for them all too.
    complete: bool,
}

impl KnownMedia {
    /// What a record's stated media say it is made of, each word read in its
    /// own catalog's vocabulary.
    ///
    /// A MusicBrainz medium names one format, so a word the table lacks
    /// leaves that medium unknown; a Discogs `format` array mixes format
    /// names with descriptions, so a word the table lacks is almost always a
    /// description — the descriptions are added regularly while the format
    /// names barely move — and is passed over. Either way the word is logged,
    /// because the vocabulary is what needs fixing.
    fn of(source: Catalog, release_id: &str, media: &StatedMedia) -> Self {
        let mut known: Vec<Medium> = Vec::new();
        let mut read = |word: &str| {
            let lookup = match source {
                Catalog::MusicBrainz => Medium::musicbrainz(word),
                Catalog::Discogs => Medium::discogs(word),
                other => unreachable!("{other} answers no lookups, so it states no media"),
            };
            match lookup {
                Lookup::Carrier(medium) => {
                    if !known.contains(&medium) {
                        known.push(medium);
                    }
                }
                Lookup::Unrecognized => warn!(
                    %source,
                    release_id,
                    word,
                    "a stated medium is outside the catalog's vocabulary"
                ),
                Lookup::NamesNoCarrier | Lookup::Description => {}
            }
            lookup
        };
        let complete = match media {
            StatedMedia::Undescribed => false,
            // A medium whose format is absent, or whose name the table
            // lacks, could be anything — but every entry is read rather than
            // stopped at the first of those, because what the later ones name
            // is still known.
            StatedMedia::PerMedium(entries) => {
                let mut complete = !entries.is_empty();
                for entry in entries {
                    complete &= entry
                        .as_deref()
                        .is_some_and(|name| matches!(read(name), Lookup::Carrier(_)));
                }
                complete
            }
            // The array lists the release's formats, so one carrier in it is
            // an account of what the release is made of. Words that name no
            // carrier describe it; a record of nothing but those describes
            // no medium at all.
            StatedMedia::Descriptors(words) => {
                let mut complete = false;
                for word in words {
                    complete |= matches!(read(word), Lookup::Carrier(_));
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
    pub(crate) fn between(a: &PressingFacts<'_>, b: &PressingFacts<'_>) -> Self {
        let barcode = if a
            .barcodes
            .iter()
            .any(|key| b.barcodes.contains(key))
        {
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
        // A country against a region is not comparable; neither is an
        // unresolved value.
        let country = match (a.country, b.country) {
            (Some(Place::Country(a)), Some(Place::Country(b))) => compare_stated(Some(a), Some(b)),
            (Some(Place::Region(a)), Some(Place::Region(b))) => compare_stated(Some(a), Some(b)),
            _ => Comparison::Unknown,
        };
        // Differently written names are inconclusive, not different labels.
        let label = match (&a.label, &b.label) {
            (Some(a), Some(b)) if a == b => Comparison::Same,
            _ => Comparison::Unknown,
        };
        Self {
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
    /// A link, a shared barcode, or a shared catalog number corroborated by
    /// another agreeing fact makes them candidates. A different barcode,
    /// year, country or medium is a contradiction that removes an inferred
    /// candidate; a linked pair is stated rather than inferred, and stands.
    pub(crate) fn support(&self) -> Option<Support> {
        let agreed = [self.year, self.country, self.label, self.medium]
            .iter()
            .filter(|comparison| **comparison == Comparison::Same)
            .count() as u8;
        let barcode = self.barcode == Comparison::Same;
        let catalog = self.catalog == Comparison::Same;
        if !(self.link || barcode || (catalog && agreed >= 1)) {
            return None;
        }
        let contradicted = [self.barcode, self.year, self.country, self.medium]
            .contains(&Comparison::Different);
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

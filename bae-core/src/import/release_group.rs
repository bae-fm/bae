//! Release-group bundling for the import results UI.
//!
//! Import search and identification return individual releases (pressings).
//! The UI renders them grouped under the album they belong to — a
//! release-group on MusicBrainz, a master on Discogs — with one card per
//! group, and one row per physical pressing beneath it.
//!
//! The two providers answer independently, so the same album and the same
//! pressing arrive twice. Both collapses happen here: two sources' groups
//! become one card when they name the same album, and two sources' releases
//! become one row when they name the same physical pressing. A row is then a
//! pressing on however many sources listed it, and picking it claims every one
//! of them — [`Pressing::pick`] says exactly what.
//!
//! The order is decided here too, so no surface sorts anything: rows come
//! most-agreed-with first — how much of the candidate's own text states the
//! pressing — and cards come in the order of their best row. A caller with
//! nothing to rank by, a typed search, hands over [`Agreements::NONE`] for
//! every release, which leaves the pressing year as the whole of the order.

use crate::identify::agreements::Agreements;
use crate::import::cover_art::RemoteCover;
use crate::import::search::MetadataResult;
use crate::import::types::MetadataSource;
use crate::signals::candidate_text::normalize;

/// An album, as one or both sources describe it, with the pressings they
/// surfaced for it.
///
/// `Serialize`/`Deserialize`: named by a ledger cell that found releases,
/// and the ledger is what `identify::TerminalVerdict` persists.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReleaseGroup {
    /// Stable card identity: the first source's group id, or the lone
    /// release's id when no source named a group.
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    /// The label the card names beside the artist — the first pressing that
    /// states one, MusicBrainz first. `None` when no pressing names a label.
    /// Which of an album's pressings speaks for it is core's call, not a
    /// surface's.
    pub label: Option<String>,
    /// Representative cover for the card — the first pressing that surfaced
    /// one, MusicBrainz first.
    pub cover_art: Option<RemoteCover>,
    /// Every source carrying this group, MusicBrainz first; each with its
    /// editorial page when the source named a group.
    pub sources: Vec<ReleaseGroupSource>,
    /// Earliest and latest pressing year, for the UI's "1992 – 2012" span.
    /// Both `None` when no pressing carries a year.
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    pub pressings: Vec<Pressing>,
}

/// One source carrying a group, and where its editorial page for it is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReleaseGroupSource {
    pub source: MetadataSource,
    /// Editorial URL for the group on this source (release-group on
    /// MusicBrainz, master on Discogs). `None` when the source returned the
    /// release ungrouped, which has no group page to open.
    pub group_url: Option<String>,
}

/// One physical pressing, on every source that lists it. A row is picked
/// whole: `releases[0]` (MusicBrainz when both carry it) is the release the
/// draft is read from, and each further entry is the same pressing as another
/// source has it, claimed alongside it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pressing {
    pub releases: Vec<MetadataResult>,
}

impl Pressing {
    /// The release a row picks when the person picks the row itself.
    pub fn lead(&self) -> &MetadataResult {
        self.releases
            .first()
            .expect("a pressing is built from at least one release")
    }

    /// What picking this row claims, as release references: the primary — the
    /// document the draft is read from — and every other source's record of
    /// the same pressing as a partner.
    ///
    /// A row is one pressing however many sources carry it, so this is the
    /// whole of what picking it means. Deciding it here rather than on each
    /// surface is what keeps macOS, Windows, Linux and the sweep picking the
    /// same thing.
    pub(crate) fn claims(&self) -> (crate::import::MetadataRef, Vec<crate::import::MetadataRef>) {
        let mut releases = self.releases.iter().map(|release| {
            crate::import::MetadataRef::new(release.release_id.clone(), release.source)
        });
        let primary = releases
            .next()
            .expect("a pressing is built from at least one release");
        (primary, releases.collect())
    }

    /// What the candidate's own text agrees with about this row: every one of
    /// its records' agreements together.
    ///
    /// A row is one physical object however many sources carry it, and it is
    /// picked whole, so a catalog number only Discogs prints and a disc ID
    /// only MusicBrainz answers are both true of the row.
    pub fn agreements(&self, judged: &Judgements) -> Agreements {
        self.releases
            .iter()
            .fold(Agreements::NONE, |so_far, release| {
                so_far.with(judged.of_release(release))
            })
    }

    /// What picking this row claims — the primary release and every other
    /// source's record of the same pressing — as the provenance a pick stores.
    pub fn pick(&self) -> crate::import::MetadataProvenance {
        let (primary, partners) = self.claims();
        crate::import::MetadataProvenance::ExternalRelease {
            source: primary.source,
            release_id: primary.id,
            partners,
        }
    }
}

/// What the candidate's own text agrees with about each release, by the key
/// that tells releases apart.
///
/// Built once from the judged results, and read back after they have been
/// paired: a pressing row is only known once pairing has run, and what the row
/// agrees with is what its records agree with.
#[derive(Debug, Clone, Default)]
pub struct Judgements(std::collections::HashMap<(MetadataSource, String), Agreements>);

impl Judgements {
    /// What was said about these results, ready to be asked release by
    /// release.
    pub fn of(results: &[Judged]) -> Self {
        Self(
            results
                .iter()
                .map(|(release, agreements)| {
                    ((release.source, release.release_id.clone()), *agreements)
                })
                .collect(),
        )
    }

    /// What was said about one release. A release these were not built from
    /// was never judged, which is nothing agreeing.
    fn of_release(&self, release: &MetadataResult) -> Agreements {
        self.0
            .get(&(release.source, release.release_id.clone()))
            .copied()
            .unwrap_or(Agreements::NONE)
    }
}

/// One source's bucket of releases under one of its groups, each with what the
/// candidate's own text agrees with about it.
struct Bucket {
    source: MetadataSource,
    source_group_id: Option<String>,
    releases: Vec<Judged>,
}

/// One release and what the candidate's own text agrees with about it — what
/// [`group_results`] orders the rows and the cards by.
pub type Judged = (MetadataResult, Agreements);

impl Bucket {
    /// What decides whether this bucket describes the same album as another
    /// source's: the album's title and artist, normalized. `None` artist
    /// matches only `None`.
    fn album_key(&self) -> (String, Option<String>) {
        let (first, _) = self
            .releases
            .first()
            .expect("a bucket is built from at least one release");
        (
            normalize(&first.title),
            self.releases
                .iter()
                .find_map(|(release, _)| release.artist.as_deref())
                .map(normalize),
        )
    }

    fn as_source(&self) -> ReleaseGroupSource {
        ReleaseGroupSource {
            source: self.source,
            group_url: self
                .source_group_id
                .as_deref()
                .map(|group_id| self.source.group_url(group_id)),
        }
    }
}

/// How many pressing rows these results make.
///
/// The list shows one row per physical pressing and a row is picked whole, so
/// "how many pressings did this candidate match" is this number rather than
/// how many result rows came back: a MusicBrainz release and a Discogs release
/// describing the same object are one answer, not two. The Ready rule and the
/// sweep's settle step both ask it.
///
/// These are the rows [`group_results`] builds, so nothing counts one thing
/// and shows another.
pub fn pressing_count(results: Vec<MetadataResult>) -> usize {
    // Counting is order-blind, so there is nothing to rank the rows by.
    group_results(unranked(results))
        .iter()
        .map(|group| group.pressings.len())
        .sum()
}

/// Every release with nothing said about it — what a caller hands over when
/// the rows are not being ranked.
pub fn unranked(results: Vec<MetadataResult>) -> Vec<Judged> {
    results
        .into_iter()
        .map(|result| (result, Agreements::NONE))
        .collect()
}

/// Group results into album cards with one row per physical pressing.
///
/// Five steps: bucket each source's releases by its own group, merge a
/// MusicBrainz bucket with a Discogs bucket describing the same album, pair
/// the two sources' releases into shared pressing rows, order the rows by how
/// much of the candidate's text agrees with them and then by pressing year,
/// and order the cards by their best row.
pub fn group_results(results: Vec<Judged>) -> Vec<ReleaseGroup> {
    let judgements = Judgements::of(&results);
    let mut cards: Vec<(ReleaseGroup, u32)> = merge_buckets(bucket_by_source_group(results))
        .into_iter()
        .map(|card| build_group(card, &judgements))
        .collect();
    // Stable: cards nothing tells apart keep the order the signals named them
    // in, which is the order they were bucketed.
    cards.sort_by_key(|(_, best)| std::cmp::Reverse(*best));
    cards.into_iter().map(|(group, _)| group).collect()
}

/// Bucket by `(source, source_group_id)`, preserving first-seen order. A
/// result without a group id can't share one, so it becomes its own bucket.
fn bucket_by_source_group(results: Vec<Judged>) -> Vec<Bucket> {
    use std::collections::HashMap;

    let mut buckets: Vec<Bucket> = Vec::new();
    let mut index: HashMap<(MetadataSource, String), usize> = HashMap::new();
    for judged in results {
        match judged.0.source_group_id.clone() {
            Some(group_id) => {
                let key = (judged.0.source, group_id.clone());
                match index.get(&key) {
                    Some(&at) => buckets[at].releases.push(judged),
                    None => {
                        index.insert(key, buckets.len());
                        buckets.push(Bucket {
                            source: judged.0.source,
                            source_group_id: Some(group_id),
                            releases: vec![judged],
                        });
                    }
                }
            }
            None => buckets.push(Bucket {
                source: judged.0.source,
                source_group_id: None,
                releases: vec![judged],
            }),
        }
    }
    buckets
}

/// Pair each bucket with at most one bucket from the other source describing
/// the same album. A merged card sits at the earlier bucket's position, and
/// its sources are ordered MusicBrainz first.
fn merge_buckets(buckets: Vec<Bucket>) -> Vec<Vec<Bucket>> {
    let mut buckets: Vec<Option<Bucket>> = buckets.into_iter().map(Some).collect();
    let mut cards: Vec<Vec<Bucket>> = Vec::new();
    for at in 0..buckets.len() {
        let Some(bucket) = buckets[at].take() else {
            continue;
        };
        let key = bucket.album_key();
        let partner = (at + 1..buckets.len()).find(|&other| {
            buckets[other].as_ref().is_some_and(|candidate| {
                candidate.source != bucket.source && candidate.album_key() == key
            })
        });
        let mut card = match partner.and_then(|other| buckets[other].take()) {
            Some(partner) => vec![bucket, partner],
            None => vec![bucket],
        };
        card.sort_by_key(|bucket| match bucket.source {
            MetadataSource::MusicBrainz => 0,
            MetadataSource::Discogs => 1,
        });
        cards.push(card);
    }
    cards
}

/// The card, and how much the candidate's text agrees with its best row —
/// what orders the cards against each other.
fn build_group(card: Vec<Bucket>, judgements: &Judgements) -> (ReleaseGroup, u32) {
    let sources: Vec<ReleaseGroupSource> = card.iter().map(Bucket::as_source).collect();
    let releases: Vec<&MetadataResult> = card
        .iter()
        .flat_map(|bucket| bucket.releases.iter().map(|(release, _)| release))
        .collect();
    let lead = releases
        .first()
        .expect("a card is built from at least one release");
    let id = card
        .iter()
        .find_map(|bucket| bucket.source_group_id.clone())
        .unwrap_or_else(|| lead.release_id.clone());
    let title = lead.title.clone();
    let artist = releases.iter().find_map(|release| release.artist.clone());
    let label = releases.iter().find_map(|release| release.label.clone());
    let cover_art = releases
        .iter()
        .find_map(|release| release.cover_art.clone());
    let years: Vec<i32> = releases.iter().filter_map(|release| release.year).collect();
    let year_min = years.iter().min().copied();
    let year_max = years.iter().max().copied();

    let mut cards = card.into_iter();
    let first = cards
        .next()
        .expect("a card is built from at least one bucket");
    let pressings = match cards.next() {
        Some(second) => pair_pressings(releases_of(first.releases), releases_of(second.releases)),
        None => releases_of(first.releases)
            .into_iter()
            .map(|release| Pressing {
                releases: vec![release],
            })
            .collect(),
    };
    let rows = ordered_rows(
        pressings
            .into_iter()
            .map(|pressing| Row {
                agreements: pressing.agreements(judgements).count(),
                pressing,
            })
            .collect(),
    );
    let best = rows.first().map_or(0, |row| row.agreements);

    (
        ReleaseGroup {
            id,
            title,
            artist,
            label,
            cover_art,
            sources,
            year_min,
            year_max,
            pressings: rows.into_iter().map(|row| row.pressing).collect(),
        },
        best,
    )
}

/// One pressing row and how much of the candidate's text agrees with it — its
/// records' agreements together, since the row is picked whole.
struct Row {
    pressing: Pressing,
    agreements: u32,
}

/// The releases a bucket holds, with what was said about them left behind:
/// pairing asks which records name one physical object, which is a question
/// about the records themselves.
fn releases_of(judged: Vec<Judged>) -> Vec<MetadataResult> {
    judged.into_iter().map(|(release, _)| release).collect()
}

/// Pair the two sources' releases into shared pressing rows. A barcode both
/// state is the strongest evidence that they name the same physical object, so
/// every barcode pair is taken before any catalog-number pair; each release
/// pairs at most once, and what is left over is its own single-source row.
///
/// A label's reissues share a barcode and a catalog number with the pressing
/// they reissue, so a code can name several of the other source's records. The
/// pressing year is what tells them apart: a record pressed the same year as
/// the lead is taken over one that merely prints the same code.
fn pair_pressings(lead: Vec<MetadataResult>, other: Vec<MetadataResult>) -> Vec<Pressing> {
    let mut other: Vec<Option<MetadataResult>> = other.into_iter().map(Some).collect();
    let mut partners: Vec<Option<usize>> = vec![None; lead.len()];

    for key_of in [
        barcode_key as fn(&MetadataResult) -> Option<String>,
        catalog_key,
    ] {
        for (at, release) in lead.iter().enumerate() {
            if partners[at].is_some() {
                continue;
            }
            let Some(key) = key_of(release) else {
                continue;
            };
            let taken: std::collections::HashSet<usize> =
                partners.iter().flatten().copied().collect();
            let states_key = |candidate: &Option<MetadataResult>| {
                candidate
                    .as_ref()
                    .is_some_and(|candidate| key_of(candidate).as_deref() == Some(key.as_str()))
            };
            let same_year = |candidate: &Option<MetadataResult>| {
                release.year.is_some()
                    && candidate
                        .as_ref()
                        .is_some_and(|candidate| candidate.year == release.year)
            };
            let found = other
                .iter()
                .enumerate()
                .position(|(index, candidate)| {
                    !taken.contains(&index) && states_key(candidate) && same_year(candidate)
                })
                .or_else(|| {
                    other.iter().enumerate().position(|(index, candidate)| {
                        !taken.contains(&index) && states_key(candidate)
                    })
                });
            partners[at] = found;
        }
    }

    let mut rows: Vec<Pressing> = Vec::with_capacity(lead.len() + other.len());
    for (release, partner) in lead.into_iter().zip(&partners) {
        let mut releases = vec![release];
        if let Some(partner) = partner.and_then(|at| other[at].take()) {
            releases.push(partner);
        }
        rows.push(Pressing { releases });
    }
    rows.extend(other.into_iter().flatten().map(|release| Pressing {
        releases: vec![release],
    }));
    rows
}

/// The digits of a stated barcode. Sources print the same code with different
/// spacing, and one of them pads it with a leading zero, so only the digits
/// are comparable. `None` when the release states none, or states something
/// with no digits in it — neither pairs with anything.
fn barcode_key(release: &MetadataResult) -> Option<String> {
    let digits: String = release
        .barcode
        .as_deref()?
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    (!digits.is_empty()).then_some(digits)
}

/// A stated catalog number, trimmed and case-folded. Weaker than a barcode:
/// the sources punctuate multi-disc numbers differently ("… 2 2" vs "… 2-2"),
/// so only an exact match after folding counts.
fn catalog_key(release: &MetadataResult) -> Option<String> {
    let trimmed = release.catalog_number.as_deref()?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_ascii_lowercase())
}

/// Order the rows: most agreed with first, and among rows the candidate's text
/// says as much about, by the year the row's lead release was pressed,
/// earliest first, with a pressing whose year nobody states last. Stable, so
/// rows nothing tells apart keep the order the sources listed them in.
fn ordered_rows(mut rows: Vec<Row>) -> Vec<Row> {
    rows.sort_by_key(|row| {
        let lead = row.pressing.lead();
        (
            std::cmp::Reverse(row.agreements),
            lead.year.is_none(),
            lead.year,
        )
    });
    rows
}

#[cfg(test)]
#[path = "release_group_tests.rs"]
mod tests;

//! Release-group bundling for the import results UI.
//!
//! Import search and identification return individual releases (pressings).
//! The UI renders them grouped under the album they belong to — a
//! release-group on MusicBrainz, a master on Discogs — with one card per
//! group, and one row per physical pressing beneath it.
//!
//! The two providers answer independently, so the same album and the same
//! pressing arrive twice. Both collapses happen here, pressings first: two
//! sources' releases become one row when the evidence their records carry
//! says they name the same physical object — what `pressing_evidence`
//! weighs — and two sources' groups become one card when a row joins them
//! or when they name the same album.
//! A row is then a pressing on however many sources listed it, and picking
//! it claims every one of them — [`Pressing::pick`] says exactly what.
//!
//! The order is decided here too, so no surface sorts anything: rows come
//! most-agreed-with first — how much of the candidate's own text states the
//! pressing — a row's own records come the same way, and cards come in the
//! order of their best row. A caller with nothing to rank by, a typed search,
//! hands over [`Agreements::NONE`] for every release, which leaves the
//! pressing year ordering the rows and the source name ordering the records
//! within one.

use crate::identify::agreements::Agreements;
use crate::import::cover_art::RemoteCover;
use crate::import::pressing_evidence::{PressingEvidence, PressingFacts, Support};
use crate::import::search::MetadataResult;
use crate::import::types::Catalog;
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
    /// Every source carrying this group, in the one order surfaces list
    /// sources in, each with its editorial page when the source named a
    /// group. The chips under an album's title and the names on the rows
    /// beneath it read the same way round, whichever record a row was read
    /// from.
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
    pub source: Catalog,
    /// Editorial URL for the group on this source (release-group on
    /// MusicBrainz, master on Discogs). `None` when the source returned the
    /// release ungrouped, which has no group page to open.
    pub group_url: Option<String>,
}

/// One physical pressing, on every source that lists it. A row is picked
/// whole: `releases[0]` is the release the draft is read from, and each
/// further entry is the same pressing as another source has it, claimed
/// alongside it. The pressing's constructor says which record that first one is.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pressing {
    pub releases: Vec<MetadataResult>,
}

impl Pressing {
    /// One pressing's records, ordered by what the folder says about each.
    ///
    /// Both sources describe the same physical object, and neither of them is
    /// the one the draft is read from by name. The record the candidate's own
    /// text agrees with most is; among records it says as much about, the one
    /// that states a tracklist, since the draft's rows and the settle's check
    /// of them against the audio are read out of that tracklist. Only where
    /// the two are indistinguishable on both does the source name decide,
    /// MusicBrainz first.
    fn of(mut releases: Vec<MetadataResult>, judged: &Judgements) -> Self {
        releases.sort_by_key(|release| {
            (
                std::cmp::Reverse(judged.of_release(release).count()),
                !states_tracklist(release),
                source_rank(release.source),
            )
        });
        Self { releases }
    }

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
            crate::import::MetadataRef::new(release.source, release.release_id.clone())
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
            record: primary,
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
pub struct Judgements(std::collections::HashMap<(Catalog, String), Agreements>);

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

/// One source's bucket of releases under one of its groups: the releases'
/// indexes into the result list, in the order the source listed them.
struct Bucket {
    source: Catalog,
    source_group_id: Option<String>,
    members: Vec<usize>,
}

/// One release and what the candidate's own text agrees with about it — what
/// [`group_results`] orders the rows and the cards by.
pub type Judged = (MetadataResult, Agreements);

impl Bucket {
    /// What decides whether this bucket describes the same album as another
    /// source's, where no pair of releases has already said so: the album's
    /// title and artist, normalized. `None` artist matches only `None`.
    fn album_key(&self, releases: &[MetadataResult]) -> (String, Option<String>) {
        let first = &releases[*self
            .members
            .first()
            .expect("a bucket is built from at least one release")];
        (
            normalize(&first.title),
            self.members
                .iter()
                .find_map(|&at| releases[at].artist.as_deref())
                .map(normalize),
        )
    }

    fn as_source(&self) -> ReleaseGroupSource {
        ReleaseGroupSource {
            source: self.source,
            group_url: self
                .source_group_id
                .as_deref()
                .and_then(|group_id| self.source.group_url(group_id)),
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
/// Pressings are matched before albums are: the two sources' releases are
/// paired over the whole list by the evidence their records carry, so the
/// spelling of an album's title never keeps two records of one object apart.
/// Then each source's releases are bucketed by its own group, buckets a pair
/// joins become one card, a MusicBrainz card and a Discogs card whose album
/// text agrees merge, the rows are ordered by how much of the candidate's
/// text agrees with them and then by pressing year, and the cards by their
/// best row.
pub fn group_results(results: Vec<Judged>) -> Vec<ReleaseGroup> {
    let judgements = Judgements::of(&results);
    let releases: Vec<MetadataResult> = results.into_iter().map(|(release, _)| release).collect();
    let pairs = pair_releases(&releases);
    let cards = merge_buckets(bucket_by_source_group(&releases), &releases, &pairs);
    let mut releases: Vec<Option<MetadataResult>> = releases.into_iter().map(Some).collect();
    let mut cards: Vec<(ReleaseGroup, u32)> = cards
        .into_iter()
        .map(|card| build_group(card, &mut releases, &pairs, &judgements))
        .collect();
    // Stable: cards nothing tells apart keep the order the signals named them
    // in, which is the order they were bucketed.
    cards.sort_by_key(|(_, best)| std::cmp::Reverse(*best));
    cards.into_iter().map(|(group, _)| group).collect()
}

/// The pairs of MusicBrainz and Discogs records that name one pressing, as
/// indexes into `releases`, the MusicBrainz record first.
///
/// Every MusicBrainz record is weighed against every Discogs record — the
/// two members of [`Catalog::LOOKUP`]; a record from any other catalog is a
/// programming error. Candidates are taken from the best-supported level
/// down: at each level, the candidate pairs whose two members are both still
/// free are examined together, a member that appears in more than one of them
/// is ambiguous and is settled unpaired, and every remaining pair is taken. A
/// member whose only pair at a level named an ambiguous member stays free for
/// the levels below. Nothing depends on the order the records arrived in.
fn pair_releases(releases: &[MetadataResult]) -> Vec<(usize, usize)> {
    let facts: Vec<PressingFacts<'_>> = releases.iter().map(PressingFacts::of).collect();
    let facts = &facts;
    let mut musicbrainz = Vec::new();
    let mut discogs = Vec::new();
    for (at, release) in releases.iter().enumerate() {
        match release.source {
            Catalog::MusicBrainz => musicbrainz.push(at),
            Catalog::Discogs => discogs.push(at),
            other => unreachable!("{other} answers no lookups, so it has no results to pair"),
        }
    }
    let mut candidates: Vec<(Support, usize, usize)> = musicbrainz
        .iter()
        .flat_map(|&a| {
            discogs.iter().filter_map(move |&b| {
                PressingEvidence::between(&facts[a], &facts[b])
                    .support()
                    .map(|support| (support, a, b))
            })
        })
        .collect();
    candidates.sort_by(|(a, _, _), (b, _, _)| b.cmp(a));

    let mut free = vec![true; releases.len()];
    let mut pairs = Vec::new();
    let mut level = candidates.as_slice();
    while let Some((support, _, _)) = level.first() {
        let end = level
            .iter()
            .position(|(other, _, _)| other != support)
            .unwrap_or(level.len());
        let live: Vec<(usize, usize)> = level[..end]
            .iter()
            .filter(|(_, a, b)| free[*a] && free[*b])
            .map(|(_, a, b)| (*a, *b))
            .collect();
        let mut named = vec![0usize; releases.len()];
        for &(a, b) in &live {
            named[a] += 1;
            named[b] += 1;
        }
        for (a, b) in live {
            let ambiguous = named[a] > 1 || named[b] > 1;
            if ambiguous {
                for member in [a, b] {
                    if named[member] > 1 {
                        free[member] = false;
                    }
                }
            } else {
                pairs.push((a, b));
                free[a] = false;
                free[b] = false;
            }
        }
        level = &level[end..];
    }
    pairs
}

/// Bucket by `(source, source_group_id)`, preserving first-seen order. A
/// result without a group id can't share one, so it becomes its own bucket.
fn bucket_by_source_group(releases: &[MetadataResult]) -> Vec<Bucket> {
    use std::collections::HashMap;

    let mut buckets: Vec<Bucket> = Vec::new();
    let mut index: HashMap<(Catalog, String), usize> = HashMap::new();
    for (at, release) in releases.iter().enumerate() {
        match release.source_group_id.clone() {
            Some(group_id) => {
                let key = (release.source, group_id.clone());
                match index.get(&key) {
                    Some(&bucket) => buckets[bucket].members.push(at),
                    None => {
                        index.insert(key, buckets.len());
                        buckets.push(Bucket {
                            source: release.source,
                            source_group_id: Some(group_id),
                            members: vec![at],
                        });
                    }
                }
            }
            None => buckets.push(Bucket {
                source: release.source,
                source_group_id: None,
                members: vec![at],
            }),
        }
    }
    buckets
}

/// The cards the buckets make. Buckets a pair joins are one card — the album
/// grouping the matched pressings establish, which may put more than one of
/// a source's buckets on a card. Then a card carrying only one source merges
/// with the first later card carrying only the other source whose album text
/// agrees with it. A card sits at its earliest bucket's position, and its
/// buckets are ordered by [`source_rank`] and then first-seen — the order the
/// card's title, label and cover are read in, and the order it names its
/// sources in.
fn merge_buckets(
    buckets: Vec<Bucket>,
    releases: &[MetadataResult],
    pairs: &[(usize, usize)],
) -> Vec<Vec<Bucket>> {
    let mut bucket_of = vec![0usize; releases.len()];
    for (at, bucket) in buckets.iter().enumerate() {
        for &member in &bucket.members {
            bucket_of[member] = at;
        }
    }
    let mut cards: Vec<Vec<Bucket>> = Vec::new();
    let mut card_of: Vec<Option<usize>> = vec![None; buckets.len()];
    let mut buckets: Vec<Option<Bucket>> = buckets.into_iter().map(Some).collect();
    for at in 0..buckets.len() {
        if card_of[at].is_some() {
            continue;
        }
        // Everything reachable from this bucket through pairs, in first-seen
        // order.
        let mut joined = vec![at];
        let mut next = 0;
        while next < joined.len() {
            let bucket = joined[next];
            for &(a, b) in pairs {
                let (from, to) = if bucket_of[a] == bucket {
                    (bucket, bucket_of[b])
                } else if bucket_of[b] == bucket {
                    (bucket, bucket_of[a])
                } else {
                    continue;
                };
                if from != to && !joined.contains(&to) {
                    joined.push(to);
                }
            }
            next += 1;
        }
        let card = cards.len();
        let mut members: Vec<Bucket> = Vec::with_capacity(joined.len());
        for bucket in joined {
            card_of[bucket] = Some(card);
            members.push(buckets[bucket].take().expect("a bucket joins one card"));
        }
        cards.push(members);
    }

    let only_source = |card: &[Bucket]| -> Option<Catalog> {
        let source = card.first()?.source;
        card.iter().all(|bucket| bucket.source == source).then_some(source)
    };
    let mut cards: Vec<Option<Vec<Bucket>>> = cards.into_iter().map(Some).collect();
    let mut merged: Vec<Vec<Bucket>> = Vec::new();
    for at in 0..cards.len() {
        let Some(mut card) = cards[at].take() else {
            continue;
        };
        if let Some(source) = only_source(&card) {
            let key = card[0].album_key(releases);
            let partner = (at + 1..cards.len()).find(|&other| {
                cards[other].as_ref().is_some_and(|candidate| {
                    only_source(candidate).is_some_and(|other_source| other_source != source)
                        && candidate[0].album_key(releases) == key
                })
            });
            if let Some(partner) = partner.and_then(|other| cards[other].take()) {
                card.extend(partner);
            }
        }
        merged.push(card);
    }
    for card in &mut merged {
        card.sort_by_key(|bucket| (source_rank(bucket.source), bucket.members[0]));
    }
    merged
}

/// The card, and how much the candidate's text agrees with its best row —
/// what orders the cards against each other.
///
/// Takes the card's releases out of `releases`: each lands on exactly one
/// card.
fn build_group(
    card: Vec<Bucket>,
    releases: &mut [Option<MetadataResult>],
    pairs: &[(usize, usize)],
    judgements: &Judgements,
) -> (ReleaseGroup, u32) {
    let sources: Vec<ReleaseGroupSource> = card.iter().map(Bucket::as_source).collect();
    let members: Vec<usize> = card
        .iter()
        .flat_map(|bucket| bucket.members.iter().copied())
        .collect();
    let read = |at: usize| -> &MetadataResult {
        releases[at]
            .as_ref()
            .expect("a release is read before its card takes it")
    };
    let lead = read(
        *members
            .first()
            .expect("a card is built from at least one release"),
    );
    let id = card
        .iter()
        .find_map(|bucket| bucket.source_group_id.clone())
        .unwrap_or_else(|| lead.release_id.clone());
    let title = lead.title.clone();
    let artist = members.iter().find_map(|&at| read(at).artist.clone());
    let label = members.iter().find_map(|&at| read(at).label.clone());
    let cover_art = members.iter().find_map(|&at| read(at).cover_art.clone());
    let years: Vec<i32> = members.iter().filter_map(|&at| read(at).year).collect();
    let year_min = years.iter().min().copied();
    let year_max = years.iter().max().copied();

    let partner_of = |at: usize| -> Option<usize> {
        pairs.iter().find_map(|&(a, b)| {
            if a == at {
                Some(b)
            } else if b == at {
                Some(a)
            } else {
                None
            }
        })
    };
    let mut rows: Vec<Row> = Vec::with_capacity(members.len());
    for at in members {
        let Some(release) = releases[at].take() else {
            // Already taken as its partner's other record.
            continue;
        };
        let mut records = vec![release];
        if let Some(partner) = partner_of(at) {
            records.push(
                releases[partner]
                    .take()
                    .expect("a pair's two records are on one card"),
            );
        }
        let pressing = Pressing::of(records, judgements);
        rows.push(Row {
            agreements: pressing.agreements(judgements).count(),
            pressing,
        });
    }
    let rows = ordered_rows(rows);
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

/// Whether the source listed this release's own tracks. A record that answered
/// with a tracklist has rows to fill a draft with and lengths to check the
/// audio against; one nobody has asked yet, and one that answered with
/// nothing, have neither.
fn states_tracklist(release: &MetadataResult) -> bool {
    matches!(
        release.source_tracks,
        Some(crate::import::search::SourceTracks::Listed { .. })
    )
}

/// Where a source sits in the one order surfaces list sources in. The last
/// tie-break between records and between buckets, where nothing the folder says
/// tells them apart, and what puts a card's sources in that order: the buckets
/// a card is built from are sorted by it, and its sources are read off them.
fn source_rank(source: Catalog) -> usize {
    Catalog::ALL
        .iter()
        .position(|listed| *listed == source)
        .expect("every source is one of Catalog::ALL")
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

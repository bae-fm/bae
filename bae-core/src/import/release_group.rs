//! Release-group bundling for the import results UI.
//!
//! Import search and identification return individual releases (pressings).
//! The UI renders them grouped under the album they belong to — a
//! release-group on MusicBrainz, a master on Discogs — with one card per
//! group, and one row per physical pressing beneath it.
//!
//! The two providers answer independently, so the same album and the same
//! pressing arrive twice. Both collapses happen here, pressings first: a
//! record from each catalog becomes one row when the evidence they carry
//! says they name the same physical object — what `pressing_evidence`
//! weighs. Two records of one catalog stay two rows, because the catalog's
//! editors separated them. Groups become one card
//! when a row joins them or when they name the same album. A row is then a
//! pressing under however many records name it, and picking it claims one
//! record per catalog — [`Pressing::pick`] says exactly what.
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

/// One physical pressing, under every record that names it. A row is picked
/// whole: `releases[0]` is the release the draft is read from, and each
/// further entry is the same pressing as another record has it — another
/// catalog's, or the same catalog's second record of one object. The
/// pressing's constructor says which record that first one is.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pressing {
    pub releases: Vec<MetadataResult>,
}

impl Pressing {
    /// One pressing's records, ordered by what the folder says about each.
    ///
    /// Every record describes the same physical object, and none of them is
    /// the one the draft is read from by name. The record the candidate's own
    /// text agrees with most is; among records it says as much about, the one
    /// that states a tracklist, since the draft's rows and the settle's check
    /// of them against the audio are read out of that tracklist. Where the
    /// two are indistinguishable on both the source name decides, MusicBrainz
    /// first; and between one catalog's two records of the object, the one
    /// another record of the pressing names as the same release stands for
    /// that catalog, over the one nothing names.
    fn of(mut releases: Vec<MetadataResult>, judged: &Judgements) -> Self {
        let named: Vec<bool> = releases
            .iter()
            .map(|release| {
                let reference =
                    crate::import::MetadataRef::new(release.source, release.release_id.clone());
                releases.iter().any(|other| other.links.contains(&reference))
            })
            .collect();
        let mut order: Vec<usize> = (0..releases.len()).collect();
        order.sort_by_key(|&at| {
            let release = &releases[at];
            (
                std::cmp::Reverse(judged.of_release(release).count()),
                !states_tracklist(release),
                source_rank(release.source),
                !named[at],
            )
        });
        let mut releases: Vec<Option<MetadataResult>> = releases.drain(..).map(Some).collect();
        Self {
            releases: order
                .into_iter()
                .map(|at| releases[at].take().expect("each record is placed once"))
                .collect(),
        }
    }

    /// The release a row picks when the person picks the row itself.
    pub fn lead(&self) -> &MetadataResult {
        self.releases
            .first()
            .expect("a pressing is built from at least one release")
    }

    /// What picking this row claims, as release references: the primary — the
    /// document the draft is read from — and, for every other catalog that
    /// names the pressing, its first record of it as a partner.
    ///
    /// A row is one pressing however many records name it, so this is the
    /// whole of what picking it means. A claim names one record per catalog:
    /// where a catalog lists the object twice, the record the folder says
    /// most about stands for it and the other is the same object already
    /// claimed. Deciding it here rather than on each surface is what keeps
    /// macOS, Windows, Linux and the sweep picking the same thing.
    pub(crate) fn claims(&self) -> (crate::import::MetadataRef, Vec<crate::import::MetadataRef>) {
        let mut releases = self.releases.iter().map(|release| {
            crate::import::MetadataRef::new(release.source, release.release_id.clone())
        });
        let primary = releases
            .next()
            .expect("a pressing is built from at least one release");
        let mut partners: Vec<crate::import::MetadataRef> = Vec::new();
        for release in releases {
            let claimed = release.catalog == primary.catalog
                || partners.iter().any(|partner| partner.catalog == release.catalog);
            if !claimed {
                partners.push(release);
            }
        }
        (primary, partners)
    }

    /// What the candidate's own text agrees with about this row: every one of
    /// its records' agreements together.
    ///
    /// A row is one physical object however many records name it, and it is
    /// picked whole, so a catalog number only Discogs prints and a disc ID
    /// only MusicBrainz answers are both true of the row.
    pub fn agreements(&self, judged: &Judgements) -> Agreements {
        self.releases
            .iter()
            .fold(Agreements::NONE, |so_far, release| {
                so_far.with(judged.of_release(release))
            })
    }

    /// What picking this row claims — the primary release and each other
    /// catalog's record of the same pressing — as the provenance a pick stores.
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

/// How many pressing rows a list holds, read off the row each of its
/// releases belongs to.
///
/// The list shows one row per physical pressing and a row is picked whole, so
/// "how many pressings did this candidate match" is this number rather than
/// how many result rows came back: a MusicBrainz release and a Discogs release
/// describing the same object are one answer, not two. The Ready rule and the
/// sweep's settle step both ask it.
///
/// The rows are the run's own, so nothing counts one thing and shows another
/// — and nothing re-forms a sublist's rows, which a run's own answer is not
/// enough to rebuild: a record the run settled as ambiguous because of a
/// record in the other list rolls up when it is grouped without it.
pub fn row_count(rows: &[u32]) -> usize {
    rows.iter().collect::<std::collections::HashSet<_>>().len()
}

/// The row each of these results belongs to, formed afresh: the list is
/// grouped as it stands and the rows are numbered by where they first appear
/// in it, which is the numbering a run gives its own answers.
///
/// Forming rows is what a run does with the whole of what it found. A reader
/// of one of its lists reads the rows it recorded — [`group_formed_rows`] —
/// because that list alone does not hold what the run decided them against.
pub fn form_rows(results: &[MetadataResult]) -> Vec<u32> {
    let mut grouped: std::collections::HashMap<(Catalog, String), usize> =
        std::collections::HashMap::new();
    for (row, pressing) in group_results(unranked(results.to_vec()))
        .iter()
        .flat_map(|card| &card.pressings)
        .enumerate()
    {
        for release in &pressing.releases {
            grouped.insert((release.source, release.release_id.clone()), row);
        }
    }
    let mut numbered: Vec<usize> = Vec::new();
    results
        .iter()
        .map(|result| {
            let row = grouped
                .get(&(result.source, result.release_id.clone()))
                .copied()
                .expect("the grouping is over this list's own releases");
            match numbered.iter().position(|named| *named == row) {
                Some(at) => at as u32,
                None => {
                    numbered.push(row);
                    (numbered.len() - 1) as u32
                }
            }
        })
        .collect()
}

/// How many pressing rows these results make, by forming the rows afresh.
///
/// Production reads the rows a run built — [`row_count`] over what it
/// recorded. This forms them, which is what the grouping's own tests assert
/// about.
#[cfg(test)]
pub(crate) fn pressing_count(results: Vec<MetadataResult>) -> usize {
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
/// Pressings are matched before albums are: the releases are gathered into
/// pressings over the whole list by the evidence their records carry, so the
/// spelling of an album's title never keeps two records of one object apart.
/// Then each source's releases are bucketed by its own group, buckets a
/// pressing spans become one card, a MusicBrainz card and a Discogs card
/// whose album text agrees merge, the rows are ordered by how much of the
/// candidate's text agrees with them and then by pressing year, and the
/// cards by their best row.
pub fn group_results(results: Vec<Judged>) -> Vec<ReleaseGroup> {
    let judgements = Judgements::of(&results);
    let releases: Vec<MetadataResult> = results.into_iter().map(|(release, _)| release).collect();
    let pressings = gather_pressings(&releases);
    cards(releases, pressings, &judgements)
}

/// Group results into album cards over rows that are already formed.
///
/// `rows` says which row each result belongs to, index-aligned with
/// `results`: the numbers of a run's own grouping, as its verdict recorded
/// them. Only the cards are built here — the rows are read, never re-formed,
/// because a sublist of a run's answers does not hold what the run decided
/// them against, and grouping it alone can roll up records the run kept
/// apart.
pub fn group_formed_rows(results: Vec<Judged>, rows: &[u32]) -> Vec<ReleaseGroup> {
    assert_eq!(
        results.len(),
        rows.len(),
        "each result names the row it belongs to"
    );
    let judgements = Judgements::of(&results);
    let releases: Vec<MetadataResult> = results.into_iter().map(|(release, _)| release).collect();
    let pressings = formed_pressings(rows);
    cards(releases, pressings, &judgements)
}

/// The album cards `releases` make, given the pressing rows they are in.
fn cards(
    releases: Vec<MetadataResult>,
    pressings: Vec<Vec<usize>>,
    judgements: &Judgements,
) -> Vec<ReleaseGroup> {
    let cards = merge_buckets(bucket_by_source_group(&releases), &releases, &pressings);
    let mut releases: Vec<Option<MetadataResult>> = releases.into_iter().map(Some).collect();
    let mut cards: Vec<(ReleaseGroup, u32)> = cards
        .into_iter()
        .map(|card| build_group(card, &mut releases, &pressings, judgements))
        .collect();
    // Stable: cards nothing tells apart keep the order the signals named them
    // in, which is the order they were bucketed.
    cards.sort_by_key(|(_, best)| std::cmp::Reverse(*best));
    cards.into_iter().map(|(group, _)| group).collect()
}

/// The sets of records each already-formed row holds, in the shape
/// [`gather_pressings`] names them: indexes into the result list, in list
/// order, and only the sets of two or more, since a record alone in its row
/// is a row of its own without one here.
fn formed_pressings(rows: &[u32]) -> Vec<Vec<usize>> {
    let mut sets: Vec<(u32, Vec<usize>)> = Vec::new();
    for (at, row) in rows.iter().enumerate() {
        match sets.iter_mut().find(|(named, _)| named == row) {
            Some((_, members)) => members.push(at),
            None => sets.push((*row, vec![at])),
        }
    }
    sets.into_iter()
        .map(|(_, members)| members)
        .filter(|members| members.len() > 1)
        .collect()
}

/// The sets of records that name one pressing, as indexes into `releases`,
/// each in the order its records arrived. Only the sets of two or more: a
/// record no other names is a row of its own without one here.
///
/// Every record is weighed against every other, and only records of two
/// different catalogs can name one pressing — see
/// [`PressingEvidence::support`]. Candidates are taken from the
/// best-supported level down. At
/// each level, the candidate edges between distinct sets that are still open
/// are read together: the sets an edge chain connects become one when every
/// record across them supports every other; where they do not, a set that
/// more than one of the chain's edges names is ambiguous and is settled as
/// it stands, and a set named once stays open for the levels below. Nothing
/// depends on the order the records arrived in.
fn gather_pressings(releases: &[MetadataResult]) -> Vec<Vec<usize>> {
    let facts: Vec<PressingFacts<'_>> = releases.iter().map(PressingFacts::of).collect();
    let count = releases.len();
    let mut supported = vec![vec![false; count]; count];
    let mut edges: Vec<(Support, usize, usize)> = Vec::new();
    for a in 0..count {
        for b in a + 1..count {
            if let Some(support) = PressingEvidence::between(&facts[a], &facts[b]).support() {
                supported[a][b] = true;
                supported[b][a] = true;
                edges.push((support, a, b));
            }
        }
    }
    edges.sort_by(|(a, _, _), (b, _, _)| b.cmp(a));

    // A set is named by the lowest index in it, which holds its members.
    let mut set_of: Vec<usize> = (0..count).collect();
    let mut members: Vec<Vec<usize>> = (0..count).map(|at| vec![at]).collect();
    let mut settled = vec![false; count];
    let mut level = edges.as_slice();
    while let Some((top, _, _)) = level.first() {
        let end = level
            .iter()
            .position(|(other, _, _)| other != top)
            .unwrap_or(level.len());
        // The distinct pairs of open sets this level's edges connect.
        let mut live: Vec<(usize, usize)> = level[..end]
            .iter()
            .map(|(_, a, b)| (set_of[*a].min(set_of[*b]), set_of[*a].max(set_of[*b])))
            .filter(|(a, b)| a != b && !settled[*a] && !settled[*b])
            .collect();
        live.sort_unstable();
        live.dedup();
        for chain in chains(&live) {
            let all_support = chain.iter().enumerate().all(|(i, &x)| {
                chain[i + 1..].iter().all(|&y| {
                    members[x]
                        .iter()
                        .all(|&p| members[y].iter().all(|&q| supported[p][q]))
                })
            });
            if all_support {
                let into = chain[0];
                for &set in &chain[1..] {
                    let moved = std::mem::take(&mut members[set]);
                    for &member in &moved {
                        set_of[member] = into;
                    }
                    members[into].extend(moved);
                }
                members[into].sort_unstable();
            } else {
                for &set in &chain {
                    let named = live.iter().filter(|(a, b)| *a == set || *b == set).count();
                    if named > 1 {
                        settled[set] = true;
                    }
                }
            }
        }
        level = &level[end..];
    }
    members.into_iter().filter(|set| set.len() > 1).collect()
}

/// The groups of sets the edges connect, each sorted, in the order of their
/// lowest set.
fn chains(edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut chains: Vec<Vec<usize>> = Vec::new();
    for &(a, b) in edges {
        let of_a = chains.iter().position(|chain| chain.contains(&a));
        let of_b = chains.iter().position(|chain| chain.contains(&b));
        match (of_a, of_b) {
            (Some(x), Some(y)) if x == y => {}
            (Some(x), Some(y)) => {
                let (keep, drop) = (x.min(y), x.max(y));
                let moved = chains.remove(drop);
                chains[keep].extend(moved);
            }
            (Some(x), None) => chains[x].push(b),
            (None, Some(y)) => chains[y].push(a),
            (None, None) => chains.push(vec![a, b]),
        }
    }
    for chain in &mut chains {
        chain.sort_unstable();
    }
    chains.sort_by_key(|chain| chain[0]);
    chains
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

/// The cards the buckets make. Buckets a pressing spans are one card — the
/// album grouping the matched pressings establish, which may put more than
/// one of a source's buckets on a card. Then a card carrying only one source merges
/// with the first later card carrying only the other source whose album text
/// agrees with it. A card sits at its earliest bucket's position, and its
/// buckets are ordered by [`source_rank`] and then first-seen — the order the
/// card's title, label and cover are read in, and the order it names its
/// sources in.
fn merge_buckets(
    buckets: Vec<Bucket>,
    releases: &[MetadataResult],
    pressings: &[Vec<usize>],
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
        // Everything reachable from this bucket through pressings, in
        // first-seen order.
        let mut joined = vec![at];
        let mut next = 0;
        while next < joined.len() {
            let bucket = joined[next];
            for pressing in pressings {
                if !pressing.iter().any(|&member| bucket_of[member] == bucket) {
                    continue;
                }
                for &member in pressing {
                    let to = bucket_of[member];
                    if to != bucket && !joined.contains(&to) {
                        joined.push(to);
                    }
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
    pressings: &[Vec<usize>],
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

    let mut rows: Vec<Row> = Vec::with_capacity(members.len());
    for at in members {
        let Some(release) = releases[at].take() else {
            // Already taken as another record of its pressing.
            continue;
        };
        let mut records = vec![release];
        if let Some(pressing) = pressings.iter().find(|pressing| pressing.contains(&at)) {
            for &other in pressing.iter().filter(|&&other| other != at) {
                records.push(
                    releases[other]
                        .take()
                        .expect("a pressing's records are on one card"),
                );
            }
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

#[cfg(test)]
#[path = "release_group/ranking_tests.rs"]
mod ranking_tests;

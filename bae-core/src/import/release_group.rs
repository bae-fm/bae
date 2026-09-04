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

use crate::import::cover_art::RemoteCover;
use crate::import::search::MetadataResult;
use crate::import::types::MetadataSource;
use crate::signals::candidate_text::normalize;

/// An album, as one or both sources describe it, with the pressings they
/// surfaced for it.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq)]
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

    /// [`Self::claims`] as the provenance a pick stores.
    pub fn pick(&self) -> crate::import::MetadataProvenance {
        let (primary, partners) = self.claims();
        crate::import::MetadataProvenance::ExternalRelease {
            source: primary.source,
            release_id: primary.id,
            partners,
        }
    }
}

/// One source's bucket of releases under one of its groups.
struct Bucket {
    source: MetadataSource,
    source_group_id: Option<String>,
    releases: Vec<MetadataResult>,
}

impl Bucket {
    /// What decides whether this bucket describes the same album as another
    /// source's: the album's title and artist, normalized. `None` artist
    /// matches only `None`.
    fn album_key(&self) -> (String, Option<String>) {
        let first = self
            .releases
            .first()
            .expect("a bucket is built from at least one release");
        (
            normalize(&first.title),
            self.releases
                .iter()
                .find_map(|release| release.artist.as_deref())
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

/// Group results into album cards with one row per physical pressing.
///
/// Four steps: bucket each source's releases by its own group, merge a
/// MusicBrainz bucket with a Discogs bucket describing the same album, pair
/// the two sources' releases into shared pressing rows, and order the rows by
/// pressing year.
pub fn group_results(results: Vec<MetadataResult>) -> Vec<ReleaseGroup> {
    merge_buckets(bucket_by_source_group(results))
        .into_iter()
        .map(build_group)
        .collect()
}

/// Bucket by `(source, source_group_id)`, preserving first-seen order. A
/// result without a group id can't share one, so it becomes its own bucket.
fn bucket_by_source_group(results: Vec<MetadataResult>) -> Vec<Bucket> {
    use std::collections::HashMap;

    let mut buckets: Vec<Bucket> = Vec::new();
    let mut index: HashMap<(MetadataSource, String), usize> = HashMap::new();
    for result in results {
        match result.source_group_id.clone() {
            Some(group_id) => {
                let key = (result.source, group_id.clone());
                match index.get(&key) {
                    Some(&at) => buckets[at].releases.push(result),
                    None => {
                        index.insert(key, buckets.len());
                        buckets.push(Bucket {
                            source: result.source,
                            source_group_id: Some(group_id),
                            releases: vec![result],
                        });
                    }
                }
            }
            None => buckets.push(Bucket {
                source: result.source,
                source_group_id: None,
                releases: vec![result],
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

fn build_group(card: Vec<Bucket>) -> ReleaseGroup {
    let sources: Vec<ReleaseGroupSource> = card.iter().map(Bucket::as_source).collect();
    let releases: Vec<&MetadataResult> = card
        .iter()
        .flat_map(|bucket| bucket.releases.iter())
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
        Some(second) => pair_pressings(first.releases, second.releases),
        None => first
            .releases
            .into_iter()
            .map(|release| Pressing {
                releases: vec![release],
            })
            .collect(),
    };
    let pressings = ordered_by_year(pressings);

    ReleaseGroup {
        id,
        title,
        artist,
        label,
        cover_art,
        sources,
        year_min,
        year_max,
        pressings,
    }
}

/// Pair the two sources' releases into shared pressing rows. A barcode both
/// state is the strongest evidence that they name the same physical object, so
/// every barcode pair is taken before any catalog-number pair; each release
/// pairs at most once, and what is left over is its own single-source row.
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
            let found = other.iter().enumerate().position(|(index, candidate)| {
                !taken.contains(&index)
                    && candidate
                        .as_ref()
                        .is_some_and(|candidate| key_of(candidate).as_deref() == Some(key.as_str()))
            });
            partners[at] = found;
        }
    }

    let mut pressings: Vec<Pressing> = Vec::with_capacity(lead.len() + other.len());
    for (release, partner) in lead.into_iter().zip(&partners) {
        let mut releases = vec![release];
        if let Some(partner) = partner.and_then(|at| other[at].take()) {
            releases.push(partner);
        }
        pressings.push(Pressing { releases });
    }
    pressings.extend(other.into_iter().flatten().map(|release| Pressing {
        releases: vec![release],
    }));
    pressings
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

/// Order the rows by the year the row's lead release was pressed, earliest
/// first; a pressing whose year nobody states goes last. Stable, so rows that
/// share a year keep the order the sources listed them in.
fn ordered_by_year(mut pressings: Vec<Pressing>) -> Vec<Pressing> {
    pressings.sort_by_key(|pressing| (pressing.lead().year.is_none(), pressing.lead().year));
    pressings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mb(release_id: &str, group_id: Option<&str>, year: Option<i32>) -> MetadataResult {
        MetadataResult {
            source: MetadataSource::MusicBrainz,
            release_id: release_id.to_string(),
            title: "Album Title".to_string(),
            artist: Some("Artist Name".to_string()),
            year,
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
            cover_art: None,
            source_group_id: group_id.map(str::to_string),
            source_tracks: None,
        }
    }

    /// The same album on Discogs, whose group is a master and whose card URL
    /// therefore differs from MusicBrainz's.
    fn discogs(release_id: &str, group_id: Option<&str>, year: Option<i32>) -> MetadataResult {
        MetadataResult {
            source: MetadataSource::Discogs,
            ..mb(release_id, group_id, year)
        }
    }

    fn cover() -> RemoteCover {
        RemoteCover {
            url: "https://caa.example/front.jpg".to_string(),
            thumbnail_url: "https://caa.example/thumb.jpg".to_string(),
            label: MetadataSource::MusicBrainz.cover_source_label().to_string(),
            source: MetadataSource::MusicBrainz,
        }
    }

    fn lead_ids(group: &ReleaseGroup) -> Vec<Vec<&str>> {
        group
            .pressings
            .iter()
            .map(|pressing| {
                pressing
                    .releases
                    .iter()
                    .map(|release| release.release_id.as_str())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn same_group_collapses_into_one_card() {
        let groups = group_results(vec![
            mb("rel-1", Some("group-x"), Some(1992)),
            mb("rel-2", Some("group-x"), Some(2012)),
        ]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, "group-x");
        assert_eq!(groups[0].pressings.len(), 2);
        assert_eq!(
            groups[0].sources,
            vec![ReleaseGroupSource {
                source: MetadataSource::MusicBrainz,
                group_url: Some("https://musicbrainz.org/release-group/group-x".to_string()),
            }]
        );
    }

    #[test]
    fn distinct_groups_keep_first_seen_order() {
        let mut second = mb("rel-2", Some("group-a"), None);
        second.title = "Other Album".to_string();
        let groups = group_results(vec![
            mb("rel-1", Some("group-b"), None),
            second,
            mb("rel-3", Some("group-b"), None),
        ]);
        assert_eq!(
            groups.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(),
            ["group-b", "group-a"]
        );
        assert_eq!(groups[0].pressings.len(), 2);
        assert_eq!(groups[1].pressings.len(), 1);
    }

    #[test]
    fn ungrouped_result_is_its_own_single_pressing_card() {
        let groups = group_results(vec![mb("rel-1", None, Some(1999))]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, "rel-1");
        assert_eq!(
            groups[0].sources,
            vec![ReleaseGroupSource {
                source: MetadataSource::MusicBrainz,
                group_url: None,
            }]
        );
        assert_eq!(groups[0].year_min, Some(1999));
        assert_eq!(groups[0].year_max, Some(1999));
    }

    /// Two ungrouped MusicBrainz results are two albums as far as MusicBrainz
    /// is concerned: the cross-source merge never merges one source with
    /// itself, whatever the titles say.
    #[test]
    fn two_ungrouped_results_from_one_source_do_not_merge() {
        let groups = group_results(vec![mb("rel-1", None, None), mb("rel-2", None, None)]);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn two_musicbrainz_groups_never_merge_with_each_other() {
        let groups = group_results(vec![
            mb("rel-1", Some("group-a"), None),
            mb("rel-2", Some("group-b"), None),
        ]);
        assert_eq!(groups.len(), 2);
    }

    /// The two providers describing the same album become one card carrying
    /// both, MusicBrainz first, each with its own editorial page.
    #[test]
    fn the_same_album_across_sources_merges_into_one_card() {
        let groups = group_results(vec![
            discogs("dg-1", Some("master-7"), Some(2001)),
            mb("mb-1", Some("group-x"), Some(1992)),
        ]);
        assert_eq!(groups.len(), 1);
        // The Discogs bucket was seen first, so the card sits at its position
        // — but MusicBrainz leads the sources and names the card.
        assert_eq!(groups[0].id, "group-x");
        assert_eq!(
            groups[0].sources,
            vec![
                ReleaseGroupSource {
                    source: MetadataSource::MusicBrainz,
                    group_url: Some("https://musicbrainz.org/release-group/group-x".to_string()),
                },
                ReleaseGroupSource {
                    source: MetadataSource::Discogs,
                    group_url: Some("https://www.discogs.com/master/master-7".to_string()),
                },
            ]
        );
        assert_eq!(groups[0].year_min, Some(1992));
        assert_eq!(groups[0].year_max, Some(2001));
    }

    #[test]
    fn different_titles_across_sources_stay_apart() {
        let mut other = discogs("dg-1", Some("master-7"), None);
        other.title = "Another Album".to_string();
        let groups = group_results(vec![mb("mb-1", Some("group-x"), None), other]);
        assert_eq!(groups.len(), 2);
    }

    /// Casing and punctuation differences in the title are not different
    /// albums; a different artist is.
    #[test]
    fn the_album_key_ignores_case_and_edge_punctuation() {
        let mut other = discogs("dg-1", Some("master-7"), None);
        other.title = "  album title!".to_string();
        let groups = group_results(vec![mb("mb-1", Some("group-x"), None), other]);
        assert_eq!(groups.len(), 1);

        let mut different_artist = discogs("dg-2", Some("master-8"), None);
        different_artist.artist = Some("Other Artist".to_string());
        let groups = group_results(vec![mb("mb-2", Some("group-y"), None), different_artist]);
        assert_eq!(groups.len(), 2);
    }

    /// A named artist and no artist at all are not the same album.
    #[test]
    fn an_absent_artist_matches_only_an_absent_artist() {
        let mut anonymous = discogs("dg-1", Some("master-7"), None);
        anonymous.artist = None;
        let groups = group_results(vec![mb("mb-1", Some("group-x"), None), anonymous.clone()]);
        assert_eq!(groups.len(), 2);

        let mut also_anonymous = mb("mb-2", Some("group-y"), None);
        also_anonymous.artist = None;
        let groups = group_results(vec![also_anonymous, anonymous]);
        assert_eq!(groups.len(), 1);
    }

    /// Only one bucket per source merges into a card: a second Discogs master
    /// with the same title stays its own card rather than joining.
    #[test]
    fn each_bucket_merges_at_most_once() {
        let groups = group_results(vec![
            mb("mb-1", Some("group-x"), None),
            discogs("dg-1", Some("master-7"), None),
            discogs("dg-2", Some("master-8"), None),
        ]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].sources.len(), 2);
        assert_eq!(groups[1].sources.len(), 1);
    }

    /// A row paired across both sources is picked whole: the lead is the
    /// document the draft is read from, and the other source's record of the
    /// same pressing rides along as a partner.
    #[test]
    fn a_paired_row_is_picked_with_its_partner() {
        let mut one = mb("mb-1", Some("group-x"), Some(1992));
        one.barcode = Some("012345678905".to_string());
        let mut other = discogs("dg-1", Some("master-7"), Some(1992));
        other.barcode = Some("012345678905".to_string());

        let groups = group_results(vec![one, other]);
        assert_eq!(
            groups[0].pressings[0].pick(),
            crate::import::MetadataProvenance::ExternalRelease {
                source: MetadataSource::MusicBrainz,
                release_id: "mb-1".to_string(),
                partners: vec![crate::import::MetadataRef::new(
                    "dg-1",
                    MetadataSource::Discogs
                )],
            }
        );
    }

    /// A pressing only one source lists claims only that source.
    #[test]
    fn a_lone_row_is_picked_with_no_partner() {
        let groups = group_results(vec![discogs("dg-1", Some("master-7"), Some(1992))]);
        assert_eq!(
            groups[0].pressings[0].pick(),
            crate::import::MetadataProvenance::ExternalRelease {
                source: MetadataSource::Discogs,
                release_id: "dg-1".to_string(),
                partners: vec![],
            }
        );
    }

    /// Barcodes the two sources punctuate differently still name one pressing.
    #[test]
    fn releases_sharing_a_barcode_are_one_pressing() {
        let mut one = mb("mb-1", Some("group-x"), Some(1992));
        one.barcode = Some("0 12345 67890 5".to_string());
        let mut other = discogs("dg-1", Some("master-7"), Some(1992));
        other.barcode = Some("012345678905".to_string());

        let groups = group_results(vec![one, other]);
        assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);
    }

    #[test]
    fn releases_sharing_a_catalog_number_are_one_pressing() {
        let mut one = mb("mb-1", Some("group-x"), Some(1992));
        one.catalog_number = Some("CAT-7 ".to_string());
        let mut other = discogs("dg-1", Some("master-7"), Some(1992));
        other.catalog_number = Some("cat-7".to_string());

        let groups = group_results(vec![one, other]);
        assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);
    }

    /// The sources punctuate multi-disc catalog numbers differently, and this
    /// pairing is not clever enough to tell "the same number, spelled
    /// differently" from "a different number" — so it declines to pair.
    #[test]
    fn a_formatting_difference_in_the_catalog_number_does_not_pair() {
        let mut one = mb("mb-1", Some("group-x"), Some(1992));
        one.catalog_number = Some("CAT 2 2".to_string());
        let mut other = discogs("dg-1", Some("master-7"), Some(1992));
        other.catalog_number = Some("CAT 2-2".to_string());

        let groups = group_results(vec![one, other]);
        assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1"], vec!["dg-1"]]);
    }

    /// A barcode is stronger evidence than a catalog number, so the barcode
    /// pair is taken even though an earlier release shares a catalog number
    /// with the same Discogs row.
    #[test]
    fn a_barcode_pair_outranks_a_catalog_pair_for_the_same_release() {
        let mut catalog_only = mb("mb-catalog", Some("group-x"), Some(1992));
        catalog_only.catalog_number = Some("CAT-7".to_string());
        let mut barcoded = mb("mb-barcode", Some("group-x"), Some(1994));
        barcoded.barcode = Some("012345678905".to_string());
        barcoded.catalog_number = Some("CAT-7".to_string());
        let mut other = discogs("dg-1", Some("master-7"), Some(1994));
        other.barcode = Some("012345678905".to_string());
        other.catalog_number = Some("CAT-7".to_string());

        let groups = group_results(vec![catalog_only, barcoded, other]);
        assert_eq!(
            lead_ids(&groups[0]),
            vec![vec!["mb-catalog"], vec!["mb-barcode", "dg-1"]]
        );
    }

    /// Releases with nothing to pair on stay their own rows, and the Discogs
    /// leftovers land as single-source pressings.
    #[test]
    fn unpaired_releases_are_single_source_pressings() {
        let groups = group_results(vec![
            mb("mb-1", Some("group-x"), Some(1992)),
            discogs("dg-1", Some("master-7"), Some(2001)),
        ]);
        assert_eq!(
            lead_ids(&groups[0]),
            vec![vec!["mb-1"], vec!["dg-1"]],
            "one card, two rows"
        );
    }

    #[test]
    fn rows_are_ordered_by_pressing_year_with_unknown_years_last() {
        let groups = group_results(vec![
            mb("rel-undated", Some("group-x"), None),
            mb("rel-2012", Some("group-x"), Some(2012)),
            mb("rel-1992", Some("group-x"), Some(1992)),
        ]);
        assert_eq!(
            lead_ids(&groups[0]),
            vec![vec!["rel-1992"], vec!["rel-2012"], vec!["rel-undated"]]
        );
        assert_eq!(groups[0].year_min, Some(1992));
        assert_eq!(groups[0].year_max, Some(2012));
    }

    #[test]
    fn year_span_is_none_when_no_pressing_carries_a_year() {
        let groups = group_results(vec![mb("rel-1", Some("group-x"), None)]);
        assert_eq!(groups[0].year_min, None);
        assert_eq!(groups[0].year_max, None);
    }

    #[test]
    fn the_card_label_is_the_first_pressing_that_names_one() {
        let mut unlabelled = mb("rel-1", Some("group-x"), Some(1992));
        unlabelled.label = None;
        let mut labelled = mb("rel-2", Some("group-x"), Some(1994));
        labelled.label = Some("Label Name".to_string());
        let mut later = mb("rel-3", Some("group-x"), Some(2012));
        later.label = Some("Reissue Records".to_string());

        let groups = group_results(vec![unlabelled, labelled, later]);
        assert_eq!(groups[0].label.as_deref(), Some("Label Name"));
    }

    #[test]
    fn a_card_whose_pressings_name_no_label_has_none() {
        let groups = group_results(vec![mb("rel-1", Some("group-x"), Some(1992))]);
        assert_eq!(groups[0].label, None);
    }

    #[test]
    fn representative_cover_preserves_remote_cover_pair() {
        let cover = cover();
        let mut first = mb("rel-1", Some("group-x"), Some(1992));
        first.cover_art = Some(cover.clone());

        let groups = group_results(vec![first, mb("rel-2", Some("group-x"), Some(1994))]);

        assert_eq!(groups[0].cover_art, Some(cover));
    }

    /// A merged card takes its cover from MusicBrainz when both sources offer
    /// one, whichever bucket was seen first.
    #[test]
    fn a_merged_card_prefers_the_musicbrainz_cover() {
        let mut discogs_covered = discogs("dg-1", Some("master-7"), Some(2001));
        discogs_covered.cover_art = Some(RemoteCover {
            url: "https://discogs.example/front.jpg".to_string(),
            thumbnail_url: "https://discogs.example/thumb.jpg".to_string(),
            label: MetadataSource::Discogs.cover_source_label().to_string(),
            source: MetadataSource::Discogs,
        });
        let mut mb_covered = mb("mb-1", Some("group-x"), Some(1992));
        mb_covered.cover_art = Some(cover());

        let groups = group_results(vec![discogs_covered, mb_covered]);
        assert_eq!(groups[0].cover_art, Some(cover()));
    }

    /// Two results carrying the same `source_group_id` string but different
    /// sources are still bucketed apart; only the album key merges them, and
    /// here the titles differ.
    #[test]
    fn the_same_group_id_across_sources_does_not_collide() {
        let mut one = mb("rel-mb", Some("shared-id"), Some(2001));
        one.title = "Album One".to_string();
        let mut other = discogs("rel-dg", Some("shared-id"), Some(2001));
        other.title = "Album Two".to_string();

        let groups = group_results(vec![one, other]);

        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups
                .iter()
                .map(|group| group.sources[0].source)
                .collect::<Vec<_>>(),
            vec![MetadataSource::MusicBrainz, MetadataSource::Discogs]
        );
    }
}

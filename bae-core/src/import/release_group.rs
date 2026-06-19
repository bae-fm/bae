//! Release-group bundling for the import results UI.
//!
//! Import search and auto-identify return individual releases (pressings).
//! The UI renders them grouped under the album they belong to — a
//! release-group on MusicBrainz, a master on Discogs — with one card per
//! group. This module does the grouping and pre-formats the card's display
//! labels (source name, editorial URL, year span + pressing count) so the UI
//! just iterates and renders.

use crate::identify::GroupKey;
use crate::import::cover_art::RemoteCover;
use crate::import::search::MetadataResult;
use crate::import::types::MetadataSource;

/// An album's release group plus the pressings the search/identify surfaced
/// for it, with the display labels the group card needs.
#[derive(Debug, Clone)]
pub struct ReleaseGroup {
    /// Stable card identity: the shared `source_group_id` when the pressings
    /// belong to one group, otherwise the lone pressing's release id (an
    /// ungrouped result is its own single-pressing card).
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    /// Representative cover for the card — the first pressing that surfaced one.
    pub cover_art: Option<RemoteCover>,
    /// Human-readable source name ("MusicBrainz" / "Discogs").
    pub source_label: String,
    /// Editorial URL for the group on its source (release-group on
    /// MusicBrainz, master on Discogs). `None` for an ungrouped result, which
    /// has no group page to open.
    pub group_url: Option<String>,
    /// Earliest and latest pressing year, for the UI's "1992 – 2012" span. Both
    /// `None` when no pressing carries a year. The UI pluralizes the pressing
    /// count from `pressings.len()`.
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    pub pressings: Vec<MetadataResult>,
}

impl ReleaseGroup {
    /// Build the single release group of an auto-identify `Found` result. Every
    /// match shares `group` by construction, so the card's id and editorial URL
    /// come straight from the authoritative `GroupKey` rather than being
    /// re-derived from a pressing.
    pub fn from_group(group: GroupKey, pressings: Vec<MetadataResult>) -> Self {
        let group_url = Some(group.source.group_url(&group.source_group_id));
        Self::build(group.source_group_id, group.source, group_url, pressings)
    }

    /// Assemble a group from its (non-empty) pressings, deriving the card's
    /// title, artist, representative cover, and meta label.
    fn build(
        id: String,
        source: MetadataSource,
        group_url: Option<String>,
        pressings: Vec<MetadataResult>,
    ) -> Self {
        let first = pressings
            .first()
            .expect("release group built from at least one pressing");
        let title = first.title.clone();
        let artist = pressings.iter().find_map(|p| p.artist.clone());
        let cover_art = pressings.iter().find_map(|p| p.cover_art.clone());
        let years: Vec<i32> = pressings.iter().filter_map(|p| p.year).collect();
        let year_min = years.iter().min().copied();
        let year_max = years.iter().max().copied();
        Self {
            id,
            title,
            artist,
            cover_art,
            source_label: source.display_name().to_string(),
            group_url,
            year_min,
            year_max,
            pressings,
        }
    }
}

/// Group search results by `(source, source_group_id)`, preserving the order
/// in which each group first appears (and pressing order within a group). A
/// result without a `source_group_id` can't share a group, so it becomes its
/// own single-pressing card keyed by its release id.
pub fn group_results(results: Vec<MetadataResult>) -> Vec<ReleaseGroup> {
    use std::collections::HashMap;

    // Bucket pressings by group, preserving first-seen order via `buckets`.
    // `index` maps a grouped key to its bucket; ungrouped results bypass it so
    // they never merge with anything else.
    let mut buckets: Vec<(MetadataSource, Option<String>, Vec<MetadataResult>)> = Vec::new();
    let mut index: HashMap<(MetadataSource, String), usize> = HashMap::new();
    for r in results {
        match r.source_group_id.clone() {
            Some(gid) => {
                let key = (r.source, gid.clone());
                match index.get(&key) {
                    Some(&i) => buckets[i].2.push(r),
                    None => {
                        index.insert(key, buckets.len());
                        buckets.push((r.source, Some(gid), vec![r]));
                    }
                }
            }
            None => buckets.push((r.source, None, vec![r])),
        }
    }

    buckets
        .into_iter()
        .map(|(source, gid, pressings)| match gid {
            Some(gid) => {
                let group_url = Some(source.group_url(&gid));
                ReleaseGroup::build(gid, source, group_url, pressings)
            }
            None => {
                let id = pressings
                    .first()
                    .expect("ungrouped bucket built from its lone pressing")
                    .release_id
                    .clone();
                ReleaseGroup::build(id, source, None, pressings)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn result(release_id: &str, group_id: Option<&str>, year: Option<i32>) -> MetadataResult {
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
            cover_art: None,
            source_group_id: group_id.map(str::to_string),
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

    #[test]
    fn same_group_collapses_into_one_card() {
        let groups = group_results(vec![
            result("rel-1", Some("group-x"), Some(1992)),
            result("rel-2", Some("group-x"), Some(2012)),
        ]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, "group-x");
        assert_eq!(groups[0].pressings.len(), 2);
        assert_eq!(
            groups[0].group_url.as_deref(),
            Some("https://musicbrainz.org/release-group/group-x")
        );
    }

    #[test]
    fn distinct_groups_keep_first_seen_order() {
        let groups = group_results(vec![
            result("rel-1", Some("group-b"), None),
            result("rel-2", Some("group-a"), None),
            result("rel-3", Some("group-b"), None),
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
        let groups = group_results(vec![result("rel-1", None, Some(1999))]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, "rel-1");
        assert_eq!(groups[0].group_url, None);
        assert_eq!(groups[0].year_min, Some(1999));
        assert_eq!(groups[0].year_max, Some(1999));
    }

    #[test]
    fn two_ungrouped_results_do_not_merge() {
        let groups = group_results(vec![
            result("rel-1", None, None),
            result("rel-2", None, None),
        ]);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn year_span_uses_min_and_max() {
        let groups = group_results(vec![
            result("rel-1", Some("g"), Some(1992)),
            result("rel-2", Some("g"), Some(2006)),
            result("rel-3", Some("g"), Some(2012)),
        ]);
        assert_eq!(groups[0].year_min, Some(1992));
        assert_eq!(groups[0].year_max, Some(2012));
        assert_eq!(groups[0].pressings.len(), 3);
    }

    #[test]
    fn year_span_collapses_when_equal() {
        let groups = group_results(vec![
            result("rel-1", Some("g"), Some(1994)),
            result("rel-2", Some("g"), Some(1994)),
        ]);
        assert_eq!(groups[0].year_min, Some(1994));
        assert_eq!(groups[0].year_max, Some(1994));
    }

    #[test]
    fn year_span_is_none_when_no_year() {
        let groups = group_results(vec![result("rel-1", Some("g"), None)]);
        assert_eq!(groups[0].year_min, None);
        assert_eq!(groups[0].year_max, None);
    }

    #[test]
    fn from_group_uses_group_key_for_id_and_url() {
        let group = GroupKey {
            source: MetadataSource::Discogs,
            source_group_id: "master-7".to_string(),
        };
        let rg =
            ReleaseGroup::from_group(group, vec![result("rel-1", Some("master-7"), Some(2001))]);
        assert_eq!(rg.id, "master-7");
        assert_eq!(rg.source_label, "Discogs");
        assert_eq!(
            rg.group_url.as_deref(),
            Some("https://www.discogs.com/master/master-7")
        );
        assert_eq!(rg.title, "Album Title");
        assert_eq!(rg.artist.as_deref(), Some("Artist Name"));
    }

    #[test]
    fn representative_cover_preserves_remote_cover_pair() {
        let cover = cover();
        let mut first = result("rel-1", Some("g"), Some(1992));
        first.cover_art = Some(cover.clone());

        let groups = group_results(vec![first, result("rel-2", Some("g"), Some(1994))]);

        assert_eq!(groups[0].cover_art, Some(cover));
    }
}

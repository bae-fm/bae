//! Release-group bundling for the import results UI.
//!
//! Import search and auto-identify return individual releases (pressings).
//! The UI renders them grouped under the album they belong to — a
//! release-group on MusicBrainz, a master on Discogs — with one card per
//! group. This module does the grouping and pre-formats the card's display
//! labels (source name, editorial URL, year span + pressing count) so the UI
//! just iterates and renders.

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
    /// The metadata source's group identity. Absent when the source returned
    /// an ungrouped release, whose card identity is its release id instead.
    pub source_group_id: Option<String>,
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
    /// Assemble a group from its (non-empty) pressings, deriving the card's
    /// title, artist, representative cover, and meta label.
    fn build(
        source_group_id: Option<String>,
        source: MetadataSource,
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
        let (id, group_url) = match source_group_id.as_deref() {
            Some(group_id) => (group_id.to_string(), Some(source.group_url(group_id))),
            None => (first.release_id.clone(), None),
        };
        Self {
            id,
            source_group_id,
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
        .map(|(source, source_group_id, pressings)| {
            ReleaseGroup::build(source_group_id, source, pressings)
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
            source_tracks: None,
        }
    }

    /// The same result on Discogs, whose group is a master and whose card URL
    /// therefore differs from MusicBrainz's.
    fn discogs_result(
        release_id: &str,
        group_id: Option<&str>,
        year: Option<i32>,
    ) -> MetadataResult {
        MetadataResult {
            source: MetadataSource::Discogs,
            ..result(release_id, group_id, year)
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
            result(
                "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
                Some("group-x"),
                Some(1992),
            ),
            result(
                "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
                Some("group-x"),
                Some(2012),
            ),
        ]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, "group-x");
        assert_eq!(groups[0].source_group_id.as_deref(), Some("group-x"));
        assert_eq!(groups[0].pressings.len(), 2);
        assert_eq!(
            groups[0].group_url.as_deref(),
            Some("https://musicbrainz.org/release-group/group-x")
        );
    }

    #[test]
    fn distinct_groups_keep_first_seen_order() {
        let groups = group_results(vec![
            result(
                "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
                Some("group-b"),
                None,
            ),
            result(
                "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
                Some("group-a"),
                None,
            ),
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
        let groups = group_results(vec![result(
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
            None,
            Some(1999),
        )]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e");
        assert_eq!(groups[0].source_group_id, None);
        assert_eq!(groups[0].group_url, None);
        assert_eq!(groups[0].year_min, Some(1999));
        assert_eq!(groups[0].year_max, Some(1999));
    }

    #[test]
    fn two_ungrouped_results_do_not_merge() {
        let groups = group_results(vec![
            result("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", None, None),
            result("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", None, None),
        ]);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn year_span_uses_min_and_max() {
        let groups = group_results(vec![
            result(
                "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
                Some("g"),
                Some(1992),
            ),
            result(
                "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
                Some("g"),
                Some(2006),
            ),
            result("rel-3", Some("g"), Some(2012)),
        ]);
        assert_eq!(groups[0].year_min, Some(1992));
        assert_eq!(groups[0].year_max, Some(2012));
        assert_eq!(groups[0].pressings.len(), 3);
    }

    /// Two results carrying the same `source_group_id` string but different
    /// sources (MB vs Discogs) do not collide — the group key is
    /// `(source, source_group_id)`, so they become two separate cards.
    #[test]
    fn same_group_id_across_sources_does_not_collide() {
        let mut mb = result("rel-mb", Some("shared-id"), Some(2001));
        mb.source = MetadataSource::MusicBrainz;
        let mut discogs = result("rel-dg", Some("shared-id"), Some(2001));
        discogs.source = MetadataSource::Discogs;

        let groups = group_results(vec![mb, discogs]);

        assert_eq!(
            groups.len(),
            2,
            "same group_id across sources stays separate"
        );
        assert!(groups
            .iter()
            .any(|g| g.source_label == "MusicBrainz" && g.pressings[0].release_id == "rel-mb"));
        assert!(groups
            .iter()
            .any(|g| g.source_label == "Discogs" && g.pressings[0].release_id == "rel-dg"));
    }

    #[test]
    fn year_span_is_none_when_no_year() {
        let groups = group_results(vec![result(
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
            Some("g"),
            None,
        )]);
        assert_eq!(groups[0].year_min, None);
        assert_eq!(groups[0].year_max, None);
    }

    #[test]
    fn a_grouped_result_takes_its_group_id_and_url() {
        let groups = group_results(vec![discogs_result(
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
            Some("master-7"),
            Some(2001),
        )]);
        let rg = &groups[0];
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
        let mut first = result(
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
            Some("g"),
            Some(1992),
        );
        first.cover_art = Some(cover.clone());

        let groups = group_results(vec![
            first,
            result(
                "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
                Some("g"),
                Some(1994),
            ),
        ]);

        assert_eq!(groups[0].cover_art, Some(cover));
    }
}

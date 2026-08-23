//! Combine logic for the triangulation pipeline.
//!
//! Once both signals settle, the reducer hands their results and the candidate's
//! catalog candidates to `combine_results`, which intersects them (or falls through
//! to whichever signal had results) and narrows by catalog match. Pure: no I/O,
//! no state.
//!
//! Every checked signal is a claim about the same disc, so an intersection is
//! what agreement looks like. Signals that do not intersect are not a failure
//! to identify: both saw something, and the union of what they saw is the set
//! the user picks from, each row carrying which signal produced it.

use crate::db::LibraryStatus;
use crate::import::search::MetadataResult;
use crate::import::MetadataSource;
use crate::signals::SourcedValue;

/// Which signals produced or confirmed one result, for the UI's per-row badges.
/// `by_disc_id` / `by_barcode`: the result came back from that signal's lookup.
/// `matches_catalog`: its catalog number matches one harvested from the candidate.
///
/// `Serialize`/`Deserialize`: carried on `identify::TerminalVerdict::Found`,
/// which `import_candidate_state` persists.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResultProvenance {
    pub by_disc_id: bool,
    pub by_barcode: bool,
    pub matches_catalog: bool,
}

/// What combine decided; the reducer lifts it into a terminal `IdentifyState`.
#[derive(Debug, Clone)]
pub enum CombineOutcome {
    /// One or more results. `provenance` is index-aligned with `matches` and
    /// says which signal produced each one.
    Found {
        matches: Vec<MetadataResult>,
        library_statuses: Vec<LibraryStatus>,
        provenance: Vec<ResultProvenance>,
    },
    /// Both signals settled with zero results.
    NotFoundAnywhere,
}

/// Settle both signals' results into a `CombineOutcome`, in three steps:
///
/// 1. **Combine.** Both non-empty: intersect by `(source, release_id)`, keeping
///    disc-ID order. One non-empty: that one is the set. Both empty:
///    `NotFoundAnywhere`.
/// 2. **Empty intersection.** Only an intersection can empty the set, so an
///    empty set here means both signals had results and shared none. Neither is
///    wrong about having seen something, so the set becomes their union —
///    disc-ID order first, then the barcode-only ones — and each row says which
///    signal produced it.
/// 3. **Catalog filter.** Narrow to the results a catalog candidate confirms, but
///    only if that leaves at least one — a filter that empties the set would lose
///    real signal.
pub fn combine_results(
    discid_results: Vec<(MetadataResult, LibraryStatus)>,
    barcode_results: Vec<(MetadataResult, LibraryStatus)>,
    catalog_candidates: &[SourcedValue],
) -> CombineOutcome {
    use std::collections::HashSet;

    let discid_empty = discid_results.is_empty();
    let barcode_empty = barcode_results.is_empty();

    if discid_empty && barcode_empty {
        return CombineOutcome::NotFoundAnywhere;
    }

    // Provenance says which signal's set a release came from, so capture membership
    // before the working set narrows it away.
    let discid_keys: HashSet<(MetadataSource, String)> = discid_results
        .iter()
        .map(|(r, _)| (r.source, r.release_id.clone()))
        .collect();
    let barcode_keys: HashSet<(MetadataSource, String)> = barcode_results
        .iter()
        .map(|(r, _)| (r.source, r.release_id.clone()))
        .collect();

    // Intersect when both signals have results, else take the non-empty side. Order
    // follows disc-ID, the more authoritative signal.
    let combined: Vec<(MetadataResult, LibraryStatus)> = if !discid_empty && !barcode_empty {
        intersect_by_release(&discid_results, &barcode_results)
    } else if !discid_empty {
        discid_results.clone()
    } else {
        barcode_results.clone()
    };

    // Both signals found something but nothing in common. Each still saw a real
    // release, so the set is their union and the user picks from it.
    let combined = if combined.is_empty() {
        union_by_release(discid_results, barcode_results)
    } else {
        combined
    };

    let filtered = apply_catalog_filter(combined, catalog_candidates);

    let provenance: Vec<ResultProvenance> = filtered
        .iter()
        .map(|(r, _)| ResultProvenance {
            by_disc_id: discid_keys.contains(&(r.source, r.release_id.clone())),
            by_barcode: barcode_keys.contains(&(r.source, r.release_id.clone())),
            matches_catalog: catalog_matches(r.catalog_number.as_deref(), catalog_candidates),
        })
        .collect();
    let (matches, library_statuses) = unzip_results(filtered);
    CombineOutcome::Found {
        matches,
        library_statuses,
        provenance,
    }
}

/// Both sets, disc-ID order first, then the barcode results the disc ID did not
/// already name. Only reached when the two share nothing, so in practice the
/// second half is all of it — the de-duplication is what keeps that a property
/// of the data rather than a thing the caller has to have checked.
fn union_by_release(
    discid_results: Vec<(MetadataResult, LibraryStatus)>,
    barcode_results: Vec<(MetadataResult, LibraryStatus)>,
) -> Vec<(MetadataResult, LibraryStatus)> {
    use std::collections::HashSet;
    let seen: HashSet<(MetadataSource, String)> = discid_results
        .iter()
        .map(|(r, _)| (r.source, r.release_id.clone()))
        .collect();
    discid_results
        .into_iter()
        .chain(
            barcode_results
                .into_iter()
                .filter(|(r, _)| !seen.contains(&(r.source, r.release_id.clone()))),
        )
        .collect()
}

/// Intersect by `(source, release_id)`. Order follows the first vec.
fn intersect_by_release(
    a: &[(MetadataResult, LibraryStatus)],
    b: &[(MetadataResult, LibraryStatus)],
) -> Vec<(MetadataResult, LibraryStatus)> {
    use std::collections::HashSet;
    let b_keys: HashSet<(MetadataSource, &str)> = b
        .iter()
        .map(|(r, _)| (r.source, r.release_id.as_str()))
        .collect();
    a.iter()
        .filter(|(r, _)| b_keys.contains(&(r.source, r.release_id.as_str())))
        .cloned()
        .collect()
}

/// The narrowed set when at least one result matches a confirming candidate, else
/// the input unchanged.
fn apply_catalog_filter(
    combined: Vec<(MetadataResult, LibraryStatus)>,
    candidates: &[SourcedValue],
) -> Vec<(MetadataResult, LibraryStatus)> {
    let filtered: Vec<(MetadataResult, LibraryStatus)> = combined
        .iter()
        .filter(|(r, _)| catalog_matches(r.catalog_number.as_deref(), candidates))
        .cloned()
        .collect();
    if filtered.is_empty() {
        combined
    } else {
        filtered
    }
}

/// Whether a result's catalog number matches any candidate. Also drives the
/// per-result "catalog confirmed" badge, whether or not the filter narrowed
/// anything.
fn catalog_matches(catalog_number: Option<&str>, candidates: &[SourcedValue]) -> bool {
    candidates
        .iter()
        .any(|c| catalog_matches_candidate(catalog_number, c))
}

/// Lowercase, strip everything that isn't `[a-z0-9]`. Aligns OCR variants
/// like `CAT 80001` against a registry's `CAT-80001`.
pub(crate) fn normalize_catalog(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// A match needs both: the candidate's origin is one that may confirm a catalog,
/// and the two values are equal once normalized.
pub(crate) fn catalog_matches_candidate(
    catalog_number: Option<&str>,
    candidate: &SourcedValue,
) -> bool {
    candidate.origin.can_confirm_catalog()
        && catalog_number
            .is_some_and(|c| normalize_catalog(c) == normalize_catalog(&candidate.value))
}

fn unzip_results(
    pairs: Vec<(MetadataResult, LibraryStatus)>,
) -> (Vec<MetadataResult>, Vec<LibraryStatus>) {
    pairs.into_iter().unzip()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signals::SignalOrigin;

    fn mk_result(
        release_id: &str,
        group_id: Option<&str>,
        catalog: Option<&str>,
    ) -> MetadataResult {
        MetadataResult {
            source: MetadataSource::MusicBrainz,
            release_id: release_id.to_string(),
            title: "Album".to_string(),
            artist: None,
            year: None,
            format: None,
            label: None,
            catalog_number: catalog.map(str::to_string),
            country: None,
            cover_art: None,
            source_group_id: group_id.map(str::to_string),
            source_tracks: None,
        }
    }

    fn mk_status(release_id: &str) -> LibraryStatus {
        LibraryStatus {
            release_id: release_id.to_string(),
            release_in_library: false,
            album_in_library: false,
            album_title: None,
            album_id: None,
        }
    }

    fn pair(
        release_id: &str,
        group_id: Option<&str>,
        catalog: Option<&str>,
    ) -> (MetadataResult, LibraryStatus) {
        (
            mk_result(release_id, group_id, catalog),
            mk_status(release_id),
        )
    }

    fn pair_src(
        source: MetadataSource,
        release_id: &str,
        group_id: Option<&str>,
    ) -> (MetadataResult, LibraryStatus) {
        let mut result = mk_result(release_id, group_id, None);
        result.source = source;
        (result, mk_status(release_id))
    }

    fn catalog(value: &str, origin: SignalOrigin) -> SourcedValue {
        SourcedValue::new(value.to_string(), origin)
    }

    #[test]
    fn both_empty_yields_not_found_anywhere() {
        let outcome = combine_results(vec![], vec![], &[]);
        assert!(matches!(outcome, CombineOutcome::NotFoundAnywhere));
    }

    /// A single-group set passes straight through to `Found`, with every result.
    #[test]
    fn single_group_sets_pass_through_to_found() {
        // (name, discid, barcode, expected match count)
        let cases: Vec<(&str, Vec<_>, Vec<_>, usize)> = vec![
            (
                "only disc-id",
                vec![pair(
                    "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
                    Some("group-1"),
                    None,
                )],
                vec![],
                1,
            ),
            (
                "only barcode",
                vec![],
                vec![
                    pair(
                        "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
                        Some("group-1"),
                        None,
                    ),
                    pair(
                        "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
                        Some("group-1"),
                        None,
                    ),
                ],
                2,
            ),
            (
                "single group, multiple pressings",
                vec![
                    pair(
                        "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
                        Some("group-1"),
                        None,
                    ),
                    pair(
                        "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
                        Some("group-1"),
                        None,
                    ),
                    pair("rel-3", Some("group-1"), None),
                ],
                vec![],
                3,
            ),
        ];
        for (name, discid, barcode, expected) in cases {
            let outcome = combine_results(discid, barcode, &[]);
            match outcome {
                CombineOutcome::Found { matches, .. } => {
                    assert_eq!(matches.len(), expected, "{name}");
                }
                other => panic!("{name}: expected Found, got {other:?}"),
            }
        }
    }

    /// Results that share nothing but a group-id string across two sources are
    /// two different releases, and both stay in the set for the user to pick
    /// between — the grouping the surface draws is what tells them apart.
    #[test]
    fn results_from_two_sources_both_stay_in_the_set() {
        let results = vec![
            pair_src(MetadataSource::MusicBrainz, "rel-mb", Some("shared-id")),
            pair_src(MetadataSource::Discogs, "rel-dg", Some("shared-id")),
        ];
        let CombineOutcome::Found { matches, .. } = combine_results(results, vec![], &[]) else {
            panic!("expected Found");
        };
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn intersection_with_one_match_succeeds() {
        let discid = vec![
            pair(
                "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
                Some("group-1"),
                None,
            ),
            pair(
                "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
                Some("group-1"),
                None,
            ),
        ];
        let barcode = vec![pair(
            "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
            Some("group-1"),
            None,
        )];
        let outcome = combine_results(discid, barcode, &[]);
        match outcome {
            CombineOutcome::Found { matches, .. } => {
                assert_eq!(matches.len(), 1);
                assert_eq!(
                    matches[0].release_id,
                    "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b"
                );
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    /// Signals that share no result are not a failure to identify: each saw a
    /// real release, so the set is their union, disc-ID order first, and each
    /// row says which signal produced it.
    fn empty_intersection_with_both_having_results_is_their_union() {
        let discid = vec![pair(
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
            Some("group-1"),
            None,
        )];
        let barcode = vec![pair(
            "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
            Some("group-2"),
            None,
        )];
        let CombineOutcome::Found {
            matches,
            provenance,
            ..
        } = combine_results(discid, barcode, &[])
        else {
            panic!("expected Found");
        };
        assert_eq!(
            matches
                .iter()
                .map(|m| m.release_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
                "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b"
            ]
        );
        assert!(provenance[0].by_disc_id && !provenance[0].by_barcode);
        assert!(!provenance[1].by_disc_id && provenance[1].by_barcode);
    }

    #[test]
    fn multi_group_in_intersection_stays_one_match_list() {
        // Both signals agree on two releases, in two groups. Which of the two
        // is on disk is the user's call, so both are offered.
        let discid = vec![
            pair(
                "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
                Some("group-1"),
                None,
            ),
            pair(
                "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
                Some("group-2"),
                None,
            ),
        ];
        let barcode = vec![
            pair(
                "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
                Some("group-1"),
                None,
            ),
            pair(
                "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
                Some("group-2"),
                None,
            ),
        ];
        let CombineOutcome::Found { matches, .. } = combine_results(discid, barcode, &[]) else {
            panic!("expected Found");
        };
        assert_eq!(matches.len(), 2);
    }

    /// A result the source returned without a group id is still a release the
    /// user can pick; it stands as its own single-pressing card.
    #[test]
    fn a_result_with_no_group_id_stays_in_the_set() {
        let results = vec![
            pair("rel-a", Some("group-x"), None),
            pair("rel-b", None, None),
        ];
        let CombineOutcome::Found { matches, .. } = combine_results(results, vec![], &[]) else {
            panic!("expected Found");
        };
        assert_eq!(matches.len(), 2);

        let CombineOutcome::Found { matches, .. } =
            combine_results(vec![pair("rel-a", None, None)], vec![], &[])
        else {
            panic!("expected Found");
        };
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn catalog_filter_narrows_intersection() {
        // Two pressings in one group; only one carries the catalog.
        let discid = vec![
            pair("rel-a", Some("group-x"), Some("WPCR-80001")),
            pair("rel-b", Some("group-x"), Some("WPCR-80002")),
        ];
        let barcode = vec![
            pair("rel-a", Some("group-x"), Some("WPCR-80001")),
            pair("rel-b", Some("group-x"), Some("WPCR-80002")),
        ];
        let candidates = vec![catalog("WPCR 80001", SignalOrigin::CueSheet)];
        let outcome = combine_results(discid, barcode, &candidates);
        match outcome {
            CombineOutcome::Found { matches, .. } => {
                assert_eq!(matches.len(), 1);
                assert_eq!(matches[0].release_id, "rel-a");
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn catalog_filter_narrows_multi_group_to_single() {
        // Without filter: multi-group conflict. With filter: single group, Found.
        let results = vec![
            pair("rel-a", Some("group-x"), Some("LBL-001")),
            pair("rel-b", Some("group-y"), Some("LBL-002")),
        ];
        let candidates = vec![catalog("LBL-001", SignalOrigin::CueSheet)];
        let outcome = combine_results(results, vec![], &candidates);
        match outcome {
            CombineOutcome::Found { matches, .. } => {
                assert_eq!(matches.len(), 1);
                assert_eq!(matches[0].release_id, "rel-a");
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn catalog_filter_with_no_matches_keeps_combined_set() {
        // No result carries this catno, so the set stays whole rather than empty.
        let results = vec![
            pair("rel-a", Some("group-x"), Some("LBL-001")),
            pair("rel-b", Some("group-x"), Some("LBL-002")),
        ];
        let candidates = vec![catalog("XXX-999", SignalOrigin::CueSheet)];
        let outcome = combine_results(results, vec![], &candidates);
        match outcome {
            CombineOutcome::Found { matches, .. } => {
                assert_eq!(matches.len(), 2);
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn normalize_catalog_aligns_separators() {
        assert_eq!(normalize_catalog("WPCR-80001"), "wpcr80001");
        assert_eq!(normalize_catalog("WPCR 80001"), "wpcr80001");
        assert_eq!(normalize_catalog("wpcr-80001"), "wpcr80001");
        assert_eq!(normalize_catalog("WPCR/80001"), "wpcr80001");
    }

    #[test]
    fn artwork_catalog_candidate_does_not_narrow_or_confirm() {
        let results = vec![
            pair("rel-a", Some("group-x"), Some("LBL-001")),
            pair("rel-b", Some("group-x"), Some("LBL-002")),
        ];
        let candidates = vec![catalog("LBL-002", SignalOrigin::Artwork)];
        let outcome = combine_results(results, vec![], &candidates);
        match outcome {
            CombineOutcome::Found {
                matches,
                provenance,
                ..
            } => {
                assert_eq!(matches.len(), 2);
                assert!(provenance.iter().all(|p| !p.matches_catalog));
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn normalize_catalog_strips_non_ascii_alphanumerics() {
        // Only ASCII letters and digits survive, so a cosmetically decorated
        // catalog number still compares equal to the plain one.
        assert_eq!(normalize_catalog("LBL\u{00ae}-001"), "lbl001"); // ® dropped
        assert_eq!(normalize_catalog("LBL\u{00a0}001"), "lbl001"); // non-breaking space
        assert_eq!(normalize_catalog("café-12"), "caf12"); // é dropped
        assert_eq!(normalize_catalog(""), "");
    }
}

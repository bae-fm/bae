//! Combine logic for the triangulation pipeline.
//!
//! After both disc-ID and barcode signals settle, the reducer hands their
//! per-signal result vecs and the candidate's sourced catalog candidates to
//! `combine_results`. This module produces the final outcome:
//!
//! * intersect or fall-through to whichever signal had results,
//! * narrow by sourced catalog match against MB / Discogs canonical catnos,
//! * branch on cardinality and per-group agreement.
//!
//! Pure functions: no I/O, no state. The caller (reducer) maps the
//! `CombineOutcome` to a terminal `IdentifyState`.
//!
//! Group identity: a result's group is `(source, source_group_id)` —
//! cross-source releases naturally land in distinct groups, so a triangulation
//! that spans MB-X and Discogs-Y is a multi-group result and signals a
//! genuine disagreement between the two signal pipes.

use crate::db::LibraryStatus;
use crate::import::search::MetadataResult;
use crate::import::MetadataSource;
use crate::signals::SourcedValue;

/// Which signals produced or confirmed one result, for the per-row badges in
/// the UI. `by_disc_id` / `by_barcode`: the result came back from that signal's
/// lookup. `matches_catalog`: the result's catalog number matches a catalog
/// candidate harvested from the candidate's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultProvenance {
    pub by_disc_id: bool,
    pub by_barcode: bool,
    pub matches_catalog: bool,
}

/// Final outcome of the combine step. The reducer converts this into a
/// terminal `IdentifyState`.
#[derive(Debug, Clone)]
pub enum CombineOutcome {
    /// Combined set has 1+ results that all share a single group. The UI
    /// shows them with per-row buttons; `group` lets it render
    /// "N pressings of one release group" copy when appropriate.
    /// `provenance` is index-aligned with `matches`.
    Found {
        matches: Vec<MetadataResult>,
        library_statuses: Vec<LibraryStatus>,
        group: GroupKey,
        provenance: Vec<ResultProvenance>,
    },
    /// Combined set spans multiple groups, OR intersection was empty when
    /// both signals had results. UI presents the per-signal sections so the
    /// user can pick one or ignore a signal.
    Conflict {
        discid_results: Vec<(MetadataResult, LibraryStatus)>,
        barcode_results: Vec<(MetadataResult, LibraryStatus)>,
    },
    /// Both signals settled with zero results. Truly nothing to show.
    NotFoundAnywhere,
}

/// A group key. Two results "agree" iff they share `(source, source_group_id)`.
/// Results without a `source_group_id` aren't groupable — they can't co-occupy
/// a group with anyone else, so a set containing one always ends up in
/// `Conflict` once it has any other result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupKey {
    pub source: MetadataSource,
    pub source_group_id: String,
}

/// Settle both signals' results into a `CombineOutcome`.
///
/// Steps, in order:
///
/// 1. **Combine** — if both vecs are non-empty, intersect them by
///    `(source, release_id)`; preserve disc-id ordering. If only one vec
///    is non-empty, that vec becomes the combined set. If both are empty,
///    return `NotFoundAnywhere` immediately.
/// 2. **Catalog filter** — when `catalog_candidates` is non-empty AND the
///    filter retains at least one result, narrow the set. Otherwise leave
///    the combined set untouched (a filter that empties the set isn't
///    useful — it'd lose real signal).
/// 3. **Cardinality / grouping** — group results by
///    `(source, source_group_id)`. Single-group → `Found`. Multi-group or
///    a result with `None` group_id → `Conflict`.
/// 4. **Empty-intersection conflict** — when step 1's combine path was
///    intersection AND the result is empty, return `Conflict` carrying the
///    original per-signal vecs so the UI can render both sections.
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

    // Per-result provenance is computed against which signal's set a release
    // came from, so capture membership before the working set is chosen.
    let discid_keys: HashSet<(MetadataSource, String)> = discid_results
        .iter()
        .map(|(r, _)| (r.source, r.release_id.clone()))
        .collect();
    let barcode_keys: HashSet<(MetadataSource, String)> = barcode_results
        .iter()
        .map(|(r, _)| (r.source, r.release_id.clone()))
        .collect();

    // Choose the working set: intersection when both are non-empty;
    // otherwise the non-empty side. Preserves disc-id ordering when an
    // intersection is taken (disc-id is the more authoritative signal).
    let combined: Vec<(MetadataResult, LibraryStatus)> = if !discid_empty && !barcode_empty {
        intersect_by_release(&discid_results, &barcode_results)
    } else if !discid_empty {
        discid_results.clone()
    } else {
        barcode_results.clone()
    };

    // Empty intersection while both signals had results → Conflict, not
    // NotFoundAnywhere. The UI needs to show what each signal saw.
    if combined.is_empty() {
        return CombineOutcome::Conflict {
            discid_results,
            barcode_results,
        };
    }

    let filtered = apply_catalog_filter(combined, catalog_candidates);

    let group = single_group(&filtered);
    match group {
        Some(group) => {
            let provenance: Vec<ResultProvenance> = filtered
                .iter()
                .map(|(r, _)| ResultProvenance {
                    by_disc_id: discid_keys.contains(&(r.source, r.release_id.clone())),
                    by_barcode: barcode_keys.contains(&(r.source, r.release_id.clone())),
                    matches_catalog: catalog_matches(
                        r.catalog_number.as_deref(),
                        catalog_candidates,
                    ),
                })
                .collect();
            let (matches, library_statuses) = unzip_results(filtered);
            CombineOutcome::Found {
                matches,
                library_statuses,
                group,
                provenance,
            }
        }
        None => CombineOutcome::Conflict {
            discid_results,
            barcode_results,
        },
    }
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

/// Apply the catalog filter. Returns the narrowed set if at least one
/// result matches a candidate; otherwise returns the input unchanged.
///
/// Match rule: a result matches when its `catalog_number` (after
/// `normalize_catalog`) equals any candidate's normalized form. Bypass when
/// `candidates` is empty or when no result has a `catalog_number`.
fn apply_catalog_filter(
    combined: Vec<(MetadataResult, LibraryStatus)>,
    candidates: &[SourcedValue],
) -> Vec<(MetadataResult, LibraryStatus)> {
    if candidates.is_empty() {
        return combined;
    }
    let normalized_candidates: Vec<String> = candidates
        .iter()
        .filter(|c| c.origin.can_confirm_catalog())
        .map(|c| normalize_catalog(&c.value))
        .collect();
    if normalized_candidates.is_empty() {
        return combined;
    }
    let filtered: Vec<(MetadataResult, LibraryStatus)> = combined
        .iter()
        .filter(|(r, _)| {
            let Some(cat) = r.catalog_number.as_deref() else {
                return false;
            };
            let normalized = normalize_catalog(cat);
            normalized_candidates.iter().any(|c| c == &normalized)
        })
        .cloned()
        .collect();
    if filtered.is_empty() {
        combined
    } else {
        filtered
    }
}

/// Whether a result's catalog number matches any catalog candidate (after
/// `normalize_catalog`). Used for the per-result "catalog confirmed" badge —
/// independent of whether the filter narrowed the set.
fn catalog_matches(catalog_number: Option<&str>, candidates: &[SourcedValue]) -> bool {
    let Some(cat) = catalog_number else {
        return false;
    };
    if candidates.is_empty() {
        return false;
    }
    let normalized = normalize_catalog(cat);
    candidates
        .iter()
        .filter(|c| c.origin.can_confirm_catalog())
        .any(|c| normalize_catalog(&c.value) == normalized)
}

/// Lowercase, strip everything that isn't `[a-z0-9]`. Aligns OCR variants
/// like `CAT 80001` against a registry's `CAT-80001`.
pub(crate) fn normalize_catalog(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Returns `Some(group)` if every result shares the same
/// `(source, source_group_id)`. Returns `None` if any result has
/// `source_group_id == None` or if results span multiple groups.
fn single_group(results: &[(MetadataResult, LibraryStatus)]) -> Option<GroupKey> {
    let mut iter = results.iter();
    let (first, _) = iter.next()?;
    let group_id = first.source_group_id.clone()?;
    let key = GroupKey {
        source: first.source,
        source_group_id: group_id,
    };
    for (r, _) in iter {
        let g = r.source_group_id.as_deref()?;
        if r.source != key.source || g != key.source_group_id {
            return None;
        }
    }
    Some(key)
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

    /// A single-group set from one signal (or both, when they don't intersect)
    /// passes straight through to `Found` with every result and that group.
    #[test]
    fn single_group_sets_pass_through_to_found() {
        // (name, discid, barcode, expected match count)
        let cases: Vec<(&str, Vec<_>, Vec<_>, usize)> = vec![
            (
                "only disc-id",
                vec![pair("rel-1", Some("group-1"), None)],
                vec![],
                1,
            ),
            (
                "only barcode",
                vec![],
                vec![
                    pair("rel-1", Some("group-1"), None),
                    pair("rel-2", Some("group-1"), None),
                ],
                2,
            ),
            (
                "single group, multiple pressings",
                vec![
                    pair("rel-1", Some("group-1"), None),
                    pair("rel-2", Some("group-1"), None),
                    pair("rel-3", Some("group-1"), None),
                ],
                vec![],
                3,
            ),
        ];
        for (name, discid, barcode, expected) in cases {
            let outcome = combine_results(discid, barcode, &[]);
            match outcome {
                CombineOutcome::Found { matches, group, .. } => {
                    assert_eq!(matches.len(), expected, "{name}");
                    assert_eq!(group.source_group_id, "group-1", "{name}");
                }
                other => panic!("{name}: expected Found, got {other:?}"),
            }
        }
    }

    /// The headline cross-source invariant: two results carrying the *same*
    /// `source_group_id` string but different sources (MB vs Discogs) are two
    /// distinct groups — the group key is `(source, source_group_id)` — so the
    /// set is a Conflict, not a single Found group.
    #[test]
    fn same_group_id_string_across_sources_stays_two_groups() {
        let results = vec![
            pair_src(MetadataSource::MusicBrainz, "rel-mb", Some("shared-id")),
            pair_src(MetadataSource::Discogs, "rel-dg", Some("shared-id")),
        ];
        let outcome = combine_results(results, vec![], &[]);
        assert!(
            matches!(outcome, CombineOutcome::Conflict { .. }),
            "same group_id across sources must not collapse into one group"
        );
    }

    #[test]
    fn intersection_with_one_match_succeeds() {
        let discid = vec![
            pair("rel-1", Some("group-1"), None),
            pair("rel-2", Some("group-1"), None),
        ];
        let barcode = vec![pair("rel-2", Some("group-1"), None)];
        let outcome = combine_results(discid, barcode, &[]);
        match outcome {
            CombineOutcome::Found { matches, .. } => {
                assert_eq!(matches.len(), 1);
                assert_eq!(matches[0].release_id, "rel-2");
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn empty_intersection_with_both_having_results_is_conflict() {
        let discid = vec![pair("rel-1", Some("group-1"), None)];
        let barcode = vec![pair("rel-2", Some("group-2"), None)];
        let outcome = combine_results(discid, barcode, &[]);
        match outcome {
            CombineOutcome::Conflict {
                discid_results,
                barcode_results,
            } => {
                // Both signals found one release each; they didn't intersect.
                assert_eq!(discid_results.len(), 1);
                assert_eq!(barcode_results.len(), 1);
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn multi_group_in_intersection_is_conflict() {
        // Both signals share two releases, in two groups — the user can't
        // commit a single identity from this set.
        let discid = vec![
            pair("rel-1", Some("group-1"), None),
            pair("rel-2", Some("group-2"), None),
        ];
        let barcode = vec![
            pair("rel-1", Some("group-1"), None),
            pair("rel-2", Some("group-2"), None),
        ];
        let outcome = combine_results(discid, barcode, &[]);
        assert!(matches!(outcome, CombineOutcome::Conflict { .. }));
    }

    #[test]
    fn missing_group_id_is_conflict_when_alongside_others() {
        let results = vec![
            pair("rel-a", Some("group-x"), None),
            pair("rel-b", None, None),
        ];
        let outcome = combine_results(results, vec![], &[]);
        assert!(matches!(outcome, CombineOutcome::Conflict { .. }));
    }

    #[test]
    fn single_result_with_missing_group_is_conflict() {
        // Group-membership is the disambiguator. A single groupless result
        // can't be presented as "1 pressing of group X" — route it to
        // Conflict so the user picks intentionally.
        let results = vec![pair("rel-a", None, None)];
        let outcome = combine_results(results, vec![], &[]);
        assert!(matches!(outcome, CombineOutcome::Conflict { .. }));
    }

    #[test]
    fn catalog_filter_narrows_intersection() {
        // Two pressings share group; only one matches the catalog.
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
            CombineOutcome::Found { matches, group, .. } => {
                assert_eq!(matches.len(), 1);
                assert_eq!(group.source_group_id, "group-x");
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn catalog_filter_with_no_matches_keeps_combined_set() {
        // Candidate doesn't match any result's catnos — fall back to
        // unfiltered set rather than collapse to empty.
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
        // Only ASCII letters/digits survive: registered-trademark marks,
        // non-breaking spaces, and accented characters are dropped so that
        // cosmetically-decorated catalog numbers still compare equal.
        assert_eq!(normalize_catalog("LBL\u{00ae}-001"), "lbl001"); // ® dropped
        assert_eq!(normalize_catalog("LBL\u{00a0}001"), "lbl001"); // non-breaking space
        assert_eq!(normalize_catalog("café-12"), "caf12"); // é dropped
        assert_eq!(normalize_catalog(""), "");
    }
}

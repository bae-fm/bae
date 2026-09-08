//! Combine logic for the triangulation pipeline.
//!
//! Once the checked signals settle, the reducer hands their result sets to
//! `combine_results`, which intersects them. Pure: no I/O, no state.
//!
//! Every checked signal is a claim about the same disc, so a release has to
//! satisfy all of them — an intersection is what agreement looks like. Signals
//! the user left unchecked are not in the intersection at all: they arrive here
//! as an empty set and drop out. Signals that do not intersect are not a
//! failure to identify: each saw something, and the union of what they saw is
//! the set the user picks from, each row carrying which signal produced it.

use crate::db::LibraryStatus;
use crate::import::search::MetadataResult;
use crate::import::MetadataSource;
use std::collections::HashSet;

/// Which signals produced one result, for the UI's per-row badges: the result
/// came back from that signal's lookup.
///
/// `Serialize`/`Deserialize`: carried on `identify::TerminalVerdict::Found`,
/// which `import_candidate_match` persists.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResultProvenance {
    pub by_disc_id: bool,
    pub by_barcode: bool,
    pub by_catalog: bool,
}

/// The releases agreement left out — every release a checked signal named that
/// the intersection does not hold.
///
/// Agreement is what makes a short list: a disc ID that named three releases
/// and a barcode that named two settle on the one they share, and the other
/// four never reach the person. Each of those four is a real answer from a real
/// lookup, and one of them may be the disc on the desk, so combine hands them
/// back beside the matches instead of dropping them.
///
/// Empty when nothing was narrowed: one signal answering alone is the whole
/// answer, and signals that shared nothing already list their union.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NarrowedOut {
    /// In signal order, each release once.
    pub matches: Vec<MetadataResult>,
    /// Index-aligned with `matches`.
    pub library_statuses: Vec<LibraryStatus>,
    /// Index-aligned with `matches`: which signals named each one.
    pub provenance: Vec<ResultProvenance>,
}

impl NarrowedOut {
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }
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
        narrowed_out: NarrowedOut,
    },
    /// Every checked signal settled with zero results.
    NotFoundAnywhere,
}

type Results = Vec<(MetadataResult, LibraryStatus)>;
type ReleaseKey = (MetadataSource, String);

/// Settle the checked signals' results into a `CombineOutcome`.
///
/// A signal the user left unchecked arrives empty and takes no part. So does a
/// checked signal whose lookup found nothing — which lands on the same answer
/// either way, since an intersection it emptied would fall through to the
/// union of the rest.
///
/// 1. **Nothing.** Every set empty: `NotFoundAnywhere`.
/// 2. **One set.** That set is the answer.
/// 3. **Several sets.** Intersect them by `(source, release_id)`, keeping the
///    first set's order. An empty intersection means the signals named
///    different releases; neither is wrong about having seen something, so the
///    set becomes their union, in signal order, and each row says which signal
///    produced it.
pub fn combine_results(
    discid_results: Results,
    barcode_results: Results,
    catalog_results: Results,
) -> CombineOutcome {
    let by_signal = [&discid_results, &barcode_results, &catalog_results];
    let keys: Vec<HashSet<ReleaseKey>> = by_signal.iter().map(|set| release_keys(set)).collect();

    let present: Vec<&Results> = by_signal
        .into_iter()
        .filter(|set| !set.is_empty())
        .collect();
    let Some((first, rest)) = present.split_first() else {
        return CombineOutcome::NotFoundAnywhere;
    };

    // Only an intersection narrows anything: one set alone is the whole answer,
    // and sets that share nothing already list their union.
    let (combined, left_out) = if rest.is_empty() {
        ((*first).clone(), Results::new())
    } else {
        let intersected = intersect_all(first, rest);
        if intersected.is_empty() {
            (union_all(&present), Results::new())
        } else {
            let agreed = release_keys(&intersected);
            let left_out = union_all(&present)
                .into_iter()
                .filter(|(r, _)| !agreed.contains(&(r.source, r.release_id.clone())))
                .collect();
            (intersected, left_out)
        }
    };

    let provenance_of = |results: &Results| -> Vec<ResultProvenance> {
        results
            .iter()
            .map(|(r, _)| {
                let key = (r.source, r.release_id.clone());
                ResultProvenance {
                    by_disc_id: keys[0].contains(&key),
                    by_barcode: keys[1].contains(&key),
                    by_catalog: keys[2].contains(&key),
                }
            })
            .collect()
    };

    let provenance = provenance_of(&combined);
    let narrowed_out_provenance = provenance_of(&left_out);
    let (matches, library_statuses) = combined.into_iter().unzip();
    let (narrowed_matches, narrowed_statuses) = left_out.into_iter().unzip();
    CombineOutcome::Found {
        matches,
        library_statuses,
        provenance,
        narrowed_out: NarrowedOut {
            matches: narrowed_matches,
            library_statuses: narrowed_statuses,
            provenance: narrowed_out_provenance,
        },
    }
}

fn release_keys(results: &Results) -> HashSet<ReleaseKey> {
    results
        .iter()
        .map(|(r, _)| (r.source, r.release_id.clone()))
        .collect()
}

/// The releases every set names, in the first set's order.
fn intersect_all(first: &Results, rest: &[&Results]) -> Results {
    let rest_keys: Vec<HashSet<ReleaseKey>> = rest.iter().map(|set| release_keys(set)).collect();
    first
        .iter()
        .filter(|(r, _)| {
            let key = (r.source, r.release_id.clone());
            rest_keys.iter().all(|keys| keys.contains(&key))
        })
        .cloned()
        .collect()
}

/// Every release any set names, in signal order, each release once. Only
/// reached when the sets share nothing, so in practice nothing is dropped — the
/// de-duplication is what keeps that a property of the data rather than a thing
/// the caller has to have checked.
fn union_all(sets: &[&Results]) -> Results {
    let mut seen: HashSet<ReleaseKey> = HashSet::new();
    let mut out = Results::new();
    for set in sets {
        for pair in set.iter() {
            if seen.insert((pair.0.source, pair.0.release_id.clone())) {
                out.push(pair.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_result(release_id: &str, group_id: Option<&str>) -> MetadataResult {
        MetadataResult::for_test(MetadataSource::MusicBrainz, release_id, group_id)
    }

    fn pair(release_id: &str, group_id: Option<&str>) -> (MetadataResult, LibraryStatus) {
        (
            mk_result(release_id, group_id),
            LibraryStatus::absent(release_id),
        )
    }

    fn pair_src(
        source: MetadataSource,
        release_id: &str,
        group_id: Option<&str>,
    ) -> (MetadataResult, LibraryStatus) {
        let mut result = mk_result(release_id, group_id);
        result.source = source;
        (result, LibraryStatus::absent(release_id))
    }

    fn ids(matches: &[MetadataResult]) -> Vec<&str> {
        matches.iter().map(|m| m.release_id.as_str()).collect()
    }

    fn narrowed(outcome: CombineOutcome) -> NarrowedOut {
        match outcome {
            CombineOutcome::Found { narrowed_out, .. } => narrowed_out,
            other => panic!("expected Found, got {other:?}"),
        }
    }

    fn found(outcome: CombineOutcome) -> (Vec<MetadataResult>, Vec<ResultProvenance>) {
        match outcome {
            CombineOutcome::Found {
                matches,
                provenance,
                ..
            } => (matches, provenance),
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn nothing_checked_or_nothing_found_yields_not_found_anywhere() {
        let outcome = combine_results(vec![], vec![], vec![]);
        assert!(matches!(outcome, CombineOutcome::NotFoundAnywhere));
    }

    /// One checked signal answers on its own: there is nothing to agree with.
    #[test]
    fn one_set_alone_is_the_answer() {
        for (name, discid, barcode, catalog) in [
            (
                "disc id alone",
                vec![pair("rel-a", Some("group-1"))],
                vec![],
                vec![],
            ),
            (
                "barcode alone",
                vec![],
                vec![pair("rel-a", Some("group-1")), pair("rel-b", None)],
                vec![],
            ),
            (
                "catalog alone",
                vec![],
                vec![],
                vec![pair("rel-a", Some("group-1"))],
            ),
        ] {
            let expected = discid.len().max(barcode.len()).max(catalog.len());
            let (matches, _) = found(combine_results(discid, barcode, catalog));
            assert_eq!(matches.len(), expected, "{name}");
        }
    }

    /// Several pressings of one release group all stand: which one is on disk
    /// is the user's call.
    #[test]
    fn every_pressing_the_signals_agree_on_stays() {
        let both = vec![
            pair("rel-a", Some("group-1")),
            pair("rel-b", Some("group-2")),
        ];
        let (matches, _) = found(combine_results(both.clone(), both, vec![]));
        assert_eq!(matches.len(), 2);
    }

    /// The intersection is what agreement looks like — the release both signals
    /// name, in the first signal's order.
    #[test]
    fn two_checked_signals_intersect() {
        let discid = vec![
            pair("rel-a", Some("group-1")),
            pair("rel-b", Some("group-1")),
        ];
        let barcode = vec![pair("rel-b", Some("group-1"))];
        let (matches, provenance) = found(combine_results(discid, barcode, vec![]));
        assert_eq!(ids(&matches), vec!["rel-b"]);
        assert!(provenance[0].by_disc_id && provenance[0].by_barcode);
        assert!(!provenance[0].by_catalog);
    }

    /// Three checked signals have to all agree, not just two of them.
    #[test]
    fn three_checked_signals_intersect() {
        let discid = vec![pair("rel-a", None), pair("rel-b", None)];
        let barcode = vec![pair("rel-a", None), pair("rel-b", None)];
        let catalog = vec![pair("rel-b", None)];
        let (matches, provenance) = found(combine_results(discid, barcode, catalog));
        assert_eq!(ids(&matches), vec!["rel-b"]);
        assert!(provenance[0].by_disc_id);
        assert!(provenance[0].by_barcode);
        assert!(provenance[0].by_catalog);
    }

    /// Signals that share no result are not a failure to identify: each saw a
    /// real release, so the set is their union, in signal order, and each row
    /// says which signal produced it.
    #[test]
    fn an_empty_intersection_falls_through_to_the_union() {
        let discid = vec![pair("rel-a", Some("group-1"))];
        let barcode = vec![pair("rel-b", Some("group-2"))];
        let catalog = vec![pair("rel-c", Some("group-3"))];
        let (matches, provenance) = found(combine_results(discid, barcode, catalog));
        assert_eq!(ids(&matches), vec!["rel-a", "rel-b", "rel-c"]);
        assert!(provenance[0].by_disc_id && !provenance[0].by_barcode);
        assert!(provenance[1].by_barcode && !provenance[1].by_disc_id);
        assert!(provenance[2].by_catalog && !provenance[2].by_disc_id);
    }

    /// The union names each release once even when two signals both saw it —
    /// which happens when a third signal is what emptied the intersection.
    #[test]
    fn the_union_names_each_release_once() {
        let discid = vec![pair("rel-a", None)];
        let barcode = vec![pair("rel-a", None)];
        let catalog = vec![pair("rel-b", None)];
        let (matches, provenance) = found(combine_results(discid, barcode, catalog));
        assert_eq!(ids(&matches), vec!["rel-a", "rel-b"]);
        assert!(provenance[0].by_disc_id && provenance[0].by_barcode);
    }

    /// A checked signal that found nothing takes no part: the rest still
    /// answer, rather than the empty set emptying everything.
    #[test]
    fn a_signal_that_found_nothing_does_not_empty_the_set() {
        let barcode = vec![pair("rel-a", None)];
        let (matches, _) = found(combine_results(vec![], barcode, vec![]));
        assert_eq!(ids(&matches), vec!["rel-a"]);
    }

    /// Releases are told apart by source as well as id, so the same id on two
    /// providers is two releases and never intersects by accident.
    #[test]
    fn the_same_id_on_two_providers_is_two_releases() {
        let discid = vec![pair_src(MetadataSource::MusicBrainz, "rel-a", None)];
        let barcode = vec![pair_src(MetadataSource::Discogs, "rel-a", None)];
        let (matches, _) = found(combine_results(discid, barcode, vec![]));
        assert_eq!(matches.len(), 2);
    }

    /// What agreement left out comes back beside the matches: every release a
    /// signal named that the intersection does not hold, in signal order, each
    /// once, saying which signal named it.
    #[test]
    fn an_intersection_hands_back_what_it_narrowed_out() {
        let discid = vec![
            pair("rel-a", None),
            pair("rel-shared", None),
            pair("rel-b", None),
        ];
        let barcode = vec![pair("rel-shared", None), pair("rel-c", None)];
        let outcome = combine_results(discid, barcode, vec![]);
        let (matches, _) = found(outcome.clone());
        assert_eq!(ids(&matches), vec!["rel-shared"]);

        let narrowed = narrowed(outcome);
        assert_eq!(ids(&narrowed.matches), vec!["rel-a", "rel-b", "rel-c"]);
        assert_eq!(narrowed.library_statuses.len(), 3);
        assert!(narrowed.provenance[0].by_disc_id && !narrowed.provenance[0].by_barcode);
        assert!(narrowed.provenance[2].by_barcode && !narrowed.provenance[2].by_disc_id);
    }

    /// A release two signals both named, that a third narrowed out, is one
    /// entry saying both named it.
    #[test]
    fn a_narrowed_out_release_two_signals_named_is_named_once() {
        let discid = vec![pair("rel-a", None), pair("rel-shared", None)];
        let barcode = vec![pair("rel-a", None), pair("rel-shared", None)];
        let catalog = vec![pair("rel-shared", None)];
        let narrowed = narrowed(combine_results(discid, barcode, catalog));
        assert_eq!(ids(&narrowed.matches), vec!["rel-a"]);
        assert!(narrowed.provenance[0].by_disc_id && narrowed.provenance[0].by_barcode);
        assert!(!narrowed.provenance[0].by_catalog);
    }

    /// Signals that share nothing already list everything they saw, and one
    /// signal answering alone is the whole answer: neither narrowed anything.
    #[test]
    fn a_union_and_a_lone_signal_narrow_nothing() {
        let disagreeing =
            combine_results(vec![pair("rel-a", None)], vec![pair("rel-b", None)], vec![]);
        assert!(narrowed(disagreeing).is_empty());

        let alone = combine_results(
            vec![pair("rel-a", None), pair("rel-b", None)],
            vec![],
            vec![],
        );
        assert!(narrowed(alone).is_empty());
    }

    /// A result the source returned without a group id is still a release the
    /// user can pick; it stands as its own single-pressing card.
    #[test]
    fn a_result_with_no_group_id_stays_in_the_set() {
        let results = vec![pair("rel-a", Some("group-x")), pair("rel-b", None)];
        let (matches, _) = found(combine_results(results, vec![], vec![]));
        assert_eq!(matches.len(), 2);
    }
}

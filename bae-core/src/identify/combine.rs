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
/// which `import_candidate_state` persists.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResultProvenance {
    pub by_disc_id: bool,
    pub by_barcode: bool,
    pub by_catalog: bool,
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

    let combined = if rest.is_empty() {
        (*first).clone()
    } else {
        let intersected = intersect_all(first, rest);
        if intersected.is_empty() {
            union_all(&present)
        } else {
            intersected
        }
    };

    let provenance: Vec<ResultProvenance> = combined
        .iter()
        .map(|(r, _)| {
            let key = (r.source, r.release_id.clone());
            ResultProvenance {
                by_disc_id: keys[0].contains(&key),
                by_barcode: keys[1].contains(&key),
                by_catalog: keys[2].contains(&key),
            }
        })
        .collect();
    let (matches, library_statuses) = combined.into_iter().unzip();
    CombineOutcome::Found {
        matches,
        library_statuses,
        provenance,
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
        MetadataResult {
            source: MetadataSource::MusicBrainz,
            release_id: release_id.to_string(),
            title: "Album".to_string(),
            artist: None,
            year: None,
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

    fn mk_status(release_id: &str) -> LibraryStatus {
        LibraryStatus {
            release_id: release_id.to_string(),
            release_in_library: false,
            album_in_library: false,
            album_title: None,
            album_id: None,
        }
    }

    fn pair(release_id: &str, group_id: Option<&str>) -> (MetadataResult, LibraryStatus) {
        (mk_result(release_id, group_id), mk_status(release_id))
    }

    fn pair_src(
        source: MetadataSource,
        release_id: &str,
        group_id: Option<&str>,
    ) -> (MetadataResult, LibraryStatus) {
        let mut result = mk_result(release_id, group_id);
        result.source = source;
        (result, mk_status(release_id))
    }

    fn ids(matches: &[MetadataResult]) -> Vec<&str> {
        matches.iter().map(|m| m.release_id.as_str()).collect()
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

    /// A result the source returned without a group id is still a release the
    /// user can pick; it stands as its own single-pressing card.
    #[test]
    fn a_result_with_no_group_id_stays_in_the_set() {
        let results = vec![pair("rel-a", Some("group-x")), pair("rel-b", None)];
        let (matches, _) = found(combine_results(results, vec![], vec![]));
        assert_eq!(matches.len(), 2);
    }
}

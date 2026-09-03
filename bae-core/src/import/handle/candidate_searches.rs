//! The typed searches in flight, held by the driver that runs them.
//!
//! A search's two sources land on their own tasks. Each landing folds its
//! answer into the search and publishes the result; the copy the candidate
//! runtime holds is filled from that publication by the recorder task, later.
//! So the value a landing folds into has to be this one, under this lock — a
//! landing that read the runtime's copy could fold into a value the other
//! source's landing had already moved past, and publish over it.

use crate::db::LibraryStatus;
use crate::import::candidate_search::CandidateSearch;
use crate::import::search::MetadataResult;
use crate::import::types::MetadataSource;
use crate::signals::LookupFailure;
use std::collections::HashMap;

/// One candidate's search and the run it is on. A run number tells a landing
/// from a superseded search apart from one that is still current.
struct RunningSearch {
    run: u64,
    search: CandidateSearch,
}

/// Every candidate's search in flight. A key with no entry has no search, so
/// a landing for it is stale.
#[derive(Default)]
pub(super) struct RunningCandidateSearches {
    running: HashMap<String, RunningSearch>,
    next_run: u64,
}

impl RunningCandidateSearches {
    /// Put `key` on a new run carrying `search`, superseding whatever it was
    /// on. The number returned is what a landing proves it is still current by.
    pub(super) fn start(&mut self, key: &str, search: CandidateSearch) -> u64 {
        let run = self.next_run;
        self.next_run += 1;
        self.running
            .insert(key.to_string(), RunningSearch { run, search });
        run
    }

    /// The search `key` is on, for a Retry to re-ask from.
    pub(super) fn current(&self, key: &str) -> Option<&CandidateSearch> {
        self.running.get(key).map(|running| &running.search)
    }

    /// Whether `run` is still the run `key`'s search is on.
    pub(super) fn is_current(&self, key: &str, run: u64) -> bool {
        self.running
            .get(key)
            .is_some_and(|running| running.run == run)
    }

    /// Land one source's answer on `key`'s search, if `run` is still its run.
    /// The search after the landing is what the caller publishes; `None` means
    /// the run was cleared or superseded and the answer goes nowhere.
    pub(super) fn land(
        &mut self,
        key: &str,
        run: u64,
        source: MetadataSource,
        outcome: Result<Vec<(MetadataResult, LibraryStatus)>, LookupFailure>,
    ) -> Option<CandidateSearch> {
        let running = self.running.get_mut(key)?;
        if running.run != run {
            return None;
        }
        running.search.record(source, outcome);
        Some(running.search.clone())
    }

    /// Take `key` off whatever run it is on, so nothing that run has out can
    /// land.
    pub(super) fn clear(&mut self, key: &str) {
        self.running.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::candidate_search::SourceSearch;
    use crate::import::search::SearchQuery;

    fn query() -> SearchQuery {
        SearchQuery::General {
            artist: "Artist Name".to_string(),
            album: "Album Title".to_string(),
        }
    }

    fn result(source: MetadataSource, release_id: &str) -> (MetadataResult, LibraryStatus) {
        (
            MetadataResult {
                source,
                release_id: release_id.to_string(),
                title: "Album Title".to_string(),
                artist: Some("Artist Name".to_string()),
                year: Some(1992),
                format: None,
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
                cover_art: None,
                source_group_id: None,
                source_tracks: None,
            },
            LibraryStatus {
                release_id: release_id.to_string(),
                release_in_library: false,
                album_in_library: false,
                album_title: None,
                album_id: None,
            },
        )
    }

    /// Both sources land on the same run, one after the other, and neither
    /// landing loses the other's answer.
    #[test]
    fn two_landings_on_one_run_both_stand() {
        let mut searches = RunningCandidateSearches::default();
        let run = searches.start("k", CandidateSearch::started(query(), true));

        let after_mb = searches
            .land(
                "k",
                run,
                MetadataSource::MusicBrainz,
                Ok(vec![result(MetadataSource::MusicBrainz, "mb-1")]),
            )
            .expect("the run is current");
        assert!(matches!(after_mb.musicbrainz, SourceSearch::Done { .. }));
        assert!(matches!(after_mb.discogs, SourceSearch::Searching));

        let after_discogs = searches
            .land(
                "k",
                run,
                MetadataSource::Discogs,
                Ok(vec![result(MetadataSource::Discogs, "dg-1")]),
            )
            .expect("the run is current");
        assert!(matches!(
            after_discogs.musicbrainz,
            SourceSearch::Done { .. }
        ));
        assert!(matches!(after_discogs.discogs, SourceSearch::Done { .. }));
        assert_eq!(after_discogs.library_statuses.len(), 2);
    }

    /// A superseded run's landing goes nowhere, and the new run is untouched.
    #[test]
    fn a_superseded_run_cannot_land() {
        let mut searches = RunningCandidateSearches::default();
        let first = searches.start("k", CandidateSearch::started(query(), true));
        let second = searches.start("k", CandidateSearch::started(query(), true));
        assert!(!searches.is_current("k", first));
        assert!(searches.is_current("k", second));

        let landed = searches.land(
            "k",
            first,
            MetadataSource::MusicBrainz,
            Ok(vec![result(MetadataSource::MusicBrainz, "mb-1")]),
        );
        assert!(landed.is_none());
        assert!(matches!(
            searches
                .current("k")
                .expect("the second run stands")
                .musicbrainz,
            SourceSearch::Searching
        ));
    }

    /// A dropped key has no search for a landing to fold into.
    #[test]
    fn a_dropped_search_cannot_land() {
        let mut searches = RunningCandidateSearches::default();
        let run = searches.start("k", CandidateSearch::started(query(), true));
        searches.clear("k");
        assert!(searches.current("k").is_none());
        let landed = searches.land(
            "k",
            run,
            MetadataSource::MusicBrainz,
            Ok(vec![result(MetadataSource::MusicBrainz, "mb-1")]),
        );
        assert!(landed.is_none());
    }
}

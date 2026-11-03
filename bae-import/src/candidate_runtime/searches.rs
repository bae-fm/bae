//! A person's typed search, stored beside the standings
//! (`import_candidate_search`, see [`CandidateSearch`]): each catalog's
//! answer lands on the run it was asked for in a write of its own.

use super::CandidateRuntime;
use crate::candidate_search::{CandidateSearch, SearchRun};
use crate::CandidateKey;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::search::{MetadataResult, SearchQuery};
use bae_catalogs::Failure;
use bae_db::DbError;

impl CandidateRuntime {
    /// Store `search`, just started, as `key`'s in place of any search it
    /// had, and return the run each catalog's answer names to land on it.
    pub(crate) async fn start_search(
        &self,
        key: &CandidateKey,
        search: &CandidateSearch,
    ) -> Result<SearchRun, DbError> {
        self.tables.start_candidate_search(key, search).await
    }

    /// Put every failed part of `key`'s search back to searching, returning
    /// the run it is on, the query and the catalogs to ask again; `None` when
    /// there is nothing to ask again.
    pub(crate) async fn retry_search(
        &self,
        key: &CandidateKey,
    ) -> Result<Option<(SearchRun, SearchQuery, Vec<LookupCatalog>)>, DbError> {
        self.tables.retry_candidate_search(key).await
    }

    /// Stop asking `source` on every search. Each keeps its run, so the
    /// other source's answer still lands; the dropped source's is refused
    /// because that part is no longer searching.
    pub(crate) async fn switch_source_off(&self, source: LookupCatalog) -> Result<(), DbError> {
        self.tables.switch_search_source_off(source).await
    }

    /// Whether `run` is still the run `key`'s search is on.
    #[cfg(test)]
    pub(crate) async fn search_run_is_current(&self, key: &CandidateKey, run: SearchRun) -> bool {
        self.tables.candidate_search_run(key).await.expect("a test reads the search it started")
            == Some(run)
    }

    /// Land `source`'s answer on its part of the search `run`, if that part
    /// is still searching; `false` when the search was replaced or ended, or
    /// the part settled or was switched off. The landing decides in its own
    /// write, so an answer of an older search never writes over a newer one.
    pub(crate) async fn land_search(
        &self,
        run: SearchRun,
        source: LookupCatalog,
        outcome: Result<Vec<MetadataResult>, Failure>,
    ) -> Result<bool, DbError> {
        self.tables.land_candidate_search(run, source, outcome).await
    }

    /// Land what `release`'s documents state its album is on whatever search
    /// `key` is on now — a statement about the release stays true whichever
    /// search holds it — returning what to keep; nothing when the key has no
    /// search, or its search does not hold the release.
    pub(crate) async fn land_picked(
        &self,
        key: &CandidateKey,
        release: &bae_catalogs::records::LookupRef,
        links: bae_catalogs::album_links::AlbumLinks,
    ) -> Result<Vec<(String, Vec<bae_catalogs::records::MetadataRef>)>, DbError> {
        self.tables.land_candidate_search_pick(key, release, links).await
    }
}

#[cfg(test)]
#[path = "searches_tests.rs"]
mod tests;

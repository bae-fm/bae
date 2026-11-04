//! A person's typed search for a candidate (`import_candidate_search`): the
//! query as columns, each catalog's part of it
//! (`import_candidate_search_source`), and the releases each part returned,
//! as match rows the search owns ([`MatchOwner::Search`]). The cards a
//! surface draws are folded from them as they are read, never stored.
//!
//! Each change is one write that decides from the rows it changes. A
//! catalog's answer lands only on a part that is still searching, of the
//! search it was asked for: an answer to a replaced search, to a part
//! switched off, or a second answer to a part that settled lands nowhere. A
//! folder candidate's search is of the files it names, and the scan write
//! that leaves the candidate with other files or none ends it
//! ([`end_search_unless_of_on`]). A library release's names none; a new
//! search replaces it, and the table is emptied as the library opens.

use super::lookup_failure_columns::{failure_columns, failure_of};
use super::match_rows::load_search_matches_on;
use super::match_rows::{store_match, store_picked, Listing, MatchOwner, Written};
use super::*;
use crate::candidate_search::{CandidateSearch, PickedResult, SearchRun, SourceSearch};
use crate::CandidateFiles;
use bae_catalogs::album_links::AlbumLinks;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::search::{MetadataResult, SearchQuery};
use bae_catalogs::{Failure, InternalFailure};
use std::str::FromStr;

/// Delete every typed search: none outlives the process that asked it.
pub(super) fn delete_every_search_on(sql: &CovenWrite<'_>) -> Result<(), DbError> {
    sql.execute("DELETE FROM import_candidate_search", [])?;
    Ok(())
}

/// A query as its row's columns: the mode, and the values that mode asks.
struct QueryColumns<'a> {
    mode: &'static str,
    artist: Option<&'a str>,
    album: Option<&'a str>,
    catalog_number: Option<&'a str>,
    barcode: Option<&'a str>,
}

fn query_columns(query: &SearchQuery) -> QueryColumns<'_> {
    let none =
        QueryColumns { mode: "", artist: None, album: None, catalog_number: None, barcode: None };
    match query {
        SearchQuery::General { artist, album } => {
            QueryColumns { mode: "general", artist: Some(artist), album: Some(album), ..none }
        }
        SearchQuery::CatalogNumber { catalog_number } => {
            QueryColumns { mode: "catalog_number", catalog_number: Some(catalog_number), ..none }
        }
        SearchQuery::Barcode { barcode } => {
            QueryColumns { mode: "barcode", barcode: Some(barcode), ..none }
        }
    }
}

/// The query a search row's columns hold.
fn query_of(row: &Row<'_>) -> Result<SearchQuery, DbError> {
    let mode: String = row.get("mode")?;
    let value = |column: &str| {
        row.get::<_, Option<String>>(column)?
            .ok_or_else(|| DbError::Message(format!("a {mode} search states no {column}")))
    };
    Ok(match mode.as_str() {
        "general" => SearchQuery::General { artist: value("artist")?, album: value("album")? },
        "catalog_number" => SearchQuery::CatalogNumber { catalog_number: value("catalog_number")? },
        "barcode" => SearchQuery::Barcode { barcode: value("barcode")? },
        other => return Err(DbError::unreadable("import_candidate_search.mode", other)),
    })
}

/// A part's state as its columns: the state, a catalog's failure and its
/// HTTP status, or the error chain of bae breaking.
struct PartColumns<'a> {
    state: &'static str,
    failure: Option<&'static str>,
    failure_status: Option<i64>,
    error_detail: Option<&'a str>,
}

fn part_columns(part: &SourceSearch) -> PartColumns<'_> {
    let settled =
        |state| PartColumns { state, failure: None, failure_status: None, error_detail: None };
    match part {
        SourceSearch::Off => settled("off"),
        SourceSearch::NotConfigured => settled("not_configured"),
        SourceSearch::Searching => settled("searching"),
        SourceSearch::Done { .. } => settled("done"),
        SourceSearch::Failed(Failure::Lookup(failure)) => {
            let columns = failure_columns(Some(failure));
            PartColumns {
                failure: columns.kind,
                failure_status: columns.status,
                ..settled("failed")
            }
        }
        SourceSearch::Failed(Failure::Internal(failure)) => {
            PartColumns { error_detail: Some(&failure.detail), ..settled("failed") }
        }
    }
}

/// One part as its row holds it, its results still to be joined to it.
struct StoredPart {
    catalog: String,
    state: String,
    failure: Option<String>,
    failure_status: Option<i64>,
    error_detail: Option<String>,
}

impl StoredPart {
    /// The part, with `results` the releases it returned in order; a part
    /// that has not answered holds none.
    fn part(&self, results: Vec<MetadataResult>) -> Result<SourceSearch, DbError> {
        let holds_none = |part: SourceSearch| {
            if results.is_empty() {
                Ok(part)
            } else {
                Err(DbError::Message(format!(
                    "a search's {} part is {} but holds releases",
                    self.catalog, self.state
                )))
            }
        };
        match self.state.as_str() {
            "off" => holds_none(SourceSearch::Off),
            "not_configured" => holds_none(SourceSearch::NotConfigured),
            "searching" => holds_none(SourceSearch::Searching),
            "done" => Ok(SourceSearch::Done { results }),
            "failed" => {
                let failure = match (
                    failure_of(self.failure.clone(), self.failure_status)?,
                    self.error_detail.clone(),
                ) {
                    (Some(failure), None) => Failure::Lookup(failure),
                    (None, Some(detail)) => Failure::Internal(InternalFailure { detail }),
                    _ => {
                        return Err(DbError::Message(format!(
                            "a search's failed {} part states no one failure",
                            self.catalog
                        )))
                    }
                };
                holds_none(SourceSearch::Failed(failure))
            }
            other => Err(DbError::unreadable("import_candidate_search_source.state", other)),
        }
    }
}

/// Every search, or `key`'s alone, by key, each with the run it is on.
fn load_on(
    sql: &impl bae_db::client::query::QueryRows,
    key: Option<&CandidateKey>,
) -> Result<HashMap<CandidateKey, (SearchRun, CandidateSearch)>, DbError> {
    let searches = sql.query(
        "SELECT search_run, candidate_key, mode, artist, album, catalog_number, barcode \
         FROM import_candidate_search WHERE ?1 IS NULL OR candidate_key = ?1",
        params![key],
        |row| {
            Ok((
                row.get::<_, i64>("search_run")?,
                row.get::<_, CandidateKey>("candidate_key")?,
                query_of(row),
            ))
        },
    )?;
    let parts = sql.query(
        "SELECT part.search_run, part.catalog, part.state, part.failure, part.failure_status, \
                part.error_detail \
         FROM import_candidate_search_source AS part \
         JOIN import_candidate_search AS search ON search.search_run = part.search_run \
         WHERE ?1 IS NULL OR search.candidate_key = ?1 \
         ORDER BY part.search_run, part.position",
        params![key],
        |row| {
            Ok((
                row.get::<_, i64>("search_run")?,
                StoredPart {
                    catalog: row.get("catalog")?,
                    state: row.get("state")?,
                    failure: row.get("failure")?,
                    failure_status: row.get("failure_status")?,
                    error_detail: row.get("error_detail")?,
                },
            ))
        },
    )?;
    let mut parts_by_search: HashMap<i64, Vec<StoredPart>> = HashMap::new();
    for (search, part) in parts {
        parts_by_search.entry(search).or_default().push(part);
    }
    let mut releases = load_search_matches_on(sql, key)?;
    let mut loaded = HashMap::new();
    for (run, key, query) in searches {
        let query = query?;
        let mut picked: Vec<PickedResult> = Vec::new();
        let mut by_catalog: HashMap<LookupCatalog, Vec<MetadataResult>> = HashMap::new();
        for row in releases.remove(&run).unwrap_or_default() {
            let Listing::Returned { picked: was_picked } = row.listing else {
                return Err(DbError::Message(format!(
                    "{key}'s search holds release {} listed as no search lists one",
                    row.position
                )));
            };
            if was_picked && !picked.iter().any(|read| read.release_id == row.result.release_id) {
                picked.push(PickedResult {
                    release_id: row.result.release_id.clone(),
                    links: row.result.album_links.clone(),
                });
            }
            by_catalog.entry(row.result.source).or_default().push(row.result);
        }
        let sources = parts_by_search
            .remove(&run)
            .unwrap_or_default()
            .into_iter()
            .map(|part| {
                let catalog = LookupCatalog::from_str(&part.catalog).map_err(DbError::Message)?;
                let results = by_catalog.remove(&catalog).unwrap_or_default();
                Ok((catalog, part.part(results)?))
            })
            .collect::<Result<Vec<_>, DbError>>()?;
        if let Some(catalog) = by_catalog.keys().next() {
            return Err(DbError::Message(format!(
                "{key}'s search holds {} releases but has no {} part",
                catalog.as_str(),
                catalog.as_str()
            )));
        }
        loaded.insert(key, (SearchRun::of(run), CandidateSearch::of(query, sources, picked)));
    }
    Ok(loaded)
}

/// `key`'s search; `None` when it has none.
pub(super) fn load_search_on(
    sql: &impl bae_db::client::query::QueryRows,
    key: &CandidateKey,
) -> Result<Option<CandidateSearch>, DbError> {
    Ok(load_on(sql, Some(key))?.remove(key).map(|(_, search)| search))
}

/// Every key's search.
pub(super) fn load_searches_on(
    sql: &impl bae_db::client::query::QueryRows,
) -> Result<HashMap<CandidateKey, CandidateSearch>, DbError> {
    Ok(load_on(sql, None)?.into_iter().map(|(key, (_, search))| (key, search)).collect())
}

/// End, inside the caller's scan write, `key`'s search unless it is of
/// `held` — the files the write leaves the candidate with, `None` for a write
/// that leaves the scan without it. Every row it owns goes with it.
pub(super) fn end_search_unless_of_on(
    sql: &CovenWrite<'_>,
    key: &CandidateKey,
    held: Option<&CandidateFiles>,
) -> Result<(), DbError> {
    match held {
        None => {
            sql.execute("DELETE FROM import_candidate_search WHERE candidate_key = ?", [key])?
        }
        Some(files) => sql.execute(
            "DELETE FROM import_candidate_search WHERE candidate_key = ? \
             AND (content_hash, file_edit_revision) IS NOT (?, ?)",
            params![
                key,
                files.content_hash,
                to_i64(files.file_edit_revision, "a file edit revision")?
            ],
        )?,
    };
    Ok(())
}

/// Store `search`, just started, as `key`'s, of `files`, in place of any
/// search it had; the run its parts' answers land on.
fn start_on(
    sql: &CovenWrite<'_>,
    key: &CandidateKey,
    files: Option<&CandidateFiles>,
    search: &CandidateSearch,
) -> Result<SearchRun, DbError> {
    sql.execute("DELETE FROM import_candidate_search WHERE candidate_key = ?", [key])?;
    let query = query_columns(&search.query);
    let run: i64 = sql.query_row(
        "INSERT INTO import_candidate_search \
             (candidate_key, content_hash, file_edit_revision, mode, artist, album, \
              catalog_number, barcode) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING search_run",
        params![
            key,
            files.map(|files| files.content_hash.as_str()),
            files
                .map(|files| to_i64(files.file_edit_revision, "a file edit revision"))
                .transpose()?,
            query.mode,
            query.artist,
            query.album,
            query.catalog_number,
            query.barcode,
        ],
        |row| row.get(0),
    )?;
    for (position, (catalog, part)) in search.sources.iter().enumerate() {
        if !matches!(
            part,
            SourceSearch::Off | SourceSearch::NotConfigured | SourceSearch::Searching
        ) {
            return Err(DbError::Message(format!(
                "{key}'s search starts with its {} part already answered",
                catalog.as_str()
            )));
        }
        let columns = part_columns(part);
        sql.execute(
            "INSERT INTO import_candidate_search_source \
                 (search_run, catalog, position, state) VALUES (?, ?, ?, ?)",
            params![
                run,
                catalog.as_str(),
                super::match_rows::ordinal_column(position)?,
                columns.state
            ],
        )?;
    }
    Ok(SearchRun::of(run))
}

/// Land `catalog`'s answer on its part of the search `run`, if that part is
/// still searching; whether it landed.
fn land_on(
    sql: &CovenWrite<'_>,
    run: SearchRun,
    catalog: LookupCatalog,
    outcome: &Result<Vec<MetadataResult>, Failure>,
) -> Result<bool, DbError> {
    let empty = Vec::new();
    let (part, results) = match outcome {
        Ok(results) => (SourceSearch::Done { results: Vec::new() }, results),
        Err(failure) => (SourceSearch::Failed(failure.clone()), &empty),
    };
    let columns = part_columns(&part);
    let landed = sql.execute(
        "UPDATE import_candidate_search_source \
         SET state = ?, failure = ?, failure_status = ?, error_detail = ? \
         WHERE search_run = ? AND catalog = ? AND state = 'searching'",
        params![
            columns.state,
            columns.failure,
            columns.failure_status,
            columns.error_detail,
            run.number(),
            catalog.as_str()
        ],
    )?;
    if landed == 0 {
        return Ok(false);
    }
    let next: i64 = sql.query_row(
        "SELECT COALESCE(MAX(position) + 1, 0) FROM import_candidate_match WHERE search_run = ?",
        [run.number()],
        |row| row.get(0),
    )?;
    let next = usize::try_from(next)
        .map_err(|error| DbError::Message(format!("a search's next position {next}: {error}")))?;
    for (offset, result) in results.iter().enumerate() {
        if result.source != catalog {
            return Err(DbError::Message(format!(
                "{} answered a search with a {} release",
                catalog.as_str(),
                result.source.as_str()
            )));
        }
        store_match(
            sql,
            MatchOwner::Search(run.number()),
            next + offset,
            result,
            Written::Returned,
        )?;
    }
    Ok(true)
}

impl ImportTables {
    /// Store `search`, just started, as `key`'s in place of any search it
    /// had: a folder candidate's of the files the scan holds it with now, a
    /// library release's of none. Answers the run its parts' answers land
    /// on. Refused for a folder key the scan holds no candidate at.
    pub(crate) async fn start_candidate_search(
        &self,
        key: &CandidateKey,
        search: &CandidateSearch,
    ) -> Result<SearchRun, DbError> {
        let key = key.clone();
        let search = search.clone();
        self.database
            .call(move |sql| {
                let files = if key.names_release() {
                    None
                } else {
                    let stored = crate::tables::release_groupings::load_candidate_on(sql, &key)?
                        .ok_or_else(|| {
                            DbError::Message(format!("{key} is no candidate a search can be of"))
                        })?;
                    Some(CandidateFiles::of(&stored.candidate))
                };
                start_on(sql, &key, files.as_ref(), &search)
            })
            .await
    }

    /// Put every failed part of `key`'s search back to searching: the run it
    /// is on, its query and the catalogs to ask again, in their order.
    /// `None`, writing nothing, when the key has no search or none of its
    /// parts failed.
    pub(crate) async fn retry_candidate_search(
        &self,
        key: &CandidateKey,
    ) -> Result<Option<(SearchRun, SearchQuery, Vec<LookupCatalog>)>, DbError> {
        let key = key.clone();
        self.database
            .call(move |sql| {
                let Some((run, query)) = sql
                    .query_row(
                        "SELECT search_run, mode, artist, album, catalog_number, barcode \
                     FROM import_candidate_search WHERE candidate_key = ?",
                        [&key],
                        |row| Ok((row.get::<_, i64>("search_run")?, query_of(row))),
                    )
                    .optional()?
                else {
                    return Ok(None);
                };
                let mut retried = sql.query(
                    "UPDATE import_candidate_search_source \
                 SET state = 'searching', failure = NULL, failure_status = NULL, \
                     error_detail = NULL \
                 WHERE search_run = ? AND state = 'failed' \
                 RETURNING position, catalog",
                    [run],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                )?;
                if retried.is_empty() {
                    return Ok(None);
                }
                retried.sort();
                let catalogs = retried
                    .into_iter()
                    .map(|(_, catalog)| LookupCatalog::from_str(&catalog).map_err(DbError::Message))
                    .collect::<Result<_, _>>()?;
                Ok(Some((SearchRun::of(run), query?, catalogs)))
            })
            .await
    }

    /// Stop asking `catalog` on every search: its part closes, and the
    /// releases it returned go. Each search keeps its run, so another
    /// catalog's answer still lands; `catalog`'s, still out, finds its part
    /// no longer searching.
    pub(crate) async fn switch_search_source_off(
        &self,
        catalog: LookupCatalog,
    ) -> Result<(), DbError> {
        self.database
            .call(move |sql| {
                // A part holds releases only while it stands answered, so they go
                // first.
                super::match_rows::delete_search_matches_of(sql, catalog)?;
                sql.execute(
                    "UPDATE import_candidate_search_source \
                 SET state = 'off', failure = NULL, failure_status = NULL, error_detail = NULL \
                 WHERE catalog = ? AND state <> 'off'",
                    [catalog.as_str()],
                )?;
                Ok(())
            })
            .await
    }

    /// Land `catalog`'s answer on its part of the search `run`, if that part
    /// is still searching; whether it landed.
    pub(crate) async fn land_candidate_search(
        &self,
        run: SearchRun,
        catalog: LookupCatalog,
        outcome: Result<Vec<MetadataResult>, Failure>,
    ) -> Result<bool, DbError> {
        self.database.call(move |sql| land_on(sql, run, catalog, &outcome)).await
    }

    /// Land what the documents of `release`, which a pick read, state its
    /// album is on `key`'s search, and keep what its album on the search's
    /// list was then read to be for its release group, in one write. Nothing
    /// when the key has no search, or `release` is no MusicBrainz release on
    /// its list.
    pub(crate) async fn land_candidate_search_pick(
        &self,
        key: &CandidateKey,
        release: &bae_catalogs::records::LookupRef,
        links: AlbumLinks,
    ) -> Result<(), DbError> {
        let key = key.clone();
        let release = release.clone();
        self.database
            .call(move |sql| {
                let Some((run, mut search)) = load_on(sql, Some(&key))?.remove(&key) else {
                    return Ok(());
                };
                let Some(kept) = search.record_picked(&release, links.clone()) else {
                    return Ok(());
                };
                store_picked(sql, run.number(), &release, &links)?;
                crate::tables::album_link_rows::replace_group_album_links_on(sql, &kept)
            })
            .await
    }
}

#[cfg(test)]
#[path = "search_rows_tests.rs"]
mod tests;

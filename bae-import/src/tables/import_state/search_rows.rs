//! A person's typed search for a candidate (`import_candidate_search`): the
//! query as columns, each catalog's part of it
//! (`import_candidate_search_source`), and the releases each part returned,
//! as match rows the search owns ([`MatchOwner::Search`]). The read builds
//! the cards a surface draws from these rows, and bae never stores them. The
//! read also counts each part's releases and decides the search's status as
//! a whole ([`status_column`]).
//!
//! Each change is one write that decides from the rows it changes. A
//! catalog's answer lands only on a part that is still searching, of the
//! search it was asked for: an answer to a replaced search, to a part
//! switched off, or a second answer to a part that settled lands nowhere. A
//! folder candidate's search is of the files it names, and the scan write
//! that leaves the candidate with other files or none ends it
//! ([`end_search_unless_of_on`]). A library release's names none; a new
//! search replaces it, and the table is emptied as the library opens.
//! Its tables are `search_rows.sql`, which every device's library holds.

pub(super) const SQL: &str = include_str!("search_rows.sql");

use super::cards_read::{groups_of, search_cards_sql};
use super::column_list::{ColumnList, Columns, JsonColumns};
use super::layout::lay_out_search_on;
use super::lookup_failure_columns::{failure_columns, failure_of};
use super::match_rows::{store_match, MatchOwner, Written};
use super::*;
use crate::candidate_search::{CandidateSearch, SearchRun, SearchStatus, SourceSearch};
use crate::CandidateFiles;
use bae_catalogs::album_links::AlbumLinks;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::search::{MetadataResult, SearchQuery};
use bae_catalogs::{Failure, InternalFailure};
use bae_preferences::{CatalogAvailability, SourceAvailability};
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
fn query_of(row: &impl Columns) -> Result<SearchQuery, DbError> {
    let mode: String = row.get("mode")?;
    let value = |column: &str| {
        row.get::<Option<String>>(column)?.ok_or_else(|| {
            DbError::unreadable(
                format!("import_candidate_search.{column} of a {mode} search"),
                None::<String>,
            )
        })
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

/// The state a part starts in, given whether the library asks its catalog.
fn starting_state(availability: SourceAvailability) -> &'static str {
    match availability {
        SourceAvailability::On => "searching",
        SourceAvailability::Off => "off",
        SourceAvailability::NotConfigured => "not_configured",
    }
}

/// The columns of a part a catalog answered with `outcome`.
fn landed_columns(outcome: &Result<Vec<MetadataResult>, Failure>) -> PartColumns<'_> {
    let settled =
        |state| PartColumns { state, failure: None, failure_status: None, error_detail: None };
    match outcome {
        Ok(_) => settled("done"),
        Err(Failure::Lookup(failure)) => {
            let columns = failure_columns(Some(failure));
            PartColumns {
                failure: columns.kind,
                failure_status: columns.status,
                ..settled("failed")
            }
        }
        Err(Failure::Internal(failure)) => {
            PartColumns { error_detail: Some(&failure.detail), ..settled("failed") }
        }
    }
}

/// One part as [`parts_column`] holds it: its row, and how many releases it
/// returned.
#[derive(serde::Deserialize)]
struct StoredPart {
    catalog: String,
    state: String,
    failure: Option<String>,
    failure_status: Option<i64>,
    error_detail: Option<String>,
    found: i64,
}

/// The parts of the search a column names as one JSON list column in the
/// order of their positions, each with the count of releases it returned. A
/// search's release references a part that answered, so only such a part
/// counts any.
fn parts_column(search_run: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
                  'catalog', part.catalog, 'state', part.state, 'failure', part.failure, \
                  'failure_status', part.failure_status, 'error_detail', part.error_detail, \
                  'found', (SELECT COUNT(*) FROM import_candidate_match AS found \
                            WHERE found.search_run = part.search_run \
                              AND found.source = part.catalog)) \
                  ORDER BY part.position) \
          FROM import_candidate_search_source AS part \
          WHERE part.search_run = {search_run})"
    )
}

/// Where the search a column names stands as a whole, as the text
/// [`status_of`] reads: searching while a part is, else failed when a part
/// failed, else no matches when a part answered and no part returned a
/// release, else found.
fn status_column(search_run: &str) -> String {
    let part_in = |state: &str| {
        format!(
            "EXISTS (SELECT 1 FROM import_candidate_search_source AS part \
                     WHERE part.search_run = {search_run} AND part.state = '{state}')"
        )
    };
    format!(
        "CASE WHEN {searching} THEN 'searching' \
              WHEN {failed} THEN 'failed' \
              WHEN {done} AND NOT EXISTS (SELECT 1 FROM import_candidate_match AS found \
                                          WHERE found.search_run = {search_run}) \
                  THEN 'no_matches' \
              ELSE 'found' END",
        searching = part_in("searching"),
        failed = part_in("failed"),
        done = part_in("done"),
    )
}

/// The status a [`status_column`] holds.
fn status_of(status: &str) -> Result<SearchStatus, DbError> {
    Ok(match status {
        "searching" => SearchStatus::Searching,
        "found" => SearchStatus::Found,
        "no_matches" => SearchStatus::NoMatches,
        "failed" => SearchStatus::Failed,
        other => return Err(DbError::unreadable("a search's status", other)),
    })
}

impl StoredPart {
    /// The part and the catalog it is of.
    fn part(self) -> Result<(LookupCatalog, SourceSearch), DbError> {
        let Self { catalog, state, failure, failure_status, error_detail, found } = self;
        let part = match state.as_str() {
            "off" => SourceSearch::Off,
            "not_configured" => SourceSearch::NotConfigured,
            "searching" => SourceSearch::Searching,
            "done" => SourceSearch::Done {
                found: u32::try_from(found).map_err(|_too_many| {
                    DbError::unreadable(format!("the count of a search's {catalog} part"), found)
                })?,
            },
            "failed" => {
                SourceSearch::Failed(match (failure_of(failure, failure_status)?, error_detail) {
                    (Some(failure), None) => Failure::Lookup(failure),
                    (None, Some(detail)) => Failure::Internal(InternalFailure { detail }),
                    (failure, detail) => {
                        return Err(DbError::unreadable(
                            format!("the failure of a search's failed {catalog} part"),
                            (failure, detail),
                        ))
                    }
                })
            }
            other => {
                return Err(DbError::unreadable("import_candidate_search_source.state", other))
            }
        };
        Ok((catalog_of(&catalog)?, part))
    }
}

/// The search of the key a read joins as `keys`, as one JSON object holding
/// the columns [`search_in`] reads, NULL for a key with none.
pub(super) fn search_object_sql() -> String {
    ColumnList(vec![
        ("mode", "typed.mode".to_string()),
        ("artist", "typed.artist".to_string()),
        ("album", "typed.album".to_string()),
        ("catalog_number", "typed.catalog_number".to_string()),
        ("barcode", "typed.barcode".to_string()),
        ("cards", search_cards_sql("typed.search_run")),
        ("parts", parts_column("typed.search_run")),
        ("status", status_column("typed.search_run")),
    ])
    .object()
}

/// The search a [`search_object_sql`] object holds.
pub(super) fn search_in(search: &JsonColumns) -> Result<CandidateSearch, DbError> {
    let parts = search.get::<String>("parts")?;
    let stored: Vec<StoredPart> = serde_json::from_str(&parts)
        .map_err(|error| DbError::unreadable_json("a search's stored parts", &parts, error))?;
    Ok(CandidateSearch {
        query: query_of(search)?,
        sources: stored.into_iter().map(StoredPart::part).collect::<Result<_, _>>()?,
        groups: groups_of(&search.get::<String>("cards")?)?,
        status: status_of(&search.get::<String>("status")?)?,
    })
}

/// A MusicBrainz result a pick read: what its documents state its album is on
/// the other lookup catalog, `Unread` where bae could not fetch them.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct PickedResult {
    pub(super) release_id: String,
    pub(super) links: AlbumLinks,
}

/// The picks of the search a column names, as one JSON list column in the
/// order bae stored them, each with the albums its documents named.
fn picks_column(search_run: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
                  'release_id', pick.release_id, 'links', pick.links, \
                  'albums', json((SELECT json_group_array(json_object( \
                                        'catalog', album.catalog, 'key', album.key) \
                                    ORDER BY album.ordinal) \
                             FROM import_candidate_search_pick_album AS album \
                             WHERE album.search_run = pick.search_run \
                               AND album.release_id = pick.release_id))) \
                  ORDER BY pick.rowid) \
          FROM import_candidate_search_pick AS pick \
          WHERE pick.search_run = {search_run})"
    )
}

/// One pick as [`picks_column`] holds it.
#[derive(serde::Deserialize)]
struct StoredPick {
    release_id: String,
    links: String,
    albums: Vec<crate::tables::album_link_rows::AlbumLinkRow>,
}

fn picks_of(json: &str) -> Result<Vec<PickedResult>, DbError> {
    let picks: Vec<StoredPick> = serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json("a search's stored picks", json, error))?;
    picks
        .into_iter()
        .map(|StoredPick { release_id, links, albums }| {
            let links = match links.as_str() {
                "read" => AlbumLinks::Read(
                    albums
                        .into_iter()
                        .map(crate::tables::album_link_rows::AlbumLinkRow::album)
                        .collect::<Result<_, _>>()?,
                ),
                "not_asked" if albums.is_empty() => AlbumLinks::NotAsked,
                "unread" if albums.is_empty() => AlbumLinks::Unread,
                other => {
                    return Err(DbError::unreadable("import_candidate_search_pick.links", other))
                }
            };
            Ok(PickedResult { release_id, links })
        })
        .collect()
}

/// What the picks of the search `run` read, in the order bae stored them.
pub(super) fn load_picks_on(
    sql: &impl coven::SqlRead,
    run: i64,
) -> Result<Vec<PickedResult>, DbError> {
    let json: String =
        sql.query_row(&format!("SELECT {}", picks_column("?1")), [run], |row| row.get(0))?;
    picks_of(&json)
}

/// Store what a pick of the release `release_id` read, in place of what an
/// earlier pick of it read.
fn store_pick_on(
    sql: &CovenWrite<'_>,
    run: i64,
    release_id: &str,
    links: &AlbumLinks,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_search_pick WHERE search_run = ? AND release_id = ?",
        params![run, release_id],
    )?;
    sql.execute(
        "INSERT INTO import_candidate_search_pick (search_run, release_id, links) \
         VALUES (?, ?, ?)",
        params![run, release_id, super::match_rows::album_links_column(links)],
    )?;
    for (ordinal, album) in links.read().iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_search_pick_album \
                 (search_run, release_id, ordinal, catalog, key) VALUES (?, ?, ?, ?, ?)",
            params![
                run,
                release_id,
                super::match_rows::ordinal_column(ordinal)?,
                album.catalog.as_str(),
                album.key
            ],
        )?;
    }
    Ok(())
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

/// Store a search of `query` as `key`'s, of `files`, in place of any search
/// it had, with a part for each of `sources` in their order: searching where
/// the library asks the catalog. The run its parts' answers land on, and the
/// catalogs to ask, in their order.
fn start_on(
    sql: &CovenWrite<'_>,
    key: &CandidateKey,
    files: Option<&CandidateFiles>,
    query: &SearchQuery,
    sources: &[CatalogAvailability],
) -> Result<(SearchRun, Vec<LookupCatalog>), DbError> {
    sql.execute("DELETE FROM import_candidate_search WHERE candidate_key = ?", [key])?;
    let query = query_columns(query);
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
    for (position, source) in sources.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_search_source \
                 (search_run, catalog, position, state) VALUES (?, ?, ?, ?)",
            params![
                run,
                source.catalog.as_str(),
                super::match_rows::ordinal_column(position)?,
                starting_state(source.state)
            ],
        )?;
    }
    // The parts just written searching are the catalogs to ask.
    let asked = sources
        .iter()
        .filter(|source| source.state == SourceAvailability::On)
        .map(|source| source.catalog)
        .collect();
    Ok((SearchRun::of(run), asked))
}

/// The catalog a part's `catalog` column names.
fn catalog_of(catalog: &str) -> Result<LookupCatalog, DbError> {
    LookupCatalog::from_str(catalog)
        .map_err(|_unknown| DbError::unreadable("import_candidate_search_source.catalog", catalog))
}

/// Land `catalog`'s answer on its part of the search `run`, if that part is
/// still searching; whether it landed.
fn land_on(
    sql: &CovenWrite<'_>,
    run: SearchRun,
    catalog: LookupCatalog,
    outcome: &Result<Vec<MetadataResult>, Failure>,
) -> Result<bool, DbError> {
    let results = outcome.as_deref().unwrap_or_default();
    let columns = landed_columns(outcome);
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
    let next = to_usize(next, "a search's next position")?;
    for (offset, result) in results.iter().enumerate() {
        // A catalog's search answers with its own releases.
        assert!(
            result.source == catalog,
            "{} answered a search with a {} release",
            catalog.as_str(),
            result.source.as_str()
        );
        store_match(
            sql,
            MatchOwner::Search(run.number()),
            next + offset,
            result,
            Written::Returned,
        )?;
    }
    // Every catalog's answer pairs with the others', so this write groups the
    // whole list again.
    lay_out_search_on(sql, run.number())?;
    Ok(true)
}

impl ImportTables {
    /// Store a search of `query` as `key`'s in place of any search it had,
    /// with a part for each of `sources`: a folder candidate's of the files
    /// the scan holds it with now, a library release's of none. Answers the
    /// run its parts' answers land on and the catalogs to ask, in their
    /// order. Refuses a folder key the scan holds no candidate at.
    pub(crate) async fn start_candidate_search(
        &self,
        key: &CandidateKey,
        query: &SearchQuery,
        sources: &[CatalogAvailability],
    ) -> Result<(SearchRun, Vec<LookupCatalog>), crate::ImportError> {
        let key = key.clone();
        let query = query.clone();
        let sources = sources.to_vec();
        self.store
            .write(move |sql| {
                let sql = &sql;
                let files = if key.names_release() {
                    None
                } else {
                    let Some(stored) =
                        crate::tables::release_groupings::load_candidate_on(sql, &key)?
                    else {
                        return Err(CandidateStateRefusal::NotScanned(key).into());
                    };
                    Some(CandidateFiles::of(&stored.candidate))
                };
                Ok(start_on(sql, &key, files.as_ref(), &query, &sources)?)
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
        self.store
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
                    .map(|(_, catalog)| catalog_of(&catalog))
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
        self.store
            .call(move |sql| {
                // A part holds releases only while it stands answered, so they go
                // first, and every search that held some lays out what is left.
                let held = sql.query(
                    "SELECT DISTINCT search_run FROM import_candidate_match \
                     WHERE search_run IS NOT NULL AND source = ? ORDER BY search_run",
                    [catalog.as_str()],
                    |row| row.get::<_, i64>(0),
                )?;
                super::match_rows::delete_search_matches_of(sql, catalog)?;
                if catalog == LookupCatalog::MusicBrainz {
                    // A person can pick only a MusicBrainz release.
                    sql.execute("DELETE FROM import_candidate_search_pick", [])?;
                }
                for run in held {
                    lay_out_search_on(sql, run)?;
                }
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
        self.store.call(move |sql| land_on(sql, run, catalog, &outcome)).await
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
        self.store
            .call(move |sql| {
                if release.catalog != LookupCatalog::MusicBrainz {
                    return Ok(());
                }
                // The search's first listing of the release names its group.
                let Some((run, Some(group))) = sql
                    .query_row(
                        "SELECT typed.search_run, found.source_group_id \
                         FROM import_candidate_search AS typed \
                         JOIN import_candidate_match AS found \
                             ON found.search_run = typed.search_run \
                         WHERE typed.candidate_key = ? AND found.source = ? \
                           AND found.release_id = ? \
                         ORDER BY found.position LIMIT 1",
                        params![key, release.catalog.as_str(), release.key],
                        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
                    )
                    .optional()?
                else {
                    return Ok(());
                };
                store_pick_on(sql, run, &release.key, &links)?;
                let read = lay_out_search_on(sql, run)?;
                let landed = super::match_record::load_search_run_matches_on(sql, run)?;
                let landed: Vec<&MetadataResult> = landed.iter().map(|row| &row.result).collect();
                let groups: Vec<_> = read.into_iter().filter(|read| read.group == group).collect();
                let kept = bae_catalogs::album_links::to_keep(&groups, &landed);
                crate::tables::album_link_rows::replace_group_album_links_on(sql, &kept)
            })
            .await
    }
}

#[cfg(test)]
impl ImportTables {
    /// Plant damage for a test: every release a search returns, in the
    /// library under `directory`, fails to be stored, as a full disk fails
    /// it.
    pub(crate) async fn fail_search_match_writes_for_test(directory: &std::path::Path) {
        let directory = directory.to_path_buf();
        bae_util::test_blocking::on_blocking_pool(move || {
            bae_store::test_fixtures::planted_damage::plant_in_library_file(
                &directory,
                "import_candidate_match",
                "CREATE TRIGGER fail_search_match_write BEFORE INSERT ON import_candidate_match \
                 BEGIN SELECT RAISE(ABORT, 'the disk is full'); END;",
            )
        })
        .await;
    }
}

#[cfg(test)]
#[path = "search_rows_tests.rs"]
pub(crate) mod tests;

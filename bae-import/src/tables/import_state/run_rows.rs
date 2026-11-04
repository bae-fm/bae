//! Each identify run in flight (`import_identify_run`), stored in the shape
//! of the verdict it may end on: how it stands is a verdict row, its matches
//! and its ledger rows, and what extraction read for it is a signals row,
//! each owned by the run (see [`Owner::Run`]).
//!
//! A folder candidate's run has its rows only while the candidate's standing
//! names the run. Its first state is written in the write that starts it
//! ([`ImportTables::start_candidate_run`]), a later one only while the standing
//! still names it ([`ImportTables::publish_candidate_run`]), and the run table's
//! triggers delete it in whatever write moves the standing off it. A library
//! release's re-identify run has no standing: its rows stay until the run
//! ends or the release's sheet closes. So every key's runs with no
//! identification standing are a release's.

use super::match_rows::load_run_matches_on;
use super::search_rows::{load_search_on, load_searches_on};
use super::signal_rows::{read_signals, signals_read_columns, store_signals};
use super::standing_rows::{change_identification_on, load_standing_on, load_standings_on};
use super::verdict_rows::{outcome_of, read_verdict_row, store_outcome, verdict_columns};
use super::verdict_rows::{Outcome, Owner};
use super::*;
use crate::candidate_runtime::{IdentificationChange, IdentificationMove, StoredRuntime};
use crate::CandidateFiles;
use crate::IdentifyRunId;
use bae_catalogs::source_release::SourceRelease;
use bae_identify::signals::Signals;
use bae_identify::{IdentifyState, PublishedRun, RunOutcome};
use std::collections::BTreeMap;

/// What every run in flight, or `key`'s runs alone, last published, with the
/// signals each last read, by key and run.
fn load_runs_on(
    sql: &impl bae_db::client::query::QueryRows,
    key: Option<&CandidateKey>,
) -> Result<HashMap<CandidateKey, BTreeMap<IdentifyRunId, PublishedRun>>, DbError> {
    let mut matches = load_run_matches_on(sql, key)?;
    let rows = sql.query(
        &format!(
            "SELECT run.candidate_key AS run_key, run.identify_run AS run_number, \
                    {verdict}, {signals} \
             FROM import_identify_run AS run \
             JOIN import_candidate_verdict AS verdict ON verdict.identify_run = run.identify_run \
             LEFT JOIN import_candidate_signals AS signals \
                 ON signals.identify_run = run.identify_run \
             WHERE ?1 IS NULL OR run.candidate_key = ?1",
            verdict = verdict_columns(),
            signals = signals_read_columns(),
        ),
        params![key],
        |row| {
            Ok((
                row.get::<_, CandidateKey>("run_key")?,
                row.get::<_, i64>("run_number")?,
                read_verdict_row(row),
                read_signals(row)?,
            ))
        },
    )?;
    let mut runs: HashMap<CandidateKey, BTreeMap<IdentifyRunId, PublishedRun>> = HashMap::new();
    for (key, run, verdict, signals) in rows {
        let verdict = verdict?
            .ok_or_else(|| DbError::Message(format!("{key}'s run {run} reads no verdict row")))?;
        let (outcome, _) = outcome_of(verdict, matches.remove(&run).unwrap_or_default())?;
        let signals = signals.map(|stored| stored.signals()).transpose()?;
        let number = to_u64(run, &format!("{key}'s run number"))?;
        runs.entry(key)
            .or_default()
            .insert(IdentifyRunId::of_number(number), PublishedRun { outcome, signals });
    }
    if let Some(run) = matches.keys().next() {
        return Err(DbError::Message(format!("run {run} holds matches but no verdict row")));
    }
    Ok(runs)
}

/// Where `key` stands, what each of its runs in flight last published, and
/// its typed search.
pub(crate) fn load_stored_runtime_on(
    sql: &(impl bae_db::client::query::QueryOne + bae_db::client::query::QueryRows),
    key: &CandidateKey,
) -> Result<StoredRuntime, DbError> {
    Ok(StoredRuntime {
        standing: load_standing_on(sql, key)?,
        runs: load_runs_on(sql, Some(key))?.remove(key).unwrap_or_default(),
        search: load_search_on(sql, key)?,
    })
}

/// Every key something is running for: where it stands, what each of its
/// runs in flight last published, and its typed search.
/// Delete every run's published state: nothing a run published outlives
/// the process that ran it.
pub(super) fn delete_every_run_on(sql: &CovenWrite<'_>) -> Result<(), DbError> {
    sql.execute("DELETE FROM import_identify_run", [])?;
    Ok(())
}

fn load_stored_runtimes_on(
    sql: &SqlReadContext<'_>,
) -> Result<HashMap<CandidateKey, StoredRuntime>, DbError> {
    let mut runtimes: HashMap<CandidateKey, StoredRuntime> = load_standings_on(sql)?
        .into_iter()
        .map(|(key, standing)| (key, StoredRuntime { standing, ..StoredRuntime::default() }))
        .collect();
    for (key, runs) in load_runs_on(sql, None)? {
        runtimes.entry(key).or_default().runs = runs;
    }
    for (key, search) in load_searches_on(sql)? {
        runtimes.entry(key).or_default().search = Some(search);
    }
    Ok(runtimes)
}

/// Store `outcome` as how `run` of `key` stands, with each of `releases`
/// its rows name, and `signals`, where given, as what extraction last read
/// for it.
fn store_run_on(
    sql: &CovenWrite<'_>,
    key: &CandidateKey,
    run: i64,
    outcome: &RunOutcome,
    releases: &[SourceRelease],
    signals: Option<&Signals>,
) -> Result<(), DbError> {
    let stored_key: Option<CandidateKey> = sql
        .query(
            "INSERT INTO import_identify_run (identify_run, candidate_key) VALUES (?1, ?2) \
             ON CONFLICT (identify_run) DO UPDATE SET candidate_key = candidate_key \
             RETURNING candidate_key",
            params![run, key],
            |row| row.get(0),
        )?
        .into_iter()
        .next();
    if stored_key.as_ref() != Some(key) {
        return Err(DbError::Message(format!("run {run} of {key} is stored as another key's")));
    }
    let owner = Owner::Run(run);
    store_outcome(sql, owner, Outcome::of(outcome), false)?;
    crate::tables::source_releases::store_named_releases_on(sql, releases)?;
    if let Some(signals) = signals {
        store_signals(sql, owner, signals)?;
    }
    Ok(())
}

/// How `state`, which `run` of `key` published, stands; a run publishes no
/// `Idle` while it runs.
fn published_outcome(
    key: &CandidateKey,
    run: IdentifyRunId,
    state: &IdentifyState,
) -> Result<RunOutcome, DbError> {
    RunOutcome::of(state)
        .ok_or_else(|| DbError::Message(format!("{key}'s run {run:?} published no state to store")))
}

/// Store how `run` of the library release whose re-identify key is `key`
/// stands, with each of the `releases` it read that its rows name, and the
/// signals it last read where it read new ones.
pub(crate) fn store_release_run_on(
    sql: &CovenWrite<'_>,
    key: &CandidateKey,
    run: IdentifyRunId,
    outcome: &RunOutcome,
    releases: &[SourceRelease],
    signals: Option<&Signals>,
) -> Result<(), DbError> {
    let run = to_i64(run.number(), "an identify run")?;
    store_run_on(sql, key, run, outcome, releases, signals)
}

/// Store how `run` of the folder candidate `key` stands, with the releases
/// its rows name, and the signals it last read, if its standing names that
/// run; whether it did.
fn publish_candidate_run_on(
    sql: &CovenWrite<'_>,
    key: &CandidateKey,
    run: IdentifyRunId,
    outcome: &RunOutcome,
    releases: &[SourceRelease],
    signals: Option<&Signals>,
) -> Result<bool, DbError> {
    let run = to_i64(run.number(), "an identify run")?;
    let named = sql.query_row(
        "SELECT EXISTS (SELECT 1 FROM import_candidate_standing \
                        WHERE candidate_key = ? AND identify_run = ?)",
        params![key, run],
        |row| row.get::<_, bool>(0),
    )?;
    if named {
        store_run_on(sql, key, run, outcome, releases, signals)?;
    }
    Ok(named)
}

impl ImportTables {
    /// Where `key` stands and what each of its runs in flight last published.
    pub(crate) async fn load_candidate_runtime(
        &self,
        key: &CandidateKey,
    ) -> Result<StoredRuntime, DbError> {
        let key = key.clone();
        self.database.read(move |sql| load_stored_runtime_on(&sql, &key)).await
    }

    /// Every key something is running for, as [`Self::load_candidate_runtime`]
    /// reads each.
    pub(crate) async fn load_candidate_runtimes(
        &self,
    ) -> Result<HashMap<CandidateKey, StoredRuntime>, DbError> {
        self.database.read(|sql| load_stored_runtimes_on(&sql)).await
    }

    /// The signals `run` of `key` last read, as its rows hold them; `None`
    /// once the run's rows are gone or before extraction reported to it.
    pub(crate) async fn load_candidate_run_signals(
        &self,
        key: &CandidateKey,
        run: IdentifyRunId,
    ) -> Result<Option<Signals>, DbError> {
        let key = key.clone();
        let run = to_i64(run.number(), "an identify run")?;
        self.database
            .read(move |sql| {
                let stored = sql.query(
                    &format!(
                        "SELECT {signals} \
                     FROM import_identify_run AS run \
                     JOIN import_candidate_signals AS signals \
                         ON signals.identify_run = run.identify_run \
                     WHERE run.candidate_key = ?1 AND run.identify_run = ?2",
                        signals = signals_read_columns(),
                    ),
                    params![key, run],
                    read_signals,
                )?;
                stored.into_iter().next().flatten().map(|stored| stored.signals()).transpose()
            })
            .await
    }

    /// What is in flight for `key`, live: its snapshot as each write of its
    /// standing, runs or search leaves it, `None` while nothing is.
    pub(crate) fn subscribe_candidate_runtime(
        &self,
        key: &CandidateKey,
    ) -> coven::LiveQuery<Option<crate::CandidateRuntimeSnapshot>> {
        let key = key.clone();
        self.database.subscribe(move |sql| {
            load_stored_runtime_on(&sql, &key)
                .map(|stored| crate::candidate_runtime::compose(&key, &stored))
                .map_err(CovenError::from)
        })
    }

    /// Start `run` from `key`'s wait for `files`, with `state` as the first
    /// thing it published and the releases it read, in one write: the run is
    /// stored only if the standing moves to it.
    pub(crate) async fn start_candidate_run(
        &self,
        key: &CandidateKey,
        files: &CandidateFiles,
        run: IdentifyRunId,
        state: &IdentifyState,
    ) -> Result<IdentificationMove, DbError> {
        let outcome = published_outcome(key, run, state)?;
        let releases = state.read_releases();
        let key = key.clone();
        let change = IdentificationChange::RunStarted { files: files.clone(), run };
        self.database
            .call(move |sql| {
                let moved = change_identification_on(sql, &key, &change)?;
                publish_candidate_run_on(sql, &key, run, &outcome, &releases, None)?;
                Ok(moved)
            })
            .await
    }

    /// Store `state` as how `run` of the folder candidate `key` stands, with
    /// each of the releases it read that its rows name, and the signals it
    /// last read where it read new ones, if its standing still names the run;
    /// whether it did.
    pub(crate) async fn publish_candidate_run(
        &self,
        key: &CandidateKey,
        run: IdentifyRunId,
        state: &IdentifyState,
        signals: Option<&Signals>,
    ) -> Result<bool, DbError> {
        let outcome = published_outcome(key, run, state)?;
        let releases = state.read_releases();
        let key = key.clone();
        let signals = signals.cloned();
        self.database
            .call(move |sql| {
                publish_candidate_run_on(sql, &key, run, &outcome, &releases, signals.as_ref())
            })
            .await
    }

    /// Store `state` as how `run` of the library release whose re-identify
    /// key is `key` stands, with each of the releases it read that its rows
    /// name, and the signals it last read where it read new ones. `Idle` ends
    /// the run: its rows go.
    pub(crate) async fn publish_release_run(
        &self,
        key: &CandidateKey,
        run: IdentifyRunId,
        state: &IdentifyState,
        signals: Option<&Signals>,
    ) -> Result<(), DbError> {
        let outcome = RunOutcome::of(state);
        let releases = state.read_releases();
        let key = key.clone();
        let signals = signals.cloned();
        self.database
            .call(move |sql| match outcome {
                Some(outcome) => {
                    store_release_run_on(sql, &key, run, &outcome, &releases, signals.as_ref())
                }
                None => {
                    sql.execute(
                        "DELETE FROM import_identify_run \
                         WHERE candidate_key = ? AND identify_run = ?",
                        params![key, to_i64(run.number(), "an identify run")?],
                    )?;
                    Ok(())
                }
            })
            .await
    }

    /// End `key`'s identification, whatever its standing holds, and every run
    /// of it, a library release's included, in one write.
    pub(crate) async fn cancel_candidate_identification(
        &self,
        key: &CandidateKey,
    ) -> Result<IdentificationMove, DbError> {
        let key = key.clone();
        self.database
            .call(move |sql| {
                let moved = change_identification_on(sql, &key, &IdentificationChange::Cancelled)?;
                sql.execute("DELETE FROM import_identify_run WHERE candidate_key = ?", [&key])?;
                Ok(moved)
            })
            .await
    }
}

#[cfg(test)]
#[path = "run_rows_tests.rs"]
mod tests;

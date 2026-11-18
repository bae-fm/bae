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
//! Its table is `run_rows.sql`, which every device's library holds.

pub(super) const SQL: &str = include_str!("run_rows.sql");

use super::cards_read::StoredVerdictCards;
use super::column_list::JsonColumns;
use super::match_record::{match_record_sql, StoredRecord};
use super::match_rows::StoredMatches;
use super::search_rows::{search_in, search_object_sql};
use super::signal_rows::{read_signals, signals_read_columns, store_signals};
use super::standing_rows::{
    change_identification_on, StandingColumns, COLUMNS as STANDING_COLUMNS,
};
use super::verdict_rows::{outcome_of, read_verdict_row, store_outcome, verdict_columns};
use super::verdict_rows::{Outcome, Owner};
use super::*;
use crate::candidate_runtime::{
    IdentificationChange, IdentificationMove, StoredRuntime, StoredVerdict,
};
use crate::CandidateFiles;
use crate::IdentifyRunId;
use bae_catalogs::source_release::SourceRelease;
use bae_identify::signals::Signals;
use bae_identify::{IdentifyState, PublishedRun, RunOutcome};

/// One run of a key as [`load_runtimes_on`] nests it.
#[derive(serde::Deserialize)]
struct StoredRun {
    run: i64,
    verdict: JsonColumns,
    signals: JsonColumns,
    cards: StoredVerdictCards,
    matches: Vec<StoredRecord>,
}

impl StoredRun {
    /// What the run last published, under the key it is of.
    fn published(self, key: &CandidateKey) -> Result<(IdentifyRunId, PublishedRun), DbError> {
        let Self { run, verdict, signals, cards, matches } = self;
        let verdict = read_verdict_row(&verdict)?.ok_or_else(|| {
            DbError::unreadable(format!("the verdict row of {key}'s run {run}"), None::<String>)
        })?;
        let (outcome, cards) = outcome_and_cards(verdict, matches, cards, &format!("run {run}"))?;
        let signals = read_signals(&signals)?.map(|stored| stored.signals()).transpose()?;
        let number = to_u64(run, &format!("{key}'s run number"))?;
        Ok((IdentifyRunId::of_number(number), PublishedRun { outcome, signals, cards }))
    }
}

/// `key`'s stored verdict as its pane shows it, read and converted as the
/// pane's read does: the verdict of its current state with the cards its rows
/// hold, and the sample rate its stored signals say rules out a CD.
fn load_stored_verdict_on(
    sql: &impl coven::SqlRead,
    key: &CandidateKey,
) -> Result<Option<StoredVerdict>, DbError> {
    let current = super::load_current_state_on(sql, key)?;
    let cards = super::load_candidate_verdict_cards_on(sql, key)?;
    let not_cd_rate = current
        .as_ref()
        .and_then(|state| state.signals.as_ref())
        .and_then(|signals| signals.origin.not_cd_rate);
    Ok(super::stored_verdict_view(current.as_ref(), cards)
        .map(|view| StoredVerdict { view, not_cd_rate }))
}

/// How a verdict row stands, with its matches, and the cards it shows: a run
/// in flight shows its cards, and an answer only when it holds findings.
fn outcome_and_cards(
    verdict: super::verdict_rows::VerdictRow,
    matches: Vec<StoredRecord>,
    cards: StoredVerdictCards,
    owner: &str,
) -> Result<(RunOutcome, bae_identify::VerdictCards), DbError> {
    let found = StoredMatches {
        rows: matches
            .into_iter()
            .map(|record| record.match_row(owner))
            .collect::<Result<_, _>>()?,
    };
    let (outcome, _) = outcome_of(verdict, found)?;
    let cards = match &outcome {
        RunOutcome::Running { .. } => true,
        RunOutcome::Answered(verdict) => verdict.findings().is_some(),
    }
    .then(|| cards.cards())
    .transpose()?
    .unwrap_or_default();
    Ok((outcome, cards))
}

/// The run a key's runtime shows, as one JSON object column: the run its
/// identification standing names, or, for a key with no identification
/// standing (a library release's re-identify session), its latest run in
/// flight, else its latest answer.
fn run_column(key: &str) -> String {
    format!(
        "(SELECT json_object( \
                  'run', run.identify_run, \
                  'verdict', {verdict}, \
                  'signals', {signals}, \
                  'cards', json({cards}), \
                  'matches', json((SELECT json_group_array(json({record}) ORDER BY found.position) \
                                   FROM import_candidate_match AS found \
                                   WHERE found.identify_run = run.identify_run))) \
          FROM import_identify_run AS run \
          JOIN import_candidate_verdict AS verdict ON verdict.identify_run = run.identify_run \
          LEFT JOIN import_candidate_signals AS signals \
              ON signals.identify_run = run.identify_run \
          WHERE run.candidate_key = {key} \
            AND (standing.identification IS NULL \
                 OR run.identify_run = standing.identify_run) \
          ORDER BY verdict.kind = 'running' DESC, run.identify_run DESC \
          LIMIT 1)",
        record = match_record_sql("found"),
        verdict = verdict_columns().object(),
        signals = signals_read_columns().object(),
        cards = super::cards_read::verdict_cards_sql("verdict"),
    )
}

/// Every key something is running for, or `key` alone whatever runs for it,
/// as one statement reads it: its standing, the run its runtime shows with
/// the verdict, signals, cards and releases nested in it, and its typed
/// search nested as one object.
fn load_runtimes_on(
    sql: &impl coven::SqlRead,
    key: Option<&CandidateKey>,
) -> Result<Vec<(CandidateKey, StoredRuntime)>, DbError> {
    let keys = match key {
        Some(_) => "SELECT ?1 AS candidate_key",
        None => {
            "SELECT candidate_key FROM import_candidate_standing \
             UNION SELECT run.candidate_key FROM import_identify_run AS run \
                   JOIN import_candidate_verdict AS verdict \
                       ON verdict.identify_run = run.identify_run \
             UNION SELECT candidate_key FROM import_candidate_search"
        }
    };
    let rows = sql.query(
        &format!(
            "SELECT keys.candidate_key AS runtime_key, \
                    {STANDING_COLUMNS}, \
                    {run} AS runtime_run, \
                    (SELECT {search} FROM import_candidate_search AS typed \
                     WHERE typed.candidate_key = keys.candidate_key) AS runtime_search \
             FROM ({keys}) AS keys \
             LEFT JOIN import_candidate_standing AS standing \
                 ON standing.candidate_key = keys.candidate_key \
             ORDER BY keys.candidate_key",
            STANDING_COLUMNS = STANDING_COLUMNS,
            search = search_object_sql(),
            run = run_column("keys.candidate_key"),
        ),
        rusqlite::params_from_iter(key),
        |row| {
            Ok((
                row.get::<_, CandidateKey>("runtime_key")?,
                StandingColumns::read(row, 1)?,
                row.get::<_, Option<String>>("runtime_run")?,
                row.get::<_, Option<String>>("runtime_search")?,
            ))
        },
    )?;
    rows.into_iter()
        .map(|(key, standing, run, search)| {
            let run = run
                .map(|json| {
                    let stored: StoredRun = serde_json::from_str(&json).map_err(|error| {
                        DbError::unreadable_json("a key's stored run", &json, error)
                    })?;
                    stored.published(&key)
                })
                .transpose()?;
            let search = search
                .map(|json| {
                    let columns: JsonColumns = serde_json::from_str(&json).map_err(|error| {
                        DbError::unreadable_json("a key's stored search", &json, error)
                    })?;
                    search_in(&columns)
                })
                .transpose()?;
            let runtime = StoredRuntime { standing: standing.standing(&key)?, run, search };
            Ok((key, runtime))
        })
        .collect()
}

/// Where `key` stands, what the run its runtime shows last published, and
/// its typed search.
pub(crate) fn load_stored_runtime_on(
    sql: &impl coven::SqlRead,
    key: &CandidateKey,
) -> Result<StoredRuntime, DbError> {
    // The statement reads one row for the one key it is given.
    Ok(load_runtimes_on(sql, Some(key))?
        .into_iter()
        .next()
        .map(|(_, runtime)| runtime)
        .unwrap_or_default())
}

/// Delete every run's published state: nothing a run published outlives
/// the process that ran it.
pub(super) fn delete_every_run_on(sql: &CovenWrite<'_>) -> Result<(), DbError> {
    sql.execute("DELETE FROM import_identify_run", [])?;
    Ok(())
}

/// Every key something is running for: where it stands, what the run its
/// runtime shows last published, and its typed search.
fn load_stored_runtimes_on(
    sql: &SqlReadContext<'_>,
) -> Result<Vec<(CandidateKey, StoredRuntime)>, DbError> {
    load_runtimes_on(sql, None)
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
    // Each run is minted for one key.
    assert!(stored_key.as_ref() == Some(key), "run {run} of {key} is stored as another key's");
    let owner = Owner::Run(run);
    // Store the signals first, because the outcome write checks its rows
    // against their text.
    if let Some(signals) = signals {
        store_signals(sql, owner, signals)?;
    }
    store_outcome(sql, owner, Outcome::of(outcome), false)?;
    crate::tables::source_releases::store_named_releases_on(sql, releases)?;
    Ok(())
}

/// How `outcome`, which `run` of `key` published, stands, read in the store's
/// work: a run publishes no `Idle` while it runs.
fn published_outcome(
    key: &CandidateKey,
    run: IdentifyRunId,
    outcome: Option<RunOutcome>,
) -> RunOutcome {
    outcome.unwrap_or_else(|| panic!("{key}'s run {run:?} published no state to store"))
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
        self.store.read(move |sql| load_stored_runtime_on(&sql, &key)).await
    }

    /// Every key something is running for, as [`Self::load_candidate_runtime`]
    /// reads each.
    pub(crate) async fn load_candidate_runtimes(
        &self,
    ) -> Result<Vec<(CandidateKey, StoredRuntime)>, DbError> {
        self.store.read(|sql| load_stored_runtimes_on(&sql)).await
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
        self.store
            .read(move |sql| {
                let stored = sql.query(
                    &format!(
                        "SELECT {signals} \
                     FROM import_identify_run AS run \
                     JOIN import_candidate_signals AS signals \
                         ON signals.identify_run = run.identify_run \
                     WHERE run.candidate_key = ?1 AND run.identify_run = ?2",
                        signals = signals_read_columns().select(),
                    ),
                    params![key, run],
                    |row| read_signals(row),
                )?;
                stored.into_iter().next().flatten().map(|stored| stored.signals()).transpose()
            })
            .await
    }

    /// The identify state `key`'s pane shows and what is in flight for it,
    /// live: a value as each write of its standing, runs, stored verdict or
    /// search leaves it.
    pub(crate) fn subscribe_candidate_runtime(
        &self,
        key: &CandidateKey,
    ) -> coven::LiveQuery<crate::CandidateRuntimeView, bae_store::DbError> {
        let key = key.clone();
        self.store.subscribe(move |sql| {
            let stored = load_stored_runtime_on(&sql, &key)?;
            let stored_verdict = load_stored_verdict_on(&sql, &key)?;
            Ok(crate::candidate_runtime::compose_view(&key, &stored, stored_verdict))
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
        let outcome = RunOutcome::of(state);
        let releases = state.read_releases();
        let key = key.clone();
        let change = IdentificationChange::RunStarted { files: files.clone(), run };
        self.store
            .call(move |sql| {
                let outcome = published_outcome(&key, run, outcome);
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
        let outcome = RunOutcome::of(state);
        let releases = state.read_releases();
        let key = key.clone();
        let signals = signals.cloned();
        self.store
            .call(move |sql| {
                let outcome = published_outcome(&key, run, outcome);
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
        self.store
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
        self.store
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

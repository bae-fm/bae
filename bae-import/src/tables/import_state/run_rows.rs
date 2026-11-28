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

use super::column_list::JsonColumns;
use super::search_rows::search_in;
use super::signal_rows::{store_signals, StoredPaneSignals, StoredSignals};
use super::standing_rows::change_identification_on;
use super::verdict_rows::{outcome_of, store_outcome};
use super::verdict_rows::{Outcome, Owner};
use super::*;
use crate::candidate_runtime::{
    IdentificationChange, IdentificationMove, StoredRuntime, StoredVerdict,
};
use crate::candidate_search::CandidateSearch;
use crate::CandidateFiles;
use crate::IdentifyRunId;
use bae_catalogs::source_release::SourceRelease;
use bae_identify::signals::Signals;
use bae_identify::{IdentifyState, PublishedRun, RunOutcome};
use bae_store::WriteStatements as _;

sqlive_compose::fragment!(
    RuntimeRows with keys(candidate_key TEXT) {
        runtime_key: crate::CandidateKey,
        runtime_run: Option<i64>
    },
    r#"
    WITH chosen AS (
        SELECT keys.candidate_key AS candidate_key, (
            SELECT run.identify_run
            FROM import_identify_run AS run
            JOIN import_candidate_verdict AS verdict ON verdict.identify_run = run.identify_run
            LEFT JOIN import_candidate_standing AS standing
                ON standing.candidate_key = run.candidate_key
            WHERE run.candidate_key = keys.candidate_key
                AND (standing.identification IS NULL OR run.identify_run = standing.identify_run)
            ORDER BY verdict.kind = 'running' DESC, run.identify_run DESC
            LIMIT 1
        ) AS identify_run
        FROM keys
    ), owners AS (
        SELECT NULL AS content_hash, chosen.identify_run AS identify_run
        FROM chosen
        WHERE chosen.identify_run IS NOT NULL
    )
    SELECT chosen.candidate_key AS runtime_key, chosen.identify_run AS runtime_run,
        standing.*, state.*, lines.*, cards.*, search.*
    FROM chosen
    LEFT JOIN @crate::tables::import_state::fragments::Standing(
        keys: (SELECT * FROM keys)) AS standing ON standing.standing_key = chosen.candidate_key
    LEFT JOIN @crate::tables::import_state::state_fragments::CurrentState(
        owner: (SELECT * FROM owners)) AS state ON state.owner_row = 'run ' || chosen.identify_run
    LEFT JOIN @crate::tables::import_state::state_fragments::SignalTextPool(
        owner: (SELECT * FROM owners)) AS lines ON lines.owner_row = 'run ' || chosen.identify_run
    LEFT JOIN @crate::tables::import_state::card_fragments::VerdictCards(
        owner: (SELECT * FROM owners)) AS cards ON cards.owner_row = 'run ' || chosen.identify_run
    LEFT JOIN @crate::tables::import_state::search_rows::RuntimeSearch(
        keys: (SELECT * FROM keys)) AS search ON search.search_key = chosen.candidate_key
"#);

sqlive_compose::read!(
    writer RuntimeOf(key: crate::CandidateKey),
    r#"
    SELECT runtime.*
    FROM @crate::tables::import_state::run_rows::RuntimeRows(
        keys: (SELECT :key AS candidate_key)) AS runtime
"#
);

sqlive_compose::read!(
    writer EveryRuntime,
    r#"
    SELECT runtime.*
    FROM @crate::tables::import_state::run_rows::RuntimeRows(keys: (
        SELECT standing.candidate_key AS candidate_key FROM import_candidate_standing AS standing
        UNION SELECT run.candidate_key FROM import_identify_run AS run
            JOIN import_candidate_verdict AS verdict ON verdict.identify_run = run.identify_run
        UNION SELECT typed.candidate_key FROM import_candidate_search AS typed)) AS runtime
    ORDER BY runtime.runtime_key
"#
);

sqlive_compose::read!(
    writer RunSignals(key: crate::CandidateKey, run: i64),
    r#"
    WITH owners AS (
        SELECT NULL AS content_hash, run.identify_run AS identify_run
        FROM import_identify_run AS run
        WHERE run.candidate_key = :key AND run.identify_run = :run
    )
    SELECT signals.*, lines.*
    FROM @crate::tables::import_state::state_fragments::Signals(
        owner: (SELECT * FROM owners)) AS signals
    LEFT JOIN @crate::tables::import_state::state_fragments::SignalTextPool(
        owner: (SELECT * FROM owners)) AS lines ON lines.owner_row = signals.owner_row
"#
);

/// What the run a runtime read chose last published, under the key it is of.
fn published_of(
    key: &CandidateKey,
    row: RuntimeRows,
) -> Result<Option<(IdentifyRunId, PublishedRun)>, DbError> {
    let Some(run) = row.runtime_run else {
        return Ok(None);
    };
    let state = row.state.ok_or_else(|| {
        DbError::unreadable(format!("the stored rows of {key}'s run {run}"), None::<String>)
    })?;
    let cards_row = row.cards.ok_or_else(|| {
        DbError::unreadable(format!("the cards row of {key}'s run {run}"), None::<String>)
    })?;
    let owner = format!("run {run}");
    let verdict = super::verdict_rows::verdict_row_of_state(&state)?.ok_or_else(|| {
        DbError::unreadable(format!("the verdict row of {key}'s run {run}"), None::<String>)
    })?;
    let matches = super::match_record::match_rows_of(&owner, &state.candidate_matches)?;
    let (outcome, _) = outcome_of(verdict, matches)?;
    // A run in flight shows its cards, and an answer only when it holds
    // findings.
    let cards = match &outcome {
        RunOutcome::Running { .. } => true,
        RunOutcome::Answered(verdict) => verdict.findings().is_some(),
    }
    .then(|| super::verdict_cards_of_column(cards_row.verdict_cards.as_deref()))
    .transpose()?
    .unwrap_or_default();
    let signals = StoredPaneSignals::of(state.signals.as_ref())
        .map(|pane| {
            let lines = row.lines.map(|lines| lines.signals_text_pool);
            StoredSignals::of(pane, super::required(lines, "the signals' text lines")?).signals()
        })
        .transpose()?;
    let number = to_u64(run, &format!("{key}'s run number"))?;
    Ok(Some((IdentifyRunId::of_number(number), PublishedRun { outcome, signals, cards })))
}

/// The verdict stored for a candidate whose current state and cards are
/// `current` and `cards`.
fn stored_verdict_of(
    current: Option<DbImportCandidateState<bae_identify::signals::PaneSignals>>,
    cards: bae_identify::VerdictCards,
) -> Option<StoredVerdict> {
    let not_cd_rate = current
        .as_ref()
        .and_then(|state| state.signals.as_ref())
        .and_then(|signals| signals.origin.not_cd_rate);
    super::stored_verdict_view(current.as_ref(), cards)
        .map(|view| StoredVerdict { view, not_cd_rate })
}

/// The typed search a runtime read returned.
fn search_of(
    search: Option<super::search_rows::RuntimeSearch>,
) -> Result<Option<CandidateSearch>, DbError> {
    search
        .map(|found| {
            let columns: JsonColumns = serde_json::from_str(&found.search).map_err(|error| {
                DbError::unreadable_json("a key's stored search", &found.search, error)
            })?;
            search_in(&columns)
        })
        .transpose()
}

/// What one row of a runtime read holds, as the runtime shows it: where its key
/// stands, what the run it names last published, and its typed search.
fn runtime_of(row: RuntimeRows) -> Result<(CandidateKey, StoredRuntime), DbError> {
    let key = row.runtime_key.clone();
    let standing = super::standing_of(row.standing.clone(), &key)?;
    let search = search_of(row.search.clone())?;
    Ok((key.clone(), StoredRuntime { standing, run: published_of(&key, row)?, search }))
}

/// The runtime of the one key the row of a [`RuntimeOf`] read holds.
fn runtime_in(row: RuntimeOfRow) -> Result<StoredRuntime, DbError> {
    runtime_of(row.runtime).map(|(_, runtime)| runtime)
}

sqlive_compose::write!(
    DeleteEveryRun,
    r#"
    DELETE FROM import_identify_run
"#
);

/// Delete every run's published state: nothing a run published outlives
/// the process that ran it.
pub(super) fn delete_every_run_on(sql: &CovenWrite<'_>) -> Result<(), DbError> {
    sql.run(&DeleteEveryRunParams {})?;
    Ok(())
}

sqlive_compose::write!(
    ClaimRunKey(
        run: i64,
        key: &'p crate::CandidateKey,
    ) { candidate_key: crate::CandidateKey },
    r#"
    INSERT INTO import_identify_run (identify_run, candidate_key) VALUES (:run, :key)
    ON CONFLICT (identify_run) DO UPDATE SET candidate_key = candidate_key
    RETURNING candidate_key
"#
);

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
    let stored_key = sql.returning_one(&ClaimRunKeyParams { run, key })?.candidate_key;
    // Each run is minted for one key.
    assert!(stored_key == *key, "run {run} of {key} is stored as another key's");
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

sqlive_compose::write!(
    DeleteRunOfKey(key: &'p crate::CandidateKey, run: i64),
    r#"
    DELETE FROM import_identify_run WHERE candidate_key = :key AND identify_run = :run
"#
);

sqlive_compose::write!(
    DeleteRunsOfKey(key: &'p crate::CandidateKey),
    r#"
    DELETE FROM import_identify_run WHERE candidate_key = :key
"#
);

impl ImportTables {
    /// Where `key` stands and what each of its runs in flight last published.
    pub(crate) async fn load_candidate_runtime(
        &self,
        key: &CandidateKey,
    ) -> Result<StoredRuntime, DbError> {
        runtime_in(self.store.read_row(RuntimeOfParams { key: key.clone() }).await?)
    }

    /// Every key something is running for, as [`Self::load_candidate_runtime`]
    /// reads each.
    pub(crate) async fn load_candidate_runtimes(
        &self,
    ) -> Result<Vec<(CandidateKey, StoredRuntime)>, DbError> {
        self.store
            .read_rows(EveryRuntimeParams {})
            .await?
            .into_iter()
            .map(|row| runtime_of(row.runtime))
            .collect()
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
                let request = RunSignalsParams { key, run };
                let row = statement_optional(&sql, &request)?;
                row.map(|row| {
                    let lines = row.lines.map(|lines| lines.signals_text_pool);
                    StoredPaneSignals::of(Some(&row.signals))
                        .map(|pane| {
                            StoredSignals::of(
                                pane,
                                super::required(lines, "the signals' text lines")?,
                            )
                            .signals()
                        })
                        .transpose()
                })
                .transpose()
                .map(Option::flatten)
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
        let reads = coven::Both(
            RuntimeOfParams { key: key.clone() },
            super::stored_verdict_read::StoredVerdictParams { key: key.clone() },
        );
        let key = key.clone();
        self.store.subscribe(reads).process(move |(runtime, verdict)| {
            let stored = runtime_in(bae_store::exactly_one::<RuntimeOfParams, _>(runtime)?)?;
            let verdict = bae_store::exactly_one::<
                super::stored_verdict_read::StoredVerdictParams,
                _,
            >(verdict)?;
            let (current, cards) = super::stored_verdict_read::current_state_and_cards_of(
                verdict.state,
                verdict.cards,
            )?;
            let stored_verdict = stored_verdict_of(current, cards);
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
                    sql.run(&DeleteRunOfKeyParams {
                        key: &key,
                        run: to_i64(run.number(), "an identify run")?,
                    })?;
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
                sql.run(&DeleteRunsOfKeyParams { key: &key })?;
                Ok(moved)
            })
            .await
    }
}

#[cfg(test)]
#[path = "run_rows_tests.rs"]
mod tests;

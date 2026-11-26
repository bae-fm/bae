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

sqlive_compose::fragment!(
    RuntimeSearch with keys(candidate_key TEXT) {
        search_key: crate::CandidateKey,
        search: String
    },
    r#"
    SELECT typed.candidate_key AS search_key, (
            json_object('mode', typed.mode, 'artist', typed.artist, 'album', typed.album,
                'catalog_number', typed.catalog_number, 'barcode', typed.barcode, 'cards',(
                WITH recs AS (
                SELECT m.*
                    FROM (
                        SELECT ROW_NUMBER() OVER (ORDER BY part.position, found.position) - 1 AS
                            ord, found.card, found.pressing, found.record_order, found.section,
                            found.row_order, 'returned' AS kind, found.source, found.release_id,
                            0 AS a_disc_id, 0 AS a_barcode, 0 AS a_catalog, 0 AS a_label, 0 AS
                            a_year, 0 AS a_country, 0 AS a_title, 0 AS a_artist, NULL AS
                            named_note,(EXISTS(
                        SELECT 1
                        FROM import_candidate_release_link AS release_link
                            WHERE release_link.content_hash =(
                            SELECT asked.content_hash
                            FROM import_candidate_search AS asked
                            WHERE asked.search_run = found.search_run
                        ) AND release_link.kind = 'pressing' AND release_link.source =
                            found.source AND release_link.release_id = found.release_id
                        ) OR (0 AND NOT EXISTS (
                        SELECT 1
                        FROM import_candidate_release_link AS release_link
                            WHERE release_link.content_hash =(
                            SELECT asked.content_hash
                            FROM import_candidate_search AS asked
                            WHERE asked.search_run = found.search_run
                        ) AND release_link.kind = 'pressing'
                    ))) AS chosen, 0 AS pick_being_read
                    FROM import_candidate_match AS found
                    JOIN import_candidate_search_source AS part ON part.search_run =
                        found.search_run AND part.catalog = found.source
                    WHERE found.search_run = typed.search_run
                ) AS m
                ), pressings AS (
                    SELECT r.card, r.pressing, r.kind, MIN(r.section) AS section,
                        MIN(r.row_order) AS row_order, MAX(r.chosen) AS chosen,
                        MAX(r.pick_being_read) AS pick_being_read,
                        json_group_array(json(rc.record) ORDER BY r.record_order) AS records,
                        MAX(json_object('catalog', r.source, 'key', r.release_id)) FILTER (WHERE
                        r.record_order = 0) AS lead, MAX(EXISTS(
                    SELECT 1
                    FROM @bae_records::records::spread_fragments::HeldPressing AS held
                    WHERE held.catalog = r.source AND held.key = r.release_id
                )) FILTER (WHERE r.record_order = 0) AS in_library, json_object('disc_id',
                    MAX(r.a_disc_id), 'barcode', MAX(r.a_barcode), 'catalog', MAX(r.a_catalog),
                    'label', MAX(r.a_label), 'year', MAX(r.a_year), 'country', MAX(r.a_country),
                    'title', MAX(r.a_title), 'artist', MAX(r.a_artist)) AS agreed
                FROM recs AS r
                    JOIN (
                        SELECT ROW_NUMBER() OVER (ORDER BY part.position, found.position) - 1 AS
                            ord, json(@crate::tables::import_state::match_record::MatchRecord(parent: found)) AS record
                    FROM import_candidate_match AS found
                    JOIN import_candidate_search_source AS part ON part.search_run =
                        found.search_run AND part.catalog = found.source
                    WHERE found.search_run = typed.search_run
                ) AS rc ON rc.ord = r.ord
                GROUP BY r.card, r.pressing, r.kind
                ), notes AS (
                SELECT g.card, g.pressing, g.kind, json_group_array(json_object('note', g.note,
                    'catalogs', json(CASE WHEN g.noted > 1 THEN g.catalogs ELSE '[]' END)) ORDER
                    BY g.first) AS notes
                    FROM (
                    SELECT h.*, COUNT(*) OVER (PARTITION BY h.card, h.pressing, h.kind) AS noted
                        FROM (
                        SELECT d.card, d.pressing, d.kind, d.named_note AS note, MIN(d.first) AS
                            first, json_group_array(d.source ORDER BY
                            bae_catalog_rank(d.source)) AS catalogs
                            FROM (
                            SELECT DISTINCT r.card, r.pressing, r.kind, r.named_note, r.source,
                                MIN(r.record_order) OVER (PARTITION BY r.card, r.pressing,
                                r.kind, r.named_note) AS first
                            FROM recs AS r
                            WHERE r.named_note IS NOT NULL
                        ) AS d
                        GROUP BY d.card, d.pressing, d.kind, d.named_note
                    ) AS h
                ) AS g
                GROUP BY g.card, g.pressing, g.kind
                ), rowj AS (
                    SELECT p.*, json_object('records', json(p.records), 'in_library',
                        p.in_library, 'agreed', json(p.agreed), 'chosen', p.chosen,
                        'pick_being_read', p.pick_being_read, 'notes', json(COALESCE(n.notes,
                        '[]')), 'statements', json((
                    SELECT json_group_array(json_object('fact', t.fact, 'line', t.line,
                        'ordinal', t.ordinal, 'value', t.value, 'catalogs', json(t.catalogs))
                        ORDER BY t.line, t.ordinal)
                    FROM import_candidate_row_statement AS t
                        WHERE t.list =(
                        SELECT l.list
                        FROM import_candidate_card_list AS l
                        WHERE l.search_run = typed.search_run
                    ) AND t.listing = p.kind AND t.pressing = p.pressing
                ))) AS row
                FROM pressings AS p
                LEFT JOIN notes AS n ON n.card = p.card AND n.pressing = p.pressing AND n.kind =
                    p.kind
                ), secj AS (
                SELECT r.card, r.section, json_object('pressings',
                    json((json_group_array(json(r.row) ORDER BY r.row_order) FILTER (WHERE
                    r.kind <> 'narrowed_out'))), 'narrowed_out',
                    json((json_group_array(json(r.row) ORDER BY r.row_order) FILTER (WHERE
                    r.kind = 'narrowed_out')))) AS section_json, json_group_array(json(r.lead)
                    ORDER BY r.row_order) FILTER (WHERE r.kind <> 'narrowed_out') AS leads
                FROM rowj AS r
                GROUP BY r.card, r.section
                ), card_secs AS (
                SELECT s.card, json_group_array(json(s.section_json) ORDER BY s.section) AS
                    sections, json_group_array(json(s.leads) ORDER BY s.section) AS leads
                FROM secj AS s
                GROUP BY s.card
                ) SELECT json_group_array(json_object('id', c.id, 'titles', json((
                SELECT json_group_array(t.text ORDER BY t.ordinal)
                FROM import_candidate_card_text AS t
                    WHERE t.list =(
                    SELECT l.list
                    FROM import_candidate_card_list AS l
                    WHERE l.search_run = typed.search_run
                ) AND t.card = c.card AND t.kind = 'title'
                )), 'artists', json((
                SELECT json_group_array(t.text ORDER BY t.ordinal)
                FROM import_candidate_card_text AS t
                    WHERE t.list =(
                    SELECT l.list
                    FROM import_candidate_card_list AS l
                    WHERE l.search_run = typed.search_run
                ) AND t.card = c.card AND t.kind = 'artist'
                )), 'labels', json((
                SELECT json_group_array(t.text ORDER BY t.ordinal)
                FROM import_candidate_card_text AS t
                    WHERE t.list =(
                    SELECT l.list
                    FROM import_candidate_card_list AS l
                    WHERE l.search_run = typed.search_run
                ) AND t.card = c.card AND t.kind = 'label'
                )), 'cover', json((
                SELECT json(rc.record)
                    FROM (
                        SELECT ROW_NUMBER() OVER (ORDER BY part.position, found.position) - 1 AS
                            ord, json(@crate::tables::import_state::match_record::MatchRecord(parent: found)) AS record
                    FROM import_candidate_match AS found
                    JOIN import_candidate_search_source AS part ON part.search_run =
                        found.search_run AND part.catalog = found.source
                    WHERE found.search_run = typed.search_run
                ) AS rc
                WHERE rc.ord = c.cover_ord
                )), 'leads', json((
                SELECT json_group_array(json(lead.value))
                FROM json_each(cs.leads) AS sec, json_each(sec.value) AS lead
            )), 'sources', json(c.sources), 'headings', json(c.headings), 'year_min',
                c.year_min, 'year_max', c.year_max, 'sections', json(cs.sections)) ORDER BY
                c.card_order)
                FROM (
                SELECT t.*
                FROM import_candidate_card AS t
                    WHERE t.list =(
                    SELECT l.list
                    FROM import_candidate_card_list AS l
                    WHERE l.search_run = typed.search_run
                )
            ) AS c
            JOIN card_secs AS cs ON cs.card = c.card
            ), 'parts',(
                SELECT json_group_array(json_object('catalog', part.catalog, 'state',
                    part.state, 'failure', part.failure, 'failure_status', part.failure_status,
                    'error_detail', part.error_detail, 'found',(
                SELECT COUNT(*)
                FROM import_candidate_match AS found
                WHERE found.search_run = part.search_run AND found.source = part.catalog
            )) ORDER BY part.position)
            FROM import_candidate_search_source AS part
            WHERE part.search_run = typed.search_run
            ), 'status', CASE WHEN EXISTS (
            SELECT 1
            FROM import_candidate_search_source AS part
            WHERE part.search_run = typed.search_run AND part.state = 'searching'
            ) THEN 'searching' WHEN EXISTS (
            SELECT 1
            FROM import_candidate_search_source AS part
            WHERE part.search_run = typed.search_run AND part.state = 'failed'
            ) THEN 'failed' WHEN EXISTS (
            SELECT 1
            FROM import_candidate_search_source AS part
            WHERE part.search_run = typed.search_run AND part.state = 'done'
            ) AND NOT EXISTS (
            SELECT 1
            FROM import_candidate_match AS found
            WHERE found.search_run = typed.search_run
        ) THEN 'no_matches' ELSE 'found' END)
    ) AS search
    FROM keys
    JOIN import_candidate_search AS typed ON typed.candidate_key = keys.candidate_key
"#);

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
    LEFT JOIN @crate::tables::import_state::run_rows::RuntimeSearch(
        keys: (SELECT * FROM keys)) AS search ON search.search_key = chosen.candidate_key
"#);

sqlive_compose::read!(
    RuntimeOf(key: crate::CandidateKey),
    r#"
    SELECT runtime.*
    FROM @crate::tables::import_state::run_rows::RuntimeRows(
        keys: (SELECT :key AS candidate_key)) AS runtime
"#
);

sqlive_compose::read!(
    EveryRuntime,
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
    RunSignals(key: crate::CandidateKey, run: i64),
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
    .then(|| {
        super::verdict_cards_of_column(row.cards.and_then(|cards| cards.verdict_cards).as_deref())
    })
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

/// `key`'s stored verdict as its pane shows it, read and converted as the
/// pane's read does: the verdict of its current state with the cards its rows
/// hold, and the sample rate its stored signals say rules out a CD.
fn load_stored_verdict_on(
    sql: &impl coven::SqlRead,
    key: &CandidateKey,
) -> Result<Option<StoredVerdict>, DbError> {
    let (current, cards) = super::load_current_state_and_cards_on(sql, key)?;
    let not_cd_rate = current
        .as_ref()
        .and_then(|state| state.signals.as_ref())
        .and_then(|signals| signals.origin.not_cd_rate);
    Ok(super::stored_verdict_view(current.as_ref(), cards)
        .map(|view| StoredVerdict { view, not_cd_rate }))
}

/// The typed search a runtime read returned.
fn search_of(search: Option<RuntimeSearch>) -> Result<Option<CandidateSearch>, DbError> {
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

/// Where `key` stands, what the run its runtime shows last published, and
/// its typed search. The runtime shows the run its identification standing
/// names or, for a key with no identification standing (a library release's
/// re-identify session), its latest run in flight, else its latest answer.
pub(crate) fn load_stored_runtime_on(
    sql: &impl coven::SqlRead,
    key: &CandidateKey,
) -> Result<StoredRuntime, DbError> {
    let request = RuntimeOfParams { key: key.clone() };
    // The statement reads one row for the one key it is given.
    sql.query(RuntimeOf::SQL, request.params().as_slice(), RuntimeOfRow::read)?
        .into_iter()
        .next()
        .map(|row| runtime_of(row.runtime).map(|(_, runtime)| runtime))
        .unwrap_or_else(|| Ok(StoredRuntime::default()))
}

/// Every key something is running for: where it stands, what the run its
/// runtime shows last published, and its typed search.
fn load_stored_runtimes_on(
    sql: &SqlReadContext<'_>,
) -> Result<Vec<(CandidateKey, StoredRuntime)>, DbError> {
    sql.query(EveryRuntime::SQL, EveryRuntimeParams {}.params().as_slice(), EveryRuntimeRow::read)?
        .into_iter()
        .map(|row| runtime_of(row.runtime))
        .collect()
}

/// Delete every run's published state: nothing a run published outlives
/// the process that ran it.
pub(super) fn delete_every_run_on(sql: &CovenWrite<'_>) -> Result<(), DbError> {
    sql.execute("DELETE FROM import_identify_run", [])?;
    Ok(())
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
                let request = RunSignalsParams { key, run };
                let row = sql
                    .query(RunSignals::SQL, request.params().as_slice(), RunSignalsRow::read)?
                    .into_iter()
                    .next();
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

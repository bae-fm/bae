//! The ledger a run recorded, as rows: how each clue stood (the verdict's
//! step columns), the catalogs the run asked, the ISRCs it looked up, the
//! catalog numbers it offered, and its rows, each with its lookups and the
//! releases each found. A found release is the match row that holds it, so
//! the ledger keeps no copy of a release; its cells lay out again the
//! releases they link, as the run laid them out.
//!
//! A verdict's ledger is recorded once every lookup has settled, so it is
//! stored settled: a clue still being read, a cell still queued or looking
//! up, or artwork still being scanned is no state a verdict ends on, and
//! storing one fails. A run in flight stores its ledger as it stands.
//! Its tables are `ledger_rows.sql`, which every device's library holds.

pub(super) const SQL: &str = include_str!("ledger_rows.sql");

use super::lookup_failure_columns::{failure_columns, failure_of};
use super::verdict_rows::Owner;
use super::*;
use bae_catalogs::catalog::{Catalog, LookupCatalog};
use bae_catalogs::search::MetadataResult;
use bae_identify::{
    BarcodeStepView, CatalogCandidateView, CatalogStepView, DiscIdRow, DiscIdStepView,
    IdentifyRunView, IsrcStepView, LookupView, NotAskedReason, ProviderCell, SearchStepView,
    SignalValueRow,
};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

/// How each clue stood in a recorded ledger, as the verdict's step columns;
/// every one `None` for a verdict that recorded no ledger.
pub(super) struct StepColumns<'a> {
    pub(super) disc_id: Option<&'static str>,
    pub(super) barcode: Option<&'static str>,
    pub(super) catalog: Option<&'static str>,
    pub(super) isrc: Option<&'static str>,
    pub(super) search: Option<&'static str>,
    pub(super) search_album: Option<&'a str>,
    pub(super) search_artist: Option<&'a str>,
}

/// A ledger's clue in a state no settled run records: a settled verdict is
/// written only once every clue settled.
fn unsettled(owner: Owner<'_>, what: &str) -> ! {
    panic!("the ledger recorded for {owner} shows {what}, which no settled run records")
}

/// How each clue of `ledger` stands, as step columns. `running`: the ledger
/// is a run's in flight, whose clues may not have settled.
pub(super) fn step_columns<'a>(
    owner: Owner<'_>,
    ledger: Option<&'a IdentifyRunView>,
    running: bool,
) -> StepColumns<'a> {
    let Some(ledger) = ledger else {
        return StepColumns {
            disc_id: None,
            barcode: None,
            catalog: None,
            isrc: None,
            search: None,
            search_album: None,
            search_artist: None,
        };
    };
    let disc_id = match &ledger.disc_id {
        DiscIdStepView::Reading if running => "reading",
        DiscIdStepView::Reading => unsettled(owner, "a disc ID being read"),
        DiscIdStepView::Absent => "absent",
        DiscIdStepView::NotCdAudio => "not_cd_audio",
        DiscIdStepView::Read { .. } => "read",
    };
    let barcode = match &ledger.barcode {
        BarcodeStepView::Absent => "absent",
        BarcodeStepView::NoCodes => "no_codes",
        BarcodeStepView::Rows { scanning: true, .. } if running => "scanning",
        BarcodeStepView::Rows { scanning: true, .. } => {
            unsettled(owner, "barcodes still being scanned")
        }
        BarcodeStepView::Rows { scanning: false, .. } => "rows",
    };
    let catalog = match &ledger.catalog {
        CatalogStepView::NoneFound => "none_found",
        CatalogStepView::Numbers { scanning: true, .. } if running => "scanning",
        CatalogStepView::Numbers { scanning: true, .. } => {
            unsettled(owner, "catalog numbers still being scanned")
        }
        CatalogStepView::Numbers { scanning: false, .. } => "numbers",
    };
    let isrc = match &ledger.isrc {
        IsrcStepView::Reading if running => "reading",
        IsrcStepView::Reading => unsettled(owner, "ISRCs being read"),
        IsrcStepView::Absent => "absent",
        IsrcStepView::Read { .. } => "read",
    };
    let (search, search_album, search_artist) = match &ledger.search {
        SearchStepView::NotNeeded => ("not_needed", None, None),
        SearchStepView::NoTitle => ("no_title", None, None),
        SearchStepView::Waiting { album, artist } if running => {
            ("waiting", Some(album.as_str()), Some(artist.as_str()))
        }
        SearchStepView::Waiting { .. } => unsettled(owner, "a title search waiting to go out"),
        SearchStepView::Searched { album, artist, .. } => {
            ("searched", Some(album.as_str()), Some(artist.as_str()))
        }
    };
    StepColumns {
        disc_id: Some(disc_id),
        barcode: Some(barcode),
        catalog: Some(catalog),
        isrc: Some(isrc),
        search: Some(search),
        search_album,
        search_artist,
    }
}

/// Clear the ledger rows `owner` holds; the lookups and the releases they
/// found go with their rows.
pub(super) fn delete_ledger(sql: &CovenWrite<'_>, owner: Owner<'_>) -> Result<(), DbError> {
    for table in [
        "import_candidate_lookup_value",
        "import_candidate_lookup_provider",
        "import_candidate_lookup_isrc",
        "import_candidate_lookup_offered_catalog",
    ] {
        sql.execute(&format!("DELETE FROM {table} WHERE {} = ?", owner.column()), params![owner])?;
    }
    Ok(())
}

/// The match rows a ledger's releases are, by release: each one's position
/// and the copy the row holds.
pub(super) type MatchPositions<'a> = HashMap<(LookupCatalog, &'a str), (usize, &'a MetadataResult)>;

/// Write `ledger`'s rows under `owner`'s verdict row, whose step columns are
/// written and whose previous ledger rows are gone. Each release a cell lists
/// is linked to the match row holding it, which must hold the cell's copy of
/// it: the cell is laid out again from that row. `running`: the ledger is a
/// run's in flight, whose cells may be queued or looking up.
pub(super) fn store_ledger(
    sql: &CovenWrite<'_>,
    owner: Owner<'_>,
    ledger: &IdentifyRunView,
    matches: &MatchPositions<'_>,
    running: bool,
) -> Result<(), DbError> {
    let (content_hash, run) = (owner.content_hash(), owner.run());
    for (position, catalog) in ledger.providers.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_lookup_provider \
                 (content_hash, identify_run, position, catalog) \
             VALUES (?, ?, ?, ?)",
            params![content_hash, run, column(position)?, catalog.as_str()],
        )?;
    }
    let row = |step: &str, position: usize, value: Option<&str>, excluded: bool| {
        sql.execute(
            "INSERT INTO import_candidate_lookup_value \
                 (content_hash, identify_run, step, position, value, excluded) \
             VALUES (?, ?, ?, ?, ?, ?)",
            params![content_hash, run, step, column(position)?, value, excluded],
        )?;
        Ok::<_, DbError>(())
    };
    let cell = |step: &str,
                position: usize,
                cell: usize,
                catalog: Option<Catalog>,
                lookup: &LookupView| {
        store_lookup(
            sql,
            owner,
            LookupKey { step, position, cell },
            catalog,
            lookup,
            (matches, running),
        )
    };
    let value_rows = |step: &str, rows: &[SignalValueRow]| {
        for (position, value_row) in rows.iter().enumerate() {
            row(step, position, Some(&value_row.value), value_row.excluded)?;
            for (at, provider) in value_row.cells.iter().enumerate() {
                cell(step, position, at, Some(provider.source.catalog()), &provider.lookup)?;
            }
        }
        Ok::<_, DbError>(())
    };
    if let DiscIdStepView::Read { discs } = &ledger.disc_id {
        for (position, disc) in discs.iter().enumerate() {
            row(STEP_DISC_ID, position, Some(&disc.disc_id), false)?;
            cell(STEP_DISC_ID, position, 0, None, &disc.lookup)?;
        }
    }
    if let BarcodeStepView::Rows { rows, .. } = &ledger.barcode {
        value_rows(STEP_BARCODE, rows)?;
    }
    if let CatalogStepView::Numbers { rows, candidates, .. } = &ledger.catalog {
        value_rows(STEP_CATALOG, rows)?;
        for (position, candidate) in candidates.iter().enumerate() {
            sql.execute(
                "INSERT INTO import_candidate_lookup_offered_catalog \
                     (content_hash, identify_run, position, value) \
                 VALUES (?, ?, ?, ?)",
                params![content_hash, run, column(position)?, candidate.value],
            )?;
        }
    }
    if let IsrcStepView::Read { isrcs, lookup } = &ledger.isrc {
        for (position, isrc) in isrcs.iter().enumerate() {
            sql.execute(
                "INSERT INTO import_candidate_lookup_isrc \
                     (content_hash, identify_run, position, isrc) \
                 VALUES (?, ?, ?, ?)",
                params![content_hash, run, column(position)?, isrc],
            )?;
        }
        row(STEP_ISRC, 0, None, false)?;
        cell(STEP_ISRC, 0, 0, None, lookup)?;
    }
    if let SearchStepView::Searched { cells, .. } = &ledger.search {
        row(STEP_SEARCH, 0, None, false)?;
        for (at, provider) in cells.iter().enumerate() {
            cell(STEP_SEARCH, 0, at, Some(provider.source.catalog()), &provider.lookup)?;
        }
    }
    Ok(())
}

const STEP_DISC_ID: &str = "disc_id";
const STEP_BARCODE: &str = "barcode";
const STEP_CATALOG: &str = "catalog";
const STEP_ISRC: &str = "isrc";
const STEP_SEARCH: &str = "search";

/// Which cell of which row a lookup is.
#[derive(Clone, Copy)]
struct LookupKey<'a> {
    step: &'a str,
    position: usize,
    cell: usize,
}

/// One cell, and the releases it found linked to their match rows; queued
/// or looking up only in a run's ledger in flight (`running`).
fn store_lookup(
    sql: &CovenWrite<'_>,
    owner: Owner<'_>,
    key: LookupKey<'_>,
    catalog: Option<Catalog>,
    lookup: &LookupView,
    (matches, running): (&MatchPositions<'_>, bool),
) -> Result<(), DbError> {
    let LookupKey { step, position, cell } = key;
    let (answer, reason, failure) = match lookup {
        LookupView::Queued if running => ("queued", None, None),
        LookupView::LookingUp if running => ("looking_up", None, None),
        LookupView::Queued => unsettled(owner, "a queued lookup"),
        LookupView::LookingUp => unsettled(owner, "a lookup under way"),
        LookupView::NotAsked { reason } => (
            "not_asked",
            Some(match reason {
                NotAskedReason::LeftOut => "left_out",
                NotAskedReason::NoCatalog => "no_catalog",
            }),
            None,
        ),
        LookupView::Found { .. } | LookupView::NoMatch => ("answered", None, None),
        LookupView::Failed { failure } => ("failed", None, Some(failure)),
    };
    let failure = failure_columns(failure);
    sql.execute(
        "INSERT INTO import_candidate_lookup \
             (content_hash, identify_run, step, position, cell, catalog, answer, \
              not_asked_reason, failure, failure_status) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            owner.content_hash(),
            owner.run(),
            step,
            column(position)?,
            column(cell)?,
            catalog.map(|catalog| catalog.as_str()),
            answer,
            reason,
            failure.kind,
            failure.status,
        ],
    )?;
    for (ordinal, release) in lookup.releases().into_iter().enumerate() {
        let Some((match_position, held)) =
            matches.get(&(release.source, release.release_id.as_str()))
        else {
            panic!(
                "a ledger cell of {owner} lists {} {}, which no match row holds",
                release.source.as_str(),
                release.release_id
            );
        };
        // The verdict's lookups list the releases its matches hold, written
        // before them in the same write.
        assert!(
            *held == release,
            "a ledger cell of {owner} lists {} {} as other than its match row holds it",
            release.source.as_str(),
            release.release_id
        );
        sql.execute(
            "INSERT INTO import_candidate_lookup_release \
                 (content_hash, identify_run, step, position, cell, ordinal, match_position) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                owner.content_hash(),
                owner.run(),
                step,
                column(position)?,
                column(cell)?,
                column(ordinal)?,
                column(*match_position)?
            ],
        )?;
    }
    Ok(())
}

fn column(index: usize) -> Result<i64, DbError> {
    to_i64(index, "a ledger row's place in its list")
}

/// A verdict's ledger as columns of a read that joins its row as `verdict`:
/// the step columns, and the ledger's lists riding on them as JSON arrays —
/// the providers, the ISRCs and the offered catalog numbers in order, and
/// the rows in order, each with its lookups in cell order and each lookup
/// with the match positions of the releases it found in its order.
pub(super) const LEDGER_COLUMNS: &str = "verdict.disc_id_step AS ledger_disc_id_step, \
     verdict.barcode_step AS ledger_barcode_step, \
     verdict.catalog_step AS ledger_catalog_step, verdict.isrc_step AS ledger_isrc_step, \
     verdict.search_step AS ledger_search_step, verdict.search_album AS ledger_search_album, \
     verdict.search_artist AS ledger_search_artist, \
     (SELECT json_group_array(provider.catalog ORDER BY provider.position) \
      FROM import_candidate_lookup_provider AS provider \
      WHERE provider.content_hash IS verdict.content_hash AND provider.identify_run IS verdict.identify_run) AS ledger_providers, \
     (SELECT json_group_array(isrc.isrc ORDER BY isrc.position) \
      FROM import_candidate_lookup_isrc AS isrc \
      WHERE isrc.content_hash IS verdict.content_hash AND isrc.identify_run IS verdict.identify_run) AS ledger_isrcs, \
     (SELECT json_group_array(offered.value ORDER BY offered.position) \
      FROM import_candidate_lookup_offered_catalog AS offered \
      WHERE offered.content_hash IS verdict.content_hash AND offered.identify_run IS verdict.identify_run) AS ledger_offered_catalogs, \
     (SELECT json_group_array(json_object( \
          'step', ledger_row.step, 'value', ledger_row.value, \
          'excluded', ledger_row.excluded, \
          'cells', json((SELECT json_group_array(json_object( \
              'catalog', lookup.catalog, 'answer', lookup.answer, \
              'reason', lookup.not_asked_reason, 'failure', lookup.failure, \
              'status', lookup.failure_status, \
              'releases', json((SELECT json_group_array(found.match_position \
                                                        ORDER BY found.ordinal) \
                                FROM import_candidate_lookup_release AS found \
                                WHERE found.content_hash IS lookup.content_hash AND found.identify_run IS lookup.identify_run \
                                  AND found.step = lookup.step \
                                  AND found.position = lookup.position \
                                  AND found.cell = lookup.cell))) \
              ORDER BY lookup.cell) \
              FROM import_candidate_lookup AS lookup \
              WHERE lookup.content_hash IS ledger_row.content_hash AND lookup.identify_run IS ledger_row.identify_run \
                AND lookup.step = ledger_row.step \
                AND lookup.position = ledger_row.position))) \
          ORDER BY ledger_row.step, ledger_row.position) \
      FROM import_candidate_lookup_value AS ledger_row \
      WHERE ledger_row.content_hash IS verdict.content_hash AND ledger_row.identify_run IS verdict.identify_run) AS ledger_rows";

/// One verdict's [`LEDGER_COLUMNS`] as SQLite hands them over, its lists
/// still JSON.
pub(super) struct StoredLedger {
    disc_id: Option<String>,
    barcode: Option<String>,
    catalog: Option<String>,
    isrc: Option<String>,
    search: Option<String>,
    search_album: Option<String>,
    search_artist: Option<String>,
    providers: String,
    isrcs: String,
    offered_catalogs: String,
    rows: String,
}

pub(super) fn read_ledger(row: &Row<'_>) -> rusqlite::Result<StoredLedger> {
    Ok(StoredLedger {
        disc_id: row.get("ledger_disc_id_step")?,
        barcode: row.get("ledger_barcode_step")?,
        catalog: row.get("ledger_catalog_step")?,
        isrc: row.get("ledger_isrc_step")?,
        search: row.get("ledger_search_step")?,
        search_album: row.get("ledger_search_album")?,
        search_artist: row.get("ledger_search_artist")?,
        providers: row.get("ledger_providers")?,
        isrcs: row.get("ledger_isrcs")?,
        offered_catalogs: row.get("ledger_offered_catalogs")?,
        rows: row.get("ledger_rows")?,
    })
}

/// One ledger row as [`LEDGER_COLUMNS`] holds it.
#[derive(serde::Deserialize)]
struct StoredRow {
    step: String,
    value: Option<String>,
    excluded: i64,
    cells: Vec<StoredLookup>,
}

/// One lookup as [`LEDGER_COLUMNS`] holds it.
#[derive(serde::Deserialize)]
struct StoredLookup {
    catalog: Option<String>,
    answer: String,
    reason: Option<String>,
    failure: Option<String>,
    status: Option<i64>,
    releases: Vec<i64>,
}

impl StoredLedger {
    /// The ledger these rows record, its cells laying out the releases at
    /// the match positions they link, `None` for a verdict that recorded
    /// none; and every match position a cell links.
    pub(super) fn ledger(
        self,
        content_hash: &str,
        matches: &HashMap<i64, &MetadataResult>,
    ) -> Result<(Option<IdentifyRunView>, HashSet<i64>), DbError> {
        let Self {
            disc_id,
            barcode,
            catalog,
            isrc,
            search,
            search_album,
            search_artist,
            providers,
            isrcs,
            offered_catalogs,
            rows,
        } = self;
        let list = |json: &str, what: &str| {
            serde_json::from_str::<Vec<String>>(json).map_err(|error| {
                DbError::unreadable_json(
                    format!("the stored ledger {what} of {content_hash}"),
                    json,
                    error,
                )
            })
        };
        let providers = list(&providers, "providers")?;
        let isrcs = list(&isrcs, "ISRCs")?;
        let offered = list(&offered_catalogs, "offered catalog numbers")?;
        let rows: Vec<StoredRow> = serde_json::from_str(&rows).map_err(|error| {
            DbError::unreadable_json(
                format!("the stored ledger rows of {content_hash}"),
                &rows,
                error,
            )
        })?;
        let (Some(disc_id), Some(barcode), Some(catalog), Some(isrc), Some(search)) =
            (disc_id, barcode, catalog, isrc, search)
        else {
            if !providers.is_empty() || !isrcs.is_empty() || !offered.is_empty() || !rows.is_empty()
            {
                return Err(DbError::unreadable(
                    format!("the ledger of {content_hash}, whose verdict records none"),
                    (&providers, &isrcs, &offered, rows.len()),
                ));
            }
            return Ok((None, HashSet::new()));
        };
        let linked: HashSet<i64> = rows
            .iter()
            .flat_map(|row| &row.cells)
            .flat_map(|cell| cell.releases.iter().copied())
            .collect();
        let mut by_step: HashMap<String, Vec<StoredRow>> = HashMap::new();
        for row in rows {
            by_step.entry(row.step.clone()).or_default().push(row);
        }
        let mut take = |step: &str| by_step.remove(step).unwrap_or_default();
        let lookup = |stored: StoredLookup| lookup_of(content_hash, stored, matches);
        let one_lookup = |row: StoredRow, what: &str| {
            let mut cells = row.cells.into_iter();
            match (cells.next(), cells.next()) {
                (Some(cell), None) => lookup(cell),
                (first, second) => Err(DbError::unreadable(
                    format!("the lookups of a stored {what} row of {content_hash}"),
                    (first.is_some(), second.is_some()),
                )),
            }
        };
        let provider_cells = |row: Vec<StoredLookup>| {
            row.into_iter()
                .map(|cell| {
                    let source = cell
                        .catalog
                        .as_deref()
                        .ok_or_else(|| {
                            DbError::unreadable(
                                format!("import_candidate_lookup.catalog of {content_hash}"),
                                None::<String>,
                            )
                        })
                        .and_then(|catalog| {
                            LookupCatalog::from_str(catalog).map_err(|_unknown| {
                                DbError::unreadable("import_candidate_lookup.catalog", catalog)
                            })
                        })?;
                    Ok(ProviderCell { source, lookup: lookup(cell)? })
                })
                .collect::<Result<Vec<_>, DbError>>()
        };
        let value_rows = |rows: Vec<StoredRow>| {
            rows.into_iter()
                .map(|row| {
                    let value = row.value.ok_or_else(|| {
                        DbError::unreadable(
                            format!("the value of a stored {} row of {content_hash}", row.step),
                            None::<String>,
                        )
                    })?;
                    Ok(SignalValueRow {
                        value,
                        excluded: row.excluded != 0,
                        cells: provider_cells(row.cells)?,
                    })
                })
                .collect::<Result<Vec<_>, DbError>>()
        };
        let stray = |rows: &[StoredRow], step: &str, stood: &str| {
            if rows.is_empty() {
                return Ok(());
            }
            Err(DbError::unreadable(
                format!("the {step} rows of {content_hash}, whose {step} step is {stood}"),
                rows.len(),
            ))
        };

        let disc_rows = take(STEP_DISC_ID);
        let disc_id = match disc_id.as_str() {
            "read" => DiscIdStepView::Read {
                discs: disc_rows
                    .into_iter()
                    .map(|row| {
                        let disc_id = row.value.clone().ok_or_else(|| {
                            DbError::unreadable(
                                format!("the Disc ID of a stored Disc ID row of {content_hash}"),
                                None::<String>,
                            )
                        })?;
                        Ok(DiscIdRow { disc_id, lookup: one_lookup(row, "disc ID")? })
                    })
                    .collect::<Result<_, DbError>>()?,
            },
            stood @ ("reading" | "absent" | "not_cd_audio") => {
                stray(&disc_rows, STEP_DISC_ID, stood)?;
                match stood {
                    "reading" => DiscIdStepView::Reading,
                    "absent" => DiscIdStepView::Absent,
                    _ => DiscIdStepView::NotCdAudio,
                }
            }
            other => {
                return Err(DbError::unreadable("import_candidate_verdict.disc_id_step", other))
            }
        };
        let barcode_rows = take(STEP_BARCODE);
        let barcode = match barcode.as_str() {
            stood @ ("rows" | "scanning") => BarcodeStepView::Rows {
                scanning: stood == "scanning",
                rows: value_rows(barcode_rows)?,
            },
            stood @ ("absent" | "no_codes") => {
                stray(&barcode_rows, STEP_BARCODE, stood)?;
                match stood {
                    "absent" => BarcodeStepView::Absent,
                    _ => BarcodeStepView::NoCodes,
                }
            }
            other => {
                return Err(DbError::unreadable("import_candidate_verdict.barcode_step", other))
            }
        };
        let catalog_rows = take(STEP_CATALOG);
        let catalog = match catalog.as_str() {
            stood @ ("numbers" | "scanning") => CatalogStepView::Numbers {
                scanning: stood == "scanning",
                rows: value_rows(catalog_rows)?,
                candidates: offered
                    .into_iter()
                    .map(|value| CatalogCandidateView { value })
                    .collect(),
            },
            "none_found" => {
                stray(&catalog_rows, STEP_CATALOG, "none_found")?;
                if !offered.is_empty() {
                    return Err(DbError::unreadable(
                        format!("the catalog numbers offered by {content_hash}, which found none"),
                        &offered,
                    ));
                }
                CatalogStepView::NoneFound
            }
            other => {
                return Err(DbError::unreadable("import_candidate_verdict.catalog_step", other))
            }
        };
        let isrc_rows = take(STEP_ISRC);
        let isrc = match isrc.as_str() {
            "read" => {
                let count = isrc_rows.len();
                let mut rows = isrc_rows.into_iter();
                let (Some(row), None) = (rows.next(), rows.next()) else {
                    return Err(DbError::unreadable(
                        format!("the ISRC rows of {content_hash}, which read ISRCs"),
                        count,
                    ));
                };
                IsrcStepView::Read { isrcs, lookup: one_lookup(row, "ISRC")? }
            }
            stood @ ("reading" | "absent") => {
                stray(&isrc_rows, STEP_ISRC, stood)?;
                if !isrcs.is_empty() {
                    return Err(DbError::unreadable(
                        format!("the ISRCs of {content_hash}, whose ISRC step is {stood}"),
                        &isrcs,
                    ));
                }
                match stood {
                    "reading" => IsrcStepView::Reading,
                    _ => IsrcStepView::Absent,
                }
            }
            other => return Err(DbError::unreadable("import_candidate_verdict.isrc_step", other)),
        };
        let search_rows = take(STEP_SEARCH);
        let search = match (search.as_str(), search_album, search_artist) {
            ("searched", Some(album), Some(artist)) => {
                let count = search_rows.len();
                let mut rows = search_rows.into_iter();
                let (Some(row), None) = (rows.next(), rows.next()) else {
                    return Err(DbError::unreadable(
                        format!("the search rows of {content_hash}, which searched"),
                        count,
                    ));
                };
                SearchStepView::Searched { album, artist, cells: provider_cells(row.cells)? }
            }
            ("waiting", Some(album), Some(artist)) => {
                stray(&search_rows, STEP_SEARCH, "waiting")?;
                SearchStepView::Waiting { album, artist }
            }
            (stood @ ("not_needed" | "no_title"), None, None) => {
                stray(&search_rows, STEP_SEARCH, stood)?;
                match stood {
                    "not_needed" => SearchStepView::NotNeeded,
                    _ => SearchStepView::NoTitle,
                }
            }
            (other, _, _) => {
                return Err(DbError::unreadable("import_candidate_verdict.search_step", other))
            }
        };
        if let Some(step) = by_step.keys().next() {
            return Err(DbError::unreadable("import_candidate_lookup.step", step));
        }
        let ledger = IdentifyRunView {
            providers: providers
                .iter()
                .map(|catalog| {
                    LookupCatalog::from_str(catalog).map_err(|_unknown| {
                        DbError::unreadable("a stored ledger's providers", catalog)
                    })
                })
                .collect::<Result<_, _>>()?,
            disc_id,
            barcode,
            catalog,
            isrc,
            search,
        };
        Ok((Some(ledger), linked))
    }
}

/// One stored lookup as its cell: why nobody was asked, how it failed, or
/// the releases it found laid out as the run laid them out.
fn lookup_of(
    content_hash: &str,
    stored: StoredLookup,
    matches: &HashMap<i64, &MetadataResult>,
) -> Result<LookupView, DbError> {
    let StoredLookup { catalog: _, answer, reason, failure, status, releases } = stored;
    if answer != "answered" && !releases.is_empty() {
        return Err(DbError::unreadable(
            format!("the releases a {answer} lookup of {content_hash} found"),
            &releases,
        ));
    }
    match answer.as_str() {
        "queued" => Ok(LookupView::Queued),
        "looking_up" => Ok(LookupView::LookingUp),
        "not_asked" => Ok(LookupView::NotAsked {
            reason: match reason.as_deref() {
                Some("left_out") => NotAskedReason::LeftOut,
                Some("no_catalog") => NotAskedReason::NoCatalog,
                other => {
                    return Err(DbError::unreadable(
                        "import_candidate_lookup.not_asked_reason",
                        other.unwrap_or("NULL"),
                    ))
                }
            },
        }),
        "failed" => Ok(LookupView::Failed {
            failure: failure_of(failure, status)?.ok_or_else(|| {
                DbError::unreadable(
                    format!("the failure of a failed lookup of {content_hash}"),
                    None::<String>,
                )
            })?,
        }),
        "answered" => {
            let found = releases
                .into_iter()
                .map(|position| {
                    matches.get(&position).map(|held| (*held).clone()).ok_or_else(|| {
                        DbError::unreadable(
                            format!("a match a lookup of {content_hash} found, which no match row holds"),
                            position,
                        )
                    })
                })
                .collect::<Result<Vec<_>, DbError>>()?;
            Ok(LookupView::answered(&found))
        }
        other => Err(DbError::unreadable("a stored lookup answer", other)),
    }
}

#[cfg(test)]
#[path = "ledger_rows_tests.rs"]
mod tests;

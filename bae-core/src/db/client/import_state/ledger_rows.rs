//! The ledger a run recorded, as rows: how each clue stood (the verdict's
//! step columns), the catalogs the run asked, the ISRCs it looked up, the
//! catalog numbers it offered, and its rows, each with its lookups and the
//! releases each found. A found release is the match row that holds it, so
//! the ledger keeps no copy of a release; its cells lay out again the
//! releases they link, as the run laid them out.
//!
//! A ledger is recorded once every lookup has settled, so only a settled
//! ledger is stored: a clue still being read, a cell still queued or looking
//! up, or artwork still being scanned is no state a verdict ends on, and
//! storing one fails.

use super::lookup_failure_columns::{failure_columns, failure_of};
use super::verdict_rows::unreadable;
use super::*;
use crate::identify::{
    BarcodeStepView, CatalogCandidateView, CatalogStepView, DiscIdRow, DiscIdStepView,
    IdentifyRunView, IsrcStepView, LookupView, NotAskedReason, ProviderCell, SearchStepView,
    SignalValueRow,
};
use crate::import::search::MetadataResult;
use bae_catalogs::catalog::Catalog;
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

/// A ledger's clue in a state no settled run records.
fn unsettled(content_hash: &str, what: &str) -> DbError {
    DbError::Message(format!(
        "the ledger recorded for {content_hash} shows {what}, which no settled run records"
    ))
}

pub(super) fn step_columns<'a>(
    content_hash: &str,
    ledger: Option<&'a IdentifyRunView>,
) -> Result<StepColumns<'a>, DbError> {
    let Some(ledger) = ledger else {
        return Ok(StepColumns {
            disc_id: None,
            barcode: None,
            catalog: None,
            isrc: None,
            search: None,
            search_album: None,
            search_artist: None,
        });
    };
    let disc_id = match &ledger.disc_id {
        DiscIdStepView::Reading => return Err(unsettled(content_hash, "a disc ID being read")),
        DiscIdStepView::Absent => "absent",
        DiscIdStepView::NotCdAudio => "not_cd_audio",
        DiscIdStepView::Read { .. } => "read",
    };
    let barcode = match &ledger.barcode {
        BarcodeStepView::Absent => "absent",
        BarcodeStepView::NoCodes => "no_codes",
        BarcodeStepView::Rows { scanning: true, .. } => {
            return Err(unsettled(content_hash, "barcodes still being scanned"))
        }
        BarcodeStepView::Rows {
            scanning: false, ..
        } => "rows",
    };
    let catalog = match &ledger.catalog {
        CatalogStepView::NoneFound => "none_found",
        CatalogStepView::Numbers { scanning: true, .. } => {
            return Err(unsettled(
                content_hash,
                "catalog numbers still being scanned",
            ))
        }
        CatalogStepView::Numbers {
            scanning: false, ..
        } => "numbers",
    };
    let isrc = match &ledger.isrc {
        IsrcStepView::Reading => return Err(unsettled(content_hash, "ISRCs being read")),
        IsrcStepView::Absent => "absent",
        IsrcStepView::Read { .. } => "read",
    };
    let (search, search_album, search_artist) = match &ledger.search {
        SearchStepView::NotNeeded => ("not_needed", None, None),
        SearchStepView::NoTitle => ("no_title", None, None),
        SearchStepView::Waiting { .. } => {
            return Err(unsettled(content_hash, "a title search waiting to go out"))
        }
        SearchStepView::Searched { album, artist, .. } => {
            ("searched", Some(album.as_str()), Some(artist.as_str()))
        }
    };
    Ok(StepColumns {
        disc_id: Some(disc_id),
        barcode: Some(barcode),
        catalog: Some(catalog),
        isrc: Some(isrc),
        search: Some(search),
        search_album,
        search_artist,
    })
}

/// Clear the ledger rows under `content_hash`; the lookups and the releases
/// they found go with their rows.
pub(super) fn delete_ledger(sql: &SqlContext<'_, '_>, content_hash: &str) -> Result<(), DbError> {
    for table in [
        "import_candidate_lookup_value",
        "import_candidate_lookup_provider",
        "import_candidate_lookup_isrc",
        "import_candidate_lookup_offered_catalog",
    ] {
        sql.execute(
            &format!("DELETE FROM {table} WHERE content_hash = ?"),
            [content_hash],
        )?;
    }
    Ok(())
}

/// The match rows a ledger's releases are, by release: each one's position
/// and the copy the row holds.
pub(super) type MatchPositions<'a> = HashMap<(Catalog, &'a str), (usize, &'a MetadataResult)>;

/// Write `ledger`'s rows under the verdict at `content_hash`, whose step
/// columns are written and whose previous ledger rows are gone. Each release
/// a cell lists is linked to the match row holding it, which must hold the
/// cell's copy of it: the cell is laid out again from that row.
pub(super) fn store_ledger(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    ledger: &IdentifyRunView,
    matches: &MatchPositions<'_>,
) -> Result<(), DbError> {
    for (position, catalog) in ledger.providers.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_lookup_provider (content_hash, position, catalog) \
             VALUES (?, ?, ?)",
            params![content_hash, column(position)?, catalog.as_str()],
        )?;
    }
    let row = |step: &str, position: usize, value: Option<&str>, excluded: bool| {
        sql.execute(
            "INSERT INTO import_candidate_lookup_value \
                 (content_hash, step, position, value, excluded) \
             VALUES (?, ?, ?, ?, ?)",
            params![content_hash, step, column(position)?, value, excluded],
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
            content_hash,
            LookupKey {
                step,
                position,
                cell,
            },
            catalog,
            lookup,
            matches,
        )
    };
    let value_rows = |step: &str, rows: &[SignalValueRow]| {
        for (position, value_row) in rows.iter().enumerate() {
            row(step, position, Some(&value_row.value), value_row.excluded)?;
            for (at, provider) in value_row.cells.iter().enumerate() {
                cell(step, position, at, Some(provider.source), &provider.lookup)?;
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
    if let CatalogStepView::Numbers {
        rows, candidates, ..
    } = &ledger.catalog
    {
        value_rows(STEP_CATALOG, rows)?;
        for (position, candidate) in candidates.iter().enumerate() {
            sql.execute(
                "INSERT INTO import_candidate_lookup_offered_catalog \
                     (content_hash, position, value) \
                 VALUES (?, ?, ?)",
                params![content_hash, column(position)?, candidate.value],
            )?;
        }
    }
    if let IsrcStepView::Read { isrcs, lookup } = &ledger.isrc {
        for (position, isrc) in isrcs.iter().enumerate() {
            sql.execute(
                "INSERT INTO import_candidate_lookup_isrc (content_hash, position, isrc) \
                 VALUES (?, ?, ?)",
                params![content_hash, column(position)?, isrc],
            )?;
        }
        row(STEP_ISRC, 0, None, false)?;
        cell(STEP_ISRC, 0, 0, None, lookup)?;
    }
    if let SearchStepView::Searched { cells, .. } = &ledger.search {
        row(STEP_SEARCH, 0, None, false)?;
        for (at, provider) in cells.iter().enumerate() {
            cell(STEP_SEARCH, 0, at, Some(provider.source), &provider.lookup)?;
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

/// One cell, and the releases it found linked to their match rows.
fn store_lookup(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    key: LookupKey<'_>,
    catalog: Option<Catalog>,
    lookup: &LookupView,
    matches: &MatchPositions<'_>,
) -> Result<(), DbError> {
    let LookupKey {
        step,
        position,
        cell,
    } = key;
    let (answer, reason, failure) = match lookup {
        LookupView::Queued => return Err(unsettled(content_hash, "a queued lookup")),
        LookupView::LookingUp => return Err(unsettled(content_hash, "a lookup under way")),
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
             (content_hash, step, position, cell, catalog, answer, not_asked_reason, failure, \
              failure_status) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            content_hash,
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
            return Err(DbError::Message(format!(
                "a ledger cell of {content_hash} lists {} {}, which no match row holds",
                release.source.as_str(),
                release.release_id
            )));
        };
        if *held != release {
            return Err(DbError::Message(format!(
                "a ledger cell of {content_hash} lists {} {} as other than its match row \
                 holds it",
                release.source.as_str(),
                release.release_id
            )));
        }
        sql.execute(
            "INSERT INTO import_candidate_lookup_release \
                 (content_hash, step, position, cell, ordinal, match_position) \
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
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
    i64::try_from(index).map_err(|error| {
        DbError::Message(format!(
            "a ledger list is longer than SQLite counts: {error}"
        ))
    })
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
      WHERE provider.content_hash = verdict.content_hash) AS ledger_providers, \
     (SELECT json_group_array(isrc.isrc ORDER BY isrc.position) \
      FROM import_candidate_lookup_isrc AS isrc \
      WHERE isrc.content_hash = verdict.content_hash) AS ledger_isrcs, \
     (SELECT json_group_array(offered.value ORDER BY offered.position) \
      FROM import_candidate_lookup_offered_catalog AS offered \
      WHERE offered.content_hash = verdict.content_hash) AS ledger_offered_catalogs, \
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
                                WHERE found.content_hash = lookup.content_hash \
                                  AND found.step = lookup.step \
                                  AND found.position = lookup.position \
                                  AND found.cell = lookup.cell))) \
              ORDER BY lookup.cell) \
              FROM import_candidate_lookup AS lookup \
              WHERE lookup.content_hash = ledger_row.content_hash \
                AND lookup.step = ledger_row.step \
                AND lookup.position = ledger_row.position))) \
          ORDER BY ledger_row.step, ledger_row.position) \
      FROM import_candidate_lookup_value AS ledger_row \
      WHERE ledger_row.content_hash = verdict.content_hash) AS ledger_rows";

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
                DbError::Message(format!(
                    "the stored ledger {what} of {content_hash} {json:?}: {error}"
                ))
            })
        };
        let providers = list(&providers, "providers")?;
        let isrcs = list(&isrcs, "ISRCs")?;
        let offered = list(&offered_catalogs, "offered catalog numbers")?;
        let rows: Vec<StoredRow> = serde_json::from_str(&rows).map_err(|error| {
            DbError::Message(format!(
                "the stored ledger rows of {content_hash} {rows:?}: {error}"
            ))
        })?;
        let (Some(disc_id), Some(barcode), Some(catalog), Some(isrc), Some(search)) =
            (disc_id, barcode, catalog, isrc, search)
        else {
            if !providers.is_empty() || !isrcs.is_empty() || !offered.is_empty() || !rows.is_empty()
            {
                return Err(DbError::Message(format!(
                    "the verdict for {content_hash} records no ledger but holds ledger rows"
                )));
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
                _ => Err(DbError::Message(format!(
                    "a stored {what} row of {content_hash} holds other than one lookup"
                ))),
            }
        };
        let provider_cells = |row: Vec<StoredLookup>| {
            row.into_iter()
                .map(|cell| {
                    let source = cell
                        .catalog
                        .as_deref()
                        .ok_or_else(|| {
                            DbError::Message(format!(
                                "a stored lookup of {content_hash} names no catalog"
                            ))
                        })
                        .and_then(|catalog| Catalog::from_str(catalog).map_err(DbError::Message))?;
                    Ok(ProviderCell {
                        source,
                        lookup: lookup(cell)?,
                    })
                })
                .collect::<Result<Vec<_>, DbError>>()
        };
        let value_rows = |rows: Vec<StoredRow>| {
            rows.into_iter()
                .map(|row| {
                    let value = row.value.ok_or_else(|| {
                        DbError::Message(format!(
                            "a stored ledger row of {content_hash} names no value"
                        ))
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
            Err(DbError::Message(format!(
                "the ledger of {content_hash} holds {step} rows but its {step} step is {stood}"
            )))
        };

        let disc_rows = take(STEP_DISC_ID);
        let disc_id = match disc_id.as_str() {
            "read" => DiscIdStepView::Read {
                discs: disc_rows
                    .into_iter()
                    .map(|row| {
                        let disc_id = row.value.clone().ok_or_else(|| {
                            DbError::Message(format!(
                                "a stored disc ID row of {content_hash} names no disc ID"
                            ))
                        })?;
                        Ok(DiscIdRow {
                            disc_id,
                            lookup: one_lookup(row, "disc ID")?,
                        })
                    })
                    .collect::<Result<_, DbError>>()?,
            },
            stood @ ("absent" | "not_cd_audio") => {
                stray(&disc_rows, STEP_DISC_ID, stood)?;
                match stood {
                    "absent" => DiscIdStepView::Absent,
                    _ => DiscIdStepView::NotCdAudio,
                }
            }
            other => return Err(unreadable("disc_id_step", other)),
        };
        let barcode_rows = take(STEP_BARCODE);
        let barcode = match barcode.as_str() {
            "rows" => BarcodeStepView::Rows {
                scanning: false,
                rows: value_rows(barcode_rows)?,
            },
            stood @ ("absent" | "no_codes") => {
                stray(&barcode_rows, STEP_BARCODE, stood)?;
                match stood {
                    "absent" => BarcodeStepView::Absent,
                    _ => BarcodeStepView::NoCodes,
                }
            }
            other => return Err(unreadable("barcode_step", other)),
        };
        let catalog_rows = take(STEP_CATALOG);
        let catalog = match catalog.as_str() {
            "numbers" => CatalogStepView::Numbers {
                scanning: false,
                rows: value_rows(catalog_rows)?,
                candidates: offered
                    .into_iter()
                    .map(|value| CatalogCandidateView { value })
                    .collect(),
            },
            "none_found" => {
                stray(&catalog_rows, STEP_CATALOG, "none_found")?;
                if !offered.is_empty() {
                    return Err(DbError::Message(format!(
                        "the ledger of {content_hash} offers catalog numbers but found none"
                    )));
                }
                CatalogStepView::NoneFound
            }
            other => return Err(unreadable("catalog_step", other)),
        };
        let isrc_rows = take(STEP_ISRC);
        let isrc = match isrc.as_str() {
            "read" => {
                let mut rows = isrc_rows.into_iter();
                let (Some(row), None) = (rows.next(), rows.next()) else {
                    return Err(DbError::Message(format!(
                        "the ledger of {content_hash} read ISRCs but holds other than one ISRC row"
                    )));
                };
                IsrcStepView::Read {
                    isrcs,
                    lookup: one_lookup(row, "ISRC")?,
                }
            }
            "absent" => {
                stray(&isrc_rows, STEP_ISRC, "absent")?;
                if !isrcs.is_empty() {
                    return Err(DbError::Message(format!(
                        "the ledger of {content_hash} holds ISRCs but read none"
                    )));
                }
                IsrcStepView::Absent
            }
            other => return Err(unreadable("isrc_step", other)),
        };
        let search_rows = take(STEP_SEARCH);
        let search = match (search.as_str(), search_album, search_artist) {
            ("searched", Some(album), Some(artist)) => {
                let mut rows = search_rows.into_iter();
                let (Some(row), None) = (rows.next(), rows.next()) else {
                    return Err(DbError::Message(format!(
                        "the ledger of {content_hash} searched but holds other than one search row"
                    )));
                };
                SearchStepView::Searched {
                    album,
                    artist,
                    cells: provider_cells(row.cells)?,
                }
            }
            (stood @ ("not_needed" | "no_title"), None, None) => {
                stray(&search_rows, STEP_SEARCH, stood)?;
                match stood {
                    "not_needed" => SearchStepView::NotNeeded,
                    _ => SearchStepView::NoTitle,
                }
            }
            (other, _, _) => return Err(unreadable("search_step", other)),
        };
        if let Some(step) = by_step.keys().next() {
            return Err(unreadable("ledger row step", step));
        }
        let ledger = IdentifyRunView {
            providers: providers
                .iter()
                .map(|catalog| Catalog::from_str(catalog).map_err(DbError::Message))
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
    let StoredLookup {
        catalog: _,
        answer,
        reason,
        failure,
        status,
        releases,
    } = stored;
    if answer != "answered" && !releases.is_empty() {
        return Err(DbError::Message(format!(
            "a {answer} lookup of {content_hash} lists releases it found"
        )));
    }
    match answer.as_str() {
        "not_asked" => Ok(LookupView::NotAsked {
            reason: match reason.as_deref() {
                Some("left_out") => NotAskedReason::LeftOut,
                Some("no_catalog") => NotAskedReason::NoCatalog,
                other => return Err(unreadable("not_asked_reason", other.unwrap_or("NULL"))),
            },
        }),
        "failed" => Ok(LookupView::Failed {
            failure: failure_of(failure, status)?.ok_or_else(|| {
                DbError::Message(format!(
                    "a failed lookup of {content_hash} states no failure"
                ))
            })?,
        }),
        "answered" => {
            let found = releases
                .into_iter()
                .map(|position| {
                    matches
                        .get(&position)
                        .map(|held| (*held).clone())
                        .ok_or_else(|| {
                            DbError::Message(format!(
                            "a lookup of {content_hash} found match {position}, which is not stored"
                        ))
                        })
                })
                .collect::<Result<Vec<_>, DbError>>()?;
            Ok(LookupView::answered(&found))
        }
        other => Err(unreadable("lookup answer", other)),
    }
}

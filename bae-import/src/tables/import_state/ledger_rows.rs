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

use super::cards_read::{groups_from, StoredCard};
use super::layout::{store_list_text, ListLayout, ListOwner, OwnersList};
use super::lookup_failure_columns::{failure_columns, failure_of};
use super::match_rows::MatchOwner;
use super::verdict_rows::{Owner, VerdictId};
use super::*;
use bae_catalogs::catalog::{Catalog, LookupCatalog};
use bae_catalogs::search::MetadataResult;
use bae_identify::release_group::{group_results, unranked};
use bae_identify::{
    BarcodeStepView, CatalogCandidateView, CatalogStepView, DiscIdRow, DiscIdStepView,
    IdentifyRunView, IsrcStepView, LookupView, NotAskedReason, ProviderCell, SearchStepView,
    SignalValueRow,
};
use bae_store::WriteStatements as _;
use std::collections::HashMap;
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
fn unsettled(owner: impl std::fmt::Display, what: &str) -> ! {
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

coven::write!(
    DeleteLookupValues(verdict: super::verdict_rows::VerdictId),
    r#"
    DELETE FROM import_candidate_lookup_value WHERE verdict_id = :verdict
"#
);

coven::write!(
    DeleteLookupProviders(verdict: super::verdict_rows::VerdictId),
    r#"
    DELETE FROM import_candidate_lookup_provider WHERE verdict_id = :verdict
"#
);

coven::write!(
    DeleteLookupIsrcs(verdict: super::verdict_rows::VerdictId),
    r#"
    DELETE FROM import_candidate_lookup_isrc WHERE verdict_id = :verdict
"#
);

coven::write!(
    DeleteOfferedCatalogs(verdict: super::verdict_rows::VerdictId),
    r#"
    DELETE FROM import_candidate_lookup_offered_catalog WHERE verdict_id = :verdict
"#
);

coven::write!(
    InsertLookupProvider(verdict: super::verdict_rows::VerdictId, position: i64, catalog: &'p str),
    r#"
    INSERT INTO import_candidate_lookup_provider
        (verdict_id, position, catalog)
    VALUES (:verdict, :position, :catalog)
"#
);

coven::write!(
    InsertLookupValue(
        verdict: super::verdict_rows::VerdictId,
        step: &'p str,
        position: i64,
        value: Option<&'p str>,
        excluded: bool,
    ),
    r#"
    INSERT INTO import_candidate_lookup_value
        (verdict_id, step, position, value, excluded)
    VALUES (:verdict, :step, :position, :value, :excluded)
"#
);

coven::write!(
    InsertOfferedCatalog(verdict: super::verdict_rows::VerdictId, position: i64, value: &'p str),
    r#"
    INSERT INTO import_candidate_lookup_offered_catalog
        (verdict_id, position, value)
    VALUES (:verdict, :position, :value)
"#
);

coven::write!(
    InsertLookupIsrc(verdict: super::verdict_rows::VerdictId, position: i64, isrc: &'p str),
    r#"
    INSERT INTO import_candidate_lookup_isrc
        (verdict_id, position, isrc)
    VALUES (:verdict, :position, :isrc)
"#
);

coven::write!(
    InsertLookup(
        verdict: super::verdict_rows::VerdictId,
        step: &'p str,
        position: i64,
        cell: i64,
        catalog: Option<&'p str>,
        answer: &'static str,
        not_asked_reason: Option<&'static str>,
        failure: Option<&'static str>,
        failure_status: Option<i64>,
        asked_by: Option<i64>,
    ),
    r#"
    INSERT INTO import_candidate_lookup
        (verdict_id, step, position, cell, catalog, answer, not_asked_reason, failure,
        failure_status, asked_by)
    VALUES (:verdict, :step, :position, :cell, :catalog, :answer, :not_asked_reason,
        :failure, :failure_status, :asked_by)
"#
);

coven::write!(
    InsertLookupRelease(
        verdict: super::verdict_rows::VerdictId,
        step: &'p str,
        position: i64,
        cell: i64,
        ordinal: i64,
        match_position: i64,
        pressing: u32,
        card: u32,
        record_order: u32,
        section: u32,
        row_order: u32,
    ),
    r#"
    INSERT INTO import_candidate_lookup_release
        (verdict_id, step, position, cell, ordinal, match_position, pressing, card,
        record_order, section, row_order)
    VALUES (:verdict, :step, :position, :cell, :ordinal, :match_position, :pressing, :card,
        :record_order, :section, :row_order)
"#
);

// Damage a test plants: a ledger's ISRC rows gone.
#[cfg(test)]
coven::write!(
    LoseIsrcRows(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_lookup_isrc
    WHERE verdict_id = (SELECT verdict_id FROM import_candidate_verdict
                      WHERE content_hash = :content_hash)
"#
);

// What a test rewrites the one not-asked cell of a candidate's ledger as.
#[cfg(test)]
coven::write!(
    RewriteNotAskedCell(
        answer: &'static str,
        not_asked_reason: Option<&'static str>,
        asked_by: Option<i64>,
        content_hash: &'p str,
    ),
    r#"
    UPDATE import_candidate_lookup
    SET answer = :answer, not_asked_reason = :not_asked_reason, asked_by = :asked_by
    WHERE answer = 'not_asked'
        AND verdict_id = (SELECT verdict_id FROM import_candidate_verdict
                      WHERE content_hash = :content_hash)
"#
);

/// Clear the ledger rows `verdict` holds; the lookups and the releases they
/// found go with their rows, and each cell's list with its cards.
pub(super) fn delete_ledger(sql: &CovenWrite<'_>, verdict: VerdictId) -> Result<(), DbError> {
    sql.run(&DeleteLookupValuesParams { verdict })?;
    sql.run(&DeleteLookupProvidersParams { verdict })?;
    sql.run(&DeleteLookupIsrcsParams { verdict })?;
    sql.run(&DeleteOfferedCatalogsParams { verdict })?;
    Ok(())
}

/// The match rows a ledger's releases are, by release: each one's position
/// and the copy the row holds.
pub(super) type MatchPositions<'a> = HashMap<(LookupCatalog, &'a str), (usize, &'a MetadataResult)>;

/// Write `ledger`'s rows under `verdict`, whose step columns are written and
/// whose previous ledger rows are gone. Each release a cell lists is linked to
/// the match row holding it, which must hold the cell's copy of it: the cell
/// is laid out again from that row. `asked_by`: the run in flight whose
/// ledger this is, whose cells may be queued or looking up; none for a ledger
/// that has settled.
pub(super) fn store_ledger(
    sql: &CovenWrite<'_>,
    verdict: VerdictId,
    ledger: &IdentifyRunView,
    matches: &MatchPositions<'_>,
    asked_by: Option<i64>,
) -> Result<(), DbError> {
    for (position, catalog) in ledger.providers.iter().enumerate() {
        sql.run(&InsertLookupProviderParams {
            verdict,
            position: column(position)?,
            catalog: catalog.as_str(),
        })?;
    }
    let row = |step: &str, position: usize, value: Option<&str>, excluded: bool| {
        sql.run(&InsertLookupValueParams {
            verdict,
            step,
            position: column(position)?,
            value,
            excluded,
        })?;
        Ok::<_, DbError>(())
    };
    let cell = |step: &str,
                position: usize,
                cell: usize,
                catalog: Option<Catalog>,
                lookup: &LookupView| {
        store_lookup(
            sql,
            verdict,
            LookupKey { step, position, cell },
            catalog,
            lookup,
            (matches, asked_by),
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
            sql.run(&InsertOfferedCatalogParams {
                verdict,
                position: column(position)?,
                value: &candidate.value,
            })?;
        }
    }
    if let IsrcStepView::Read { isrcs, lookup, .. } = &ledger.isrc {
        for (position, isrc) in isrcs.iter().enumerate() {
            sql.run(&InsertLookupIsrcParams { verdict, position: column(position)?, isrc })?;
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
/// or looking up only in a run's ledger in flight (`asked_by` names the run).
fn store_lookup(
    sql: &CovenWrite<'_>,
    owner: VerdictId,
    key: LookupKey<'_>,
    catalog: Option<Catalog>,
    lookup: &LookupView,
    (matches, asked_by): (&MatchPositions<'_>, Option<i64>),
) -> Result<(), DbError> {
    let LookupKey { step, position, cell } = key;
    let (answer, reason, failure) = match lookup {
        LookupView::Queued if asked_by.is_some() => ("queued", None, None),
        LookupView::LookingUp if asked_by.is_some() => ("looking_up", None, None),
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
    sql.run(&InsertLookupParams {
        verdict: owner,
        step,
        position: column(position)?,
        cell: column(cell)?,
        catalog: catalog.map(|catalog| catalog.as_str()),
        answer,
        not_asked_reason: reason,
        failure: failure.kind,
        failure_status: failure.status,
        // A queued cell or one under way names the run asking it.
        asked_by: asked_by.filter(|_| matches!(answer, "queued" | "looking_up")),
    })?;
    let releases = lookup.releases();
    if releases.is_empty() {
        return Ok(());
    }
    // `ListLayout` groups the cell's releases into rows and cards with the
    // same rules as any list, over the releases in the order the cell lists
    // them.
    let list: Vec<MetadataResult> = releases.iter().map(|release| (*release).clone()).collect();
    let layout = ListLayout::of(&list, &group_results(unranked(list.clone()), None), "answered");
    for ((ordinal, release), place) in releases.into_iter().enumerate().zip(&layout.places) {
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
        sql.run(&InsertLookupReleaseParams {
            verdict: owner,
            step,
            position: column(position)?,
            cell: column(cell)?,
            ordinal: column(ordinal)?,
            match_position: column(*match_position)?,
            pressing: place.pressing,
            card: place.card,
            record_order: place.record_order,
            section: place.section,
            row_order: place.row_order,
        })?;
    }
    store_list_text(
        sql,
        ListOwner {
            owner: MatchOwner::Verdict(owner),
            list: OwnersList::Cell { step, position: column(position)?, cell: column(cell)? },
        },
        &layout,
    )?;
    Ok(())
}

fn column(index: usize) -> Result<i64, DbError> {
    to_i64(index, "a ledger row's place in its list")
}

/// One verdict's [`ledger_columns`] as SQLite hands them over, its lists
/// still JSON.
pub(super) struct StoredLedger {
    disc_id: Option<String>,
    barcode: Option<String>,
    catalog: Option<String>,
    isrc: Option<String>,
    search: Option<String>,
    search_album: Option<String>,
    search_artist: Option<String>,
    /// The position of an unlisted match no cell found, which no write makes.
    pub(super) stray_unlisted: Option<i64>,
    providers: String,
    isrcs: String,
    isrc_shown: Option<String>,
    isrc_more_count: u32,
    offered_catalogs: String,
    offered_catalog_count: u32,
    rows: String,
}

/// The rows of each ledger step, as [`ledger_columns`] holds them: a list under
/// the name of each step that has rows.
#[derive(serde::Deserialize)]
struct StoredSteps {
    #[serde(default)]
    disc_id: Vec<StoredRow>,
    #[serde(default)]
    barcode: Vec<StoredRow>,
    #[serde(default)]
    catalog: Vec<StoredRow>,
    #[serde(default)]
    isrc: Vec<StoredRow>,
    #[serde(default)]
    search: Vec<StoredRow>,
}

/// One ledger row as [`ledger_columns`] holds it.
#[derive(serde::Deserialize)]
struct StoredRow {
    step: String,
    value: Option<String>,
    excluded: i64,
    cells: Vec<StoredLookup>,
}

/// One lookup as [`ledger_columns`] holds it.
#[derive(serde::Deserialize)]
struct StoredLookup {
    catalog: Option<String>,
    answer: String,
    reason: Option<String>,
    failure: Option<String>,
    status: Option<i64>,
    /// How many pressing rows the releases are.
    count: u32,
    cards: Vec<StoredCard>,
}

impl StoredLedger {
    /// The ledger a pane's read selects, as
    /// [`super::state_fragments::CurrentState`] names it.
    pub(super) fn of(state: &super::state_fragments::CurrentState) -> Self {
        Self {
            disc_id: state.ledger_disc_id_step.clone(),
            barcode: state.ledger_barcode_step.clone(),
            catalog: state.ledger_catalog_step.clone(),
            isrc: state.ledger_isrc_step.clone(),
            search: state.ledger_search_step.clone(),
            search_album: state.ledger_search_album.clone(),
            search_artist: state.ledger_search_artist.clone(),
            stray_unlisted: state.ledger_stray_unlisted,
            providers: state.ledger_providers.clone(),
            isrcs: state.ledger_isrcs.clone(),
            isrc_shown: state.ledger_isrc_shown.clone(),
            isrc_more_count: state.ledger_isrc_more_count,
            offered_catalogs: state.ledger_offered_catalogs.clone(),
            offered_catalog_count: state.ledger_offered_catalog_count,
            rows: state.ledger_rows.clone(),
        }
    }
}

impl StoredLedger {
    /// The ledger these rows record, its cells laying out the releases they
    /// found, `None` for a verdict that recorded none.
    pub(super) fn ledger(self, content_hash: &str) -> Result<Option<IdentifyRunView>, DbError> {
        let Self {
            disc_id,
            barcode,
            catalog,
            isrc,
            search,
            search_album,
            search_artist,
            stray_unlisted: _,
            providers,
            isrcs,
            isrc_shown,
            isrc_more_count,
            offered_catalogs,
            offered_catalog_count,
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
        let StoredSteps {
            disc_id: disc_rows,
            barcode: barcode_rows,
            catalog: catalog_rows,
            isrc: isrc_rows,
            search: search_rows,
        } = serde_json::from_str(&rows).map_err(|error| {
            DbError::unreadable_json(
                format!("the stored ledger rows of {content_hash}"),
                &rows,
                error,
            )
        })?;
        let (Some(disc_id), Some(barcode), Some(catalog), Some(isrc), Some(search)) =
            (disc_id, barcode, catalog, isrc, search)
        else {
            if !providers.is_empty()
                || !isrcs.is_empty()
                || !offered.is_empty()
                || ![&disc_rows, &barcode_rows, &catalog_rows, &isrc_rows, &search_rows]
                    .iter()
                    .all(|rows| rows.is_empty())
            {
                return Err(DbError::unreadable(
                    format!("the ledger of {content_hash}, whose verdict records none"),
                    (&providers, &isrcs, &offered),
                ));
            }
            return Ok(None);
        };
        let lookup = |stored: StoredLookup| lookup_of(stored);
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
        let catalog = match catalog.as_str() {
            stood @ ("numbers" | "scanning") => CatalogStepView::Numbers {
                scanning: stood == "scanning",
                rows: value_rows(catalog_rows)?,
                candidates: offered
                    .into_iter()
                    .map(|value| CatalogCandidateView { value })
                    .collect(),
                candidate_count: offered_catalog_count,
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
                let Some(shown) = isrc_shown else {
                    return Err(DbError::unreadable(
                        format!("the ISRCs of {content_hash}, which read ISRCs and hold none"),
                        &isrcs,
                    ));
                };
                IsrcStepView::Read {
                    shown,
                    more_count: isrc_more_count,
                    isrcs,
                    lookup: one_lookup(row, "ISRC")?,
                }
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
            ("not_needed", None, None) => {
                stray(&search_rows, STEP_SEARCH, "not_needed")?;
                SearchStepView::NotNeeded
            }
            (other, _, _) => {
                return Err(DbError::unreadable("import_candidate_verdict.search_step", other))
            }
        };
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
        Ok(Some(ledger))
    }
}

/// One stored lookup as its cell: why nobody was asked, how it failed, or
/// the releases it found laid out as the run laid them out.
fn lookup_of(stored: StoredLookup) -> Result<LookupView, DbError> {
    let StoredLookup { catalog: _, answer, reason, failure, status, count, cards } = stored;
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
                DbError::unreadable("the failure of a failed lookup", None::<String>)
            })?,
        }),
        "no_match" => Ok(LookupView::NoMatch),
        "answered" => Ok(LookupView::Found { count, groups: groups_from(cards)? }),
        other => Err(DbError::unreadable("a stored lookup answer", other)),
    }
}

#[cfg(test)]
#[path = "ledger_rows_tests.rs"]
mod tests;

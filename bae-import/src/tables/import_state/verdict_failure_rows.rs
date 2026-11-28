//! This module stores a failed verdict's lookup failures as rows of their own
//! (`import_candidate_verdict_failure`, in `verdict_rows.sql`), and picks out
//! the ones that no cell of the verdict's ledger shows, so that a screen can
//! show each of them on its own line.

use super::lookup_failure_columns::{failure_of, failure_stated};
use super::verdict_rows::VerdictId;
use super::*;
use bae_catalogs::search::SourceFailure;
use bae_identify::IdentifyFailure;
use bae_store::WriteStatements as _;

coven::write!(
    DeleteVerdictFailures(verdict: super::verdict_rows::VerdictId),
    r#"
    DELETE FROM import_candidate_verdict_failure WHERE verdict_id = :verdict
"#
);

coven::write!(
    InsertVerdictFailure(
        verdict: super::verdict_rows::VerdictId,
        position: i64,
        lookup: &'static str,
        catalog: Option<&'static str>,
        failure: &'static str,
        failure_status: Option<i64>,
    ),
    r#"
    INSERT INTO import_candidate_verdict_failure
        (verdict_id, position, lookup, catalog, failure, failure_status)
    VALUES (:verdict, :position, :lookup, :catalog, :failure, :failure_status)
"#
);

/// Write `failures` as `verdict`'s, over the failures it stored before.
pub(super) fn store_failures(
    sql: &WriteContext<'_>,
    verdict: VerdictId,
    failures: &[IdentifyFailure],
) -> Result<(), DbError> {
    sql.run(&DeleteVerdictFailuresParams { verdict })?;
    for (position, failure) in failures.iter().enumerate() {
        let (lookup, catalog, failed) = match failure {
            IdentifyFailure::DiscId(failed) => ("disc_id", None, failed),
            IdentifyFailure::Isrc(failed) => ("isrc", None, failed),
            IdentifyFailure::Barcode(failed) => ("barcode", Some(failed.source), &failed.failure),
            IdentifyFailure::Catalog(failed) => ("catalog", Some(failed.source), &failed.failure),
            IdentifyFailure::Search(failed) => ("search", Some(failed.source), &failed.failure),
            IdentifyFailure::Pressing(failed) => ("pressing", Some(failed.source), &failed.failure),
            IdentifyFailure::Cover(failed) => ("cover", Some(failed.source), &failed.failure),
        };
        let (kind, status) = failure_stated(failed);
        sql.run(&InsertVerdictFailureParams {
            verdict,
            position: to_i64(position, "a verdict's failure position")?,
            lookup,
            catalog: catalog.map(|catalog| catalog.as_str()),
            failure: kind,
            failure_status: status,
        })?;
    }
    Ok(())
}

/// What a [`failure_lines_sql`] column holds.
#[derive(serde::Deserialize)]
pub(super) struct StoredFailureLines {
    all: Vec<StoredFailure>,
    pick_failures: Option<Vec<StoredFailure>>,
    missing_searches: Vec<StoredMissingSearch>,
}

/// One lookup of [`StoredFailureLines::missing_searches`].
#[derive(serde::Deserialize)]
struct StoredMissingSearch {
    lookup: String,
    catalog: String,
}

/// The failures of one block of a [`failure_lines_sql`] column.
fn failures_of_block(
    block: Vec<StoredFailure>,
) -> Result<Vec<bae_identify::IdentifyFailure>, DbError> {
    block.into_iter().map(StoredFailure::failure).collect()
}

/// The failure lines a [`failure_lines_sql`] column holds.
pub(super) fn failure_lines_of(
    stored: StoredFailureLines,
) -> Result<bae_identify::FailureLines, DbError> {
    use bae_identify::LookupStep;
    let StoredFailureLines { all, pick_failures, missing_searches } = stored;
    Ok(bae_identify::FailureLines {
        all: failures_of_block(all)?,
        pick_failures: pick_failures.map(failures_of_block).transpose()?,
        missing_searches: missing_searches
            .into_iter()
            .map(|StoredMissingSearch { lookup, catalog }| {
                Ok(bae_identify::MissingSearch {
                    step: match lookup.as_str() {
                        "disc_id" => LookupStep::DiscId,
                        "barcode" => LookupStep::Barcode,
                        "catalog" => LookupStep::Catalog,
                        "isrc" => LookupStep::Isrc,
                        "search" => LookupStep::TitleSearch,
                        "pressing" => LookupStep::Pressing,
                        other => {
                            return Err(DbError::unreadable("a missing search's lookup", other))
                        }
                    },
                    catalog: catalog.parse().map_err(|_unknown: String| {
                        DbError::unreadable("a missing search's catalog", &catalog)
                    })?,
                })
            })
            .collect::<Result<_, DbError>>()?,
    })
}

/// One failure as a [`verdict_failures_sql`] or [`failure_lines_sql`] column
/// holds it.
#[derive(serde::Deserialize)]
pub(super) struct StoredFailure {
    lookup: String,
    catalog: Option<String>,
    failure: String,
    status: Option<i64>,
}

/// The failures a [`verdict_failures_sql`] or [`failure_lines_sql`] column
/// holds.
pub(super) fn failures_of(json: &str) -> Result<Vec<IdentifyFailure>, DbError> {
    let rows: Vec<StoredFailure> = serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json("a verdict's failures", json, error))?;
    rows.into_iter().map(StoredFailure::failure).collect()
}

impl StoredFailure {
    pub(super) fn failure(self) -> Result<IdentifyFailure, DbError> {
        let Self { lookup, catalog, failure, status } = self;
        let failed =
            failure_of(Some(failure), status)?.expect("a stored failure names how it failed");
        let source = || -> Result<SourceFailure, DbError> {
            let catalog = catalog.as_deref().ok_or_else(|| {
                DbError::unreadable(
                    format!("the catalog of a verdict's {lookup} failure"),
                    None::<String>,
                )
            })?;
            Ok(SourceFailure {
                source: catalog.parse().map_err(|_unknown: String| {
                    DbError::unreadable("a verdict failure's catalog", catalog)
                })?,
                failure: failed.clone(),
            })
        };
        Ok(match lookup.as_str() {
            "disc_id" => IdentifyFailure::DiscId(failed.clone()),
            "isrc" => IdentifyFailure::Isrc(failed.clone()),
            "barcode" => IdentifyFailure::Barcode(source()?),
            "catalog" => IdentifyFailure::Catalog(source()?),
            "search" => IdentifyFailure::Search(source()?),
            "pressing" => IdentifyFailure::Pressing(source()?),
            "cover" => IdentifyFailure::Cover(source()?),
            other => {
                return Err(DbError::unreadable("import_candidate_verdict_failure.lookup", other))
            }
        })
    }
}

//! This module stores a failed verdict's lookup failures as rows of their own
//! (`import_candidate_verdict_failure`, in `verdict_rows.sql`), and picks out
//! the ones that no cell of the verdict's ledger shows, so that a screen can
//! show each of them on its own line.

use super::lookup_failure_columns::{failure_columns, failure_of};
use super::verdict_rows::Owner;
use super::*;
use bae_catalogs::search::SourceFailure;
use bae_identify::IdentifyFailure;

/// Write `failures` as `owner`'s, over the failures it stored before.
pub(super) fn store_failures(
    sql: &CovenWrite<'_>,
    owner: Owner<'_>,
    failures: &[IdentifyFailure],
) -> Result<(), DbError> {
    sql.execute(
        &format!("DELETE FROM import_candidate_verdict_failure WHERE {} = ?", owner.column()),
        params![owner],
    )?;
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
        let columns = failure_columns(Some(failed));
        sql.execute(
            "INSERT INTO import_candidate_verdict_failure \
                 (content_hash, identify_run, position, lookup, catalog, failure, failure_status) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                owner.content_hash(),
                owner.run(),
                to_i64(position, "a verdict's failure position")?,
                lookup,
                catalog.map(|catalog| catalog.as_str()),
                columns.kind,
                columns.status,
            ],
        )?;
    }
    Ok(())
}

/// The failures of the verdict joined as `verdict` that `keep` keeps (an SQL
/// condition on `failure`, `TRUE` for every one), in the order the run met
/// them, as a JSON array that [`failures_of`] reads.
fn failures_json_sql(verdict: &str, keep: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object('lookup', failure.lookup, \
                                              'catalog', failure.catalog, \
                                              'failure', failure.failure, \
                                              'status', failure.failure_status) \
                                  ORDER BY failure.position) \
          FROM import_candidate_verdict_failure AS failure \
          WHERE failure.content_hash IS {verdict}.content_hash \
            AND failure.identify_run IS {verdict}.identify_run AND ({keep}))"
    )
}

/// Every failure of the verdict joined as `verdict`, as a JSON array that
/// [`failures_of`] reads.
pub(super) fn verdict_failures_sql(verdict: &str) -> String {
    failures_json_sql(verdict, "TRUE")
}

/// Builds the SQL for the failures of the verdict joined as `verdict` that no
/// cell of its ledger shows: no lookup of its ledger asked the same clue of
/// the same catalog and failed the same way. The ledger lays out no lookup of
/// an offered row's pressing or of the picked release's cover, so this query
/// always returns their failures, and the screen shows them on their own
/// lines. The query returns a JSON array that [`failures_of`] reads.
pub(super) fn failure_lines_sql(verdict: &str) -> String {
    failures_json_sql(
        verdict,
        "NOT EXISTS (SELECT 1 FROM import_candidate_lookup AS cell \
                     WHERE cell.content_hash IS failure.content_hash \
                       AND cell.identify_run IS failure.identify_run \
                       AND cell.step = failure.lookup AND cell.catalog IS failure.catalog \
                       AND cell.answer = 'failed' AND cell.failure = failure.failure \
                       AND cell.failure_status IS failure.failure_status)",
    )
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

//! The Storage Manager's count and total size under each filter with no
//! search: the `storage_totals` table, which triggers keep from the releases'
//! storage rows, and its reads.

use super::*;
use bae_store::statement_row;

// How many releases the library holds and their files' total size, from the
// stored totals' two rows.
sqlive_compose::read!(
    writer AllStorageTotals { release_count: Option<u64>, total_size: Option<u64> },
    r#"
    SELECT SUM(held.release_count) AS release_count, SUM(held.total_size) AS total_size
    FROM storage_totals AS held
"#
);

// How many releases hold the `stored` value `stored` and their files' total
// size, from its stored totals.
sqlive_compose::read!(
    writer KeptStorageTotals(stored: i64) { release_count: u64, total_size: u64 },
    r#"
    SELECT held.release_count, held.total_size
    FROM storage_totals AS held
    WHERE held.stored = :stored
"#
);

/// How many releases hold the `stored` value `stored` (every release with
/// `None`) and their files' total size, as the stored totals hold them.
pub(crate) fn storage_totals_on(
    sql: &SqlReadContext<'_>,
    stored: Option<i64>,
) -> Result<(u64, u64), DbError> {
    match stored {
        Some(stored) => {
            let row = statement_row(sql, &KeptStorageTotalsParams { stored })?;
            Ok((row.release_count, row.total_size))
        }
        None => {
            let row = statement_row(sql, &AllStorageTotalsParams {})?;
            let missing = || DbError::unreadable("the stored totals", None::<String>);
            Ok((row.release_count.ok_or_else(missing)?, row.total_size.ok_or_else(missing)?))
        }
    }
}

#[cfg(test)]
#[path = "storage_totals_tests.rs"]
mod tests;

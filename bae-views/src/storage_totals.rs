//! The Storage Manager's count and total size under each filter with no
//! search: the `storage_totals` table, which triggers keep from the releases'
//! storage rows, and its reads.

use super::*;

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

/// The count and total size a stored totals read returns; `None` where the
/// stored totals hold no row to sum.
pub(crate) type StoredTotals = Option<(u64, u64)>;

/// The read of how many releases hold the `stored` value `stored` (every
/// release with `None`) and their files' total size, as the stored totals
/// hold them.
pub(crate) fn storage_totals_read(stored: Option<i64>) -> coven::ReadAs<StoredTotals> {
    match stored {
        Some(stored) => coven::ReadAs::new(KeptStorageTotalsParams { stored }, |row| {
            Some((row.release_count, row.total_size))
        }),
        None => coven::ReadAs::new(AllStorageTotalsParams {}, |row| {
            row.release_count.zip(row.total_size)
        }),
    }
}

/// The count and total size from the one row a stored totals read returned.
pub(crate) fn storage_totals_of(rows: Vec<StoredTotals>) -> Result<(u64, u64), DbError> {
    bae_store::exactly_one::<coven::ReadAs<StoredTotals>, _>(rows)?
        .ok_or_else(|| DbError::unreadable("the stored totals", None::<String>))
}

#[cfg(test)]
#[path = "storage_totals_tests.rs"]
mod tests;

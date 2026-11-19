//! The Storage Manager's count and total size under each filter with no
//! search: the `storage_totals` table, which triggers keep from the releases'
//! storage rows, and its read.

use super::*;

/// The SQL creating the totals' table and triggers.
pub(super) const SQL: &str = include_str!("storage_totals.sql");

/// The statement reading how many releases hold the `stored` value `stored`
/// (both values with `None`) and their files' total size, from the stored
/// totals.
pub(crate) fn storage_totals_query(stored: Option<u8>) -> String {
    format!(
        "SELECT SUM(release_count) AS release_count, SUM(total_size) AS total_size \
         FROM storage_totals WHERE {kept}",
        kept = stored.map_or_else(|| "1".to_string(), |stored| format!("stored = {stored}")),
    )
}

/// How many releases hold the `stored` value `stored` (every release with
/// `None`) and their files' total size, as the stored totals hold them.
pub(crate) fn storage_totals_on(
    sql: &SqlReadContext<'_>,
    stored: Option<u8>,
) -> Result<(u64, u64), DbError> {
    Ok(sql.query_row(&storage_totals_query(stored), [], |row| {
        Ok((
            bae_store::read::count_column(row, "release_count")?,
            bae_store::read::count_column(row, "total_size")?,
        ))
    })?)
}

#[cfg(test)]
#[path = "storage_totals_tests.rs"]
mod tests;

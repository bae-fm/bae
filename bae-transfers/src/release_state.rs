//! What the download queue says of one release, as its detail shows it
//! beside the library's rows.

use crate::download_entry::{download_entry_column, download_entry_sql};
use crate::DownloadEntry;
use bae_store::DbError;
use coven::SqlReadContext;

/// `release_id`'s entry in this device's download queue now, and what holds
/// the queue back; `None` when it has none.
pub fn release_download_entry_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<DownloadEntry>, DbError> {
    Ok(sql.query_row(
        &format!("SELECT {}", download_entry_sql("?1")),
        [release_id],
        download_entry_column,
    )?)
}

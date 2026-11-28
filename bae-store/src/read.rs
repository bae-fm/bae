//! Reading a column of a row into a typed value, naming the column when its
//! stored value is not one.

use rusqlite::Row;
use time::UtcDateTime;

/// The error of reading column `column` (its name or index) of `row`, whose
/// stored value the mapper could not turn into its typed value, `cause`
/// saying why: it names the column's index and the type its value is stored
/// as, so a corrupt column surfaces like any other bad read instead of
/// panicking or silently mis-defaulting.
pub fn column_conversion_error<I: rusqlite::RowIndex>(
    row: &Row,
    column: I,
    cause: impl Into<Box<dyn std::error::Error + Send + Sync>>,
) -> rusqlite::Error {
    // The column was just read, so its index resolves; if it somehow doesn't,
    // that lookup error is itself a faithful failure to return.
    let index = match column.idx(row.as_ref()) {
        Ok(index) => index,
        Err(error) => return error,
    };
    match row.get_ref(index) {
        Ok(value) => {
            rusqlite::Error::FromSqlConversionFailure(index, value.data_type(), cause.into())
        }
        Err(error) => error,
    }
}

/// Read a named rfc3339 timestamp column, surfacing a malformed value as a
/// column-conversion error rather than panicking on the parse.
pub fn rfc3339_column(row: &Row, column: &str) -> rusqlite::Result<UtcDateTime> {
    let raw: String = row.get(column)?;
    bae_util::timestamp::parse_rfc3339(&raw).map_err(|e| {
        column_conversion_error(
            row,
            column,
            format!("{column} {raw:?} is not a valid rfc3339 timestamp: {e}"),
        )
    })
}

//! Reading a column of a row into a typed value, naming the column when its
//! stored value is not one.

use chrono::{DateTime, Utc};
use rusqlite::Row;

/// A read of the device's sort language from the library an ordered list is
/// read in. A live list takes it where it is subscribed, so reading the
/// language is part of the list's own read and a change of language reruns it.
pub type SortLanguageRead =
    fn(&coven::SqlReadContext<'_>) -> Result<bae_util::name_key::SortLanguage, crate::DbError>;

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

/// Read column `column` (its name or index) of `row` as the JSON text of a
/// `T`, surfacing text that isn't one as a column-conversion error.
pub fn json_column<T, I>(row: &Row, column: I) -> rusqlite::Result<T>
where
    T: serde::de::DeserializeOwned,
    I: rusqlite::RowIndex + Copy,
{
    let text: String = row.get(column)?;
    serde_json::from_str(&text)
        .map_err(|error| column_conversion_error(row, column, format!("{text:?}: {error}")))
}

/// Read a named rfc3339 timestamp column, surfacing a malformed value as a
/// column-conversion error rather than panicking on the parse.
pub fn rfc3339_column(row: &Row, column: &str) -> rusqlite::Result<DateTime<Utc>> {
    let raw: String = row.get(column)?;
    DateTime::parse_from_rfc3339(&raw).map(|dt| dt.with_timezone(&Utc)).map_err(|e| {
        column_conversion_error(
            row,
            column,
            format!("{column} {raw:?} is not a valid rfc3339 timestamp: {e}"),
        )
    })
}

/// Read a named integer column that holds a count, a size or a position,
/// which the schema keeps non-negative, as the unsigned type `T` it is
/// counted in, surfacing a negative one, or one past `T`, as a
/// column-conversion error.
pub fn count_column<T>(row: &Row, column: &str) -> rusqlite::Result<T>
where
    T: TryFrom<i64, Error = std::num::TryFromIntError>,
{
    let raw: i64 = row.get(column)?;
    T::try_from(raw).map_err(|error| {
        column_conversion_error(row, column, format!("{column} {raw} is not a count: {error}"))
    })
}

/// [`count_column`] for a nullable column.
pub fn optional_count_column<T>(row: &Row, column: &str) -> rusqlite::Result<Option<T>>
where
    T: TryFrom<i64, Error = std::num::TryFromIntError>,
{
    match row.get::<_, Option<i64>>(column)? {
        None => Ok(None),
        Some(_) => count_column(row, column).map(Some),
    }
}

/// Read a named text column back into the type whose `FromStr` it was written
/// from, surfacing a word no variant answers to as a column-conversion error.
pub fn parsed_column<T>(row: &Row, column: &str) -> rusqlite::Result<T>
where
    T: std::str::FromStr<Err = String>,
{
    let raw: String = row.get(column)?;
    raw.parse::<T>().map_err(|e| column_conversion_error(row, column, e))
}

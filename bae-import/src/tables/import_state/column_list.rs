//! Named columns read from a statement's row or from one JSON object, so that
//! one reader decodes a typed search either way.

use rusqlite::types::{FromSql, Value, ValueRef};
use serde_json::Value as Json;

/// Where a reader takes its named columns from: a statement's row, or one
/// JSON object holding them.
pub(super) trait Columns {
    fn get<T: FromSql>(&self, name: &str) -> rusqlite::Result<T>;
}

impl Columns for rusqlite::Row<'_> {
    fn get<T: FromSql>(&self, name: &str) -> rusqlite::Result<T> {
        rusqlite::Row::get(self, name)
    }
}

/// The columns of one JSON object that a statement selected. `get` returns a
/// list inside the object as its JSON text, as a row's column does.
#[derive(serde::Deserialize)]
#[serde(transparent)]
pub(super) struct JsonColumns(serde_json::Map<String, Json>);

impl Columns for JsonColumns {
    fn get<T: FromSql>(&self, name: &str) -> rusqlite::Result<T> {
        let json =
            self.0.get(name).ok_or_else(|| rusqlite::Error::InvalidColumnName(name.to_string()))?;
        let value = match json {
            Json::Null => Value::Null,
            Json::Bool(flag) => Value::Integer(i64::from(*flag)),
            Json::Number(number) => match (number.as_i64(), number.as_f64()) {
                (Some(integer), _) => Value::Integer(integer),
                (None, Some(real)) => Value::Real(real),
                (None, None) => {
                    return Err(rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Real,
                        format!("column {name} holds {number}, which no SQLite number holds")
                            .into(),
                    ))
                }
            },
            Json::String(text) => Value::Text(text.clone()),
            list @ (Json::Array(_) | Json::Object(_)) => Value::Text(list.to_string()),
        };
        T::column_result(ValueRef::from(&value)).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(0, value.data_type(), Box::new(error))
        })
    }
}

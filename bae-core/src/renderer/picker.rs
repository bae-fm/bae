//! The receivers the picker lists, as the library's device-local
//! `cast_device` table holds them: what discovery last found, written by
//! bae-cast in one write per change. The table's SQL is here, as functions
//! over the library's store.

use crate::db::{Database, DbError};
use coven::{CovenError, SqlReadContext};
use rusqlite::{params, Row};

/// Which flavor of remote renderer a device is. Shown as a tag; the picker
/// does not segregate the list by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererKind {
    Cast,
    Dlna,
    /// An AirPlay receiver (RAOP or AirPlay 2). Unlike Cast/DLNA it does not
    /// fetch a URL — bae pushes decoded audio to it — but it shares the one
    /// device list.
    AirPlay,
}

/// One receiver the picker lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastDevice {
    /// The id casting to it is asked by.
    pub id: String,
    /// The name the picker shows.
    pub name: String,
    pub kind: RendererKind,
}

fn kind_column(kind: RendererKind) -> &'static str {
    match kind {
        RendererKind::Cast => "cast",
        RendererKind::Dlna => "dlna",
        RendererKind::AirPlay => "airplay",
    }
}

fn kind_from_column(row: &Row<'_>, column: usize) -> rusqlite::Result<RendererKind> {
    match row.get::<_, String>(column)?.as_str() {
        "cast" => Ok(RendererKind::Cast),
        "dlna" => Ok(RendererKind::Dlna),
        "airplay" => Ok(RendererKind::AirPlay),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            format!("cast device kind {other} names no renderer").into(),
        )),
    }
}

/// Every receiver listed, by name, then id: the picker's order.
fn cast_devices_on(sql: &SqlReadContext<'_>) -> Result<Vec<CastDevice>, DbError> {
    Ok(sql.query("SELECT id, name, kind FROM cast_device ORDER BY name, id", [], |row| {
        Ok(CastDevice { id: row.get(0)?, name: row.get(1)?, kind: kind_from_column(row, 2)? })
    })?)
}

/// List `devices` in place of every receiver listed, in one write.
pub(crate) async fn replace_cast_devices(
    database: &Database,
    devices: Vec<CastDevice>,
) -> Result<(), DbError> {
    database
        .call(move |sql| {
            sql.execute("DELETE FROM cast_device", [])?;
            for device in &devices {
                sql.execute(
                    "INSERT INTO cast_device (id, name, kind) VALUES (?1, ?2, ?3)",
                    params![device.id, device.name, kind_column(device.kind)],
                )?;
            }
            Ok(())
        })
        .await
}

/// Follow the receivers the picker lists.
pub(crate) fn subscribe_cast_devices(database: &Database) -> coven::LiveQuery<Vec<CastDevice>> {
    database.subscribe(|sql| cast_devices_on(&sql).map_err(CovenError::from))
}

/// Empty the picker's list: what the library opens with, since no
/// discovery outlives the process that ran it.
pub(crate) async fn clear_cast_devices(database: &Database) -> Result<(), DbError> {
    database
        .call(|sql| {
            sql.execute("DELETE FROM cast_device", [])?;
            Ok(())
        })
        .await
}

#[cfg(test)]
#[path = "picker_tests.rs"]
mod tests;

//! The receivers the picker lists, as the library's device-local
//! `cast_device` table holds them: what discovery last found, written by
//! bae-cast in one write per change. The Casting settings page lists the same
//! receivers. Its table is `picker.sql`; the functions here read and write it.

/// The table's part of the library's schema.
pub(crate) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("picker.sql")]);

use bae_store::DbError;
use bae_store::Store;
use coven::SqlReadContext;
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
        other => Err(bae_store::read::column_conversion_error(
            row,
            column,
            format!("cast device kind {other} names no renderer"),
        )),
    }
}

/// Every receiver listed, by name, then id: the picker's order.
fn cast_devices_on(sql: &SqlReadContext<'_>) -> Result<Vec<CastDevice>, DbError> {
    Ok(sql.query("SELECT id, name, kind FROM cast_device ORDER BY name, id", [], |row| {
        Ok(CastDevice { id: row.get(0)?, name: row.get(1)?, kind: kind_from_column(row, 2)? })
    })?)
}

/// The picker's list in the library's store: the receivers discovery last
/// found, which the picker shows.
#[derive(Clone)]
pub struct CastPicker {
    store: Store,
}

impl CastPicker {
    /// The picker's list over the library's store, `store`.
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// List `devices` in place of every receiver listed, in one write.
    pub async fn show(&self, devices: Vec<CastDevice>) -> Result<(), DbError> {
        self.store
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
    pub fn subscribe(&self) -> coven::LiveQuery<Vec<CastDevice>, bae_store::DbError> {
        self.store.subscribe(|sql| cast_devices_on(&sql))
    }

    /// Empty the list: what the library opens with, since no discovery
    /// outlives the process that ran it.
    pub async fn clear(&self) -> Result<(), DbError> {
        self.store
            .call(|sql| {
                sql.execute("DELETE FROM cast_device", [])?;
                Ok(())
            })
            .await
    }

    /// The receivers the picker lists now.
    #[cfg(test)]
    async fn devices(&self) -> Result<Vec<CastDevice>, DbError> {
        self.store.read(|sql| cast_devices_on(&sql)).await
    }
}

#[cfg(test)]
#[path = "picker_tests.rs"]
mod tests;

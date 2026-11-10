//! The receivers the picker lists: the library's device-local `cast_device`
//! table, what discovery last found, written by bae-cast in one write per
//! change, after the receiver playback is cast to, which comes first and is
//! listed while a new search hasn't found it again. The Casting settings page
//! lists the same receivers. Its table is `picker.sql`; the functions here
//! read and write it.

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

/// One receiver discovery found, as it lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastDevice {
    /// The id casting to it is asked by.
    pub id: String,
    /// The name the picker shows.
    pub name: String,
    pub kind: RendererKind,
}

/// A receiver as the picker and the Casting page list it: what discovery
/// found, and the one playback is cast to, which comes first and is listed
/// while a new search hasn't found it again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastReceiver {
    /// The id casting to it is asked by.
    pub id: String,
    /// The name the picker shows.
    pub name: String,
    pub kind: RendererKind,
    /// Whether playback is cast to it.
    pub casting: bool,
}

/// `kind` as a `kind` column holds it.
pub(crate) fn kind_column(kind: RendererKind) -> &'static str {
    match kind {
        RendererKind::Cast => "cast",
        RendererKind::Dlna => "dlna",
        RendererKind::AirPlay => "airplay",
    }
}

/// The kind a `kind` column holds.
pub(crate) fn kind_from_column<I: rusqlite::RowIndex + Copy>(
    row: &Row<'_>,
    column: I,
) -> rusqlite::Result<RendererKind> {
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

/// The picker's list, in its order: the receiver playback is cast to, then
/// every other receiver discovery found, by name, then id.
fn cast_receivers_on(sql: &SqlReadContext<'_>) -> Result<Vec<CastReceiver>, DbError> {
    Ok(sql.query(
        "SELECT remote_device_id AS id, remote_device_name AS name, \
                remote_device_kind AS kind, 1 AS casting \
           FROM playback_now WHERE remote = 'on' \
         UNION ALL \
         SELECT id, name, kind, 0 FROM cast_device \
           WHERE id NOT IN (SELECT remote_device_id FROM playback_now WHERE remote = 'on') \
         ORDER BY casting DESC, name, id",
        [],
        |row| {
            Ok(CastReceiver {
                id: row.get("id")?,
                name: row.get("name")?,
                kind: kind_from_column(row, "kind")?,
                casting: row.get("casting")?,
            })
        },
    )?)
}

/// The picker's list in the library's store: the receivers discovery last
/// found, after the one playback is cast to.
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

    /// Follow the receivers the picker lists, the one playback is cast to
    /// first.
    pub fn subscribe(&self) -> coven::LiveQuery<Vec<CastReceiver>, bae_store::DbError> {
        self.store.subscribe(|sql| cast_receivers_on(&sql))
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
    async fn receivers(&self) -> Result<Vec<CastReceiver>, DbError> {
        self.store.read(|sql| cast_receivers_on(&sql)).await
    }
}

#[cfg(test)]
#[path = "picker_tests.rs"]
mod tests;

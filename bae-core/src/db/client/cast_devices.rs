//! The receivers the cast picker lists, as `cast_device` holds them.

use super::*;
use crate::renderer::{CastDevice, RendererKind};

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
    Ok(sql.query(
        "SELECT id, name, kind FROM cast_device ORDER BY name, id",
        [],
        |row| {
            Ok(CastDevice {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: kind_from_column(row, 2)?,
            })
        },
    )?)
}

impl Database {
    /// List `devices` in place of every receiver listed, in one write.
    pub(crate) async fn replace_cast_devices(
        &self,
        devices: Vec<CastDevice>,
    ) -> Result<(), DbError> {
        self.call(move |sql| {
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
    pub(crate) fn subscribe_cast_devices(&self) -> coven::LiveQuery<Vec<CastDevice>> {
        self.inner
            .handle
            .subscribe(|sql| cast_devices_on(&sql).map_err(CovenError::from))
    }

    /// The receivers the picker lists now.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn cast_devices(&self) -> Result<Vec<CastDevice>, DbError> {
        self.read(|sql| cast_devices_on(&sql)).await
    }

    /// Empty the picker's list: what the library opens with, since no
    /// discovery outlives the process that ran it.
    pub(crate) async fn clear_cast_devices(&self) -> Result<(), DbError> {
        self.call(|sql| {
            sql.execute("DELETE FROM cast_device", [])?;
            Ok(())
        })
        .await
    }
}

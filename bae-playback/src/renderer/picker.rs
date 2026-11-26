//! The receivers the picker lists: the library's device-local `cast_receiver`
//! table, what discovery last found, written by bae-cast in one write per
//! change, after the receiver playback is cast to, which comes first and is
//! listed while a new search hasn't found it again. The Casting settings page
//! lists the same receivers. Its table is `picker.sql`; the functions here
//! read and write it.

use bae_records::records::name_sort::sort_key_sql;
use bae_store::DbError;
use bae_store::Store;
use bae_store::{statement_optional, statement_row};
use coven::SqlReadContext;
use rusqlite::params;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};

use super::receiver::{CastReceiver, RendererKind};

/// What the Play on picker and the Playback settings list: the receiver playback is cast
/// to, listed while a new search hasn't found it again, and every other
/// receiver discovery found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastReceiverList {
    /// The receiver playback is cast to; `None` while it plays elsewhere.
    pub casting: Option<CastReceiver>,
    /// The other receivers discovery found, by name, then id.
    pub found: Vec<CastReceiver>,
    /// Whether the list holds no receiver, cast to or found. The apps show a
    /// spinner while it does.
    pub is_empty: bool,
}

/// `kind` as a `kind` column holds it.
pub(crate) fn kind_column(kind: RendererKind) -> &'static str {
    match kind {
        RendererKind::Cast => "cast",
        RendererKind::Dlna => "dlna",
        RendererKind::AirPlay => "airplay",
    }
}

impl FromSql for RendererKind {
    /// The kind a `kind` column holds.
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "cast" => Ok(Self::Cast),
            "dlna" => Ok(Self::Dlna),
            "airplay" => Ok(Self::AirPlay),
            other => Err(FromSqlError::Other(
                format!("cast device kind {other} names no renderer").into(),
            )),
        }
    }
}

sqlive_compose::read!(
    CastingNow,
    r#"
    SELECT casting.*
    FROM @crate::tables::now::Casting AS casting
"#
);

sqlive_compose::read!(
    PickerIsEmpty { is_empty: bool },
    r#"
    SELECT NOT EXISTS (SELECT 1 FROM cast_receiver)
           AND NOT EXISTS (SELECT 1 FROM @crate::tables::now::Casting AS casting) AS is_empty
"#
);

/// The picker's list: the receiver playback is cast to, then every other
/// receiver discovery found, by name in the device's language.
fn cast_receivers_on(sql: &SqlReadContext<'_>) -> Result<CastReceiverList, DbError> {
    let casting = statement_optional(sql, &CastingNowParams {})?
        .map(|row| row.casting.into_receiver())
        .transpose()?;
    let found = sql.query(
        &format!(
            "SELECT id, name, kind, model, address, takes_volume FROM cast_receiver \
               WHERE id NOT IN (SELECT cast_receiver_id FROM playback_now WHERE remote = 'on') \
             ORDER BY {}, id",
            sort_key_sql("name")
        ),
        [],
        |row| {
            Ok(CastReceiver {
                id: row.get("id")?,
                name: row.get("name")?,
                kind: row.get("kind")?,
                model: row.get("model")?,
                address: row.get("address")?,
                takes_volume: row.get("takes_volume")?,
            })
        },
    )?;
    let is_empty = statement_row(sql, &PickerIsEmptyParams {})?.is_empty;
    Ok(CastReceiverList { casting, found, is_empty })
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

    /// List `receivers` in place of every receiver listed, in one write.
    pub async fn show(&self, receivers: Vec<CastReceiver>) -> Result<(), DbError> {
        self.store
            .call(move |sql| {
                sql.execute("DELETE FROM cast_receiver", [])?;
                for receiver in &receivers {
                    sql.execute(
                        "INSERT INTO cast_receiver (id, name, kind, model, address, takes_volume) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            receiver.id,
                            receiver.name,
                            kind_column(receiver.kind),
                            receiver.model,
                            receiver.address,
                            receiver.takes_volume,
                        ],
                    )?;
                }
                Ok(())
            })
            .await
    }

    /// Follow the receivers the picker lists, the one playback is cast to
    /// apart from the rest, by name in the device's language. The query reads
    /// `name_sort`, so coven reruns it when the device's language changes.
    pub fn subscribe(&self) -> coven::LiveQuery<CastReceiverList, bae_store::DbError> {
        self.store.subscribe(|sql| cast_receivers_on(&sql))
    }

    /// Empty the list: what the library opens with, since no discovery
    /// outlives the process that ran it.
    pub async fn clear(&self) -> Result<(), DbError> {
        self.store
            .call(|sql| {
                sql.execute("DELETE FROM cast_receiver", [])?;
                Ok(())
            })
            .await
    }

    /// The receivers the picker lists now.
    #[cfg(test)]
    async fn receivers(&self) -> Result<CastReceiverList, DbError> {
        self.store.read(|sql| cast_receivers_on(&sql)).await
    }
}

#[cfg(test)]
#[path = "picker_tests.rs"]
mod tests;

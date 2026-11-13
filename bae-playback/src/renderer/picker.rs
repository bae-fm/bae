//! The receivers the picker lists: the library's device-local `cast_receiver`
//! table, what discovery last found, written by bae-cast in one write per
//! change, after the receiver playback is cast to, which comes first and is
//! listed while a new search hasn't found it again. The Casting settings page
//! lists the same receivers. Its table is `picker.sql`; the functions here
//! read and write it.

/// The table's part of the library's schema.
pub(crate) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("picker.sql")]).functions(SQL_FUNCTIONS);

/// The SQL functions the picker's read names: the key receivers order by.
const SQL_FUNCTIONS: &[&bae_store::SchemaFunction] = &[&bae_store::NAME_KEY];

use bae_store::read::SortLanguageRead;
use bae_store::DbError;
use bae_store::Store;
use bae_util::name_key::SortLanguage;
use coven::SqlReadContext;
use rusqlite::{params, OptionalExtension, Row};

use super::receiver::{CastReceiver, RendererKind};
use crate::tables::remote_receiver_from_row;

/// What the picker and the Casting page list: the receiver playback is cast
/// to, listed while a new search hasn't found it again, and every other
/// receiver discovery found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastReceiverList {
    /// The receiver playback is cast to; `None` while it plays elsewhere.
    pub casting: Option<CastReceiver>,
    /// The other receivers discovery found, by name, then id.
    pub found: Vec<CastReceiver>,
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

/// The picker's list: the receiver playback is cast to, then every other
/// receiver discovery found.
fn cast_receivers_on(
    sql: &SqlReadContext<'_>,
    language: &SortLanguage,
) -> Result<CastReceiverList, DbError> {
    let casting = sql
        .query_row(
            "SELECT cast_receiver_id, cast_receiver_name, cast_receiver_kind, \
                    cast_receiver_model, cast_receiver_address \
               FROM playback_now WHERE remote = 'on'",
            [],
            remote_receiver_from_row,
        )
        .optional()?;
    let found = sql.query(
        "SELECT id, name, kind, model, address FROM cast_receiver \
           WHERE id NOT IN (SELECT cast_receiver_id FROM playback_now WHERE remote = 'on') \
         ORDER BY bae_name_key(name, ?1), id",
        [language.as_str()],
        |row| {
            Ok(CastReceiver {
                id: row.get("id")?,
                name: row.get("name")?,
                kind: kind_from_column(row, "kind")?,
                model: row.get("model")?,
                address: row.get("address")?,
            })
        },
    )?;
    Ok(CastReceiverList { casting, found })
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
                        "INSERT INTO cast_receiver (id, name, kind, model, address) \
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            receiver.id,
                            receiver.name,
                            kind_column(receiver.kind),
                            receiver.model,
                            receiver.address,
                        ],
                    )?;
                }
                Ok(())
            })
            .await
    }

    /// Follow the receivers the picker lists, the one playback is cast to
    /// apart from the rest.
    /// The receivers are listed by name in the language `language` reads: a
    /// change of the language lists them again.
    pub fn subscribe(
        &self,
        language: SortLanguageRead,
    ) -> coven::LiveQuery<CastReceiverList, bae_store::DbError> {
        self.store.subscribe(move |sql| {
            let language = language(&sql)?;
            cast_receivers_on(&sql, &language)
        })
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

    /// The receivers the picker lists now, by name in `language`.
    #[cfg(test)]
    async fn receivers_in(&self, language: &SortLanguage) -> Result<CastReceiverList, DbError> {
        let language = language.clone();
        self.store.read(move |sql| cast_receivers_on(&sql, &language)).await
    }

    /// The receivers the picker lists now, by name in the root order.
    #[cfg(test)]
    async fn receivers(&self) -> Result<CastReceiverList, DbError> {
        self.receivers_in(&SortLanguage::root()).await
    }
}

#[cfg(test)]
#[path = "picker_tests.rs"]
mod tests;

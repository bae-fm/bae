//! Changes to the meaning of retained rows at application schema boundaries.

use coven::rusqlite::types::Value;
use coven::{
    ChangesetColumn, ChangesetOperation, ChangesetUpdate, DbError, TableChangesetMigration,
};

const ORIGIN_COLUMNS: [&str; 8] = [
    "album_title_origin",
    "album_year_origin",
    "year_origin",
    "format_origin",
    "label_origin",
    "catalog_number_origin",
    "country_origin",
    "barcode_origin",
];

pub(crate) fn remove_field_origins() -> Vec<TableChangesetMigration> {
    vec![TableChangesetMigration::new("releases", &[], |row, _| {
        row.change
            .retain_columns(|name| !ORIGIN_COLUMNS.contains(&name));
        Ok(())
    })]
}

pub(crate) fn record_kinds() -> Vec<TableChangesetMigration> {
    vec![TableChangesetMigration::new(
        "release_records",
        &["catalog", "key"],
        |row, identity| {
            let catalog = identity
                .get("catalog")
                .ok_or_else(|| DbError::Message("record catalog is absent".into()))?;
            let key = identity
                .get("key")
                .ok_or_else(|| DbError::Message("record key is absent".into()))?;
            let Value::Text(catalog) = catalog else {
                return Err(DbError::Message("record catalog is not text".into()));
            };
            let pressing = matches!(catalog.as_str(), "musicbrainz" | "discogs");
            match &mut row.change {
                ChangesetOperation::Insert(columns) | ChangesetOperation::Delete(columns) => {
                    let kind = Value::Text(if pressing { "pressing" } else { "album" }.into());
                    remap_record_columns(columns, key, pressing, kind)
                }
                ChangesetOperation::Update(columns) => remap_record_columns(
                    columns,
                    key,
                    pressing,
                    ChangesetUpdate {
                        old: None,
                        new: None,
                    },
                ),
            }
        },
    )]
}

fn remap_record_columns<T: RecordCell>(
    columns: &mut Vec<ChangesetColumn<T>>,
    key: &Value,
    pressing: bool,
    kind: T,
) -> Result<(), DbError> {
    let parent = column_mut(columns, "group_key")?;
    parent.name = "album_key".into();
    parent.value.map(|value| {
        if !pressing || value == key {
            *value = Value::Null;
        }
    });
    if !pressing {
        column_mut(columns, "reads_draft")?
            .value
            .map(|value| *value = Value::Integer(0));
    }
    columns.push(ChangesetColumn {
        name: "kind".into(),
        value: kind,
        primary_key: false,
    });
    Ok(())
}

fn column_mut<'a, T>(
    columns: &'a mut [ChangesetColumn<T>],
    name: &str,
) -> Result<&'a mut ChangesetColumn<T>, DbError> {
    columns
        .iter_mut()
        .find(|column| column.name == name)
        .ok_or_else(|| DbError::Message(format!("historical record column {name} is absent")))
}

trait RecordCell {
    fn map(&mut self, transform: impl Fn(&mut Value));
}

impl RecordCell for Value {
    fn map(&mut self, transform: impl Fn(&mut Value)) {
        transform(self);
    }
}

impl RecordCell for ChangesetUpdate {
    fn map(&mut self, transform: impl Fn(&mut Value)) {
        for value in self.old.iter_mut().chain(self.new.iter_mut()) {
            transform(value);
        }
        if self.old == self.new {
            self.old = None;
            self.new = None;
        }
    }
}

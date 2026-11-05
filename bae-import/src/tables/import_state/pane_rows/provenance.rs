//! Where the draft's values were read from: one row beside the draft, naming
//! the files' tags or the catalog release a pick read, written with the
//! draft and read back with every read that shows it.
//! Its table is `provenance.sql`, which every device's library holds.

pub(super) const SQL: &str = include_str!("provenance.sql");

use super::*;
use crate::MetadataProvenance;

/// The provenance as its columns. Only an external release names a source and
/// a release.
struct ProvenanceColumns<'a> {
    kind: &'static str,
    source: Option<&'static str>,
    release_id: Option<&'a str>,
}

fn provenance_columns(provenance: &MetadataProvenance) -> ProvenanceColumns<'_> {
    match provenance {
        MetadataProvenance::FileMetadata => {
            ProvenanceColumns { kind: "file_tags", source: None, release_id: None }
        }
        MetadataProvenance::ExternalRelease { record, .. } => ProvenanceColumns {
            kind: "external_release",
            source: Some(record.catalog.as_str()),
            release_id: Some(record.key.as_str()),
        },
    }
}

/// Make the draft's provenance `provenance`, or remove it: in place when one
/// stays, so the draft never reads as read from nowhere on its way from one
/// source to another.
pub(crate) fn store_provenance(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    provenance: Option<&MetadataProvenance>,
) -> Result<(), DbError> {
    let Some(provenance) = provenance else {
        sql.execute(
            "DELETE FROM import_candidate_draft_provenance WHERE content_hash = ?",
            [content_hash],
        )?;
        return Ok(());
    };
    let columns = provenance_columns(provenance);
    sql.execute(
        "INSERT INTO import_candidate_draft_provenance \
             (content_hash, kind, source, release_id) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT (content_hash) DO UPDATE SET \
             kind = excluded.kind, source = excluded.source, release_id = excluded.release_id",
        params![content_hash, columns.kind, columns.source, columns.release_id],
    )?;
    Ok(())
}

/// Write the provenance of a draft just written, which has none yet.
pub(crate) fn insert_provenance(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    provenance: &MetadataProvenance,
) -> Result<(), DbError> {
    let columns = provenance_columns(provenance);
    sql.execute(
        "INSERT INTO import_candidate_draft_provenance \
             (content_hash, kind, source, release_id) \
         VALUES (?, ?, ?, ?)",
        params![content_hash, columns.kind, columns.source, columns.release_id],
    )?;
    Ok(())
}

/// A candidate's draft provenance as columns of a read that joins its row as
/// `provenance`. A candidate with none reads `provenance_row` as NULL.
pub(crate) const PROVENANCE_COLUMNS: &str = "provenance.content_hash AS provenance_row, \
     provenance.kind AS provenance_kind, provenance.source AS provenance_source, \
     provenance.release_id AS provenance_release_id";

/// One candidate's [`PROVENANCE_COLUMNS`] as SQLite hands them over.
pub(crate) struct StoredProvenance {
    kind: String,
    source: Option<String>,
    release_id: Option<String>,
}

/// The [`PROVENANCE_COLUMNS`] of `row`; none for a draft that records no
/// provenance.
pub(crate) fn read_provenance(row: &Row<'_>) -> rusqlite::Result<Option<StoredProvenance>> {
    if row.get::<_, Option<String>>("provenance_row")?.is_none() {
        return Ok(None);
    }
    Ok(Some(StoredProvenance {
        kind: row.get("provenance_kind")?,
        source: row.get("provenance_source")?,
        release_id: row.get("provenance_release_id")?,
    }))
}

impl StoredProvenance {
    pub(crate) fn provenance(self) -> Result<MetadataProvenance, DbError> {
        match self.kind.as_str() {
            "file_tags" => Ok(MetadataProvenance::FileMetadata),
            "external_release" => {
                let missing = |what: &str| {
                    DbError::Message(format!("stored external metadata provenance names no {what}"))
                };
                Ok(MetadataProvenance::ExternalRelease {
                    record: lookup_reference(
                        self.source.ok_or_else(|| missing("catalog"))?,
                        self.release_id.ok_or_else(|| missing("release"))?,
                    )?,
                })
            }
            other => Err(DbError::unreadable("import_candidate_draft_provenance.kind", other)),
        }
    }
}

#[cfg(test)]
#[path = "provenance_tests.rs"]
mod tests;

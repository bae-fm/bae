//! Where the draft's values were read from: one row beside the draft, naming
//! the files' tags or the catalog release a pick read, written with the
//! draft and read back with every read that shows it.
//! Its table is `provenance.sql`, which every device's library holds.

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

/// One candidate's draft provenance as SQLite holds it.
#[derive(Clone, PartialEq)]
pub(crate) struct StoredProvenance {
    kind: String,
    source: Option<String>,
    release_id: Option<String>,
}

impl StoredProvenance {
    /// The columns of a stored provenance.
    pub(crate) fn new(kind: String, source: Option<String>, release_id: Option<String>) -> Self {
        Self { kind, source, release_id }
    }

    /// The provenance a pane's read selects, as
    /// [`super::super::state_fragments::CurrentState`] names it; none for a
    /// draft that records none.
    pub(crate) fn of(
        state: &super::super::state_fragments::CurrentState,
    ) -> Result<Option<Self>, DbError> {
        if state.provenance_row.is_none() {
            return Ok(None);
        }
        Ok(Some(Self {
            kind: super::super::required(state.provenance_kind.clone(), "provenance_kind")?,
            source: state.provenance_source.clone(),
            release_id: state.provenance_release_id.clone(),
        }))
    }
}

impl StoredProvenance {
    pub(crate) fn provenance(self) -> Result<MetadataProvenance, DbError> {
        match self.kind.as_str() {
            "file_tags" => Ok(MetadataProvenance::FileMetadata),
            "external_release" => {
                let missing = |what: &str| {
                    DbError::unreadable(
                        format!("the {what} of a stored external release provenance"),
                        None::<String>,
                    )
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

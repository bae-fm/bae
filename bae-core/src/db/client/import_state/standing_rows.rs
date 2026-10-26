//! Where each candidate something is running for stands right now
//! (`import_candidate_standing`): its identification and its import, as the
//! import list and the pane read them. The candidate runtime writes it; no row
//! outlives the process, so it is emptied as the library opens.

use super::*;
use crate::import::triage::{IdentificationStatus, ImportStanding, TriageRuntimeFacts};
use crate::import::SaveFailure;

/// Write where `candidate_key` stands: its row replaced, or removed when
/// nothing is running for it.
pub(crate) fn write_standing_on(
    sql: &SqlContext<'_, '_>,
    candidate_key: &str,
    standing: &TriageRuntimeFacts,
) -> Result<(), DbError> {
    if standing.identification.is_none() && standing.import.is_none() {
        sql.execute(
            "DELETE FROM import_candidate_standing WHERE candidate_key = ?",
            [candidate_key],
        )?;
        return Ok(());
    }
    let (identification, import_asked, save_failure_error) = match &standing.identification {
        None => (None, None, None),
        Some(IdentificationStatus::Queued) => (Some("queued"), None, None),
        Some(IdentificationStatus::Running) => (Some("running"), None, None),
        Some(IdentificationStatus::Finalizing) => (Some("finalizing"), None, None),
        Some(IdentificationStatus::ReadingPick { import_asked }) => {
            (Some("reading_pick"), Some(*import_asked), None)
        }
        Some(IdentificationStatus::FinalizationFailed { failure }) => match failure {
            SaveFailure::NotWritten { error } => (Some("not_written"), None, Some(error.as_str())),
            SaveFailure::Inapplicable { error } => {
                (Some("inapplicable"), None, Some(error.as_str()))
            }
        },
    };
    let import = standing.import.map(|import| match import {
        ImportStanding::Queued => "queued",
        ImportStanding::Running => "running",
        ImportStanding::Writing => "writing",
    });
    sql.execute(
        "INSERT INTO import_candidate_standing \
             (candidate_key, identification, import_asked, save_failure_error, import) \
         VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT (candidate_key) DO UPDATE SET \
             identification = excluded.identification, \
             import_asked = excluded.import_asked, \
             save_failure_error = excluded.save_failure_error, \
             import = excluded.import",
        params![
            candidate_key,
            identification,
            import_asked,
            save_failure_error,
            import
        ],
    )?;
    Ok(())
}

/// Where every candidate something is running for stands, by key. A key with
/// no row has nothing running.
pub(crate) fn load_standings_on(
    sql: &SqlReadContext<'_>,
) -> Result<HashMap<String, TriageRuntimeFacts>, DbError> {
    sql.query(
        "SELECT candidate_key, identification, import_asked, save_failure_error, import \
         FROM import_candidate_standing",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<bool>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        },
    )?
    .into_iter()
    .map(|(key, identification, import_asked, save_failure_error, import)| {
        let standing = standing_of(&key, identification, import_asked, save_failure_error, import)?;
        Ok((key, standing))
    })
    .collect()
}

/// Where `candidate_key` stands; nothing running when it has no row.
pub(crate) fn load_standing_on(
    sql: &SqlReadContext<'_>,
    candidate_key: &str,
) -> Result<TriageRuntimeFacts, DbError> {
    let row = sql
        .query_row(
            "SELECT identification, import_asked, save_failure_error, import \
             FROM import_candidate_standing WHERE candidate_key = ?",
            [candidate_key],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<bool>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()?;
    match row {
        // No row is the stored shape of a candidate nothing is running for.
        None => Ok(TriageRuntimeFacts::default()),
        Some((identification, import_asked, save_failure_error, import)) => standing_of(
            candidate_key,
            identification,
            import_asked,
            save_failure_error,
            import,
        ),
    }
}

fn standing_of(
    key: &str,
    identification: Option<String>,
    import_asked: Option<bool>,
    save_failure_error: Option<String>,
    import: Option<String>,
) -> Result<TriageRuntimeFacts, DbError> {
    let missing = |column: &str| {
        DbError::Message(format!("the standing of {key} names no {column}"))
    };
    let identification = match identification.as_deref() {
        None => None,
        Some("queued") => Some(IdentificationStatus::Queued),
        Some("running") => Some(IdentificationStatus::Running),
        Some("finalizing") => Some(IdentificationStatus::Finalizing),
        Some("reading_pick") => Some(IdentificationStatus::ReadingPick {
            import_asked: import_asked.ok_or_else(|| missing("import ask"))?,
        }),
        Some("not_written") => Some(IdentificationStatus::FinalizationFailed {
            failure: SaveFailure::NotWritten {
                error: save_failure_error.ok_or_else(|| missing("save failure"))?,
            },
        }),
        Some("inapplicable") => Some(IdentificationStatus::FinalizationFailed {
            failure: SaveFailure::Inapplicable {
                error: save_failure_error.ok_or_else(|| missing("save failure"))?,
            },
        }),
        Some(other) => {
            return Err(DbError::Message(format!(
                "the standing of {key} names an unknown identification {other}"
            )))
        }
    };
    let import = match import.as_deref() {
        None => None,
        Some("queued") => Some(ImportStanding::Queued),
        Some("running") => Some(ImportStanding::Running),
        Some("writing") => Some(ImportStanding::Writing),
        Some(other) => {
            return Err(DbError::Message(format!(
                "the standing of {key} names an unknown import {other}"
            )))
        }
    };
    Ok(TriageRuntimeFacts {
        identification,
        import,
    })
}

impl Database {
    /// Remove every candidate's standing: what the library opens with, since
    /// nothing a standing describes outlives the process that wrote it.
    pub(crate) async fn clear_candidate_standings(&self) -> Result<(), DbError> {
        self.call(|sql| {
            sql.execute("DELETE FROM import_candidate_standing", [])?;
            Ok(())
        })
        .await
    }

    /// Write where each of `standings` stands, in one transaction.
    pub(crate) async fn write_candidate_standings(
        &self,
        standings: Vec<(String, TriageRuntimeFacts)>,
    ) -> Result<(), DbError> {
        self.call(move |sql| {
            for (key, standing) in &standings {
                write_standing_on(sql, key, standing)?;
            }
            Ok(())
        })
        .await
    }

    /// Where every candidate something is running for stands, by key.
    pub(crate) async fn load_candidate_standings(
        &self,
    ) -> Result<HashMap<String, TriageRuntimeFacts>, DbError> {
        self.read(|sql| load_standings_on(&sql)).await
    }
}

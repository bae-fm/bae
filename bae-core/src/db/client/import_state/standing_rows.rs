//! Where each candidate something is running for stands right now
//! (`import_candidate_standing`), the only place it is held. The import list,
//! the pane and the runtime stream read it; the owners of each part change it,
//! each change decided from the row inside the write that stores it. No row
//! outlives the process, so the table is emptied as the library opens.
//!
//! A row's identification is of the files it names. Each scan write that
//! removes a candidate or reads it with other files — a pass pruning what it
//! no longer finds, an entry rewritten, a folder's reading replacing its
//! releases, a sidecar taking a folder's files, a file decision, a grouping
//! or a watched folder removed — ends the identification of each candidate
//! it changes, by key, inside itself ([`end_identification_unless_of_on`]),
//! so no read sees a candidate with an identification of files it does not
//! have, and a candidate removed and found again starts with none. Its import
//! outlives it: only that import's own ending ends a claim.
//!
//! The identification count is held beside it (`import_identification_batch`)
//! and moved by each change to an identification in that change's own write.

use super::*;
use crate::identify::IdentifyRunId;
use crate::import::candidate_runtime::{
    IdentificationChange, IdentificationMove, IdentificationStage, Identifying,
    StoredIdentification, StoredImport, StoredStanding,
};
use crate::import::candidates::{Admission, CandidateFiles, SaveFailure};
use crate::import::triage::{ImportStanding, TriageRuntimeFacts};

/// Every column of a row but its key, in the order [`StandingColumns`] reads
/// them.
const COLUMNS: &str = "identification, content_hash, file_edit_revision, admission, \
     identify_run, import_asked, save_failure_error, import, import_id";

/// One row's columns as SQLite holds them.
struct StandingColumns {
    identification: Option<String>,
    content_hash: Option<String>,
    file_edit_revision: Option<i64>,
    admission: Option<String>,
    identify_run: Option<i64>,
    import_asked: Option<bool>,
    save_failure_error: Option<String>,
    import: Option<String>,
    import_id: Option<String>,
}

impl StandingColumns {
    fn read(row: &Row<'_>, first: usize) -> rusqlite::Result<Self> {
        Ok(Self {
            identification: row.get(first)?,
            content_hash: row.get(first + 1)?,
            file_edit_revision: row.get(first + 2)?,
            admission: row.get(first + 3)?,
            identify_run: row.get(first + 4)?,
            import_asked: row.get(first + 5)?,
            save_failure_error: row.get(first + 6)?,
            import: row.get(first + 7)?,
            import_id: row.get(first + 8)?,
        })
    }

    /// The standing these columns hold for `key`. The table's checks hold
    /// every column a stage needs, so one missing is a row the checks let
    /// through, which is refused here as broken.
    fn standing(self, key: &str) -> Result<StoredStanding, DbError> {
        let broken = |what: &str| DbError::Message(format!("the standing of {key} {what}"));
        let identification = match self.identification.as_deref() {
            None => None,
            Some(stage) => {
                let run = || {
                    self.identify_run
                        .map(|run| IdentifyRunId::of_number(run as u64))
                        .ok_or_else(|| broken("names no run"))
                };
                let error = || {
                    self.save_failure_error
                        .clone()
                        .ok_or_else(|| broken("names no failure"))
                };
                let stage = match stage {
                    "queued" => IdentificationStage::Queued {
                        admission: match self.admission.as_deref() {
                            Some("automatic") => Admission::Automatic,
                            Some("requested") => Admission::Requested,
                            Some(other) => {
                                return Err(broken(&format!("names an unknown admission {other}")))
                            }
                            None => return Err(broken("names no admission")),
                        },
                    },
                    "running" => IdentificationStage::Running { run: run()? },
                    "finalizing" => IdentificationStage::Finalizing { run: run()? },
                    "reading_pick" => IdentificationStage::ReadingPick {
                        run: run()?,
                        import_asked: self
                            .import_asked
                            .ok_or_else(|| broken("names no import ask"))?,
                    },
                    "not_written" => IdentificationStage::Failed {
                        failure: SaveFailure::NotWritten { error: error()? },
                    },
                    "inapplicable" => IdentificationStage::Failed {
                        failure: SaveFailure::Inapplicable { error: error()? },
                    },
                    other => {
                        return Err(broken(&format!("names an unknown identification {other}")))
                    }
                };
                Some(StoredIdentification {
                    files: CandidateFiles {
                        content_hash: self.content_hash.ok_or_else(|| broken("names no files"))?,
                        file_edit_revision: self
                            .file_edit_revision
                            .ok_or_else(|| broken("names no file revision"))?
                            as u64,
                    },
                    stage,
                })
            }
        };
        let import = match (self.import.as_deref(), self.import_id) {
            (None, None) => None,
            (Some(standing), Some(import_id)) => Some(StoredImport {
                import_id,
                standing: match standing {
                    "queued" => ImportStanding::Queued,
                    "running" => ImportStanding::Running,
                    "writing" => ImportStanding::Writing,
                    other => return Err(broken(&format!("names an unknown import {other}"))),
                },
            }),
            (Some(_), None) | (None, Some(_)) => {
                return Err(broken(
                    "names an import without its id, or an id without it",
                ))
            }
        };
        Ok(StoredStanding {
            identification,
            import,
        })
    }
}

/// Where every candidate something is running for stands, by key. A key with
/// no entry has nothing running.
pub(crate) fn load_standings_on(
    sql: &SqlReadContext<'_>,
) -> Result<HashMap<String, StoredStanding>, DbError> {
    sql.query(
        &format!("SELECT candidate_key, {COLUMNS} FROM import_candidate_standing"),
        [],
        |row| Ok((row.get::<_, String>(0)?, StandingColumns::read(row, 1)?)),
    )?
    .into_iter()
    .map(|(key, columns)| {
        let standing = columns.standing(&key)?;
        Ok((key, standing))
    })
    .collect()
}

/// Where `candidate_key` stands; nothing running when it has no row.
pub(crate) fn load_standing_on(
    sql: &SqlReadContext<'_>,
    candidate_key: &str,
) -> Result<StoredStanding, DbError> {
    let columns = sql
        .query_row(
            &format!("SELECT {COLUMNS} FROM import_candidate_standing WHERE candidate_key = ?"),
            [candidate_key],
            |row| StandingColumns::read(row, 0),
        )
        .optional()?;
    match columns {
        // No row is the stored shape of a candidate nothing is running for.
        None => Ok(StoredStanding::default()),
        Some(columns) => columns.standing(candidate_key),
    }
}

/// Where every candidate something is running for stands, as the rows read
/// it.
pub(crate) fn load_standing_facts_on(
    sql: &SqlReadContext<'_>,
) -> Result<HashMap<String, TriageRuntimeFacts>, DbError> {
    Ok(load_standings_on(sql)?
        .into_iter()
        .map(|(key, standing)| (key, standing.facts()))
        .collect())
}

/// How far the batch of identifications in flight has got: ended of
/// admitted. `(0, 0)` is no batch.
fn load_identification_progress_on(sql: &SqlReadContext<'_>) -> Result<(u32, u32), DbError> {
    Ok(sql.query_row(
        "SELECT COALESCE(SUM(ended), 0), COUNT(*) FROM import_identification_batch",
        [],
        |row| Ok((row.get::<_, u32>(0)?, row.get::<_, u32>(1)?)),
    )?)
}

/// Every candidate's standing and the identification count, read together.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StandingsRead {
    pub(crate) standings: HashMap<String, StoredStanding>,
    pub(crate) identification_progress: (u32, u32),
}

fn load_standings_read_on(sql: &SqlReadContext<'_>) -> Result<StandingsRead, DbError> {
    Ok(StandingsRead {
        standings: load_standings_on(sql)?,
        identification_progress: load_identification_progress_on(sql)?,
    })
}

/// End, inside the caller's write, `candidate_key`'s identification unless
/// it is of `held` — the files the write leaves the scan holding the candidate
/// with, `None` for a write that leaves the scan without it.
pub(crate) fn end_identification_unless_of_on(
    sql: &SqlContext<'_, '_>,
    candidate_key: &str,
    held: Option<&CandidateFiles>,
) -> Result<(), DbError> {
    change_identification_on(
        sql,
        candidate_key,
        &IdentificationChange::ScanHolds {
            files: held.cloned(),
        },
    )?;
    Ok(())
}

/// Move the identification count by what one change did to `candidate_key`:
/// an admission joins the batch, an ending counts as ended, and the batch is
/// over once every identification it admitted has ended.
fn count_on(
    sql: &SqlContext<'_, '_>,
    candidate_key: &str,
    moved: &IdentificationMove,
) -> Result<(), DbError> {
    let was = Identifying::of(moved.before.as_ref());
    let is = Identifying::of(moved.after.as_ref());
    if !was.queued && is.queued {
        sql.execute(
            "INSERT INTO import_identification_batch (candidate_key, ended) VALUES (?, 0) \
             ON CONFLICT (candidate_key) DO UPDATE SET ended = 0",
            [candidate_key],
        )?;
    } else if was.in_flight && !is.in_flight {
        sql.execute(
            "UPDATE import_identification_batch SET ended = 1 WHERE candidate_key = ?",
            [candidate_key],
        )?;
        sql.execute(
            "DELETE FROM import_identification_batch \
             WHERE NOT EXISTS (SELECT 1 FROM import_identification_batch WHERE ended = 0)",
            [],
        )?;
    }
    Ok(())
}

/// The row as stored, whatever files the scan holds now: what a change is
/// decided from.
fn stored_on(sql: &SqlContext<'_, '_>, candidate_key: &str) -> Result<StoredStanding, DbError> {
    let columns = sql
        .query_row(
            &format!("SELECT {COLUMNS} FROM import_candidate_standing WHERE candidate_key = ?"),
            [candidate_key],
            |row| StandingColumns::read(row, 0),
        )
        .optional()?;
    match columns {
        None => Ok(StoredStanding::default()),
        Some(columns) => columns.standing(candidate_key),
    }
}

/// Store `standing` as `candidate_key`'s row, or remove the row when nothing
/// is running for it.
fn write_on(
    sql: &SqlContext<'_, '_>,
    candidate_key: &str,
    standing: &StoredStanding,
) -> Result<(), DbError> {
    if standing == &StoredStanding::default() {
        sql.execute(
            "DELETE FROM import_candidate_standing WHERE candidate_key = ?",
            [candidate_key],
        )?;
        return Ok(());
    }
    let mut identification = None;
    let mut content_hash = None;
    let mut file_edit_revision = None;
    let mut admission = None;
    let mut identify_run = None;
    let mut import_asked = None;
    let mut save_failure_error = None;
    if let Some(stored) = &standing.identification {
        content_hash = Some(stored.files.content_hash.as_str());
        file_edit_revision = Some(to_i64(
            stored.files.file_edit_revision,
            "a standing's file edit revision",
        )?);
        identify_run = stored
            .stage
            .run()
            .map(|run| to_i64(run.number(), "a standing's run"))
            .transpose()?;
        identification = Some(match &stored.stage {
            IdentificationStage::Queued { admission: queued } => {
                admission = Some(match queued {
                    Admission::Automatic => "automatic",
                    Admission::Requested => "requested",
                });
                "queued"
            }
            IdentificationStage::Running { .. } => "running",
            IdentificationStage::Finalizing { .. } => "finalizing",
            IdentificationStage::ReadingPick {
                import_asked: asked,
                ..
            } => {
                import_asked = Some(*asked);
                "reading_pick"
            }
            IdentificationStage::Failed { failure } => {
                save_failure_error = Some(failure.error());
                match failure {
                    SaveFailure::NotWritten { .. } => "not_written",
                    SaveFailure::Inapplicable { .. } => "inapplicable",
                }
            }
        });
    }
    let (import, import_id) = match &standing.import {
        None => (None, None),
        Some(stored) => (
            Some(match stored.standing {
                ImportStanding::Queued => "queued",
                ImportStanding::Running => "running",
                ImportStanding::Writing => "writing",
            }),
            Some(stored.import_id.as_str()),
        ),
    };
    sql.execute(
        &format!(
            "INSERT INTO import_candidate_standing (candidate_key, {COLUMNS}) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT (candidate_key) DO UPDATE SET \
                 identification = excluded.identification, \
                 content_hash = excluded.content_hash, \
                 file_edit_revision = excluded.file_edit_revision, \
                 admission = excluded.admission, \
                 identify_run = excluded.identify_run, \
                 import_asked = excluded.import_asked, \
                 save_failure_error = excluded.save_failure_error, \
                 import = excluded.import, \
                 import_id = excluded.import_id"
        ),
        params![
            candidate_key,
            identification,
            content_hash,
            file_edit_revision,
            admission,
            identify_run,
            import_asked,
            save_failure_error,
            import,
            import_id,
        ],
    )?;
    Ok(())
}

/// Apply `change` to `candidate_key`'s identification as stored, inside the
/// caller's write.
pub(crate) fn change_identification_on(
    sql: &SqlContext<'_, '_>,
    candidate_key: &str,
    change: &IdentificationChange,
) -> Result<IdentificationMove, DbError> {
    let mut standing = stored_on(sql, candidate_key)?;
    let before = standing.identification.take();
    let after = change.applied(before.as_ref());
    let moved = IdentificationMove { before, after };
    if moved.after != moved.before {
        standing.identification = moved.after.clone();
        write_on(sql, candidate_key, &standing)?;
        count_on(sql, candidate_key, &moved)?;
    }
    Ok(moved)
}

/// Claim `candidate_key` for `import_id`, queued. `false`, changing nothing,
/// when an import already owns it: one import of a candidate at a time.
fn claim_import_on(
    sql: &SqlContext<'_, '_>,
    candidate_key: &str,
    import_id: &str,
) -> Result<bool, DbError> {
    let mut standing = stored_on(sql, candidate_key)?;
    if standing.import.is_some() {
        return Ok(false);
    }
    standing.import = Some(StoredImport {
        import_id: import_id.to_string(),
        standing: ImportStanding::Queued,
    });
    write_on(sql, candidate_key, &standing)?;
    Ok(true)
}

/// Where the import `import_id` that owns `candidate_key` stands now, or its
/// end with `None`, inside the caller's write. Refused when that import does
/// not own it: only the import holding a claim moves or ends it.
pub(crate) fn move_import_on(
    sql: &SqlContext<'_, '_>,
    candidate_key: &str,
    import_id: &str,
    next: Option<ImportStanding>,
) -> Result<(), DbError> {
    let mut standing = stored_on(sql, candidate_key)?;
    if standing
        .import
        .as_ref()
        .is_none_or(|import| import.import_id != import_id)
    {
        return Err(DbError::Message(format!(
            "import {import_id} does not own {candidate_key}"
        )));
    }
    standing.import = next.map(|standing| StoredImport {
        import_id: import_id.to_string(),
        standing,
    });
    write_on(sql, candidate_key, &standing)
}

/// What claiming a candidate for an import did.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ImportClaim {
    /// The import owns the candidate, and the identification change the
    /// claim made with it, when it was asked to make one, did this.
    Claimed {
        identification: Option<IdentificationMove>,
    },
    /// Another import owns it; nothing changed.
    Refused,
}

impl Database {
    /// Remove every candidate's standing: what the library opens with, since
    /// nothing a standing describes outlives the process that wrote it.
    pub(crate) async fn clear_candidate_standings(&self) -> Result<(), DbError> {
        self.call(|sql| {
            sql.execute("DELETE FROM import_candidate_standing", [])?;
            sql.execute("DELETE FROM import_identification_batch", [])?;
            Ok(())
        })
        .await
    }

    /// Where every candidate something is running for stands, by key.
    pub(crate) async fn load_candidate_standings(
        &self,
    ) -> Result<HashMap<String, StoredStanding>, DbError> {
        self.read(|sql| load_standings_on(&sql)).await
    }

    /// Where `candidate_key` stands.
    pub(crate) async fn load_candidate_standing(
        &self,
        candidate_key: &str,
    ) -> Result<StoredStanding, DbError> {
        let candidate_key = candidate_key.to_string();
        self.read(move |sql| load_standing_on(&sql, &candidate_key))
            .await
    }

    /// Where every candidate something is running for stands, as the rows
    /// read it.
    pub(crate) async fn load_candidate_standing_facts(
        &self,
    ) -> Result<HashMap<String, TriageRuntimeFacts>, DbError> {
        self.read(|sql| load_standing_facts_on(&sql)).await
    }

    /// How far the batch of identifications in flight has got, for a test;
    /// the live read carries it to everyone else.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn load_identification_progress(&self) -> Result<(u32, u32), DbError> {
        self.read(|sql| load_identification_progress_on(&sql)).await
    }

    /// Every candidate's standing and the identification count, live.
    pub(crate) fn subscribe_candidate_standings(&self) -> coven::LiveQuery<StandingsRead> {
        self.inner
            .handle
            .subscribe(|sql| load_standings_read_on(&sql).map_err(CovenError::from))
    }

    /// Apply each change to its candidate's identification, in one write.
    pub(crate) async fn change_candidate_identifications(
        &self,
        changes: Vec<(String, IdentificationChange)>,
    ) -> Result<Vec<(String, IdentificationMove)>, DbError> {
        self.call(move |sql| {
            changes
                .into_iter()
                .map(|(key, change)| {
                    let moved = change_identification_on(sql, &key, &change)?;
                    Ok((key, moved))
                })
                .collect()
        })
        .await
    }

    /// Claim `candidate_key` for `import_id`, and, when the claim lands, make
    /// `identification` to the candidate's identification in the same write.
    pub(crate) async fn claim_candidate_import(
        &self,
        candidate_key: &str,
        import_id: &str,
        identification: Option<IdentificationChange>,
    ) -> Result<ImportClaim, DbError> {
        let candidate_key = candidate_key.to_string();
        let import_id = import_id.to_string();
        self.call(move |sql| {
            if !claim_import_on(sql, &candidate_key, &import_id)? {
                return Ok(ImportClaim::Refused);
            }
            let identification = identification
                .map(|change| change_identification_on(sql, &candidate_key, &change))
                .transpose()?;
            Ok(ImportClaim::Claimed { identification })
        })
        .await
    }

    /// Where the import `import_id` that owns `candidate_key` stands now, or
    /// its end with `None`.
    pub(crate) async fn move_candidate_import(
        &self,
        candidate_key: &str,
        import_id: &str,
        next: Option<ImportStanding>,
    ) -> Result<(), DbError> {
        let candidate_key = candidate_key.to_string();
        let import_id = import_id.to_string();
        self.call(move |sql| move_import_on(sql, &candidate_key, &import_id, next))
            .await
    }
}

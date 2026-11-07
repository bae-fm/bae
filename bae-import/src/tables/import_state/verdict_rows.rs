//! The identify verdict as its own row, its matches, and the read back.
//!
//! One `import_candidate_verdict` row holds a whole terminal verdict — found,
//! nothing anywhere, manual only, or failed — and the ordered release-match
//! rows (see `match_rows`) and the rows of the ledger it recorded (see
//! `ledger_rows`) hang off it. Deleting the row clears them with it, so a
//! candidate can neither hold two verdicts nor keep matches without one.
//!
//! A run in flight is stored in the same rows, owned by the run rather than by
//! a candidate's files ([`Owner`]): how it stands, the ledger as it stands and
//! the releases its answers so far combine to, then the answer it reached.
//! Its table is `verdict_rows.sql`, which every device's library holds.

pub(super) const SQL: &str = include_str!("verdict_rows.sql");

use super::ledger_rows::{
    delete_ledger, read_ledger, step_columns, store_ledger, MatchPositions, StoredLedger,
    LEDGER_COLUMNS,
};
use super::match_rows::{store_match, Listing, MatchOwner, StoredMatches, Written};
use super::*;
use bae_catalogs::search::MetadataResult;
use bae_catalogs::InternalFailure;
use bae_identify::{
    Findings, IdentifyFailure, IdentifyRunView, LookupView, MediumConflict, NamedNote, RunOutcome,
    TerminalVerdict,
};

/// Whose verdict-shaped rows these are: a candidate's verdict, under its
/// files' content hash, or how a run in flight stands, under the run.
#[derive(Clone, Copy, Debug)]
pub(super) enum Owner<'a> {
    Candidate(&'a str),
    Run(i64),
}

impl Owner<'_> {
    /// The column that names this owner on every row it owns.
    pub(super) fn column(self) -> &'static str {
        match self {
            Self::Candidate(_) => "content_hash",
            Self::Run(_) => "identify_run",
        }
    }

    pub(super) fn content_hash(self) -> Option<String> {
        match self {
            Self::Candidate(content_hash) => Some(content_hash.to_string()),
            Self::Run(_) => None,
        }
    }

    pub(super) fn run(self) -> Option<i64> {
        match self {
            Self::Candidate(_) => None,
            Self::Run(run) => Some(run),
        }
    }
}

impl rusqlite::ToSql for Owner<'_> {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        match self {
            Self::Candidate(content_hash) => content_hash.to_sql(),
            Self::Run(run) => run.to_sql(),
        }
    }
}

impl std::fmt::Display for Owner<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Candidate(content_hash) => f.write_str(content_hash),
            Self::Run(run) => write!(f, "run {run}"),
        }
    }
}

/// An outcome as its rows store it: a run's state while its lookups are out,
/// or the verdict a run ended on.
#[derive(Clone, Copy)]
pub(super) enum Outcome<'a> {
    Running { ledger: &'a IdentifyRunView, findings: &'a Findings, track_count: u32 },
    Settled(&'a TerminalVerdict),
}

impl<'a> Outcome<'a> {
    pub(super) fn of(outcome: &'a RunOutcome) -> Self {
        match outcome {
            RunOutcome::Running { ledger, findings, track_count } => {
                Self::Running { ledger, findings, track_count: *track_count }
            }
            RunOutcome::Answered(verdict) => Self::Settled(verdict),
        }
    }

    fn ledger(self) -> Option<&'a IdentifyRunView> {
        match self {
            Self::Running { ledger, .. } => Some(ledger),
            Self::Settled(verdict) => verdict.ledger(),
        }
    }

    fn findings(self) -> Option<&'a Findings> {
        match self {
            Self::Running { findings, .. } => Some(findings),
            Self::Settled(verdict) => verdict.findings(),
        }
    }

    fn running(self) -> bool {
        matches!(self, Self::Running { .. })
    }
}

/// Clear whatever verdict stands under `content_hash`. The match rows go with
/// it: they hang off the verdict row.
pub(super) fn delete_verdict(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute("DELETE FROM import_candidate_verdict WHERE content_hash = ?", [content_hash])?;
    Ok(())
}

/// Write one whole verdict, the matches it found and the ledger it recorded
/// over whatever verdict stood under this hash: the verdict row and each
/// match row in place, so the candidate never reads as holding no verdict or
/// no matches on its way from one verdict to another, and the matches past
/// the new last go.
pub(super) fn store_verdict(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    identification: &DbCandidateIdentifyResult,
) -> Result<(), DbError> {
    store_outcome(
        sql,
        Owner::Candidate(content_hash),
        Outcome::Settled(&identification.verdict),
        identification.kept_own_draft,
    )
}

/// Write `outcome` as `owner`'s, over whatever it stored before, as
/// [`store_verdict`] writes a verdict. Only a run in flight stores a ledger
/// that has not settled.
pub(super) fn store_outcome(
    sql: &CovenWrite<'_>,
    owner: Owner<'_>,
    outcome: Outcome<'_>,
    kept_own_draft: bool,
) -> Result<(), DbError> {
    let (kind, track_count) = match outcome {
        Outcome::Running { track_count, .. } => ("running", Some(track_count)),
        Outcome::Settled(TerminalVerdict::Found { track_count, .. }) => {
            ("found", Some(*track_count))
        }
        Outcome::Settled(TerminalVerdict::NotFoundAnywhere { .. }) => ("not_found", None),
        Outcome::Settled(TerminalVerdict::ManualOnly { track_count, .. }) => {
            ("manual_only", Some(*track_count))
        }
        Outcome::Settled(TerminalVerdict::Failed { track_count, .. }) => {
            ("failed", Some(*track_count))
        }
        Outcome::Settled(TerminalVerdict::Error { .. }) => ("error", None),
    };
    let steps = step_columns(owner, outcome.ledger(), outcome.running())?;
    let medium_conflict =
        outcome.findings().and_then(|findings| findings.medium_conflict).map(|conflict| {
            match conflict {
                MediumConflict::CdRip => "cd_rip",
                MediumConflict::NotCdAudio => "not_cd_audio",
            }
        });
    let failures_json =
        match outcome {
            Outcome::Settled(TerminalVerdict::Failed { failures, .. }) => {
                if failures.is_empty() {
                    return Err(DbError::Message(format!(
                        "failed verdict for {owner} contains no failed lookup"
                    )));
                }
                Some(serde_json::to_string(failures).map_err(|error| {
                    DbError::encode(format!("identify failure for {owner}"), error)
                })?)
            }
            Outcome::Running { .. } | Outcome::Settled(_) => None,
        };
    let error_detail = match outcome {
        Outcome::Settled(TerminalVerdict::Error { failure }) => Some(failure.detail.as_str()),
        Outcome::Running { .. } | Outcome::Settled(_) => None,
    };
    let column = owner.column();
    sql.execute(
        &format!(
            "INSERT INTO import_candidate_verdict \
                 (content_hash, identify_run, kind, track_count, failures_json, error_detail, \
                  disc_id_step, barcode_step, catalog_step, isrc_step, search_step, \
                  search_album, search_artist, medium_conflict, kept_own_draft) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT ({column}) DO UPDATE SET \
                 kind = excluded.kind, track_count = excluded.track_count, \
                 failures_json = excluded.failures_json, error_detail = excluded.error_detail, \
                 disc_id_step = excluded.disc_id_step, barcode_step = excluded.barcode_step, \
                 catalog_step = excluded.catalog_step, isrc_step = excluded.isrc_step, \
                 search_step = excluded.search_step, search_album = excluded.search_album, \
                 search_artist = excluded.search_artist, \
                 medium_conflict = excluded.medium_conflict, \
                 kept_own_draft = excluded.kept_own_draft"
        ),
        params![
            owner.content_hash(),
            owner.run(),
            kind,
            track_count,
            failures_json,
            error_detail,
            steps.disc_id,
            steps.barcode,
            steps.catalog,
            steps.isrc,
            steps.search,
            steps.search_album,
            steps.search_artist,
            medium_conflict,
            kept_own_draft,
        ],
    )?;
    // The ledger's rows link the match rows, and the rows under each match
    // name what the match row below may change (its cover's address, its
    // media's kind), so the previous verdict's go first. The import list
    // reads none of them.
    delete_ledger(sql, owner)?;
    super::match_rows::delete_verdict_match_entries(sql, owner)?;
    let written = store_matches(sql, owner, outcome)?;
    super::match_rows::delete_verdict_matches_from(sql, owner, written.len())?;
    if let Some(ledger) = outcome.ledger() {
        let positions: MatchPositions<'_> = written
            .iter()
            .enumerate()
            .map(|(position, result)| {
                ((result.source, result.release_id.as_str()), (position, *result))
            })
            .collect();
        store_ledger(sql, owner, ledger, &positions, outcome.running())?;
    }
    Ok(())
}

/// The releases of one outcome, written under the row that found them: the
/// matches first, then the ones agreement narrowed out, then the ones a cell
/// of its ledger lists that neither does, each once, in the order the ledger
/// lists them — all one position sequence, each marked as what it is. A
/// found verdict, a failed one and a run in flight write theirs alike; the
/// other kinds hold no findings, and a ledger of theirs lists nothing.
/// Answers the releases written, in position order.
fn store_matches<'a>(
    sql: &CovenWrite<'_>,
    owner: Owner<'_>,
    outcome: Outcome<'a>,
) -> Result<Vec<&'a MetadataResult>, DbError> {
    let (matches, pressings, narrowed_out, named_notes): (
        &[MetadataResult],
        &[u32],
        _,
        &[NamedNote],
    ) = match outcome.findings() {
        Some(findings) => (
            &findings.matches,
            &findings.pressings,
            Some(&findings.narrowed_out),
            &findings.named_notes,
        ),
        None => (&[], &[], None, &[]),
    };
    let (narrowed_matches, narrowed_pressings): (&[MetadataResult], &[u32]) = match narrowed_out {
        Some(narrowed_out) => (&narrowed_out.matches, &narrowed_out.pressings),
        None => (&[], &[]),
    };
    let named_note = |result: &MetadataResult| {
        named_notes
            .iter()
            .find(|named| {
                named.release.catalog == result.source && named.release.key == result.release_id
            })
            .map(|named| named.note.as_str())
    };
    if let Some(stray) = named_notes.iter().find(|named| {
        !matches.iter().any(|result| {
            named.release.catalog == result.source && named.release.key == result.release_id
        })
    }) {
        return Err(DbError::Message(format!(
            "a verdict for {owner} names a note of {} {}, which it does not offer",
            stray.release.catalog.as_str(),
            stray.release.key
        )));
    }
    let rowed = |what: &str, results: &[MetadataResult], rows: &[u32]| {
        if results.len() == rows.len() {
            return Ok(());
        }
        Err(DbError::Message(format!(
            "a verdict for {owner} carries {} {what} and {} pressing entries; they are \
             index-aligned",
            results.len(),
            rows.len()
        )))
    };
    rowed("matches", matches, pressings)?;
    rowed("narrowed-out releases", narrowed_matches, narrowed_pressings)?;
    let mut written: Vec<(&MetadataResult, Written<'_>)> = matches
        .iter()
        .zip(pressings.iter())
        .map(|(result, pressing)| (result, Written::Offered(*pressing, named_note(result))))
        .chain(
            narrowed_matches
                .iter()
                .zip(narrowed_pressings.iter())
                .map(|(result, pressing)| (result, Written::NarrowedOut(*pressing))),
        )
        .collect();
    for listed in outcome
        .ledger()
        .into_iter()
        .flat_map(IdentifyRunView::lookups)
        .flat_map(LookupView::releases)
    {
        let held = written.iter().any(|(result, _)| {
            result.source == listed.source && result.release_id == listed.release_id
        });
        if !held {
            written.push((listed, Written::Unlisted));
        }
    }
    for (position, (result, listing)) in written.iter().enumerate() {
        store_match(sql, MatchOwner::Verdict(owner), position, result, *listing)?;
    }
    Ok(written.into_iter().map(|(result, _)| result).collect())
}

/// The columns of one stored verdict row.
pub(super) struct VerdictRow {
    pub(super) content_hash: String,
    pub(super) kind: String,
    pub(super) track_count: Option<i64>,
    pub(super) failures_json: Option<String>,
    pub(super) error_detail: Option<String>,
    pub(super) ledger: StoredLedger,
    pub(super) medium_conflict: Option<MediumConflict>,
    pub(super) kept_own_draft: bool,
}

/// A candidate's verdict as columns of a read that joins its row as
/// `verdict`, its ledger's beside them. A candidate with none reads
/// `verdict_row` as NULL.
pub(super) fn verdict_columns() -> String {
    format!(
        "COALESCE(verdict.content_hash, 'run ' || verdict.identify_run) AS verdict_row, \
         verdict.kind AS verdict_kind, verdict.track_count AS verdict_track_count, \
         verdict.failures_json AS verdict_failures_json, \
         verdict.error_detail AS verdict_error_detail, \
         verdict.medium_conflict AS verdict_medium_conflict, \
         verdict.kept_own_draft AS verdict_kept_own_draft, {LEDGER_COLUMNS}"
    )
}

/// The [`verdict_columns`] of `row`; none for a candidate no verdict stands
/// for.
pub(super) fn read_verdict_row(row: &Row<'_>) -> Result<Option<VerdictRow>, DbError> {
    let Some(content_hash) = row.get::<_, Option<String>>("verdict_row")? else {
        return Ok(None);
    };
    Ok(Some(VerdictRow {
        content_hash,
        kind: row.get("verdict_kind")?,
        track_count: row.get("verdict_track_count")?,
        failures_json: row.get("verdict_failures_json")?,
        error_detail: row.get("verdict_error_detail")?,
        ledger: read_ledger(row)?,
        medium_conflict: medium_conflict_of(row.get("verdict_medium_conflict")?)?,
        kept_own_draft: row.get("verdict_kept_own_draft")?,
    }))
}

/// Rebuild what identification concluded from its row, the matches it found
/// and the ledger it recorded.
pub(super) fn identification_of(
    row: VerdictRow,
    found: StoredMatches,
) -> Result<DbCandidateIdentifyResult, DbError> {
    let owner = row.content_hash.clone();
    match outcome_of(row, found)? {
        (RunOutcome::Answered(verdict), kept_own_draft) => {
            Ok(DbCandidateIdentifyResult { verdict, kept_own_draft })
        }
        // A run publishes only what it reached to the candidate it was of.
        (RunOutcome::Running { .. }, _) => Err(DbError::unreadable(
            format!("import_candidate_verdict.kind of candidate {owner}"),
            "running",
        )),
    }
}

/// Rebuild how a verdict row's owner stands — a run in flight, or the
/// verdict a run reached — from its row, the matches and the ledger; and
/// whether the person kept their own draft over it.
pub(super) fn outcome_of(
    row: VerdictRow,
    found: StoredMatches,
) -> Result<(RunOutcome, bool), DbError> {
    let VerdictRow {
        content_hash,
        kind,
        track_count,
        failures_json,
        error_detail,
        ledger,
        medium_conflict,
        kept_own_draft,
    } = row;
    // Only a typed search's catalog returns a release; a verdict-shaped
    // owner's rows are listed or unlisted.
    if let Some(returned) =
        found.rows.iter().find(|row| matches!(row.listing, Listing::Returned { .. }))
    {
        return Err(DbError::unreadable(
            format!(
                "import_candidate_match.listing of match {} of {content_hash}",
                returned.position
            ),
            "returned",
        ));
    }
    let held: HashMap<i64, &MetadataResult> =
        found.rows.iter().map(|row| (row.position, &row.result)).collect();
    let (ledger, linked) = ledger.ledger(&content_hash, &held)?;
    if let Some(stray) = found
        .rows
        .iter()
        .find(|row| matches!(row.listing, Listing::Unlisted) && !linked.contains(&row.position))
    {
        return Err(DbError::unreadable(
            format!(
                "the lookups of {content_hash}'s ledger that found unlisted match {}",
                stray.position
            ),
            &linked,
        ));
    }
    let count_of = || {
        track_count
            .ok_or_else(|| {
                DbError::unreadable(
                    format!("import_candidate_verdict.track_count of {kind} {content_hash}"),
                    None::<i64>,
                )
            })
            .and_then(|count| {
                to_u32(count, &format!("the tracks verdict {kind} for {content_hash} counts"))
            })
    };
    // Only the verdicts that hold findings may list releases; a listed row
    // under any other kind is one no writer here produces.
    let findings_of = |found: StoredMatches| {
        let mut findings = Findings { medium_conflict, ..Findings::default() };
        for row in found.rows {
            match row.listing {
                Listing::Offered(listed) => {
                    if let Some(note) = listed.named_note {
                        findings
                            .named_notes
                            .push(NamedNote { release: row.result.release(), note });
                    }
                    findings.matches.push(row.result);
                    findings.pressings.push(listed.pressing);
                }
                Listing::NarrowedOut(listed) => {
                    findings.narrowed_out.matches.push(row.result);
                    findings.narrowed_out.pressings.push(listed.pressing);
                }
                Listing::Unlisted | Listing::Returned { .. } => {}
            }
        }
        findings
    };
    let no_findings = |found: &StoredMatches| {
        if found.rows.iter().all(|row| matches!(row.listing, Listing::Unlisted)) {
            return Ok(());
        }
        Err(DbError::unreadable(
            format!("the listed matches of verdict {kind} for {content_hash}"),
            found.rows.iter().filter(|row| !matches!(row.listing, Listing::Unlisted)).count(),
        ))
    };
    let verdict = match kind.as_str() {
        "running" => {
            let ledger = ledger.ok_or_else(|| {
                DbError::unreadable(format!("the ledger of run {content_hash}"), None::<String>)
            })?;
            return Ok((
                RunOutcome::Running {
                    ledger,
                    findings: findings_of(found),
                    track_count: count_of()?,
                },
                kept_own_draft,
            ));
        }
        "found" => TerminalVerdict::Found {
            findings: findings_of(found),
            track_count: count_of()?,
            ledger,
        },
        "not_found" => {
            no_findings(&found)?;
            TerminalVerdict::NotFoundAnywhere { ledger }
        }
        "manual_only" => {
            no_findings(&found)?;
            TerminalVerdict::ManualOnly { track_count: count_of()?, ledger }
        }
        "failed" => {
            let json = failures_json.ok_or_else(|| {
                DbError::unreadable(
                    format!("import_candidate_verdict.failures_json of failed {content_hash}"),
                    None::<String>,
                )
            })?;
            let failures: Vec<IdentifyFailure> = serde_json::from_str(&json).map_err(|error| {
                DbError::unreadable_json(
                    format!("identify failure for {content_hash}"),
                    &json,
                    error,
                )
            })?;
            if failures.is_empty() {
                return Err(DbError::unreadable(
                    format!("import_candidate_verdict.failures_json of failed {content_hash}"),
                    &json,
                ));
            }
            TerminalVerdict::Failed {
                failures,
                findings: findings_of(found),
                track_count: count_of()?,
                ledger,
            }
        }
        "error" => {
            no_findings(&found)?;
            let detail = error_detail.ok_or_else(|| {
                DbError::unreadable(
                    format!("import_candidate_verdict.error_detail of error {content_hash}"),
                    None::<String>,
                )
            })?;
            TerminalVerdict::Error { failure: InternalFailure { detail } }
        }
        other => return Err(DbError::unreadable("import_candidate_verdict.kind", other)),
    };
    Ok((RunOutcome::Answered(verdict), kept_own_draft))
}

/// The stored medium conflict of a verdict row.
pub(super) fn medium_conflict_of(
    kind: Option<String>,
) -> Result<Option<bae_identify::MediumConflict>, DbError> {
    let Some(kind) = kind else {
        return Ok(None);
    };
    Ok(Some(match kind.as_str() {
        "cd_rip" => bae_identify::MediumConflict::CdRip,
        "not_cd_audio" => bae_identify::MediumConflict::NotCdAudio,
        other => {
            return Err(DbError::unreadable("import_candidate_verdict.medium_conflict", other))
        }
    }))
}

impl ImportTables {
    /// Whether a verdict is stored for the files at `content_hash` as their
    /// file decisions stand at `edit_revision`.
    pub(crate) async fn verdict_stored_for(
        &self,
        content_hash: &str,
        edit_revision: u64,
    ) -> Result<bool, DbError> {
        let content_hash = content_hash.to_string();
        let edit_revision = to_i64(edit_revision, "a candidate's file edit revision")?;
        self.store
            .read(move |sql| {
                Ok(sql.query_row(
                    "SELECT EXISTS (SELECT 1 FROM import_candidate_state AS state \
                     JOIN import_candidate_verdict AS verdict \
                       ON verdict.content_hash = state.content_hash \
                     WHERE state.content_hash = ? AND state.edit_revision = ?)",
                    params![content_hash, edit_revision],
                    |row| row.get(0),
                )?)
            })
            .await
    }
}

/// What a caller supplies to record one candidate's identify verdict via
/// [`crate::tables::ImportTables::store_verdict`]. Every outcome,
/// failed included, becomes one `import_candidate_verdict` row. Their timestamp
/// is stamped by the write path from the injected clock, the same convention as
/// `created_at` in bae-records' row writers: a timestamp that
/// records "when this write happened" is the DB layer's to assign, not data a
/// caller hands in.
///
/// It carries no file decisions: those are the user's half of the row and the
/// verdict write leaves them alone.
#[derive(Debug, Clone)]
pub struct NewImportCandidateVerdict {
    /// The files the verdict answers: the row it addresses, and the
    /// file-decision revision it was derived from.
    pub content_hash: String,
    pub file_edit_revision: u64,
    /// Where the candidate was last seen on disk. Not identity — the hash is —
    /// so a moved folder keeps reading the same row under its unchanged hash.
    pub candidate_key: crate::CandidateKey,
    pub verdict: bae_identify::TerminalVerdict,
    /// The settled signals the run reached this verdict on: the disc ID, the
    /// barcodes, the classified text, and what every audio unit plays for.
    /// Stored beside the verdict so the pane and the queue read them back
    /// instead of extracting them again.
    pub signals: bae_identify::signals::Signals,
    /// The release this verdict picked unattended and the draft read from
    /// it, or `None` where it picked none. Only a run that settled on one
    /// release picks one, and the pick replaces whatever link and draft
    /// stand.
    pub pick: Option<VerdictPick>,
    /// The catalog releases the run read, and the pick's as its settle read
    /// them: each one the verdict's rows or the pick name is stored in the
    /// verdict's write.
    pub releases: Vec<bae_catalogs::source_release::SourceRelease>,
}

/// The release an identification run picked unattended, and the draft it
/// read from it.
#[derive(Debug, Clone)]
pub struct VerdictPick {
    pub link: bae_identify::PressingLink,
    pub metadata: crate::CandidateMetadataDraft,
}

/// What identification concluded about one candidate. Present as a whole or
/// absent as a whole: it is one verdict row with the match rows hanging off
/// it, written or cleared in one transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct DbCandidateIdentifyResult {
    pub verdict: bae_identify::TerminalVerdict,
    /// The person kept their own draft over what the verdict offered: none
    /// of its releases is the folder's, or it found none. A decision about
    /// this verdict, so the next one starts without it.
    pub kept_own_draft: bool,
}

#[cfg(test)]
#[path = "verdict_rows_tests.rs"]
mod tests;

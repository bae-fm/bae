//! Reading a candidate back: its row, its verdict with the matches that hang
//! off it, and the rows of other parts of its state joined onto it — its
//! file decisions, signals, draft provenance, release link and lookup
//! choices.

use super::file_decision_rows::{read_file_edits, FILE_EDITS_COLUMNS};
use super::lookup_choice_rows::LOOKUP_CHOICES_COLUMNS;
use super::match_rows::{
    match_entries_columns, match_of, read_match_entries, read_match_row, MatchColumns,
    StoredMatchEntries, StoredMatches, MATCH_COLUMNS,
};
use super::pane_rows::{read_provenance, StoredProvenance, PROVENANCE_COLUMNS};
use super::release_link_rows::{read_release_link, StoredReleaseLink, RELEASE_LINK_COLUMNS};
use super::verdict_rows::{identification_of, read_verdict_row, verdict_columns, VerdictRow};
use super::*;
use crate::tables::FinishRead;

/// One candidate's own columns and the rows joined one-to-one onto it, as
/// SQLite hands them over: everything of the candidate but its matches.
struct StoredCandidate {
    content_hash: String,
    candidate_key: crate::CandidateKey,
    metadata_revision: i64,
    author: Option<String>,
    file_edits: super::file_decision_rows::StoredFileEdits,
    signals: Option<super::signal_rows::StoredSignals>,
    provenance: Option<StoredProvenance>,
    link: Option<StoredReleaseLink>,
    lookup_choices: Option<super::lookup_choice_rows::StoredLookupChoices>,
    verdict: Option<VerdictRow>,
    matches: StoredMatches,
}

/// One row of [`load_states_rows_on`]'s read: the candidate's columns on the
/// first row of each candidate, and the match the row joins, if any.
struct StateReadRow {
    candidate: Option<Result<StoredCandidate, DbError>>,
    found: Option<Result<(MatchColumns, StoredMatchEntries), DbError>>,
}

/// The candidate `content_hash` as the first of its rows reads it.
fn read_candidate(row: &Row<'_>, content_hash: String) -> Result<StoredCandidate, DbError> {
    Ok(StoredCandidate {
        content_hash,
        candidate_key: row.get("candidate_key")?,
        metadata_revision: row.get("metadata_revision")?,
        author: row.get("draft_author")?,
        file_edits: read_file_edits(row)?,
        signals: super::signal_rows::read_signals(row)?,
        provenance: read_provenance(row)?,
        link: read_release_link(row)?,
        lookup_choices: super::lookup_choice_rows::read_lookup_choices(row)?,
        verdict: read_verdict_row(row)?,
        matches: StoredMatches::default(),
    })
}

/// The match a row joins, if it joins one.
fn read_found(row: &Row<'_>) -> Result<Option<(MatchColumns, StoredMatchEntries)>, DbError> {
    if row.get::<_, Option<i64>>("position")?.is_none() {
        return Ok(None);
    }
    Ok(Some((read_match_row(row)?, read_match_entries(row)?)))
}

/// Every stored candidate row, or the one `only` names, assembled with the
/// verdict, signals, provenance, and file-decision rows that hang off it.
pub(crate) fn load_states_on(
    sql: &(impl bae_db::client::query::QueryOne + bae_db::client::query::QueryRows),
    only: CandidateRows<'_>,
) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
    load_states_rows_on(sql, only)?()
}

/// The candidates `only` names in one read: each state row with the rows
/// that hang off it one-to-one joined onto it once, then joined to its
/// matches in position order, one row per match.
pub(crate) fn load_states_rows_on(
    sql: &(impl bae_db::client::query::QueryOne + bae_db::client::query::QueryRows),
    only: CandidateRows<'_>,
) -> Result<FinishRead<HashMap<String, DbImportCandidateState>>, DbError> {
    let filter = only.filter("state.content_hash");
    let mut previous: Option<String> = None;
    let rows = sql.query(
        &format!(
            "WITH candidate AS MATERIALIZED ( \
                 SELECT state.content_hash, state.candidate_key, state.metadata_revision, \
                        edit.author AS draft_author, {FILE_EDITS_COLUMNS}, {signals}, \
                        {PROVENANCE_COLUMNS}, {RELEASE_LINK_COLUMNS}, {LOOKUP_CHOICES_COLUMNS}, \
                        {verdict} \
                 FROM import_candidate_state AS state \
                 LEFT JOIN import_candidate_edit AS edit \
                     ON edit.content_hash = state.content_hash \
                 LEFT JOIN import_candidate_signals AS signals \
                     ON signals.content_hash = state.content_hash \
                 LEFT JOIN import_candidate_draft_provenance AS provenance \
                     ON provenance.content_hash = state.content_hash \
                 LEFT JOIN import_candidate_release_link AS link \
                     ON link.content_hash = state.content_hash \
                 LEFT JOIN import_candidate_lookup_choices AS choices \
                     ON choices.content_hash = state.content_hash \
                 LEFT JOIN import_candidate_verdict AS verdict \
                     ON verdict.content_hash = state.content_hash \
                 {clause}) \
             SELECT candidate.*, {MATCH_COLUMNS}, {entries} \
             FROM candidate \
             LEFT JOIN import_candidate_match AS found \
                 ON found.content_hash = candidate.content_hash \
             LEFT JOIN source_release AS fetched \
                 ON fetched.catalog = found.source AND fetched.release_id = found.release_id \
             ORDER BY candidate.content_hash, found.position",
            signals = super::signal_rows::signals_read_columns(),
            verdict = verdict_columns(),
            entries = match_entries_columns(),
            clause = filter.clause,
        ),
        filter.params().as_slice(),
        |row| {
            let content_hash: String = row.get("content_hash")?;
            let candidate = if previous.as_deref() == Some(content_hash.as_str()) {
                None
            } else {
                previous = Some(content_hash.clone());
                Some(read_candidate(row, content_hash))
            };
            Ok(StateReadRow { candidate, found: read_found(row).transpose() })
        },
    )?;
    Ok(Box::new(move || {
        let mut candidates: Vec<StoredCandidate> = Vec::new();
        for row in rows {
            if let Some(candidate) = row.candidate {
                candidates.push(candidate?);
            }
            let Some(found) = row.found else {
                continue;
            };
            let (columns, entries) = found?;
            let candidate =
                candidates.last_mut().expect("a match row follows its candidate's first row");
            candidate.matches.rows.push(match_of(columns, entries.entries()?)?);
        }
        candidates
            .into_iter()
            .map(|candidate| Ok((candidate.content_hash.clone(), candidate.state()?)))
            .collect()
    }))
}

impl StoredCandidate {
    fn state(self) -> Result<DbImportCandidateState, DbError> {
        let Self {
            content_hash,
            candidate_key,
            metadata_revision,
            author,
            file_edits,
            signals,
            provenance,
            link,
            lookup_choices,
            verdict,
            matches,
        } = self;
        let metadata_revision =
            to_u64(metadata_revision, "import_candidate_state.metadata_revision")?;
        // With no draft row a candidate holds its files' blank draft, which
        // nobody wrote; an imported candidate holds none, and nothing reads
        // its author.
        let author = author
            .as_deref()
            .map(super::pane_rows::author_of)
            .transpose()?
            .unwrap_or(crate::MetadataAuthor::Nobody);
        let provenance = provenance.map(StoredProvenance::provenance).transpose()?;
        author
            .check_provenance(provenance.as_ref())
            .map_err(|error| DbError::Message(format!("candidate {content_hash}: {error}")))?;
        let identify = match verdict {
            Some(verdict) => Some(identification_of(verdict, matches)?),
            // The match rows hang off the verdict row, so a candidate
            // without one has none.
            None if matches.rows.is_empty() => None,
            None => {
                return Err(DbError::Message(format!(
                    "candidate {content_hash} holds matches but no verdict"
                )))
            }
        };
        Ok(DbImportCandidateState {
            signals: signals.map(|stored| stored.signals()).transpose()?,
            lookup_choices: lookup_choices
                .map(|stored| stored.choices())
                .transpose()?
                .unwrap_or_default(),
            identify,
            metadata_author: author,
            metadata_provenance: provenance,
            release_link: link.map(StoredReleaseLink::link).transpose()?,
            file_edits: file_edits.edits()?,
            content_hash,
            candidate_key,
            metadata_revision,
        })
    }
}

/// One loaded candidate, as a read of import's candidate state returns it:
/// the `import_candidate_state` row with the verdict, signals, provenance, and
/// file decisions that hang off it.
#[derive(Debug, Clone, PartialEq)]
pub struct DbImportCandidateState {
    pub content_hash: String,
    pub candidate_key: crate::CandidateKey,
    /// What identification concluded, or `None` when nothing has identified
    /// this candidate yet — including when a file decision cleared what had,
    /// because that verdict described a folder shape that no longer applies.
    pub identify: Option<DbCandidateIdentifyResult>,
    /// The signals identification settled on, or `None` when nothing has
    /// extracted them.
    pub signals: Option<bae_identify::signals::Signals>,
    /// What a person decided this candidate's identification asks about. Every
    /// run of it reads this at its start; the default is what a candidate
    /// nobody has decided anything about runs with.
    pub lookup_choices: bae_identify::LookupChoices,
    /// The user's decisions about this candidate's files: which audio each
    /// track sheet describes, and which files are the release's tracks.
    pub file_edits: crate::folder_scanner::CandidateFileEdits,
    /// What the draft's values were read from, or `None` for a draft typed
    /// in, cleared, or blank.
    pub metadata_provenance: Option<crate::MetadataProvenance>,
    /// What the candidate is linked to in the catalogs, or `None` while it is
    /// linked to none. It names a release, not a shape, so file decisions
    /// and draft edits leave it standing.
    pub release_link: Option<crate::ReleaseLink>,
    /// Who wrote the draft: nobody, discovery's tag seed, identification's
    /// pick, or a person. Stored on the draft, since a person's draft may be
    /// read from nowhere.
    pub metadata_author: crate::MetadataAuthor,
    /// Revision of the editable metadata group. Every draft, artist, track, or
    /// cover mutation advances it so a source projection cannot overwrite a
    /// newer edit.
    pub metadata_revision: u64,
}

#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;

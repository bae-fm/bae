//! Reading a candidate back: its row, its verdict with the matches that hang
//! off it, and the rows of other parts of its state joined onto it — its
//! file decisions, signals, draft provenance, release link and lookup
//! choices.

use super::candidate_rows::CandidateFilter;
use super::file_decision_rows::{read_file_edits, FILE_EDITS_COLUMNS};
use super::lookup_choice_rows::LOOKUP_CHOICES_COLUMNS;
use super::match_record::{match_record_sql, match_rows_of};
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
    /// The candidate's matches in position order, a JSON array.
    matches: String,
}

/// The candidate one row of [`load_states_rows_on`]'s read holds.
fn read_candidate(row: &Row<'_>) -> Result<StoredCandidate, DbError> {
    Ok(StoredCandidate {
        content_hash: row.get("content_hash")?,
        candidate_key: row.get("candidate_key")?,
        metadata_revision: row.get("metadata_revision")?,
        author: row.get("draft_author")?,
        file_edits: read_file_edits(row)?,
        signals: super::signal_rows::read_signals(row)?,
        provenance: read_provenance(row)?,
        link: read_release_link(row)?,
        lookup_choices: super::lookup_choice_rows::read_lookup_choices(row)?,
        verdict: read_verdict_row(row)?,
        matches: row.get("candidate_matches")?,
    })
}

/// Every stored candidate row, or the one `only` names, assembled with the
/// verdict, signals, provenance, and file-decision rows that share its
/// content hash.
pub(crate) fn load_states_on(
    sql: &impl coven::SqlRead,
    only: CandidateRows<'_>,
) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
    load_states_rows_on(sql, only)?()
}

/// The candidates `only` names in one read: each state row with the
/// one-to-one rows that share its content hash joined onto it, and its
/// matches, in position order, nested in it as one JSON array.
pub(crate) fn load_states_rows_on(
    sql: &impl coven::SqlRead,
    only: CandidateRows<'_>,
) -> Result<FinishRead<HashMap<String, DbImportCandidateState>>, DbError> {
    states_rows_where(sql, only.filter("state.content_hash"))
}

/// The state of the candidate `content_hash` when its edit revision equals
/// the file-edit revision the scan holds for the release `key`, in the same
/// read; `None` when another revision wrote it.
pub(crate) fn load_current_state_on(
    sql: &impl coven::SqlRead,
    content_hash: &str,
    key: &crate::CandidateKey,
) -> Result<Option<DbImportCandidateState>, DbError> {
    Ok(states_rows_where(sql, CandidateFilter::current_at_scan(content_hash, key))?()?
        .remove(content_hash))
}

/// The states `filter` keeps, each with the rows that share its content hash.
fn states_rows_where(
    sql: &impl coven::SqlRead,
    filter: CandidateFilter,
) -> Result<FinishRead<HashMap<String, DbImportCandidateState>>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT state.content_hash, state.candidate_key, state.metadata_revision, \
                    edit.author AS draft_author, {FILE_EDITS_COLUMNS}, {signals}, \
                    {PROVENANCE_COLUMNS}, {RELEASE_LINK_COLUMNS}, {LOOKUP_CHOICES_COLUMNS}, \
                    {verdict}, \
                    (SELECT json_group_array({record} ORDER BY found.position) \
                     FROM import_candidate_match AS found \
                     WHERE found.content_hash = state.content_hash) AS candidate_matches \
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
             {clause}",
            signals = super::signal_rows::signals_read_columns().select(),
            verdict = verdict_columns().select(),
            record = match_record_sql("found"),
            clause = filter.clause,
        ),
        filter.params().as_slice(),
        |row| Ok(read_candidate(row)),
    )?;
    Ok(Box::new(move || {
        rows.into_iter()
            .map(|candidate| {
                let candidate = candidate?;
                Ok((candidate.content_hash.clone(), candidate.state()?))
            })
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
        author.check_provenance(provenance.as_ref()).map_err(|_refused| {
            DbError::unreadable(
                format!("candidate {content_hash}'s draft provenance"),
                (&author, &provenance),
            )
        })?;
        let identify = match verdict {
            Some(verdict) => {
                Some(identification_of(verdict, match_rows_of(&content_hash, &matches)?)?)
            }
            // The match rows hang off the verdict row, so a candidate
            // without one has none.
            None => match match_rows_of(&content_hash, &matches)?.rows.len() {
                0 => None,
                held => {
                    return Err(DbError::unreadable(
                        format!("the matches of candidate {content_hash}, which holds no verdict"),
                        held,
                    ))
                }
            },
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

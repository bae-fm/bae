//! Reading a candidate back: its row, its verdict with the matches that hang
//! off it, and the rows of other parts of its state joined onto it — its
//! file decisions, signals, draft provenance, release link and lookup
//! choices.

use super::match_record::match_rows_of;
use super::pane_rows::StoredProvenance;
use super::release_link_rows::StoredReleaseLink;
use super::state_fragments::{CurrentState, SignalTextPool};
use super::verdict_rows::{identification_of, VerdictRow};
use super::*;
use crate::tables::FinishRead;
use bae_identify::signals::PaneSignals;

/// One candidate's own columns and the rows joined one-to-one onto it, as
/// SQLite hands them over: everything of the candidate but its matches.
struct StoredCandidate<Signals> {
    content_hash: String,
    candidate_key: crate::CandidateKey,
    metadata_revision: i64,
    author: Option<String>,
    file_edits: super::file_decision_rows::FileEdits,
    signals: Option<Signals>,
    provenance: Option<StoredProvenance>,
    link: Option<StoredReleaseLink>,
    lookup_choices: Option<super::lookup_choice_rows::LookupChoiceRow>,
    verdict: Option<VerdictRow>,
    /// The candidate's matches in position order, a JSON array.
    matches: String,
}

coven::read!(
    writer StatesOf(candidate: String),
    r#"
    WITH owners AS (
        SELECT stored.content_hash AS content_hash, NULL AS identify_run
        FROM import_candidate_state AS stored
        WHERE stored.content_hash = :candidate
    )
    SELECT state.*, lines.*
    FROM @crate::tables::import_state::state_fragments::CurrentState(
        owner: (SELECT * FROM owners)) AS state
    LEFT JOIN @crate::tables::import_state::state_fragments::SignalTextPool(
        owner: (SELECT * FROM owners)) AS lines ON lines.owner_row = state.owner_row
"#
);

#[cfg(any(test, feature = "test-utils"))]
coven::read!(
    writer EveryState,
    r#"
    WITH owners AS (
        SELECT stored.content_hash AS content_hash, NULL AS identify_run
        FROM import_candidate_state AS stored
    )
    SELECT state.*, lines.*
    FROM @crate::tables::import_state::state_fragments::CurrentState(
        owner: (SELECT * FROM owners)) AS state
    LEFT JOIN @crate::tables::import_state::state_fragments::SignalTextPool(
        owner: (SELECT * FROM owners)) AS lines ON lines.owner_row = state.owner_row
"#
);

/// Every candidate as a read of every state returned it, by content hash.
#[cfg(test)]
pub(crate) fn every_state_of(
    rows: Vec<EveryStateRow>,
) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
    rows.into_iter()
        .map(|row| {
            let state = full_state_of(row.state, row.lines)?;
            Ok((state.content_hash.clone(), state))
        })
        .collect()
}

/// The candidates `only` names in one read: each state row with the
/// one-to-one rows that share its content hash joined onto it, and its
/// matches, in position order, nested in it as one JSON array.
pub(crate) fn load_states_rows_on(
    sql: &impl bae_store::WriterRead,
    only: CandidateRows<'_>,
) -> Result<FinishRead<HashMap<String, DbImportCandidateState>>, DbError> {
    let rows: Vec<(CurrentState, Option<SignalTextPool>)> = match only {
        #[cfg(any(test, feature = "test-utils"))]
        CandidateRows::All => statement_rows(sql, &EveryStateParams {})?
            .into_iter()
            .map(|row| (row.state, row.lines))
            .collect(),
        CandidateRows::One(OneCandidate::Hash(hash)) => {
            let request = StatesOfParams { candidate: hash.to_string() };
            statement_rows(sql, &request)?.into_iter().map(|row| (row.state, row.lines)).collect()
        }
    };
    Ok(Box::new(move || {
        rows.into_iter()
            .map(|(state, lines)| {
                let state = full_state_of(state, lines)?;
                Ok((state.content_hash.clone(), state))
            })
            .collect()
    }))
}

/// The candidate a read of [`CurrentState`] returned, as stored: its own
/// columns, the rows joined onto it and its matches.
fn stored_candidate_of<Signals>(
    state: CurrentState,
    signals: Option<Signals>,
) -> Result<StoredCandidate<Signals>, DbError> {
    let own = super::required(state.state_row.clone(), "import_candidate_state")?;
    Ok(StoredCandidate {
        content_hash: own.content_hash.clone(),
        candidate_key: own.candidate_key.clone(),
        metadata_revision: own.metadata_revision,
        author: own.draft_author.clone(),
        file_edits: own.file_edits.clone(),
        signals,
        provenance: StoredProvenance::of(&state)?,
        link: StoredReleaseLink::of(&state)?,
        lookup_choices: state.choices.clone(),
        verdict: super::verdict_rows::verdict_row_of_state(&state)?,
        matches: state.candidate_matches,
    })
}

/// The current state a pane's read selects, as
/// [`super::state_fragments::CurrentState`] names it.
pub(crate) fn current_state_of(
    state: CurrentState,
) -> Result<DbImportCandidateState<PaneSignals>, DbError> {
    let signals = super::signal_rows::StoredPaneSignals::of(state.signals.as_ref());
    stored_candidate_of(state, signals)?.state(super::signal_rows::StoredPaneSignals::signals)
}

/// The state of a candidate with the signals' text lines, which `lines`
/// holds, as the runs of the candidate start from them.
pub(super) fn full_state_of(
    state: CurrentState,
    lines: Option<SignalTextPool>,
) -> Result<DbImportCandidateState, DbError> {
    let text_pool = lines.map(|lines| lines.signals_text_pool);
    let signals = super::signal_rows::StoredPaneSignals::of(state.signals.as_ref())
        .map(|pane| {
            Ok::<_, DbError>(super::signal_rows::StoredSignals::of(
                pane,
                super::required(text_pool, "the signals' text lines")?,
            ))
        })
        .transpose()?;
    stored_candidate_of(state, signals)?.state(super::signal_rows::StoredSignals::signals)
}

impl<Stored> StoredCandidate<Stored> {
    fn state<Signals>(
        self,
        to_signals: fn(Stored) -> Result<Signals, DbError>,
    ) -> Result<DbImportCandidateState<Signals>, DbError> {
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
            #[expect(
                clippy::read_aggregate,
                reason = "refuses a candidate with match rows but no verdict row they hang off: checks what the statement returned for damage, and no screen shows it"
            )]
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
            signals: signals.map(to_signals).transpose()?,
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

/// The identify state that bae builds from the verdict that `state`, a
/// candidate's current state, holds, with `cards`, the cards the verdict's
/// rows hold; `None` when the state holds no verdict. The pane's read and
/// the candidate's runtime read both show a stored verdict through this.
pub(crate) fn stored_verdict_view<Signals>(
    state: Option<&DbImportCandidateState<Signals>>,
    cards: bae_identify::VerdictCards,
) -> Option<bae_identify::IdentifyStateView> {
    let identify = state?.identify.as_ref()?;
    Some(bae_identify::IdentifyStateView::of(
        bae_identify::RunOutcome::Answered(identify.verdict.clone()),
        cards,
    ))
}

/// One loaded candidate, as a read of import's candidate state returns it:
/// the `import_candidate_state` row with the verdict, signals, provenance, and
/// file decisions that hang off it.
#[derive(Debug, Clone, PartialEq)]
pub struct DbImportCandidateState<Signals = bae_identify::signals::Signals> {
    pub content_hash: String,
    pub candidate_key: crate::CandidateKey,
    /// What identification concluded, or `None` when nothing has identified
    /// this candidate yet — including when a file decision cleared what had,
    /// because that verdict described a folder shape that no longer applies.
    pub identify: Option<DbCandidateIdentifyResult>,
    /// The signals identification settled on, or `None` when nothing has
    /// extracted them.
    pub signals: Option<Signals>,
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

//! The stored verdict of a candidate's current state, read as the pane reads
//! it: the current state and the cards its verdict holds, from the fragments
//! the pane spreads.

use super::card_fragments::VerdictCards;
use super::state_fragments::CurrentState;
use super::DbImportCandidateState;
use bae_identify::signals::PaneSignals;
use bae_store::{statement_row, DbError};

sqlive_compose::read!(
    writer StoredVerdict(key: crate::CandidateKey),
    r#"
    SELECT state.*, cards.*
    FROM (SELECT :key AS key) AS stored
    LEFT JOIN @crate::tables::import_state::state_fragments::CurrentState(
        owner: @crate::tables::import_state::current_owner::CurrentOwner(key: :key)) AS state
        ON 1
    LEFT JOIN @crate::tables::import_state::card_fragments::VerdictCards(
        owner: @crate::tables::import_state::current_owner::CurrentOwner(key: :key)) AS cards
        ON 1
"#
);

/// The current state of the release the scan stores at `key`, and the cards of
/// its verdict, in one read; no state when another file-edit revision wrote
/// it, and no cards unless its verdict holds findings.
pub(crate) fn load_current_state_and_cards_on(
    sql: &impl bae_store::WriterRead,
    key: &crate::CandidateKey,
) -> Result<(Option<DbImportCandidateState<PaneSignals>>, bae_identify::VerdictCards), DbError> {
    let request = StoredVerdictParams { key: key.clone() };
    let row = statement_row(sql, &request)?;
    current_state_and_cards_of(row.state, row.cards)
}

/// The state and cards that a read of [`CurrentState`] and [`VerdictCards`]
/// returned, as the types the pane and the runtime show them in.
pub(crate) fn current_state_and_cards_of(
    state: Option<CurrentState>,
    cards: Option<VerdictCards>,
) -> Result<(Option<DbImportCandidateState<PaneSignals>>, bae_identify::VerdictCards), DbError> {
    Ok((
        state.map(super::current_state_of).transpose()?,
        super::verdict_cards_of_column(cards.and_then(|cards| cards.verdict_cards).as_deref())?,
    ))
}

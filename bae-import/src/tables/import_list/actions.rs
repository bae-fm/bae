//! This module decides in SQL which commands a release offers, from its
//! placement row and what is running for it: one column per command, named
//! `can_` and the command, `1` when the release offers it. The window's rows,
//! the pane, the selected rows and the selection's offers all read these
//! columns, so every surface offers the same commands.
//!
//! The columns follow [`CandidateAction::ALL`]. They name the placement as
//! `p` and the candidate's standing (what is running for it) as `st`, which a
//! release with nothing running leaves `NULL`.

use super::*;
use crate::CandidateAction;

/// The condition under which a release offers `action`, over the aliases `p`
/// and `st`.
///
/// While an import runs for the release, the release offers only Cancel, not
/// even Skip. Once the import starts writing the release, it offers nothing,
/// because the write finishes whatever the person asks. While an identify run
/// is in progress, the release offers only Cancel and Skip, and Import once
/// the run reads its pick, until the person asks for the import. bae does not
/// offer identifying again after a finished answer, which would fail the
/// same way, and the list shows a stored lookup as it stood, so only a
/// release with no stored lookup offers Identify. Regrouping comes before any
/// of that: a release that is imported, or has work running for it, offers
/// neither Separate nor Combine; any other release offers Separate when a
/// grouping reads it, even one that cannot be worked on as it stands, and
/// Combine when the person can act on it. Every release offers to reveal its
/// folders.
pub(super) fn offers(action: CandidateAction) -> String {
    use CandidateAction as A;
    let candidate = "p.item = 'candidate'";
    let importing = "st.import_step IS NOT NULL";
    let identifying =
        "COALESCE(st.identification IN ('queued', 'running', 'finalizing', 'reading_pick'), 0)";
    let settled = format!("(p.tab = 'done' OR {importing} OR {identifying})");
    let eligible = format!("{candidate} AND p.actionable AND NOT ({importing})");
    let found = format!("{eligible} AND p.tab = 'pending'");
    let expression = match action {
        A::Import => format!(
            "{found} AND (({identifying} AND st.identification = 'reading_pick' \
                           AND NOT st.import_asked) \
                          OR (NOT {identifying} AND p.draft_valid))"
        ),
        A::CancelImport => format!(
            "{candidate} AND p.actionable \
             AND st.import_step IN ('queued', 'validating_source_files', \
                                    'fetching_artist_pictures', 'reading_files', \
                                    'measuring_loudness')"
        ),
        A::Identify => format!(
            "{found} AND NOT {identifying} AND p.lookup IS NULL \
             AND COALESCE(st.identification <> 'inapplicable', 1)"
        ),
        A::CancelIdentification => format!("{found} AND {identifying}"),
        A::RetryIdentification => format!(
            "{found} AND NOT {identifying} \
             AND (p.lookup = 'failed' OR st.identification = 'not_written')"
        ),
        A::ResetToFileMetadata | A::ClearMetadata => format!("{found} AND NOT {identifying}"),
        A::Combine => format!("{candidate} AND NOT {settled} AND NOT p.separable AND p.actionable"),
        A::Separate => format!("{candidate} AND NOT {settled} AND p.separable"),
        A::Skip => format!("{found} AND NOT p.import_failed"),
        A::Restore => format!("{eligible} AND p.tab = 'skipped'"),
        A::RevealFolder => candidate.to_string(),
    };
    format!("COALESCE(({expression}), 0)")
}

/// The command columns, [`ACTION_COUNT`] of them in [`CandidateAction::ALL`]'s
/// order, for a statement that names the placement `p` and the standing `st`.
pub(super) fn action_columns() -> String {
    CandidateAction::ALL
        .iter()
        .map(|&action| format!("{} AS can_{action:?}", offers(action)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The commands the columns [`action_columns`] gave, in
/// [`CandidateAction::ALL`]'s order.
pub(super) fn read_actions(row: &Row<'_>) -> rusqlite::Result<Vec<CandidateAction>> {
    let mut offered = Vec::new();
    for action in CandidateAction::ALL {
        if row.get::<_, bool>(format!("can_{action:?}").as_str())? {
            offered.push(action);
        }
    }
    Ok(offered)
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;

//! What a release's commands are, decided in SQL from its placement row and
//! its standing: one column per command, `1` when the release offers it, named `can_` and the
//! command. The
//! window's rows, the pane, the selected rows and the selection's offers all
//! read these columns, so every surface offers the same commands.
//!
//! The columns follow [`CandidateAction::ALL`]. They name the placement as
//! `p` and the candidate's standing (what is running for it) as `st`, which a
//! release with nothing running leaves `NULL`.

use super::*;
use crate::CandidateAction;

/// The condition under which a release offers `action`, over the aliases `p`
/// and `st`.
///
/// An import owning the release leaves only cancelling it, not even a skip:
/// the attempt decides it now, and one writing its release completes whatever
/// is asked, so it offers no cancel either. A run for the release in flight
/// leaves only cancelling it and the skip, and importing once the run's pick
/// is read, until that is asked. Identifying again after a finished answer
/// that fails the same way is not offered, and a stored lookup is shown as it
/// stood, so only a release with none stored is offered identifying. How its
/// folders are read is decided before any of that: a release imported, or with
/// work running for it, is past regrouping; any other separates when a
/// grouping reads it, even one that cannot be worked on as it stands, and
/// combines when it can be acted on. Revealing its folders is always there.
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

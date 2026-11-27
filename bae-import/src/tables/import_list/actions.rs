//! The commands a release offers, as the `ListedActions` fragment
//! (`row_fragments`) decides them from its placement row and what is running
//! for it: one flag per command, named `can_` and the command. The window's
//! rows, the pane, the selected rows and the selection's offers all read that
//! fragment, so every surface offers the same commands.

use super::*;
use crate::CandidateAction;

/// The SQL reading the flag of `action` from the `ListedActions` document the
/// column `actions` holds: 1 when the release offers it.
pub(super) fn flag_sql(actions: &str, action: CandidateAction) -> String {
    format!("json_extract({actions}, '$.can_{action:?}')")
}

/// The commands a row offers, of [`CandidateAction::ROW`], as the
/// `ListedActions` document `json` flags them.
pub(super) fn offered(json: &str) -> Result<Vec<CandidateAction>, DbError> {
    let flags: std::collections::HashMap<String, i64> = serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json("a row's commands", json, error))?;
    CandidateAction::ROW
        .into_iter()
        .filter_map(|action| match flags.get(&format!("can_{action:?}")) {
            Some(flag) => (*flag != 0).then_some(Ok(action)),
            None => Some(Err(DbError::unreadable("a row's commands", json))),
        })
        .collect()
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;

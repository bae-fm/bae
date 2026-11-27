//! A listed release's row of the list's placement, read back as where the
//! list places it: what its commands are decided from and its state on Found.
//! The window's rows, the selection and the pane read it alike, so all of them
//! say the same.

use super::*;
use crate::{
    CandidateActionBasis, ListedRelease, NeedsYouReason, PendingStanding, TriagePlacement,
    TriageTab,
};
use bae_catalogs::InternalFailure;

/// The placement's columns of a listed release, as SQLite holds them: the
/// release's own are `NULL` on a folder that failed validation.
#[derive(Clone, PartialEq)]
pub(super) struct PlacementColumns {
    pub(super) tab: String,
    pub(super) import_failed: Option<bool>,
    pub(super) actionable: Option<bool>,
    pub(super) pending_state: Option<String>,
    pub(super) needs_you_reason: Option<String>,
    pub(super) reason_pressings: Option<i64>,
    pub(super) error_detail: Option<String>,
}

impl PlacementColumns {
    /// What the commands of the release at `key` are decided from. The
    /// table's checks hold every column a release needs, so one missing is a
    /// row the checks let through, which is refused here as broken.
    fn basis(&self, key: &crate::CandidateKey) -> Result<CandidateActionBasis, DbError> {
        let column = |column: Option<bool>| {
            column.ok_or_else(|| {
                DbError::unreadable(
                    format!("the release columns of import_list_placement of {key}"),
                    None::<bool>,
                )
            })
        };
        let placement = match (tab_of(&self.tab)?, column(self.import_failed)?) {
            (TriageTab::Pending, false) => TriagePlacement::Pending,
            (TriageTab::Pending, true) => TriagePlacement::Failed,
            (TriageTab::Done, _) => TriagePlacement::Done,
            (TriageTab::Skipped, _) => TriagePlacement::Skipped,
        };
        Ok(CandidateActionBasis { actionable: column(self.actionable)?, placement })
    }
}

/// Where a release's placement says it stands, with the commands its row
/// offers.
#[derive(Clone, PartialEq)]
pub(super) struct ListedColumns {
    placement: PlacementColumns,
    /// The commands the release offers.
    actions: Vec<crate::CandidateAction>,
}

impl ListedColumns {
    /// The placement's columns with the commands the row offers.
    pub(super) fn new(placement: PlacementColumns, actions: Vec<crate::CandidateAction>) -> Self {
        Self { placement, actions }
    }

    /// The columns of a pane's read, which
    /// [`super::fragments::Listed`] names.
    pub(super) fn of(listed: &super::fragments::Listed) -> Result<Self, DbError> {
        let actions = listed.actions.as_ref().ok_or_else(|| {
            DbError::unreadable("the commands of a listed release", None::<String>)
        })?;
        Ok(Self::new(
            PlacementColumns {
                tab: listed.tab.clone(),
                import_failed: listed.import_failed,
                actionable: listed.actionable,
                pending_state: listed.pending_state.clone(),
                needs_you_reason: listed.needs_you_reason.clone(),
                reason_pressings: listed.reason_pressings,
                error_detail: listed.error_detail.clone(),
            },
            super::actions::offered(&actions.actions)?,
        ))
    }

    /// Where the list places the release at `key`. The table's checks hold
    /// every column a state needs, so one missing is a row the checks let
    /// through, which is refused here as broken.
    pub(super) fn listed(self, key: &crate::CandidateKey) -> Result<ListedRelease, DbError> {
        let placement = &self.placement;
        let broken =
            |what: &str| DbError::unreadable(format!("import_list_placement of {key}"), what);
        let pressings = || {
            placement
                .reason_pressings
                .map(|count| to_u32(count, "a reason's pressing count"))
                .transpose()?
                .ok_or_else(|| broken("names no pressing count"))
        };
        let standing = match placement.pending_state.as_deref() {
            None => None,
            Some(state) => Some(match state {
                "not_looked_up" => PendingStanding::NotLookedUp,
                "identifying" => PendingStanding::Identifying,
                "identified" => PendingStanding::Identified,
                "unmatched" => PendingStanding::Unmatched,
                "lookup_error" => PendingStanding::LookupError,
                "importing" => PendingStanding::Importing,
                "import_error" => PendingStanding::ImportError,
                "error" => PendingStanding::Error {
                    failure: InternalFailure {
                        detail: placement
                            .error_detail
                            .clone()
                            .ok_or_else(|| broken("names no error"))?,
                    },
                },
                "needs_you" => PendingStanding::NeedsYou {
                    reason: match placement.needs_you_reason.as_deref() {
                        Some("matches") => NeedsYouReason::Matches { count: pressings()? },
                        Some("no_tracklist") => NeedsYouReason::NoTracklist,
                        Some("not_found") => NeedsYouReason::NotFound,
                        Some("nothing_to_look_up") => NeedsYouReason::NothingToLookUp,
                        Some(other) => {
                            return Err(DbError::unreadable(
                                "import_list_placement.needs_you_reason",
                                other,
                            ))
                        }
                        None => return Err(broken("needs the person for no reason")),
                    },
                },
                other => {
                    return Err(DbError::unreadable("import_list_placement.pending_state", other))
                }
            }),
        };
        Ok(ListedRelease { basis: placement.basis(key)?, actions: self.actions, standing })
    }
}

#[cfg(test)]
#[path = "listed_tests.rs"]
mod tests;

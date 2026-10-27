//! A listed release's row of the list's placement, read back as where the
//! list places it: what its commands are decided from and its state on Found.
//! The window's rows, the selection and the pane read it alike, so all of them
//! say the same.

use super::order::tab_of;
use super::*;
use crate::identify::MediumConflict;
use crate::import::{
    CandidateActionBasis, ListedRelease, NeedsYouReason, PendingStanding, StoredLookup,
    TriagePlacement, TriageTab,
};
use crate::signals::InternalFailure;

/// The placement columns a release's command basis is read from, over the
/// alias `p`, in the order [`BasisColumns::read`] reads them.
macro_rules! basis_columns {
    () => {
        "p.tab, p.import_failed, p.actionable, p.draft_valid, p.lookup, p.separable"
    };
}

pub(super) const BASIS_COLUMNS: &str = basis_columns!();

/// [`BASIS_COLUMNS`] and then where the release stands on Found, which
/// [`ListedColumns::read`] reads.
pub(super) const LISTED_COLUMNS: &str = concat!(
    basis_columns!(),
    ", p.pending_state, p.needs_you_reason, p.reason_pressings, p.reason_medium, ",
    "p.error_detail"
);

/// [`BASIS_COLUMNS`] as SQLite holds them: the release's own are `NULL` on a
/// folder that failed validation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct BasisColumns {
    tab: String,
    import_failed: Option<bool>,
    actionable: Option<bool>,
    draft_valid: Option<bool>,
    lookup: Option<String>,
    separable: Option<bool>,
}

impl BasisColumns {
    pub(super) fn read(row: &Row<'_>, first: usize) -> rusqlite::Result<Self> {
        Ok(Self {
            tab: row.get(first)?,
            import_failed: row.get(first + 1)?,
            actionable: row.get(first + 2)?,
            draft_valid: row.get(first + 3)?,
            lookup: row.get(first + 4)?,
            separable: row.get(first + 5)?,
        })
    }

    /// What the commands of the release at `key` are decided from. The
    /// table's checks hold every column a release needs, so one missing is a
    /// row the checks let through, which is refused here as broken.
    pub(super) fn basis(&self, key: &str) -> Result<CandidateActionBasis, DbError> {
        let column = |column: Option<bool>| {
            column
                .ok_or_else(|| DbError::Message(format!("the placement of {key} is no release's")))
        };
        let placement = match (tab_of(&self.tab)?, column(self.import_failed)?) {
            (TriageTab::Pending, false) => TriagePlacement::Pending,
            (TriageTab::Pending, true) => TriagePlacement::Failed,
            (TriageTab::Done, _) => TriagePlacement::Done,
            (TriageTab::Skipped, _) => TriagePlacement::Skipped,
        };
        let lookup = match self.lookup.as_deref() {
            None => None,
            Some("answered") => Some(StoredLookup::Answered),
            Some("failed") => Some(StoredLookup::Failed),
            Some(other) => return Err(unreadable("a stored lookup", other)),
        };
        Ok(CandidateActionBasis {
            actionable: column(self.actionable)?,
            placement,
            draft_valid: column(self.draft_valid)?,
            lookup,
            separable: column(self.separable)?,
        })
    }
}

/// [`LISTED_COLUMNS`] as SQLite holds them.
#[derive(Clone, PartialEq)]
pub(super) struct ListedColumns {
    basis: BasisColumns,
    pending_state: Option<String>,
    needs_you_reason: Option<String>,
    reason_pressings: Option<i64>,
    reason_medium: Option<String>,
    error_detail: Option<String>,
}

impl ListedColumns {
    /// The columns from `first` on, in [`LISTED_COLUMNS`]' order.
    pub(super) fn read(row: &Row<'_>, first: usize) -> rusqlite::Result<Self> {
        let state = first + 6;
        Ok(Self {
            basis: BasisColumns::read(row, first)?,
            pending_state: row.get(state)?,
            needs_you_reason: row.get(state + 1)?,
            reason_pressings: row.get(state + 2)?,
            reason_medium: row.get(state + 3)?,
            error_detail: row.get(state + 4)?,
        })
    }

    /// Where the list places the release at `key`. The table's checks hold
    /// every column a state needs, so one missing is a row the checks let
    /// through, which is refused here as broken.
    pub(super) fn listed(self, key: &str) -> Result<ListedRelease, DbError> {
        let broken = |what: &str| DbError::Message(format!("the placement of {key} {what}"));
        let pressings = || {
            self.reason_pressings
                .map(|count| to_u32(count, "a reason's pressing count"))
                .transpose()?
                .ok_or_else(|| broken("names no pressing count"))
        };
        let standing = match self.pending_state.as_deref() {
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
                        detail: self
                            .error_detail
                            .clone()
                            .ok_or_else(|| broken("names no error"))?,
                    },
                },
                "needs_you" => PendingStanding::NeedsYou {
                    reason: match self.needs_you_reason.as_deref() {
                        Some("matches") => NeedsYouReason::Matches {
                            count: pressings()?,
                        },
                        Some("no_tracklist") => NeedsYouReason::NoTracklist,
                        Some("medium_mismatch") => NeedsYouReason::MediumMismatch {
                            folder: match self.reason_medium.as_deref() {
                                Some("cd_rip") => MediumConflict::CdRip,
                                Some("not_cd_audio") => MediumConflict::NotCdAudio,
                                Some(other) => return Err(unreadable("a medium conflict", other)),
                                None => return Err(broken("names no medium")),
                            },
                            releases: pressings()?,
                        },
                        Some("not_found") => NeedsYouReason::NotFound,
                        Some("nothing_to_look_up") => NeedsYouReason::NothingToLookUp,
                        Some(other) => return Err(unreadable("a needs-you reason", other)),
                        None => return Err(broken("needs the person for no reason")),
                    },
                },
                other => return Err(unreadable("a Found state", other)),
            }),
        };
        Ok(ListedRelease {
            basis: self.basis.basis(key)?,
            standing,
        })
    }
}

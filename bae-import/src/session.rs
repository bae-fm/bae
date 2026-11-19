//! What the import pane shows of a candidate beyond its draft: where it opens,
//! and the failure of its last command.
//!
//! Where the pane stands while it is open (the draft or the Find online page,
//! the section that page shows, the text typed in its search form) exists only
//! on screen, so the app holds it. Core says where the pane opens, from the
//! candidate's rows, and stores the failure of the pane's last command, which
//! the pane states until its next one.

/// Which surface the pane's metadata slot shows: the candidate's draft, or the
/// Find online page a person opens to identify it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataPresentation {
    Draft,
    FindOnline,
}

impl MetadataPresentation {
    /// Where the pane opens on a candidate: Find online while identification
    /// runs for it or its answer waits on the person, the draft otherwise.
    /// An answer waits on the person while nothing drafted the candidate from
    /// anywhere (`answered` false), or while the release its run picked failed
    /// a check against the folder (`check_failed`).
    pub fn opening(
        identifying: bool,
        verdict_stored: bool,
        answered: bool,
        check_failed: bool,
    ) -> Self {
        if identifying || (verdict_stored && (!answered || check_failed)) {
            Self::FindOnline
        } else {
            Self::Draft
        }
    }
}

/// A command the pane runs for its candidate whose failure the pane states
/// until its next command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneCommand {
    Import,
    /// Read the files' own tags into the draft.
    ReadFileTags,
    /// Change which identifiers identification looks up.
    ChangeLookups,
    /// Change the words identification searches by.
    ChangeSearchWords,
    /// Change which catalog numbers count as the folder's own.
    ChangeAgreements,
    /// Keep the folder's own draft over what its lookup offered.
    KeepOwnDraft,
    /// Link the folder to the album its lookup's offered pressings are of,
    /// its pressing unknown.
    LinkSharedAlbum,
    /// Unlink the candidate from its release.
    Unlink,
}

/// The pane's last command, which failed, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneFailure {
    pub command: PaneCommand,
    pub error: bae_diagnostics::UiError,
}

/// What a pane command came to: it ran, or it failed and the pane states why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneOutcome {
    Done,
    Failed,
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;

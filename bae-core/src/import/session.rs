//! What the pane keeps per candidate between visits.
//!
//! A person works a candidate across several visits: they open Find online,
//! type half a query, click another candidate, come back. None of that is the
//! candidate's metadata — it is where the pane was — but it belongs to the
//! candidate rather than to the window, so it is stored with it and read back
//! with the rest of the detail. Only what has no meaning past the moment stays
//! in the view: which field has the keyboard, which popover is open.

use crate::import::MetadataProvenance;

/// Which surface the pane's metadata slot shows: the candidate's draft, or the
/// Find online page a person opens to identify it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataPresentation {
    Draft,
    FindOnline,
}

/// Which query the typed-search form is asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchTab {
    #[default]
    General,
    CatalogNumber,
    Barcode,
}

/// The typed-search form: which query it asks and what is typed into every
/// field, whichever tab is showing. What a submitted search turned up is not
/// here — that run lives on the candidate's runtime while it stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchForm {
    pub tab: SearchTab,
    pub artist: String,
    pub album: String,
    pub catalog: String,
    pub barcode: String,
}

/// A command the pane runs for its candidate whose failure the pane states
/// until its next command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneCommand {
    Import,
    CancelImport,
    /// Take the two library artists an import found to be one as one.
    MergeArtists,
    /// Read the files' own tags into the draft.
    ReadFileTags,
    /// Change which identifiers identification looks up.
    ChangeLookups,
    /// Change the words identification searches by.
    ChangeSearchWords,
    /// Change which catalog numbers count as the folder's own.
    ChangeAgreements,
}

/// The pane's last command, which failed, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneFailure {
    pub command: PaneCommand,
    pub error: crate::ui::UiError,
}

/// What a pane command came to: it ran, or it failed and the pane states why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneOutcome {
    Done,
    Failed,
}

/// Which section of the Find online page is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FindOnlineSection {
    /// Identification: the run's ledger and what it matched.
    #[default]
    Automatic,
    /// The typed search: its form and what it turned up.
    Search,
}

/// Every way the pane moves between the draft and the Find online page,
/// automatic or asked for. [`CandidateSession::moved`] is the one rule for
/// where each puts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneMove {
    /// Identification was admitted for the candidate: the page its run reports
    /// on.
    Admitted,
    /// A run applied its own pick and the release passed every check against
    /// the folder: only the draft and its Import are left to see.
    SettledOnPick,
    /// The person picked a source for the draft.
    Picked,
    /// The person asked for identification's results.
    Automatic,
    /// The person asked for the typed search.
    Search,
    /// The person opened one section of the Find online page.
    OpenSection(FindOnlineSection),
    /// The person asked for Find online without naming a section: it opens on
    /// the one last open.
    FindOnline,
    /// The person went back to the draft.
    Back,
}

/// The pane's per-candidate state between visits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateSession {
    pub presentation: MetadataPresentation,
    /// The section of the Find online page open when it shows, kept while the
    /// draft does.
    pub find_online_section: FindOnlineSection,
    pub search: SearchForm,
    /// The last command the pane ran for this candidate, when it failed —
    /// shown in the banner until the next command clears it.
    pub error: Option<PaneFailure>,
}

impl CandidateSession {
    /// The pane a candidate opens on before anyone has touched it: the draft,
    /// which is where the candidate's metadata is; Find online while
    /// identification has an answer nobody has acted on, since that is where
    /// the answer is.
    pub fn initial(provenance: Option<&MetadataProvenance>, has_verdict: bool) -> Self {
        let presentation = match (provenance, has_verdict) {
            (None, true) => MetadataPresentation::FindOnline,
            (Some(_), _) | (None, false) => MetadataPresentation::Draft,
        };
        Self {
            presentation,
            find_online_section: FindOnlineSection::Automatic,
            search: SearchForm::default(),
            error: None,
        }
    }

    /// This session after `pane_move`: the one rule for where the pane goes.
    pub fn moved(mut self, pane_move: PaneMove) -> Self {
        use FindOnlineSection as S;
        use MetadataPresentation as P;
        let (presentation, section) = match pane_move {
            PaneMove::Admitted | PaneMove::Automatic => (P::FindOnline, Some(S::Automatic)),
            PaneMove::Search => (P::FindOnline, Some(S::Search)),
            PaneMove::OpenSection(section) => (P::FindOnline, Some(section)),
            PaneMove::FindOnline => (P::FindOnline, None),
            PaneMove::SettledOnPick | PaneMove::Picked | PaneMove::Back => (P::Draft, None),
        };
        self.presentation = presentation;
        if let Some(section) = section {
            self.find_online_section = section;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::Catalog;

    /// A candidate nobody has touched opens on its draft — pre-filled from the
    /// folder's tags or blank, that is where its metadata is.
    #[test]
    fn a_fresh_pane_opens_on_the_draft() {
        assert_eq!(
            CandidateSession::initial(None, false).presentation,
            MetadataPresentation::Draft
        );
        assert_eq!(
            CandidateSession::initial(Some(&MetadataProvenance::FileMetadata), false).presentation,
            MetadataPresentation::Draft
        );
        let picked = MetadataProvenance::ExternalRelease {
            record: crate::import::MetadataRef::new(Catalog::MusicBrainz, "release".to_string()),
            partners: Vec::new(),
        };
        assert_eq!(
            CandidateSession::initial(Some(&picked), true).presentation,
            MetadataPresentation::Draft
        );
    }

    /// A verdict nobody has acted on — several matches, none, a failed run —
    /// opens on Find online: the answer, and the way to act on it, are there.
    #[test]
    fn an_unanswered_verdict_opens_on_find_online() {
        assert_eq!(
            CandidateSession::initial(None, true).presentation,
            MetadataPresentation::FindOnline
        );
    }

    /// Each move puts the pane where it says; a move to the draft keeps the
    /// section Find online opens on, and naming no section opens the last one.
    #[test]
    fn each_move_puts_the_pane_where_it_says() {
        let session = CandidateSession::initial(None, false);
        let searched = session.clone().moved(PaneMove::Search);
        assert_eq!(searched.presentation, MetadataPresentation::FindOnline);
        assert_eq!(searched.find_online_section, FindOnlineSection::Search);
        let back = searched.moved(PaneMove::Back);
        assert_eq!(back.presentation, MetadataPresentation::Draft);
        assert_eq!(back.find_online_section, FindOnlineSection::Search);
        let reopened = back.moved(PaneMove::FindOnline);
        assert_eq!(reopened.presentation, MetadataPresentation::FindOnline);
        assert_eq!(reopened.find_online_section, FindOnlineSection::Search);
        for pane_move in [PaneMove::Admitted, PaneMove::Automatic] {
            let moved = reopened.clone().moved(pane_move);
            assert_eq!(moved.presentation, MetadataPresentation::FindOnline);
            assert_eq!(moved.find_online_section, FindOnlineSection::Automatic);
        }
        for pane_move in [PaneMove::SettledOnPick, PaneMove::Picked, PaneMove::Back] {
            assert_eq!(
                reopened.clone().moved(pane_move).presentation,
                MetadataPresentation::Draft
            );
        }
    }
}

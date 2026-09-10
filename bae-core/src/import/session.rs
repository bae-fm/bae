//! What the pane keeps per candidate between visits.
//!
//! A person works a candidate across several visits: they open Find online,
//! type half a query, click another candidate, come back. None of that is the
//! candidate's metadata — it is where the pane was — but it belongs to the
//! candidate rather than to the window, so it is stored with it and read back
//! with the rest of the detail. Only what has no meaning past the moment stays
//! in the view: which field has the keyboard, which popover is open.

use crate::import::MetadataProvenance;

/// Which surface the pane's metadata slot shows: the draft, or one of the
/// browsers a person opens to fill it from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataPresentation {
    Draft,
    FindOnline,
    FileTags,
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

/// The pane's per-candidate state between visits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateSession {
    pub presentation: MetadataPresentation,
    pub search: SearchForm,
    /// The last command the pane ran for this candidate, when it failed —
    /// shown in the banner until the next command clears it.
    pub error: Option<String>,
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
            search: SearchForm::default(),
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::MetadataSource;

    /// A candidate nobody has touched opens on its draft — pre-filled from the
    /// folder's tags or blank, that is where its metadata is.
    #[test]
    fn a_fresh_pane_opens_on_the_draft() {
        assert_eq!(
            CandidateSession::initial(None, false).presentation,
            MetadataPresentation::Draft
        );
        assert_eq!(
            CandidateSession::initial(Some(&MetadataProvenance::FileTags), false).presentation,
            MetadataPresentation::Draft
        );
        let picked = MetadataProvenance::ExternalRelease {
            source: MetadataSource::MusicBrainz,
            release_id: "release".to_string(),
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
}

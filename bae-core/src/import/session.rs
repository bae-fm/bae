//! What the pane keeps per candidate between visits.
//!
//! A person works a candidate across several visits: they open Find online,
//! type half a query, click another candidate, come back. None of that is the
//! candidate's metadata — it is where the pane was — but it belongs to the
//! candidate rather than to the window, so it is stored with it and read back
//! with the rest of the detail. Only what has no meaning past the moment stays
//! in the view: which field has the keyboard, which popover is open.

use crate::config::DefaultImportMetadataSource;
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
    /// The pane a candidate opens on before anyone has touched it: the draft
    /// once metadata has been chosen, else the browser the default source
    /// names, with an empty form.
    pub fn initial(
        provenance: Option<&MetadataProvenance>,
        initial_source: DefaultImportMetadataSource,
    ) -> Self {
        let presentation = match (provenance, initial_source) {
            (Some(_), _) | (None, DefaultImportMetadataSource::None) => MetadataPresentation::Draft,
            (None, DefaultImportMetadataSource::FindOnline) => MetadataPresentation::FindOnline,
            (None, DefaultImportMetadataSource::FileTags) => MetadataPresentation::FileTags,
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

    /// A candidate nobody has touched opens on the browser its default source
    /// names, and on the draft once metadata has been chosen — whatever the
    /// default.
    #[test]
    fn a_fresh_pane_opens_where_the_default_source_points() {
        assert_eq!(
            CandidateSession::initial(None, DefaultImportMetadataSource::FindOnline).presentation,
            MetadataPresentation::FindOnline
        );
        assert_eq!(
            CandidateSession::initial(None, DefaultImportMetadataSource::FileTags).presentation,
            MetadataPresentation::FileTags
        );
        assert_eq!(
            CandidateSession::initial(None, DefaultImportMetadataSource::None).presentation,
            MetadataPresentation::Draft
        );
        let picked = MetadataProvenance::ExternalRelease {
            source: MetadataSource::MusicBrainz,
            release_id: "release".to_string(),
            partners: Vec::new(),
        };
        assert_eq!(
            CandidateSession::initial(Some(&picked), DefaultImportMetadataSource::FindOnline)
                .presentation,
            MetadataPresentation::Draft
        );
        assert_eq!(
            CandidateSession::initial(
                Some(&MetadataProvenance::FileTags),
                DefaultImportMetadataSource::FindOnline
            )
            .presentation,
            MetadataPresentation::Draft
        );
    }
}

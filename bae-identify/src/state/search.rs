//! The fourth step: the search by the candidate's title, asked when the
//! identifiers and the picked numbers name nothing.

use super::progress::{providers_failures, providers_results};
use super::{Effect, LookupResults, LookupState, ProviderLookup, SignalsContext};
use bae_catalogs::search::SourceFailure;

/// The album title and artist a run searches by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleSearch {
    pub album: String,
    /// The first album artist's name, or blank.
    pub artist: String,
}

impl TitleSearch {
    /// The search for these words; `None` when there is no title.
    pub fn of(album: &str, artist: &str) -> Option<Self> {
        let album = album.trim();
        (!album.is_empty())
            .then(|| Self { album: album.to_string(), artist: artist.trim().to_string() })
    }

    /// The search for a draft's own title, with trailing bracketed parts such
    /// as `[MR2002]` taken off, since catalogs match nothing with them — see
    /// `text_match::bare_album_title`.
    pub fn of_draft(album: &str, artist: &str) -> Option<Self> {
        Self::of(&bae_types::text_match::bare_album_title(album), artist)
    }
}

/// The title search's progress: the run's last step, decided once the three
/// identifiers and the ISRCs have settled.
#[derive(Clone, Debug, PartialEq)]
pub enum SearchProgress {
    /// Waiting for the three identifiers and the ISRCs.
    Pending,
    /// The identifiers answered, or there was nothing to search by.
    Skipped,
    /// One lookup per provider in the run.
    Lookups { providers: Vec<ProviderLookup> },
}

impl SearchProgress {
    pub fn is_settled(&self) -> bool {
        match self {
            SearchProgress::Pending => false,
            SearchProgress::Skipped => true,
            SearchProgress::Lookups { providers } => providers.iter().all(|l| l.state.is_settled()),
        }
    }

    /// What every provider that answered found, in provider order.
    pub fn results(&self) -> LookupResults {
        providers_results(self.lookups())
    }

    /// The providers that failed.
    pub fn failures(&self) -> Vec<SourceFailure> {
        providers_failures(self.lookups())
    }

    /// The lookups, in provider order.
    pub fn lookups(&self) -> &[ProviderLookup] {
        match self {
            SearchProgress::Lookups { providers } => providers,
            SearchProgress::Pending | SearchProgress::Skipped => &[],
        }
    }
}

/// Ask every provider the candidate's title, unless the identifiers and the
/// picked numbers already answered or there is no title. The ISRCs are no identifier here: a
/// recording is on every compilation that reissued it, and MusicBrainz lists
/// few of an album's codes, so what they return does not stand for the album
/// the way a disc ID, a barcode or a catalog number does — and Discogs, which
/// no ISRC reaches, is still asked the title.
pub(super) fn start_search_progress(
    context: &SignalsContext,
    effects: &mut Vec<Effect>,
) -> SearchProgress {
    let identifiers_answered = !context.disc.results().is_empty()
        || !context.barcode.results.is_empty()
        || !context.catalog.active_results().is_empty();
    let Some(query) = context.search.query.clone() else {
        return SearchProgress::Skipped;
    };
    if identifiers_answered || context.providers.is_empty() {
        return SearchProgress::Skipped;
    }
    SearchProgress::Lookups {
        providers: context
            .providers
            .iter()
            .map(|&source| {
                effects.push(Effect::SearchTitle { source, query: query.clone() });
                ProviderLookup { source, state: LookupState::LookingUp }
            })
            .collect(),
    }
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;

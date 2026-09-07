//! The typed search and what it turns up: the query, each provider's part of
//! it, and the album cards the results fold into.
//!
//! Split from the rest of a candidate's types because these are one subject —
//! what a person asked an external source and what came back — and because the
//! identify verdict renders from the same cards.

use super::super::*;

/// One source's record of one pressing, under a release-group card. The card
/// carries the album's title, artist, and cover, so this keeps only the
/// pressing-distinguishing fields the row renders plus what a surface needs to
/// commit it or watch its library membership.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMetadataResult {
    pub source: BridgeMetadataSource,
    pub release_id: String,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    /// The barcode this source prints for the pressing, where it prints one.
    pub barcode: Option<String>,
    /// The group this release belongs to on its own source — the other half of
    /// the key a library-membership check takes.
    pub source_group_id: Option<String>,
}

/// Search query — one of the three search modes, independent of the chosen provider.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSearchQuery {
    General { artist: String, album: String },
    CatalogNumber { catalog_number: String },
    Barcode { barcode: String },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeSearchQuery = bae_core::import::SearchQuery,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        General { artist, album },
        CatalogNumber { catalog_number },
        Barcode { barcode },
    },
}

/// An album, as one or both sources describe it, with the pressings they
/// surfaced for it. Mirrors `bae_core::import::release_group::ReleaseGroup` —
/// the grouping, the cross-source merge and the pressing pairing all happen in
/// core; the UI iterates and renders.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseGroup {
    /// Stable card identity (the first source's group id, or the lone
    /// release's id when no source named a group).
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    /// The label the card names beside the artist, where the album's pressings
    /// state one.
    pub label: Option<String>,
    /// Representative cover for the card.
    pub cover_art: Option<BridgeRemoteCover>,
    /// Every source carrying this group, MusicBrainz first.
    pub sources: Vec<BridgeReleaseGroupSource>,
    /// Earliest and latest pressing year for the UI's "1992 – 2012" span; both
    /// `None` when no pressing carries a year. Pressing count is `pressings.len()`.
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    pub pressings: Vec<BridgePressing>,
}

/// One source carrying a group, and its editorial page for it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeReleaseGroupSource {
    pub source: BridgeMetadataSource,
    /// Editorial URL for the group on this source (release-group on
    /// MusicBrainz, master on Discogs). `None` for an ungrouped result.
    pub group_url: Option<String>,
}

/// One physical pressing, on every source that lists it.
///
/// A row is picked whole. `releases` is what the row shows — its year, label,
/// catalogue number, and the name of every source carrying it — and the extra
/// entries beyond the first are labels, not separate picks. `pick` is what
/// picking the row means, decided in core: the release the draft is read from
/// plus every other source's record of the same pressing.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePressing {
    pub releases: Vec<BridgeMetadataResult>,
    pub pick: crate::types::BridgeMetadataProvenance,
}

/// One provider's part of a candidate's manual search. Mirrors
/// `bae_core::import::SourceSearch`; the results themselves reach a surface
/// through the search's `groups`, so a source reports only how many it found.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSourceSearch {
    /// Switched off: the person is not asking this source.
    Off,
    /// The source needs a credential this library does not hold, so it was
    /// never asked.
    NotConfigured,
    Searching,
    Done {
        count: u32,
    },
    Failed {
        failure: BridgeLookupFailure,
    },
}

#[cfg(feature = "desktop")]
impl BridgeSourceSearch {
    fn from_core(search: bae_core::import::SourceSearch) -> Self {
        use bae_core::import::SourceSearch;
        match search {
            SourceSearch::Off => Self::Off,
            SourceSearch::NotConfigured => Self::NotConfigured,
            SourceSearch::Searching => Self::Searching,
            SourceSearch::Done { results } => Self::Done {
                count: results.len() as u32,
            },
            SourceSearch::Failed(failure) => Self::Failed {
                failure: BridgeLookupFailure::from_core(failure),
            },
        }
    }
}

/// Where a candidate's typed search stands as a whole — the one glyph a
/// surface heads it with. Mirrors `bae_core::import::SearchStatus`; crossed
/// rather than folded per surface, so "still searching" means one thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSearchStatus {
    /// A source is still looking.
    Searching,
    /// Every source landed, none failed, and at least one named a release.
    Found,
    /// Every source landed, none failed, and a source that answered named no
    /// release.
    NoMatches,
    /// Every source landed and at least one failed, whatever the others
    /// found.
    Failed,
}

#[cfg(feature = "desktop")]
impl BridgeSearchStatus {
    fn from_core(status: bae_core::import::SearchStatus) -> Self {
        use bae_core::import::SearchStatus;
        match status {
            SearchStatus::Searching => Self::Searching,
            SearchStatus::Found => Self::Found,
            SearchStatus::NoMatches => Self::NoMatches,
            SearchStatus::Failed => Self::Failed,
        }
    }
}

/// One source's part of a candidate's typed search, as the surface that lists
/// them reads it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSourceSearchEntry {
    pub source: BridgeMetadataSource,
    pub state: BridgeSourceSearch,
}

/// A candidate's typed search as its sources land. Mirrors
/// `bae_core::import::CandidateSearch`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCandidateSearch {
    /// What was asked — the line the result area heads itself with.
    pub query: BridgeSearchQuery,
    /// Each source's part, one entry per metadata source in core's order. A
    /// surface iterates this; no source is the main one.
    pub sources: Vec<BridgeSourceSearchEntry>,
    /// Every settled source's results, folded into album cards.
    pub groups: Vec<BridgeReleaseGroup>,
    /// Library status per result, keyed by release id.
    pub library_statuses: std::collections::HashMap<String, BridgeLibraryStatus>,
    /// Where the search stands as a whole.
    pub status: BridgeSearchStatus,
}

#[cfg(feature = "desktop")]
impl BridgeCandidateSearch {
    pub(crate) fn from_core(search: bae_core::import::CandidateSearch) -> Self {
        let status = BridgeSearchStatus::from_core(search.status());
        let bae_core::import::CandidateSearch {
            query,
            sources,
            groups,
            library_statuses,
        } = search;
        Self {
            status,
            query: BridgeSearchQuery::from_core(query),
            sources: sources
                .into_iter()
                .map(|(source, state)| BridgeSourceSearchEntry {
                    source: BridgeMetadataSource::from_core(source),
                    state: BridgeSourceSearch::from_core(state),
                })
                .collect(),
            groups: groups
                .into_iter()
                .map(BridgeReleaseGroup::from_core)
                .collect(),
            library_statuses: library_statuses
                .into_iter()
                .map(|status| {
                    (
                        status.release_id.clone(),
                        BridgeLibraryStatus::from_core(status),
                    )
                })
                .collect(),
        }
    }
}

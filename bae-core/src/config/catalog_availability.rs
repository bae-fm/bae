//! Which catalogs this library asks, built from the stored preferences.

use bae_catalogs::catalog::Catalog;

/// Whether a source is asked when the sources are asked together, and when it
/// is not, why not. Core's answer, so no surface re-derives "on and reachable"
/// from a preference plus a key check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceAvailability {
    /// Asked.
    On,
    /// Switched off by the person.
    Off,
    /// The source needs a credential this library does not hold, so it cannot
    /// be asked whatever the preference says. The preference is kept
    /// underneath: supplying the credential restores the choice the person
    /// last made.
    NotConfigured,
}

impl SourceAvailability {
    pub fn is_on(self) -> bool {
        matches!(self, Self::On)
    }
}

/// One source and whether this library asks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogAvailability {
    pub catalog: Catalog,
    pub state: SourceAvailability,
}

/// The sources an availability list says to ask, in list order. What a run's
/// provider list and a search's dispatch are both a projection of.
pub fn asked_sources(sources: &[CatalogAvailability]) -> Vec<Catalog> {
    sources.iter().filter(|entry| entry.state.is_on()).map(|entry| entry.catalog).collect()
}

/// Whether `source` is the only one this library still asks, so switching it
/// off would leave nothing to ask.
///
/// The rule `LibraryManager::set_metadata_source_enabled` refuses on, and the
/// rule the bridge greys that source's switch out by — one function, so a
/// switch cannot move on exactly the writes core would turn down, rather than
/// on a surface's own guess at them.
pub fn is_the_only_asked_source(sources: &[CatalogAvailability], source: Catalog) -> bool {
    asked_sources(sources) == [source]
}

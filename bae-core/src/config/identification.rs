//! How identification runs, as `preferences.yaml`'s `identification` mapping
//! carries it: whether it starts on its own, what a run it started on its own
//! goes on to do, and which catalogs it asks.

use serde::{Deserialize, Serialize};

/// Everything a person decides about identification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentificationPreferences {
    /// Whether a release found while this is on is identified on its own.
    /// Changing it affects no candidate already found or queued. Defaults to
    /// `true`.
    pub automatic: bool,
    /// Whether a candidate an automatic run settles as auto-importable is imported
    /// straight away. Defaults to `false`, and is read only through
    /// [`Self::imports_when_identified`].
    pub import_when_identified: bool,
    /// Which catalogs identification asks — the automatic run, the typed
    /// search, and every retry.
    pub catalogs: LookupCatalogPreferences,
}

impl IdentificationPreferences {
    /// Whether an automatic run imports what it settles as auto-importable.
    pub fn imports_when_identified(&self) -> bool {
        self.automatic && self.import_when_identified
    }
}

impl Default for IdentificationPreferences {
    fn default() -> Self {
        Self {
            automatic: true,
            import_when_identified: false,
            catalogs: LookupCatalogPreferences::default(),
        }
    }
}

/// Which catalogs this library asks. One flag per
/// [`Catalog::LOOKUP`](crate::import::Catalog::LOOKUP) member, all on by
/// default.
///
/// The YAML mapping needs a name per catalog, so the fields are named — but
/// nothing reads them by name: [`Self::enabled`] and [`Self::set`] are total
/// over the asked catalogs, so adding one fails the build here rather than
/// silently defaulting. Neither is the main one; a catalog switched off is not
/// asked by anything that asks the catalogs together.
///
/// A flag stays as the person set it while the source is unreachable for
/// another reason (Discogs without a key), so supplying the key restores their
/// choice rather than turning the source on behind them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupCatalogPreferences {
    pub musicbrainz: bool,
    pub discogs: bool,
}

impl LookupCatalogPreferences {
    pub fn enabled(&self, catalog: crate::import::Catalog) -> bool {
        match catalog {
            crate::import::Catalog::MusicBrainz => self.musicbrainz,
            crate::import::Catalog::Discogs => self.discogs,
            other => unreachable!("{} answers no lookups", other.as_str()),
        }
    }

    pub fn set(&mut self, catalog: crate::import::Catalog, enabled: bool) {
        match catalog {
            crate::import::Catalog::MusicBrainz => self.musicbrainz = enabled,
            crate::import::Catalog::Discogs => self.discogs = enabled,
            other => unreachable!("{} answers no lookups", other.as_str()),
        }
    }
}

impl Default for LookupCatalogPreferences {
    fn default() -> Self {
        Self {
            musicbrainz: true,
            discogs: true,
        }
    }
}

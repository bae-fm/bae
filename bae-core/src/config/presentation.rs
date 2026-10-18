use super::Config;
use crate::import::{CatalogAvailability, SourceAvailability};

/// A catalog choice and whether changing it would leave a usable source.
pub struct LookupCatalogSetting {
    pub availability: CatalogAvailability,
    pub can_change: bool,
}

impl Config {
    pub fn lookup_catalog_settings(&self) -> Vec<LookupCatalogSetting> {
        let catalogs = self.metadata_sources();
        catalogs
            .iter()
            .map(|entry| LookupCatalogSetting {
                availability: *entry,
                can_change: entry.state != SourceAvailability::NotConfigured
                    && !crate::import::is_the_only_asked_source(&catalogs, entry.catalog),
            })
            .collect()
    }
}

use super::Preferences;
use super::{is_the_only_asked_source, CatalogAvailability};

/// A catalog choice and whether changing it would leave a source to ask.
pub struct LookupCatalogSetting {
    pub availability: CatalogAvailability,
    pub can_change: bool,
}

impl Preferences {
    pub fn lookup_catalog_settings(&self) -> Vec<LookupCatalogSetting> {
        let catalogs = self.metadata_sources();
        catalogs
            .iter()
            .map(|entry| LookupCatalogSetting {
                availability: *entry,
                can_change: !is_the_only_asked_source(&catalogs, entry.catalog),
            })
            .collect()
    }
}

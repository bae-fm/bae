use super::*;

/// One release an import pane offers, as the library-membership check reads
/// it: the catalog's own release id and, when the catalog surfaces one, its
/// group id.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibraryCheck {
    pub source: crate::types::BridgeCatalog,
    pub release_id: String,
    pub source_group_id: Option<String>,
}

/// The library membership of every checked release, by release id, as of
/// one revision of the checks.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibraryStatusSnapshot {
    pub statuses: std::collections::HashMap<String, crate::types::BridgeLibraryStatus>,
    /// The revision of the `set_checks` call these statuses answer.
    pub request_revision: u64,
}

#[uniffi::export]
impl AppHandle {
    /// Whether the releases an import pane offers are already in the
    /// library, for as long as the pane is open; it starts with no checks.
    pub fn subscribe_library_statuses(&self) -> std::sync::Arc<crate::LibraryStatusSubscription> {
        crate::screens::library_statuses(
            self.services.subscribe_library_statuses(),
            self.runtime.clone(),
        )
    }
}

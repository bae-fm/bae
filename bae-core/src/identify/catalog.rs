//! Catalog-number lookup — look one catalog number up against MB + Discogs.
//! Catalog *extraction* (OCR, CUE fields, folder and file names) lives in the
//! signal-extraction service; identify only looks the chosen number up.

use crate::discogs::client::DiscogsSearchParams;
use crate::import::search::{
    import_error_to_lookup_failure, merge_provider_results, search_mb, MetadataResult,
};
use crate::library::LibraryManager;
use crate::musicbrainz::ReleaseSearchParams;
use crate::util::rate_limiter::CallPriority;

/// Union search over every configured provider. Each result carries its own
/// source, so successful result sets concatenate without deduplication.
pub async fn lookup_catalog(
    catalog: &str,
    library_manager: &LibraryManager,
    priority: CallPriority,
) -> Result<Vec<MetadataResult>, crate::signals::LookupFailure> {
    let musicbrainz = search_mb(
        ReleaseSearchParams {
            catalog_number: Some(catalog.to_string()),
            ..Default::default()
        },
        priority,
    )
    .await
    .map_err(|error| import_error_to_lookup_failure(&error))?;

    let discogs = if library_manager.discogs_is_usable() {
        Some(
            library_manager
                .search_discogs(
                    DiscogsSearchParams {
                        catno: Some(catalog.to_string()),
                        ..Default::default()
                    },
                    priority,
                )
                .await
                .map_err(|error| import_error_to_lookup_failure(&error)),
        )
    } else {
        None
    };
    merge_provider_results(musicbrainz, discogs)
}

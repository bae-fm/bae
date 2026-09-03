//! Catalog-number lookup — look one catalog number up against MB + Discogs.
//! Catalog *extraction* (OCR, CUE fields, folder and file names) lives in the
//! signal-extraction service; identify only looks the chosen number up.

use crate::discogs::client::DiscogsSearchParams;
use crate::import::search::{import_error_to_lookup_failure, search_mb, ProviderLookups};
use crate::library::LibraryManager;
use crate::musicbrainz::ReleaseSearchParams;
use crate::util::rate_limiter::CallPriority;

/// Ask every configured provider about one catalog number, concurrently. Each
/// answer is kept apart so a failing provider never hides the other's results.
pub async fn lookup_catalog(
    catalog: &str,
    library_manager: &LibraryManager,
    priority: CallPriority,
) -> ProviderLookups {
    let musicbrainz = async {
        search_mb(
            ReleaseSearchParams {
                catalog_number: Some(catalog.to_string()),
                ..Default::default()
            },
            priority,
        )
        .await
        .map_err(|error| import_error_to_lookup_failure(&error))
    };

    // `then_some` builds the future without polling it, so an unconfigured
    // Discogs still asks nothing.
    let discogs = library_manager.discogs_is_usable().then_some(async {
        library_manager
            .search_discogs(
                DiscogsSearchParams {
                    catno: Some(catalog.to_string()),
                    ..Default::default()
                },
                priority,
            )
            .await
            .map_err(|error| import_error_to_lookup_failure(&error))
    });

    ProviderLookups::run(musicbrainz, discogs).await
}

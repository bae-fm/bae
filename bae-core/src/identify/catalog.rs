//! Catalog-number lookup — look one catalog number up at one provider.
//! Catalog *extraction* (OCR, CUE fields, folder and file names) lives in the
//! signal-extraction service; identify only looks the chosen number up.

use crate::discogs::client::DiscogsSearchParams;
use crate::import::search::{import_error_to_lookup_failure, search_mb, SourceLookup};
use crate::import::MetadataSource;
use crate::library::LibraryManager;
use crate::musicbrainz::ReleaseSearchParams;
use crate::util::rate_limiter::CallPriority;

/// Ask one provider about one catalog number. Each provider is asked on its
/// own, so a failing one never hides the other's results.
pub async fn lookup_catalog(
    source: MetadataSource,
    catalog: &str,
    library_manager: &LibraryManager,
    priority: CallPriority,
) -> SourceLookup {
    match source {
        MetadataSource::MusicBrainz => search_mb(
            ReleaseSearchParams {
                catalog_number: Some(catalog.to_string()),
                ..Default::default()
            },
            priority,
        )
        .await
        .map_err(|error| import_error_to_lookup_failure(&error)),
        MetadataSource::Discogs => library_manager
            .search_discogs(
                DiscogsSearchParams {
                    catno: Some(catalog.to_string()),
                    ..Default::default()
                },
                priority,
            )
            .await
            .map_err(|error| import_error_to_lookup_failure(&error)),
    }
}

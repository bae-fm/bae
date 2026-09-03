//! Barcode lookup helpers — look up one barcode against MB + Discogs. Barcode
//! *detection* (artwork OCR and the CUE `CATALOG` field) lives in the
//! signal-extraction service; identify only looks the codes up.

use crate::discogs::client::DiscogsSearchParams;
use crate::import::search::{import_error_to_lookup_failure, search_mb, ProviderLookups};
use crate::library::LibraryManager;
use crate::musicbrainz::ReleaseSearchParams;
use crate::util::rate_limiter::CallPriority;

/// Ask every configured provider about one barcode, concurrently. Each answer
/// is kept apart: one provider failing never hides what the other found, and
/// the reducer names the source that failed.
pub async fn lookup_barcode(
    barcode: &str,
    library_manager: &LibraryManager,
    priority: CallPriority,
) -> ProviderLookups {
    let musicbrainz = async {
        search_mb(
            ReleaseSearchParams {
                barcode: Some(barcode.to_string()),
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
                    barcode: Some(barcode.to_string()),
                    ..Default::default()
                },
                priority,
            )
            .await
            .map_err(|error| import_error_to_lookup_failure(&error))
    });

    ProviderLookups::run(musicbrainz, discogs).await
}

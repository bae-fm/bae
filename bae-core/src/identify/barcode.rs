//! Barcode lookup helpers — look up one barcode at one provider. Barcode
//! *detection* (artwork OCR and the CUE `CATALOG` field) lives in the
//! signal-extraction service; identify only looks the codes up.

use crate::discogs::client::DiscogsSearchParams;
use crate::import::search::{import_error_to_lookup_failure, search_mb, SourceLookup};
use crate::import::MetadataSource;
use crate::library::LibraryManager;
use crate::musicbrainz::ReleaseSearchParams;
use crate::util::rate_limiter::CallPriority;

/// Ask one provider about one barcode. Each provider is asked on its own, so
/// its answer lands the moment it arrives and its failure names only itself.
pub async fn lookup_barcode(
    source: MetadataSource,
    barcode: &str,
    library_manager: &LibraryManager,
    priority: CallPriority,
) -> SourceLookup {
    match source {
        MetadataSource::MusicBrainz => search_mb(
            ReleaseSearchParams {
                barcode: Some(barcode.to_string()),
                ..Default::default()
            },
            priority,
        )
        .await
        .map_err(|error| import_error_to_lookup_failure(&error)),
        MetadataSource::Discogs => library_manager
            .search_discogs(
                DiscogsSearchParams {
                    barcode: Some(barcode.to_string()),
                    ..Default::default()
                },
                priority,
            )
            .await
            .map_err(|error| import_error_to_lookup_failure(&error)),
    }
}

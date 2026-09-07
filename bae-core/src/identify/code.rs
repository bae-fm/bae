//! Printed-code lookup — look one barcode or catalog number up at one
//! provider. Finding the codes (artwork OCR, CUE fields, folder and file
//! names) lives in the signal-extraction service; identify only looks the
//! chosen one up.

use crate::discogs::client::DiscogsSearchParams;
use crate::import::search::{import_error_to_lookup_failure, search_mb, SourceLookup};
use crate::import::MetadataSource;
use crate::library::LibraryManager;
use crate::musicbrainz::ReleaseSearchParams;
use crate::util::rate_limiter::CallPriority;

/// Which code printed on the release is being looked up. The two are separate
/// search fields at both providers, so the lookup has to say which one it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintedCode {
    /// The barcode printed on the back of the physical product.
    Barcode,
    /// The label's catalog number.
    CatalogNumber,
}

/// Ask one provider about one printed code. Each provider is asked on its own,
/// so its answer lands the moment it arrives and its failure names only itself.
pub async fn lookup_code(
    source: MetadataSource,
    kind: PrintedCode,
    code: &str,
    library_manager: &LibraryManager,
    priority: CallPriority,
) -> SourceLookup {
    match source {
        MetadataSource::MusicBrainz => {
            let params = match kind {
                PrintedCode::Barcode => ReleaseSearchParams {
                    barcode: Some(code.to_string()),
                    ..Default::default()
                },
                PrintedCode::CatalogNumber => ReleaseSearchParams {
                    catalog_number: Some(code.to_string()),
                    ..Default::default()
                },
            };
            search_mb(params, priority)
                .await
                .map_err(|error| import_error_to_lookup_failure(&error))
        }
        MetadataSource::Discogs => {
            let params = match kind {
                PrintedCode::Barcode => DiscogsSearchParams {
                    barcode: Some(code.to_string()),
                    ..Default::default()
                },
                PrintedCode::CatalogNumber => DiscogsSearchParams {
                    catno: Some(code.to_string()),
                    ..Default::default()
                },
            };
            library_manager
                .search_discogs(params, priority)
                .await
                .map_err(|error| import_error_to_lookup_failure(&error))
        }
    }
}

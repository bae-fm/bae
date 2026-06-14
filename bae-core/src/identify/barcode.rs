//! Barcode lookup helpers — look up one barcode against MB + Discogs and
//! annotate the results with library status. Barcode *detection* (artwork OCR
//! and the CUE `CATALOG` field) lives in the signal-extraction service;
//! identify only looks the codes up.

use crate::db::LibraryStatus;
use crate::discogs::DiscogsClient;
use crate::import::search::{search_discogs_by_barcode, search_mb_by_barcode, MetadataResult};

/// Union search: MB first, then Discogs. Returns the merged results (MB
/// before Discogs), deduped on (source, release_id). Errors on MB bubble up
/// as `Err`; Discogs failures are logged and skipped so a provider outage
/// doesn't break the phase.
pub async fn lookup_barcode(
    barcode: &str,
    discogs_client: Option<&DiscogsClient>,
) -> Result<Vec<MetadataResult>, String> {
    let mut combined: Vec<MetadataResult> = search_mb_by_barcode(barcode.to_string()).await?;

    if let Some(client) = discogs_client {
        match search_discogs_by_barcode(client, barcode.to_string()).await {
            Ok(mut discogs) => combined.append(&mut discogs),
            Err(e) => tracing::debug!("Discogs barcode search failed for {barcode}: {e}"),
        }
    }

    Ok(combined)
}

/// Annotate search results with library status. Missing status rows default
/// to "not in library". Returns `(matches, statuses)` with aligned ordering.
pub async fn annotate_with_library_status(
    results: Vec<MetadataResult>,
    library_manager: &crate::library::LibraryManager,
) -> Result<(Vec<MetadataResult>, Vec<LibraryStatus>), String> {
    use crate::db::LibraryCheck;
    use std::collections::HashMap;

    let checks: Vec<LibraryCheck> = results.iter().map(LibraryCheck::from).collect();

    let statuses = library_manager
        .check_releases_in_library(&checks)
        .await
        .map_err(|e| format!("Failed to check library status: {e}"))?;

    let status_map: HashMap<String, LibraryStatus> = statuses
        .into_iter()
        .map(|s| (s.release_id.clone(), s))
        .collect();

    let aligned: Vec<LibraryStatus> = results
        .iter()
        .map(|r| {
            status_map
                .get(&r.release_id)
                .cloned()
                .unwrap_or_else(|| LibraryStatus {
                    release_id: r.release_id.clone(),
                    release_in_library: false,
                    album_in_library: false,
                    album_title: None,
                    album_id: None,
                })
        })
        .collect();

    Ok((results, aligned))
}

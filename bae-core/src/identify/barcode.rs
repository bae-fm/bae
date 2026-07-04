//! Barcode lookup helpers — look up one barcode against MB + Discogs and
//! annotate the results with library status. Barcode *detection* (artwork OCR
//! and the CUE `CATALOG` field) lives in the signal-extraction service;
//! identify only looks the codes up.

use crate::db::LibraryStatus;
use crate::discogs::client::{DiscogsClient, DiscogsSearchParams};
use crate::import::cover_art::CoverArtArchiveClient;
use crate::import::search::{search_discogs, search_mb, MetadataResult};
use crate::musicbrainz::ReleaseSearchParams;

/// Union search: MB first, then Discogs. Returns the merged results (MB
/// before Discogs), deduped on (source, release_id). Errors on MB bubble up
/// as `Err`; Discogs failures are logged and skipped so a provider outage
/// doesn't break the phase.
pub async fn lookup_barcode(
    cover_art_archive: &CoverArtArchiveClient,
    barcode: &str,
    discogs_client: Option<&DiscogsClient>,
) -> Result<Vec<MetadataResult>, String> {
    let mut combined: Vec<MetadataResult> = search_mb(
        cover_art_archive,
        ReleaseSearchParams {
            barcode: Some(barcode.to_string()),
            ..Default::default()
        },
    )
    .await?;

    if let Some(client) = discogs_client {
        match search_discogs(
            client,
            DiscogsSearchParams {
                barcode: Some(barcode.to_string()),
                ..Default::default()
            },
        )
        .await
        {
            Ok(mut discogs) => combined.append(&mut discogs),
            Err(e) => tracing::debug!("Discogs barcode search failed for {barcode}: {e}"),
        }
    }

    Ok(combined)
}

/// Annotate search results with library status. Returns `(matches, statuses)`
/// with the ordering from `check_releases_in_library`.
pub async fn annotate_with_library_status(
    results: Vec<MetadataResult>,
    library_manager: &crate::library::LibraryManager,
) -> Result<(Vec<MetadataResult>, Vec<LibraryStatus>), String> {
    use crate::db::LibraryCheck;

    let checks: Vec<LibraryCheck> = results.iter().map(LibraryCheck::from).collect();

    let statuses = library_manager
        .check_releases_in_library(&checks)
        .await
        .map_err(|e| format!("Failed to check library status: {e}"))?;

    Ok((results, statuses))
}

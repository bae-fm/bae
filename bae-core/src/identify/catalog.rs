//! Catalog-number lookup — look one catalog number up against MB + Discogs.
//! Catalog *extraction* (OCR, CUE fields, folder and file names) lives in the
//! signal-extraction service; identify only looks the chosen number up.

use crate::discogs::client::DiscogsSearchParams;
use crate::import::search::{search_mb, MetadataResult};
use crate::library::LibraryManager;
use crate::musicbrainz::ReleaseSearchParams;
use crate::util::rate_limiter::CallPriority;

/// Union search: MB first, then Discogs. Errors on MB bubble up as `Err`;
/// Discogs failures are logged and skipped so a provider outage doesn't break
/// the phase. Each result carries its own `source`, so the two sets are
/// disjoint by construction and the merge is a plain concatenation.
pub async fn lookup_catalog(
    catalog: &str,
    library_manager: &LibraryManager,
    priority: CallPriority,
) -> Result<Vec<MetadataResult>, String> {
    let mut results = search_mb(
        ReleaseSearchParams {
            catalog_number: Some(catalog.to_string()),
            ..Default::default()
        },
        priority,
    )
    .await
    .map_err(|e| e.to_string())?;

    if let Some(mut discogs) = discogs_catalog_lookup(library_manager, catalog, priority).await {
        results.append(&mut discogs);
    }
    Ok(results)
}

/// `None` when there is no client, or the search failed. Discogs is the
/// best-effort secondary source, so its outage must not fail the phase or drop
/// the MB matches — but dropping a whole source from a user-facing lookup is
/// abnormal enough to warn about.
async fn discogs_catalog_lookup(
    library_manager: &LibraryManager,
    catalog: &str,
    priority: CallPriority,
) -> Option<Vec<MetadataResult>> {
    match library_manager
        .search_discogs(
            DiscogsSearchParams {
                catno: Some(catalog.to_string()),
                ..Default::default()
            },
            priority,
        )
        .await
    {
        Ok(results) => Some(results),
        Err(e) => {
            tracing::warn!("Discogs catalog search failed for {catalog}, skipping Discogs: {e}");
            None
        }
    }
}

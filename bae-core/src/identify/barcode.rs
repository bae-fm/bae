//! Barcode lookup helpers — look up one barcode against MB + Discogs and
//! annotate the results with library status. Barcode *detection* (artwork OCR
//! and the CUE `CATALOG` field) lives in the signal-extraction service;
//! identify only looks the codes up.

use crate::db::LibraryStatus;
use crate::discogs::client::{DiscogsClient, DiscogsSearchParams};
use crate::import::cover_art::CoverArtArchiveClient;
use crate::import::search::{search_discogs, search_mb, MetadataResult};
use crate::musicbrainz::ReleaseSearchParams;

/// Union search: MB first, then Discogs. Errors on MB bubble up as `Err`;
/// Discogs failures are logged and skipped so a provider outage doesn't break
/// the phase.
pub async fn lookup_barcode(
    cover_art_archive: &CoverArtArchiveClient,
    barcode: &str,
    discogs_client: Option<&DiscogsClient>,
) -> Result<Vec<MetadataResult>, String> {
    let mb = search_mb(
        cover_art_archive,
        ReleaseSearchParams {
            barcode: Some(barcode.to_string()),
            ..Default::default()
        },
    )
    .await?;

    let discogs = match discogs_client {
        Some(client) => {
            match search_discogs(
                client,
                DiscogsSearchParams {
                    barcode: Some(barcode.to_string()),
                    ..Default::default()
                },
            )
            .await
            {
                Ok(results) => Some(results),
                Err(e) => {
                    // A provider outage on Discogs must not fail the phase — the
                    // MB results still stand. Skip and log.
                    tracing::debug!("Discogs barcode search failed for {barcode}: {e}");
                    None
                }
            }
        }
        None => None,
    };

    Ok(merge_barcode_results(mb, discogs))
}

/// Merge one barcode's MB and Discogs matches, MB first then Discogs.
///
/// MB results carry `source == MusicBrainz` and Discogs results
/// `source == Discogs`, so the two sets are disjoint by construction — the
/// merge is an ordered concatenation, nothing to deduplicate across them.
/// `discogs` is `None` when the Discogs lookup was skipped (no client) or
/// failed (logged by the caller); the union is then just the MB results.
fn merge_barcode_results(
    mut mb: Vec<MetadataResult>,
    discogs: Option<Vec<MetadataResult>>,
) -> Vec<MetadataResult> {
    if let Some(mut discogs) = discogs {
        mb.append(&mut discogs);
    }
    mb
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::MetadataSource;

    fn result(source: MetadataSource, release_id: &str) -> MetadataResult {
        MetadataResult {
            source,
            release_id: release_id.to_string(),
            title: "Album".to_string(),
            artist: None,
            year: None,
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            cover_art: None,
            source_group_id: None,
        }
    }

    /// The union keeps MB results first, then appends Discogs.
    #[test]
    fn merge_puts_mb_before_discogs() {
        let mb = vec![
            result(MetadataSource::MusicBrainz, "mb-1"),
            result(MetadataSource::MusicBrainz, "mb-2"),
        ];
        let discogs = Some(vec![result(MetadataSource::Discogs, "dg-1")]);

        let merged = merge_barcode_results(mb, discogs);
        let ids: Vec<(MetadataSource, &str)> = merged
            .iter()
            .map(|r| (r.source, r.release_id.as_str()))
            .collect();
        assert_eq!(
            ids,
            vec![
                (MetadataSource::MusicBrainz, "mb-1"),
                (MetadataSource::MusicBrainz, "mb-2"),
                (MetadataSource::Discogs, "dg-1"),
            ]
        );
    }

    /// A skipped or failed Discogs lookup (`None`) yields the MB results alone —
    /// the phase isn't broken by a Discogs outage.
    #[test]
    fn merge_without_discogs_is_mb_only() {
        let mb = vec![result(MetadataSource::MusicBrainz, "mb-1")];
        let merged = merge_barcode_results(mb, None);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].source, MetadataSource::MusicBrainz);
        assert_eq!(merged[0].release_id, "mb-1");
    }

    /// An empty Discogs result set leaves the MB results unchanged.
    #[test]
    fn merge_with_empty_discogs_is_mb_only() {
        let mb = vec![result(MetadataSource::MusicBrainz, "mb-1")];
        let merged = merge_barcode_results(mb, Some(vec![]));
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].release_id, "mb-1");
    }
}

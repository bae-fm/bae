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

    let discogs = discogs_barcode_lookup(discogs_client, barcode).await;

    Ok(merge_barcode_results(mb, discogs))
}

/// Look up a barcode against Discogs, tolerating a provider outage.
///
/// Returns `None` when there is no client or the search fails; `Some(results)`
/// on success. A failure is logged and skipped rather than propagated: Discogs
/// is the best-effort secondary source, so its outage must not fail the phase
/// or drop the MB matches. The skip is logged at `warn` because silently
/// dropping a whole source from a user-facing lookup is abnormal.
async fn discogs_barcode_lookup(
    discogs_client: Option<&DiscogsClient>,
    barcode: &str,
) -> Option<Vec<MetadataResult>> {
    let client = discogs_client?;
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
            tracing::warn!("Discogs barcode search failed for {barcode}, skipping Discogs: {e}");
            None
        }
    }
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

    /// A Discogs lookup that genuinely fails (client pointed at a refused port)
    /// is skipped, not propagated: the helper returns `None` and logs a warning.
    /// Combined with the merge tests above, this is the "Discogs failure logged
    /// and skipped, MB results still stand" path. No MB network is touched — the
    /// merge with `None` (MB-only) is covered separately.
    #[tokio::test]
    async fn discogs_lookup_failure_is_logged_and_skipped() {
        // Port 1 refuses immediately, so the search returns a transport error
        // without a live Discogs dependency.
        let client = DiscogsClient::with_base_url(
            "test-token".to_string(),
            "http://127.0.0.1:1".to_string(),
        );

        let mut outcome = None;
        let logs = crate::test_logs::capture_warn_logs_async(|| async {
            outcome = Some(discogs_barcode_lookup(Some(&client), "0123456789").await);
        })
        .await;

        assert!(
            outcome.expect("closure ran").is_none(),
            "a failed Discogs lookup is skipped (None)"
        );
        assert!(
            logs.contains("Discogs barcode search failed"),
            "the skip should be logged at warn, got: {logs}"
        );
    }

    /// No Discogs client → `None`, no lookup attempted, no warning.
    #[tokio::test]
    async fn discogs_lookup_without_client_is_none() {
        assert!(discogs_barcode_lookup(None, "0123456789").await.is_none());
    }
}

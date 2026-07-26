//! Barcode lookup helpers — look up one barcode against MB + Discogs. Barcode
//! *detection* (artwork OCR and the CUE `CATALOG` field) lives in the
//! signal-extraction service; identify only looks the codes up.

use crate::discogs::client::{DiscogsClient, DiscogsSearchParams};
use crate::import::cover_art::CoverArtArchiveClient;
use crate::import::search::{search_discogs, search_mb, MetadataResult};
use crate::musicbrainz::ReleaseSearchParams;
use crate::util::rate_limiter::CallPriority;

/// Union search: MB first, then Discogs. Errors on MB bubble up as `Err`;
/// Discogs failures are logged and skipped so a provider outage doesn't break
/// the phase.
pub async fn lookup_barcode(
    cover_art_archive: &CoverArtArchiveClient,
    barcode: &str,
    discogs_client: Option<&DiscogsClient>,
    priority: CallPriority,
) -> Result<Vec<MetadataResult>, String> {
    let mb = search_mb(
        cover_art_archive,
        ReleaseSearchParams {
            barcode: Some(barcode.to_string()),
            ..Default::default()
        },
        priority,
    )
    .await
    .map_err(|e| e.to_string())?;

    let discogs = discogs_barcode_lookup(discogs_client, barcode, priority).await;

    Ok(merge_barcode_results(mb, discogs))
}

/// `None` when there is no client, or the search failed. Discogs is the
/// best-effort secondary source, so its outage must not fail the phase or drop the
/// MB matches — but dropping a whole source from a user-facing lookup is abnormal
/// enough to warn about.
async fn discogs_barcode_lookup(
    discogs_client: Option<&DiscogsClient>,
    barcode: &str,
    priority: CallPriority,
) -> Option<Vec<MetadataResult>> {
    let client = discogs_client?;
    match search_discogs(
        client,
        DiscogsSearchParams {
            barcode: Some(barcode.to_string()),
            ..Default::default()
        },
        priority,
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

/// MB first, then Discogs. Each result carries its own `source`, so the two sets
/// are disjoint by construction and the merge is a plain concatenation with nothing
/// to deduplicate. `discogs` is `None` when its lookup was skipped or failed.
fn merge_barcode_results(
    mut mb: Vec<MetadataResult>,
    discogs: Option<Vec<MetadataResult>>,
) -> Vec<MetadataResult> {
    if let Some(mut discogs) = discogs {
        mb.append(&mut discogs);
    }
    mb
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::MetadataSource;
    use serial_test::serial;

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

    /// A Discogs outage doesn't break the phase: the MB results still stand.
    #[test]
    fn merge_without_discogs_is_mb_only() {
        let mb = vec![result(MetadataSource::MusicBrainz, "mb-1")];
        let merged = merge_barcode_results(mb, None);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].source, MetadataSource::MusicBrainz);
        assert_eq!(merged[0].release_id, "mb-1");
    }

    #[test]
    fn merge_with_empty_discogs_is_mb_only() {
        let mb = vec![result(MetadataSource::MusicBrainz, "mb-1")];
        let merged = merge_barcode_results(mb, Some(vec![]));
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].release_id, "mb-1");
    }

    /// A real Discogs failure is skipped, not propagated: `None` plus a warning.
    #[tokio::test]
    #[serial(discogs_rate_limiter)]
    async fn discogs_lookup_failure_is_logged_and_skipped() {
        // Port 1 refuses immediately, so the search fails without a live dependency.
        let client = DiscogsClient::with_base_url(
            "test-token".to_string(),
            "http://127.0.0.1:1".to_string(),
        );

        let mut outcome = None;
        let logs = crate::test_logs::capture_warn_logs_async(|| async {
            outcome = Some(
                discogs_barcode_lookup(Some(&client), "0123456789", CallPriority::Interactive)
                    .await,
            );
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
        assert!(
            discogs_barcode_lookup(None, "0123456789", CallPriority::Interactive)
                .await
                .is_none()
        );
    }
}

//! Barcode lookup helpers — look up one barcode against MB + Discogs. Barcode
//! *detection* (artwork OCR and the CUE `CATALOG` field) lives in the
//! signal-extraction service; identify only looks the codes up.

use crate::discogs::client::DiscogsSearchParams;
use crate::import::search::{
    import_error_to_lookup_failure, merge_provider_results, search_mb, MetadataResult,
};
use crate::library::LibraryManager;
use crate::musicbrainz::ReleaseSearchParams;
use crate::util::rate_limiter::CallPriority;

/// Union search over every configured provider. A configured provider that
/// fails makes the lookup fail; otherwise partial evidence could be presented
/// as the complete answer.
pub async fn lookup_barcode(
    barcode: &str,
    library_manager: &LibraryManager,
    priority: CallPriority,
) -> Result<Vec<MetadataResult>, crate::signals::LookupFailure> {
    let mb = search_mb(
        ReleaseSearchParams {
            barcode: Some(barcode.to_string()),
            ..Default::default()
        },
        priority,
    )
    .await
    .map_err(|error| import_error_to_lookup_failure(&error))?;

    let discogs = if library_manager.discogs_is_usable() {
        Some(
            library_manager
                .search_discogs(
                    DiscogsSearchParams {
                        barcode: Some(barcode.to_string()),
                        ..Default::default()
                    },
                    priority,
                )
                .await
                .map_err(|error| import_error_to_lookup_failure(&error)),
        )
    } else {
        None
    };

    merge_provider_results(mb, discogs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::MetadataSource;
    use crate::signals::LookupFailure;

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
            source_tracks: None,
        }
    }

    #[test]
    fn merge_puts_mb_before_discogs() {
        let mb = vec![
            result(MetadataSource::MusicBrainz, "mb-1"),
            result(MetadataSource::MusicBrainz, "mb-2"),
        ];
        let discogs = Some(Ok(vec![result(MetadataSource::Discogs, "dg-1")]));

        let merged = merge_provider_results(mb, discogs).unwrap();
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

    /// With Discogs disabled, MusicBrainz is the complete configured lookup.
    #[test]
    fn merge_with_discogs_disabled_is_mb_only() {
        let mb = vec![result(MetadataSource::MusicBrainz, "mb-1")];
        let merged = merge_provider_results(mb, None).unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].source, MetadataSource::MusicBrainz);
        assert_eq!(merged[0].release_id, "mb-1");
    }

    #[test]
    fn merge_with_empty_discogs_is_mb_only() {
        let mb = vec![result(MetadataSource::MusicBrainz, "mb-1")];
        let merged = merge_provider_results(mb, Some(Ok(vec![]))).unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].release_id, "mb-1");
    }

    #[test]
    fn configured_discogs_failure_fails_the_combined_lookup() {
        let mb = vec![result(MetadataSource::MusicBrainz, "mb-1")];
        assert_eq!(
            merge_provider_results(mb, Some(Err(LookupFailure::Network))),
            Err(LookupFailure::Network)
        );
    }
}

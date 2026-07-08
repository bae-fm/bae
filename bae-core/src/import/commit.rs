//! Import commit orchestration.
//!
//! Encapsulates the business logic for fetching a source release and mapping
//! it to `ParsedAlbum` — shared by the per-release commit path and the
//! prefetch path.

use tracing::warn;

use crate::discogs::DiscogsClient;
use crate::musicbrainz;
use coven::Clock;
use coven::IdProvider;

use super::discogs_mapper;
use super::types::MetadataSource;
use super::{ImportError, ParsedAlbum};

/// Fetch a Discogs release plus its master, without any MB cross-ref.
/// Pure Discogs: no MB API calls. Returns the parsed release, the master
/// year (when a master is linked), and metadata pairs for archival.
///
/// Cross-referencing into MB is a separate step — see
/// `crate::musicbrainz::fetch_mb_xref`. The split keeps prefetch's API
/// MB-free; only commit composes both. Mirrors the
/// `fetch_mb_response` / `fetch_discogs_xref` split on the forward path.
pub async fn fetch_discogs_release(
    client: &DiscogsClient,
    release_id: &str,
) -> Result<
    (
        crate::discogs::DiscogsRelease,
        Option<u32>,
        Vec<(String, String)>,
    ),
    ImportError,
> {
    let (discogs_release, raw_json) = client.get_release(release_id).await?;

    let mut metadata = vec![(MetadataSource::Discogs.as_str().to_string(), raw_json)];

    let master_year = if let Some(ref master_id) = discogs_release.master_id {
        match client.get_master(master_id).await {
            Ok((year, master_json)) => {
                metadata.push(("discogs_master".to_string(), master_json));
                year
            }
            Err(e) => {
                warn!("Failed to fetch Discogs master {}: {e}", master_id);
                discogs_release.year
            }
        }
    } else {
        discogs_release.year
    };

    Ok((discogs_release, master_year, metadata))
}

/// Discogs commit orchestration: fetch the Discogs release, fetch the MB
/// cross-reference (when MB has a back-link), and map to `ParsedAlbum`.
/// Returns the parsed album and accumulated metadata pairs (Discogs
/// release/master + MB cross-ref payloads).
pub async fn fetch_and_map_discogs(
    client: &DiscogsClient,
    release_id: &str,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<(ParsedAlbum, Vec<(String, String)>), ImportError> {
    let (discogs_release, master_year, mut metadata) =
        fetch_discogs_release(client, release_id).await?;
    let mb_xref = match musicbrainz::fetch_mb_xref(release_id).await {
        Some((response, xref_pairs)) => {
            metadata.extend(xref_pairs);
            Some(response)
        }
        None => None,
    };
    let parsed = discogs_mapper::map_discogs_to_db(
        &discogs_release,
        master_year,
        mb_xref.as_ref(),
        clock,
        ids,
    )?;
    Ok((parsed, metadata))
}

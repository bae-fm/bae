//! How a catalog's error reads as a lookup failure: what the catalog or
//! artwork host answered with is theirs, and what bae met on its own side is
//! bae's.

use crate::discogs::client::DiscogsError;
use crate::musicbrainz;
use crate::remote_image::RemoteImageError;
use crate::CatalogError;
use crate::{Failure, InternalFailure, LookupFailure};
use bae_util::http::TransportFailure;
use tracing::warn;

/// The failure MusicBrainz answered with, or `None` for one bae met reading
/// its answer. A release it does not have is its answer, as a 404; a disc ID
/// it does not know never reaches here — callers read that as no matches.
fn mb_provider_failure(e: &musicbrainz::MusicBrainzError) -> Option<LookupFailure> {
    use musicbrainz::MusicBrainzError;
    match e {
        MusicBrainzError::Network(_) => Some(LookupFailure::Network),
        MusicBrainzError::Timeout => Some(LookupFailure::Timeout),
        MusicBrainzError::Provider { status, .. } => {
            Some(LookupFailure::Provider { status: *status })
        }
        MusicBrainzError::NotFound(_) => Some(LookupFailure::Provider { status: Some(404) }),
        MusicBrainzError::Request(_) | MusicBrainzError::Other(_) => None,
    }
}

/// The failure a request that got no usable answer is, or `None` for one bae
/// could not make: a URL reqwest would not build, a redirect loop.
fn transport_failure(error: &reqwest::Error) -> Option<LookupFailure> {
    match TransportFailure::of(error) {
        TransportFailure::Unmakeable => None,
        TransportFailure::Timeout => Some(LookupFailure::Timeout),
        TransportFailure::Status(status) => Some(LookupFailure::Provider { status: Some(status) }),
        TransportFailure::Network => Some(LookupFailure::Network),
    }
}

/// Why a MusicBrainz lookup did not answer: its failure, or bae's own.
pub(crate) fn mb_failure(e: &musicbrainz::MusicBrainzError, what: &str) -> Failure {
    lookup_or_internal(mb_provider_failure(e), what, e)
}

/// A provider's failure, warned with the error that carried it — the typed
/// failure a surface renders keeps no cause, so this is where the DNS, TLS or
/// status behind it is recorded — or bae's own, logged as an internal one.
pub fn lookup_or_internal(
    failure: Option<LookupFailure>,
    what: &str,
    error: &dyn std::fmt::Display,
) -> Failure {
    match failure {
        Some(failure) => {
            warn!("{what}: {error}");
            Failure::Lookup(failure)
        }
        None => Failure::Internal(InternalFailure::logged(what, error)),
    }
}

/// The failure a catalog answered with, or `None` for an error bae met on its
/// own side: a request it could not build, an answer it could not parse.
pub fn provider_failure(error: &CatalogError) -> Option<LookupFailure> {
    match error {
        CatalogError::MusicBrainz(error) => mb_provider_failure(error),
        CatalogError::RemoteImage(error) => remote_image_failure(error),
        CatalogError::Discogs(error) => match error {
            DiscogsError::Transport(error) => transport_failure(error),
            DiscogsError::Provider { status, .. } => {
                Some(LookupFailure::Provider { status: Some(status.as_u16()) })
            }
            DiscogsError::InvalidApiKey => Some(LookupFailure::Provider { status: Some(401) }),
            DiscogsError::NotFound => Some(LookupFailure::Provider { status: Some(404) }),
            DiscogsError::Serialization(_) => None,
        },
        _ => None,
    }
}

/// The failure an artwork host answered with, or `None` for an answer that
/// is no failure of the host (no image where the caller needed one) or for
/// one bae met on its own side: a request it could not build, bytes that are
/// not an image, a cache task that did not finish.
pub fn remote_image_failure(error: &RemoteImageError) -> Option<LookupFailure> {
    match error {
        RemoteImageError::Transport { error, .. } => transport_failure(error),
        RemoteImageError::Status { status, .. } => {
            Some(LookupFailure::Provider { status: Some(*status) })
        }
        RemoteImageError::Unusable { .. }
        | RemoteImageError::NoImage { .. }
        | RemoteImageError::CacheTask(_) => None,
    }
}

#[cfg(test)]
#[path = "failure_tests.rs"]
mod tests;

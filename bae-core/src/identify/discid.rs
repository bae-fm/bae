//! The shared disc-ID lookup tail: look a disc ID up on MusicBrainz. Disc-ID
//! *derivation* (folder scan, release re-identify resolution) lives in
//! `crate::signals`.

use crate::signals::Failure;
use bae_catalogs::rate_limiter::CallPriority;

/// Look up a disc ID on MusicBrainz, keeping each match's document as the
/// answer states it. Empty when
/// MB has no hits — which the reducer treats as a settled signal with zero
/// results, ready for combine, exactly like a barcode that matched nothing.
pub async fn lookup_and_resolve(
    disc_id: &str,
    library_manager: &crate::library::LibraryManager,
    priority: CallPriority,
) -> Result<super::DiscIdAnswer, Failure> {
    let (results, documents) = library_manager
        .lookup_musicbrainz_discid(disc_id, priority)
        .await?
        .into_iter()
        .unzip();
    Ok(super::DiscIdAnswer { results, documents })
}

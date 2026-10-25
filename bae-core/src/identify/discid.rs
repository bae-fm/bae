//! The shared disc-ID lookup tail: look a disc ID up on MusicBrainz and
//! annotate the matches with library status. Disc-ID *derivation* (folder
//! scan, release re-identify resolution) lives in `crate::signals`.

use crate::signals::{Failure, InternalFailure};
use crate::util::rate_limiter::CallPriority;

/// Look up a disc ID on MusicBrainz and pair each match with its library
/// status, keeping each match's document as the answer states it. Empty when
/// MB has no hits — which the reducer treats as a settled signal with zero
/// results, ready for combine, exactly like a barcode that matched nothing.
pub async fn lookup_and_resolve(
    disc_id: &str,
    library_manager: &crate::library::LibraryManager,
    priority: CallPriority,
) -> Result<super::DiscIdAnswer, Failure> {
    let (matches, documents) = library_manager
        .lookup_musicbrainz_discid(disc_id, priority)
        .await?
        .into_iter()
        .unzip();
    // The in-library check is a read of bae's own store.
    let results = super::annotate_with_library_status(matches, library_manager)
        .await
        .map_err(|detail| {
            InternalFailure::logged("checking the library for the Disc ID's releases", detail)
        })?;
    Ok(super::DiscIdAnswer { results, documents })
}

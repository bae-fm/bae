//! The shared disc-ID lookup tail: look a disc ID up on MusicBrainz and
//! annotate the matches with library status. Disc-ID *derivation* (folder
//! scan, release re-identify resolution) lives in `crate::signals`.

use crate::db::LibraryStatus;
use crate::import::search::MetadataResult;
use crate::signals::{Failure, InternalFailure};
use crate::util::rate_limiter::CallPriority;

/// Look up a disc ID on MusicBrainz and pair each match with its library status.
/// Empty when MB has no hits — which the reducer treats as a settled signal with
/// zero results, ready for combine, exactly like a barcode that matched nothing.
pub async fn lookup_and_resolve(
    disc_id: &str,
    library_manager: &crate::library::LibraryManager,
    priority: CallPriority,
) -> Result<Vec<(MetadataResult, LibraryStatus)>, Failure> {
    let matches: Vec<MetadataResult> = library_manager
        .lookup_musicbrainz_discid(disc_id, priority)
        .await?;
    // The in-library check is a read of bae's own store.
    super::annotate_with_library_status(matches, library_manager)
        .await
        .map_err(|detail| {
            InternalFailure::logged("checking the library for the disc ID's releases", detail)
                .into()
        })
}

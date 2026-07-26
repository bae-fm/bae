//! The shared disc-ID lookup tail: look a disc ID up on MusicBrainz and
//! annotate the matches with library status. Disc-ID *derivation* (folder
//! scan, release re-identify resolution) lives in `crate::signals`.

use crate::db::LibraryStatus;
use crate::import::cover_art::CoverArtArchiveClient;
use crate::import::search::{lookup_by_discid, MetadataResult};
use crate::signals::LookupFailure;
use crate::util::rate_limiter::CallPriority;

/// Look up a disc ID on MusicBrainz and pair each match with its library status.
/// Empty when MB has no hits — which the reducer treats as a settled signal with
/// zero results, ready for combine, exactly like a barcode that matched nothing.
pub async fn lookup_and_resolve(
    cover_art_archive: &CoverArtArchiveClient,
    disc_id: &str,
    library_manager: &crate::library::LibraryManager,
    priority: CallPriority,
) -> Result<Vec<(MetadataResult, LibraryStatus)>, LookupFailure> {
    // The MB lookup's failure is already typed — pass it through structured.
    let matches: Vec<MetadataResult> =
        lookup_by_discid(cover_art_archive, disc_id, priority).await?;

    // The in-library check is a local DB read, so its failure is diagnostic
    // detail, never a provider verdict.
    super::annotate_with_library_status(matches, library_manager)
        .await
        .map_err(|detail| LookupFailure::Diagnostic { detail })
}

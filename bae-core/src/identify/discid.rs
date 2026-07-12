//! The shared disc-ID lookup tail: look a disc ID up on MusicBrainz and
//! annotate the matches with library status. Disc-ID *derivation* (folder
//! scan, release re-identify resolution) lives in `crate::signals`.

use crate::db::LibraryStatus;
use crate::import::cover_art::CoverArtArchiveClient;
use crate::import::search::{lookup_by_discid, DiscIdResult, MetadataResult};
use crate::signals::LookupFailure;

/// Look up a disc ID on MusicBrainz and annotate matches with library
/// status. Returns the zipped `(result, status)` pairs — possibly empty when
/// MB has no hits for this disc ID. The triangulation reducer treats empty
/// results the same way as a barcode signal that produced no matches:
/// settled with zero, ready for combine.
pub async fn lookup_and_resolve(
    cover_art_archive: &CoverArtArchiveClient,
    disc_id: &str,
    library_manager: &crate::library::LibraryManager,
) -> Result<Vec<(MetadataResult, LibraryStatus)>, LookupFailure> {
    // The MB lookup carries its own typed failure (Network / Provider /
    // Timeout) — pass it through structured.
    let result = lookup_by_discid(cover_art_archive, disc_id).await?;

    let matches: Vec<MetadataResult> = match result {
        DiscIdResult::NoMatches => return Ok(Vec::new()),
        DiscIdResult::SingleMatch(m) => vec![*m],
        DiscIdResult::MultipleMatches(matches) => matches,
    };

    // The in-library check is a local DB read — its failure is opaque
    // diagnostic detail, not a provider verdict.
    super::annotate_with_library_status(matches, library_manager)
        .await
        .map_err(|detail| LookupFailure::Diagnostic { detail })
}

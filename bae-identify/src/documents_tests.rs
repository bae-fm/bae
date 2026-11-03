//! A read document's tracks, as many as it counts for the audio.

use super::*;
use bae_catalogs::search::SourceTracks;
use bae_catalogs::test_releases::mb_release_json;

/// A read document lists the title of each track it holds for the audio, as
/// many as it counts.
#[test]
fn a_document_lists_its_tracks_titles() {
    let payloads = bae_catalogs::payloads::ReleasePayloads::for_test(
        bae_catalogs::records::LookupRef::new(
            bae_catalogs::catalog::LookupCatalog::MusicBrainz,
            "mb-1",
        ),
        mb_release_json("mb-1", "rg-1", &[200_000, 100_000, 300_000]),
        Vec::new(),
    );
    let release = payloads.extract().unwrap();
    let document = ReleaseDocument::of(
        bae_catalogs::source_release::PreparedRelease { stored: release.clone(), release },
        &bae_catalogs::reading::ReadAudio::of_lengths(&[200_000, 100_000, 300_000]),
    );
    assert_eq!(document.source_tracks, SourceTracks::Listed { count: 3 });
    assert_eq!(document.track_titles, vec!["Track 1", "Track 2", "Track 3"]);
}

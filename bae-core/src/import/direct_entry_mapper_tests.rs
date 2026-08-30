use super::*;
use crate::import::folder_scanner::{CandidateFile, FileRole, ScannedFile};

fn candidate() -> CategorizedFiles {
    CategorizedFiles {
        files: ["01-source.flac", "02-source.flac"]
            .into_iter()
            .map(|name| CandidateFile {
                file: ScannedFile::new(name.into(), name.to_string(), 100, 1)
                    .with_test_flac_audio(),
                role: FileRole::Audio,
                proposed_audio: true,
            })
            .collect(),
    }
}

#[test]
fn direct_entry_uses_the_physical_track_layout_without_provenance() {
    let clock = coven::FixedClock("2026-01-01T00:00:00Z".parse().expect("timestamp"));
    let ids = coven::SequentialIdProvider::new("manual-seed");

    let parsed = map_direct_entry_candidate_to_db(&candidate(), &clock, &ids);

    assert_eq!(parsed.album.title, "");
    assert_eq!(parsed.artists.len(), 1);
    assert_eq!(parsed.artists[0].name, "");
    assert_eq!(parsed.release.metadata_provenance, None);
    assert!(parsed.identities.is_empty());
    assert_eq!(parsed.tracks.len(), 2);
    assert!(parsed.tracks.iter().all(|track| track.title.is_empty()));
    assert_eq!(
        parsed
            .tracks
            .iter()
            .map(|track| (track.side, track.track_number))
            .collect::<Vec<_>>(),
        [(1, Some(1)), (1, Some(2))]
    );
}

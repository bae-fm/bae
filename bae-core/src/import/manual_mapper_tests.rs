use super::*;
use crate::import::folder_scanner::{CandidateFile, FileRole, ScannedFile};

fn candidate() -> CategorizedFiles {
    CategorizedFiles {
        files: ["01-source.flac", "02-source.flac"]
            .into_iter()
            .map(|name| CandidateFile {
                file: ScannedFile::new(name.into(), name.to_string(), 100),
                role: FileRole::Audio,
                proposed_audio: true,
            })
            .collect(),
        format_label: "FLAC".to_string(),
    }
}

#[test]
fn manual_seed_uses_only_the_physical_track_layout() {
    let clock = coven::FixedClock("2026-01-01T00:00:00Z".parse().expect("timestamp"));
    let ids = coven::SequentialIdProvider::new("manual-seed");

    let parsed = map_manual_candidate_to_db(&candidate(), &clock, &ids);

    assert_eq!(parsed.album.title, "");
    assert_eq!(parsed.artists.len(), 1);
    assert_eq!(parsed.artists[0].name, "");
    assert_eq!(
        parsed.release.metadata_source,
        ReleaseMetadataSource::Manual
    );
    assert_eq!(parsed.release.metadata_source_release_id, None);
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

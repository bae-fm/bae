use super::*;
use crate::import::folder_scanner::{CandidateFile, FileRole, ScannedFile};

fn draft(sides: &[Option<i32>]) -> CandidateDraft {
    let files = CategorizedFiles {
        files: sides
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let name = format!("{index}.flac");
                CandidateFile {
                    file: ScannedFile::new(name.clone().into(), name, 100, 1)
                        .with_test_flac_audio(),
                    role: FileRole::Audio,
                }
            })
            .collect(), parts: Vec::new(), 
    };
    let mut draft = blank_candidate_draft(&files);
    for (track, side) in draft.tracks.iter_mut().zip(sides) {
        track.edit.side = *side;
    }
    draft
}

/// Release metadata over `sides`, its tracks naming no audio.
fn metadata(sides: &[Option<i32>]) -> RawReleaseEdit {
    let mut edit = draft(sides).release_edit();
    for track in &mut edit.tracks {
        track.file = None;
    }
    edit
}

/// An LP ripped as one tagged disc takes the record's two sides.
#[test]
fn metadata_splits_one_draft_disc_into_sides() {
    let current = draft(&[Some(1), Some(1), Some(1), Some(1)]);
    let laid = metadata_over_audio(metadata(&[Some(1), Some(1), Some(2), Some(2)]), &current)
        .unwrap()
        .draft;
    assert_eq!(laid.tracks[2].edit.side, Some(2));
    assert_eq!(laid.tracks[2].edit.file, current.tracks[2].edit.file);
}

/// Files tagged as two discs take the one disc a release states.
#[test]
fn metadata_merges_two_draft_discs_into_one() {
    let current = draft(&[Some(1), Some(1), Some(2), Some(2)]);
    let laid = metadata_over_audio(metadata(&[Some(1), Some(1), Some(1), Some(1)]), &current)
        .unwrap()
        .draft;
    assert_eq!(laid.tracks[3].edit.side, Some(1));
}

#[test]
fn unknown_sides_do_not_prevent_metadata_application() {
    let current = draft(&[None, None, None, None]);
    let laid = metadata_over_audio(metadata(&[Some(1), Some(1), Some(2), Some(2)]), &current)
        .unwrap()
        .draft;
    assert_eq!(laid.tracks[2].edit.side, Some(2));
    assert_eq!(laid.tracks[2].edit.file, current.tracks[2].edit.file);
}

/// Each laid track keeps the identity of the row whose audio it plays.
#[test]
fn metadata_takes_the_identity_of_the_row_it_is_laid_over() {
    let mut current = draft(&[None, None]);
    current.tracks[0].edit.id = "kept-first".into();
    current.tracks[1].edit.id = "kept-second".into();
    let laid = metadata_over_audio(metadata(&[None, None]), &current)
        .unwrap()
        .draft;
    assert_eq!(
        laid.tracks
            .iter()
            .map(|track| track.edit.id.as_str())
            .collect::<Vec<_>>(),
        ["kept-first", "kept-second"],
    );
}

/// A release listing a different number of tracks than the folder has audio
/// units is refused, naming both counts.
#[test]
fn metadata_with_a_different_track_count_is_refused() {
    let current = draft(&[None, None, None]);
    assert!(matches!(
        metadata_over_audio(metadata(&[None, None]), &current),
        Err(ImportError::MetadataTrackCount {
            metadata_tracks: 2,
            audio_tracks: 3,
        })
    ));
}

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
                    proposed_audio: true,
                }
            })
            .collect(),
    };
    let mut draft = blank_candidate_draft(&files);
    for (track, side) in draft.tracks.iter_mut().zip(sides) {
        track.edit.side = *side;
    }
    draft
}

#[test]
fn metadata_cannot_contradict_known_group_boundaries() {
    let current = draft(&[Some(1), Some(1), Some(2), Some(2)]);
    let mut proposed = draft(&[Some(1), Some(2), Some(2), Some(2)]);
    assert!(apply_metadata_tracks(&mut proposed, &current).is_err());
}

#[test]
fn unknown_sides_do_not_prevent_metadata_application() {
    let current = draft(&[None, None, None, None]);
    let mut proposed = draft(&[Some(1), Some(1), Some(2), Some(2)]);
    apply_metadata_tracks(&mut proposed, &current).unwrap();
    assert_eq!(proposed.tracks[2].edit.side, Some(2));
    assert_eq!(proposed.tracks[2].edit.file, current.tracks[2].edit.file);
}

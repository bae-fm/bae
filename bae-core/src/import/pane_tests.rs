use super::*;
use crate::import::folder_scanner::{CandidateFile, FileRole, ScannedFile};

fn place(medium: u16, side: u8, number: u32) -> crate::mediums::TrackPlace {
    crate::mediums::TrackPlace {
        medium: std::num::NonZeroU16::new(medium).expect("mediums count from one"),
        side: std::num::NonZeroU8::new(side).expect("sides count from one"),
        number: std::num::NonZeroU32::new(number).expect("numbers count from one"),
    }
}

/// A draft of `tracks` loose files, all on its first disc.
fn draft(tracks: usize) -> CandidateDraft {
    let files = CategorizedFiles {
        files: (0..tracks)
            .map(|index| {
                let name = format!("{index}.flac");
                CandidateFile {
                    file: ScannedFile::new(name.clone().into(), name, 100, 1)
                        .with_test_flac_audio(),
                    role: FileRole::Audio,
                }
            })
            .collect(),
        parts: Vec::new(),
    };
    blank_candidate_draft(&files)
}

/// A reading of tracks at `places` on `mediums`.
fn metadata(
    mediums: Vec<crate::mediums::ReleaseMedium>,
    places: &[crate::mediums::TrackPlace],
) -> ReleaseValues {
    ReleaseValues {
        album_title: "Album Title".to_string(),
        album_credits: Vec::new(),
        album_year: None,
        pressing: bae_types::pressing::Pressing::blank(),
        mediums,
        release_roles: Vec::new(),
        works: Default::default(),
        tracks: places
            .iter()
            .map(|place| crate::import::release_values::TrackValues {
                title: "Track Title".to_string(),
                place: *place,
                stated_length_ms: None,
                credits: crate::import::release_values::TrackCredits::AlbumArtists,
                roles: Vec::new(),
                works: Vec::new(),
            })
            .collect(),
    }
}

/// A reading of `count` tracks on one CD.
fn one_disc(count: u32) -> ReleaseValues {
    let places: Vec<_> = (1..=count).map(|number| place(1, 1, number)).collect();
    metadata(
        vec![crate::mediums::ReleaseMedium::one_sided(Some(
            bae_types::pressing::Medium::Cd,
        ))],
        &places,
    )
}

/// An LP ripped as one disc takes the record's two sides.
#[test]
fn metadata_splits_one_draft_disc_into_sides() {
    let current = draft(4);
    let record = crate::mediums::ReleaseMedium {
        carrier: Some(bae_types::pressing::Medium::Vinyl),
        sides: std::num::NonZeroU8::new(2).expect("two sides"),
    };
    let laid = values_over_audio(
        metadata(
            vec![record],
            &[place(1, 1, 1), place(1, 1, 2), place(1, 2, 1), place(1, 2, 2)],
        ),
        &current,
    )
    .unwrap();
    assert_eq!(laid.mediums, [record]);
    assert_eq!(laid.tracks[2].place, place(1, 2, 1));
    assert_eq!(laid.tracks[2].edit.file, current.tracks[2].edit.file);
}

/// Files laid out as two discs take the one disc a release states.
#[test]
fn metadata_merges_two_draft_discs_into_one() {
    let mut current = draft(4);
    current.mediums.push(crate::mediums::ReleaseMedium::one_sided(None));
    current.tracks[2].place = place(2, 1, 1);
    current.tracks[3].place = place(2, 1, 2);
    let laid = values_over_audio(one_disc(4), &current).unwrap();
    assert_eq!(laid.mediums.len(), 1);
    assert_eq!(laid.tracks[3].place, place(1, 1, 4));
}

/// Each laid track keeps the identity of the row whose audio it plays.
#[test]
fn metadata_takes_the_identity_of_the_row_it_is_laid_over() {
    let mut current = draft(2);
    current.tracks[0].edit.id = "kept-first".into();
    current.tracks[1].edit.id = "kept-second".into();
    let laid = values_over_audio(one_disc(2), &current)
        .unwrap();
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
    let current = draft(3);
    assert!(matches!(
        values_over_audio(one_disc(2), &current),
        Err(ImportError::MetadataTrackCount {
            folder_tracks: 3,
            release_tracks: 2,
        })
    ));
}

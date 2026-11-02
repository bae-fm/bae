use super::*;
use crate::import::{AudioFile, CandidateTrack, RawPressingEdit, RawTrackEdit};
use bae_types::pressing::{MediaCount, Medium, PressingFacts};

fn label(name: &str, catalog_number: &str) -> RawLabelEdit {
    RawLabelEdit {
        name: name.to_string(),
        catalog_number: catalog_number.to_string(),
        ..Default::default()
    }
}

fn row(title: &str, catalog_number: &str, tracks: &[&str]) -> RawReleaseEdit {
    RawReleaseEdit {
        album_title: title.to_string(),
        album_artist_assignments: vec![crate::import::Credited::plain(
            crate::import::ArtistAssignment::named("Artist Name"),
        )],
        album_year: "1990".to_string(),
        pressing: RawPressingEdit {
            year: String::new(),
            labels: vec![label("Label Name", catalog_number)],
            barcodes: Vec::new(),
            facts: PressingFacts::default(),
        },
        tracks: tracks
            .iter()
            .enumerate()
            .map(|(position, title)| RawTrackEdit {
                id: format!("row-{position}"),
                title: title.to_string(),
                artist_assignments: TrackArtistAssignments::AlbumArtists,
                file: None,
            })
            .collect(),
    }
}

/// What picking each of `rows` reads: its fields, on one CD numbered in order.
fn offered(rows: Vec<RawReleaseEdit>) -> Vec<OfferedReading> {
    rows.into_iter()
        .map(|edit| {
            let mut placer = bae_types::mediums::Placer::default();
            let places = edit
                .tracks
                .iter()
                .map(|_| placer.place(std::num::NonZeroU16::MIN, std::num::NonZeroU8::MIN, None))
                .collect();
            OfferedReading {
                edit,
                mediums: vec![bae_types::mediums::ReleaseMedium::one_sided(Some(Medium::Cd))],
                places,
            }
        })
        .collect()
}

/// A draft read from the folder's tags: two tracks, a catalog number the
/// tags state, and a country.
fn draft() -> CandidateDraft {
    CandidateDraft {
        album_title: "album title (tags)".to_string(),
        album_artist_assignments: vec![crate::import::Credited::plain(
            crate::import::ArtistAssignment::named("artist from tags"),
        )],
        album_year: "1985".to_string(),
        pressing: RawPressingEdit {
            year: "2005".to_string(),
            labels: vec![label("label from tags", "TAG-1")],
            barcodes: vec!["0123456789012".to_string()],
            facts: PressingFacts {
                area: Some(bae_types::pressing::ReleaseArea::Country(
                    bae_types::pressing::Country::from_code("DE").expect("a country"),
                )),
                ..PressingFacts::default()
            },
        },
        tracks: ["first from tags", "second from tags"]
            .iter()
            .enumerate()
            .map(|(position, title)| CandidateTrack {
                edit: RawTrackEdit {
                    id: format!("candidate-track-{position}"),
                    title: title.to_string(),
                    artist_assignments: TrackArtistAssignments::AlbumArtists,
                    file: AudioFile::Standalone { file_id: format!("{position}.flac") },
                },
                place: bae_types::mediums::TrackPlace {
                    medium: std::num::NonZeroU16::MIN,
                    side: std::num::NonZeroU8::MIN,
                    number: std::num::NonZeroU32::new(position as u32 + 1).expect("from one"),
                },
                stated_length_ms: None,
                roles: Vec::new(),
                works: Vec::new(),
            })
            .collect(),
        mediums: vec![bae_types::mediums::ReleaseMedium::one_sided(None)],
        release_roles: Vec::new(),
        works: Default::default(),
    }
}

#[test]
fn what_the_pressings_agree_on_is_taken_as_the_top_row_spells_it() {
    let shared = shared_draft(
        &draft(),
        &offered(vec![
            row("Album Title", "ABC-100", &["First Song", "Second Song (Remastered)"]),
            row("ALBUM TITLE", "ABC-200", &["first song", "Second Song"]),
        ]),
        None,
    );
    assert_eq!(shared.album_title, "Album Title");
    assert_eq!(
        shared.album_artist_assignments,
        vec![crate::import::Credited::plain(crate::import::ArtistAssignment::named("Artist Name"))]
    );
    assert_eq!(shared.album_year, "1990");
    assert_eq!(
        shared.tracks.iter().map(|track| track.edit.title.as_str()).collect::<Vec<_>>(),
        ["First Song", "Second Song (Remastered)"]
    );
    assert_eq!(
        shared.pressing.labels,
        vec![label("Label Name", "TAG-1")],
        "the label is shared; which number the copy carries is not"
    );
}

#[test]
fn what_the_pressings_disagree_on_or_leave_unstated_stays_as_the_draft_has_it() {
    let current = draft();
    let mut unstated = row("Album Title", "ABC-100", &["First Song", "Second Song"]);
    unstated.album_year.clear();
    let shared = shared_draft(
        &current,
        &offered(vec![row("Album Title", "ABC-100", &["First Song", "Second Song"]), unstated]),
        None,
    );
    assert_eq!(shared.album_year, current.album_year, "never blanked");
    assert_eq!(shared.pressing.year, current.pressing.year);
    assert_eq!(shared.pressing.barcodes, current.pressing.barcodes);
    assert_eq!(shared.pressing.facts.area, current.pressing.facts.area);
    assert_eq!(shared.pressing.labels, vec![label("Label Name", "ABC-100")]);
}

#[test]
fn catalog_numbers_agree_only_as_one_number() {
    let shared = shared_draft(
        &draft(),
        &offered(vec![
            row("Album Title", "ABC-100", &["First Song", "Second Song"]),
            row("Album Title", "abc 100", &["First Song", "Second Song"]),
        ]),
        None,
    );
    assert_eq!(shared.pressing.labels, vec![label("Label Name", "ABC-100")]);
}

#[test]
fn tracks_agree_only_when_every_pressing_lists_the_folders_count() {
    let current = draft();
    let shared = shared_draft(
        &current,
        &offered(vec![
            row("Album Title", "ABC-100", &["First Song", "Second Song"]),
            row("Album Title", "ABC-200", &["First Song", "Second Song", "Bonus"]),
        ]),
        None,
    );
    assert_eq!(shared.tracks, current.tracks);
    assert_eq!(shared.album_title, "Album Title");
}

#[test]
fn media_every_pressing_states_alike_are_taken() {
    let mut first = row("Album Title", "ABC-100", &["First Song", "Second Song"]);
    first.pressing.facts.media = vec![MediaCount { medium: Medium::Cd, count: 1 }];
    let second = RawReleaseEdit {
        pressing: RawPressingEdit {
            labels: vec![label("Label Name", "ABC-200")],
            ..first.pressing.clone()
        },
        ..first.clone()
    };
    let shared = shared_draft(&draft(), &offered(vec![first.clone(), second]), None);
    assert_eq!(shared.pressing.facts.media, first.pressing.facts.media);
}

/// Where the pressings share no year, the year is the one the folder names
/// the pressing by; a year they share wins over it.
#[test]
fn a_year_the_pressings_do_not_share_is_the_folder_s() {
    let mut first = row("Album Title", "ABC-100", &["First Song", "Second Song"]);
    first.pressing.year = "2005".to_string();
    let mut second = row("Album Title", "ABC-200", &["First Song", "Second Song"]);
    second.pressing.year = "2010".to_string();
    let shared = shared_draft(&draft(), &offered(vec![first.clone(), second]), Some(1986));
    assert_eq!(shared.pressing.year, "1986");
    let agreeing = shared_draft(&draft(), &offered(vec![first.clone(), first.clone()]), Some(1986));
    assert_eq!(agreeing.pressing.year, "2005");
}

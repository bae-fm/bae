use super::*;
use crate::import::release_group::{group_results, unranked};
use crate::import::search::MetadataResult;
use crate::import::{AudioFile, CandidateTrack, RawPressingEdit, RawTrackEdit};
use crate::pressing::{MediaCount, Medium, PressingFacts};

fn release(catalog: Catalog, release_id: &str, album: Option<&str>, year: i32) -> MetadataResult {
    MetadataResult {
        source: catalog,
        release_id: release_id.to_string(),
        title: "Album Title".to_string(),
        artist: Some("Artist Name".to_string()),
        year: Some(year),
        labels: Vec::new(),
        area: None,
        status: None,
        packaging: None,
        discogs_details: Vec::new(),
        barcodes: Vec::new(),
        media: crate::pressing::StatedMedia::Undescribed,
        links: Vec::new(),
        cover_art: None,
        source_group_id: album.map(str::to_string),
        album_links: crate::import::album_links::AlbumLinks::NotAsked,
        source_tracks: None,
        document_failure: None,
        album_first_year: None,
        track_titles: Vec::new(),
        notes: Vec::new(),
    }
}

fn cards(results: Vec<MetadataResult>) -> Vec<ReleaseGroup> {
    group_results(unranked(results), None)
}

#[test]
fn several_pressings_of_one_album_name_it_in_each_catalog() {
    let shared = SharedAlbum::of(&cards(vec![
        release(Catalog::MusicBrainz, "mb-1", Some("group-1"), 1990),
        release(Catalog::MusicBrainz, "mb-2", Some("group-1"), 2001),
    ]))
    .expect("two pressings of one album");
    assert_eq!(
        shared.link.albums(),
        [MetadataRef::new(Catalog::MusicBrainz, "group-1")]
    );
    assert_eq!(
        shared.leads,
        vec![
            MetadataRef::new(Catalog::MusicBrainz, "mb-1"),
            MetadataRef::new(Catalog::MusicBrainz, "mb-2"),
        ],
        "the rows as the list orders them"
    );
}

#[test]
fn one_pressing_offers_no_album_to_be_unsure_about() {
    assert_eq!(
        SharedAlbum::of(&cards(vec![release(
            Catalog::MusicBrainz,
            "mb-1",
            Some("group-1"),
            1990
        )])),
        None
    );
}

#[test]
fn pressings_of_two_albums_share_none() {
    assert_eq!(
        SharedAlbum::of(&cards(vec![
            release(Catalog::MusicBrainz, "mb-1", Some("group-1"), 1990),
            release(Catalog::MusicBrainz, "mb-2", Some("group-2"), 2001),
        ])),
        None
    );
}

#[test]
fn pressings_no_catalog_files_under_an_album_share_none() {
    assert_eq!(
        SharedAlbum::of(&cards(vec![
            release(Catalog::Discogs, "dg-1", None, 1990),
            release(Catalog::Discogs, "dg-2", None, 2001),
        ])),
        None
    );
}

fn label(name: &str, catalog_number: &str) -> RawLabelEdit {
    RawLabelEdit {
        name: name.to_string(),
        catalog_number: catalog_number.to_string(),
    }
}

fn row(title: &str, catalog_number: &str, tracks: &[&str]) -> RawReleaseEdit {
    RawReleaseEdit {
        album_title: title.to_string(),
        album_artist_assignments: vec![ArtistAssignment::named("Artist Name")],
        album_year: "1990".to_string(),
        pressing: RawPressingEdit {
            year: String::new(),
            labels: vec![label("Label Name", catalog_number)],
            barcode: String::new(),
            facts: PressingFacts::default(),
        },
        tracks: tracks
            .iter()
            .enumerate()
            .map(|(position, title)| RawTrackEdit {
                id: format!("row-{position}"),
                title: title.to_string(),
                artist_assignments: TrackArtistAssignments::AlbumArtists,
                side: None,
                track_number: Some(position as i32 + 1),
                file: None,
            })
            .collect(),
    }
}

/// A draft read from the folder's tags: two tracks, a catalog number the
/// tags state, and a country.
fn draft() -> CandidateDraft {
    CandidateDraft {
        album_title: "album title (tags)".to_string(),
        album_artist_assignments: vec![ArtistAssignment::named("artist from tags")],
        album_year: "1985".to_string(),
        pressing: RawPressingEdit {
            year: "2005".to_string(),
            labels: vec![label("label from tags", "TAG-1")],
            barcode: "0123456789012".to_string(),
            facts: PressingFacts {
                area: Some(crate::pressing::ReleaseArea::Country(
                    crate::pressing::Country::from_code("DE").expect("a country"),
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
                    side: None,
                    track_number: position as i32 + 1,
                    file: AudioFile::Standalone {
                        file_id: format!("{position}.flac"),
                    },
                },
                source_index: None,
            })
            .collect(),
    }
}

#[test]
fn what_the_pressings_agree_on_is_taken_as_the_top_row_spells_it() {
    let shared = shared_draft(
        &draft(),
        &[
            row("Album Title", "ABC-100", &["First Song", "Second Song (Remastered)"]),
            row("ALBUM TITLE", "ABC-200", &["first song", "Second Song"]),
        ],
    );
    assert_eq!(shared.album_title, "Album Title");
    assert_eq!(
        shared.album_artist_assignments,
        vec![ArtistAssignment::named("Artist Name")]
    );
    assert_eq!(shared.album_year, "1990");
    assert_eq!(
        shared
            .tracks
            .iter()
            .map(|track| track.edit.title.as_str())
            .collect::<Vec<_>>(),
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
        &[
            row("Album Title", "ABC-100", &["First Song", "Second Song"]),
            unstated,
        ],
    );
    assert_eq!(shared.album_year, current.album_year, "never blanked");
    assert_eq!(shared.pressing.year, current.pressing.year);
    assert_eq!(shared.pressing.barcode, current.pressing.barcode);
    assert_eq!(shared.pressing.facts.area, current.pressing.facts.area);
    assert_eq!(shared.pressing.labels, vec![label("Label Name", "ABC-100")]);
}

#[test]
fn catalog_numbers_agree_only_as_one_number() {
    let shared = shared_draft(
        &draft(),
        &[
            row("Album Title", "ABC-100", &["First Song", "Second Song"]),
            row("Album Title", "abc 100", &["First Song", "Second Song"]),
        ],
    );
    assert_eq!(shared.pressing.labels, vec![label("Label Name", "ABC-100")]);
}

#[test]
fn tracks_agree_only_when_every_pressing_lists_the_folders_count() {
    let current = draft();
    let shared = shared_draft(
        &current,
        &[
            row("Album Title", "ABC-100", &["First Song", "Second Song"]),
            row("Album Title", "ABC-200", &["First Song", "Second Song", "Bonus"]),
        ],
    );
    assert_eq!(shared.tracks, current.tracks);
    assert_eq!(shared.album_title, "Album Title");
}

#[test]
fn media_every_pressing_states_alike_are_taken() {
    let mut first = row("Album Title", "ABC-100", &["First Song", "Second Song"]);
    first.pressing.facts.media = vec![MediaCount {
        medium: Medium::Cd,
        count: 1,
    }];
    let second = RawReleaseEdit {
        pressing: RawPressingEdit {
            labels: vec![label("Label Name", "ABC-200")],
            ..first.pressing.clone()
        },
        ..first.clone()
    };
    let shared = shared_draft(&draft(), &[first.clone(), second]);
    assert_eq!(shared.pressing.facts.media, first.pressing.facts.media);
}

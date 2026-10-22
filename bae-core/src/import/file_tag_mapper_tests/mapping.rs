use super::*;
use lofty::config::WriteOptions;
use lofty::tag::items::Timestamp;
use lofty::tag::{Tag, TagType};
use std::fs;
use tempfile::TempDir;

#[test]
fn cue_and_loose_audio_both_keep_their_metadata_in_playback_order() {
    use crate::import::file_tag_snapshot::{FileObservation, FileTagFact};
    use crate::import::folder_scanner::{
        CandidateFile, CategorizedFiles, FileRole, ScannedFile, SheetAudioFile, SheetBinding,
        SheetDisc,
    };
    let audio = |name: &str| CandidateFile {
        file: ScannedFile::new(PathBuf::from("/source").join(name), name.into(), 123, 1)
            .with_test_flac_audio(),
        role: FileRole::Audio,
    };
    let files = CategorizedFiles {
        files: vec![
            audio("a.flac"),
            audio("image.flac"),
            audio("z.flac"),
            CandidateFile {
                file: ScannedFile::new(
                    PathBuf::from("/source/album.cue"),
                    "album.cue".into(),
                    123,
                    1,
                ),
                role: FileRole::TrackSheet {
                    sheet: cue_sheet(
                        "Album",
                        vec![cue_track(1, "Cue One"), cue_track(2, "Cue Two")],
                    ),
                    binding: SheetBinding::Resolved {
                        files: vec![SheetAudioFile {
                            file_reference: "image.flac".into(),
                            file_id: "image.flac".into(),
                        }],
                    },
                    disc: SheetDisc::Disc { number: 2 },
                },
            },
        ],
        parts: Vec::new(),
    };
    let snapshot = FileTagSnapshot {
        scan_generation: 1,
        file_edit_revision: 0,
        embedded_cover: None,
        files: files
            .audio()
            .map(|file| FileTagFact {
                observation: FileObservation {
                    relative_path: file.relative_path.clone(),
                    size: file.size,
                    modified_at_ns: file.modified_at_ns,
                },
                title: Some(format!("Tags {}", file.relative_path)),
                track_artist: Some("Track Artist".into()),
                album_title: Some("Tagged Album".into()),
                album_artist: Some("Album Artist".into()),
                year: None,
                numbers: crate::import::track_numbers::TrackNumbers {
                    disc: None,
                    track: None,
                },
                isrc: None,
                copyright: None,
                label: None,
                store: None,
            })
            .collect(),
    };
    let parsed = file_metadata_values(&files, &snapshot, Some("Folder"))
    .unwrap();
    assert_eq!(
        parsed
            .tracks
            .iter()
            .map(|track| track.title.as_str())
            .collect::<Vec<_>>(),
        ["Tags a.flac", "Cue One", "Cue Two", "Tags z.flac"]
    );
    assert_eq!(files.track_count(), 4);
}

/// Run the mapper with deterministic fakes. Exercises the real
/// `read_file_tags`.
fn map_tags(audio_files: &[PathBuf]) -> Result<ReleaseValues, ImportError> {
    map_tags_with_folder(audio_files, None)
}

fn map_tags_with_folder(
    audio_files: &[PathBuf],
    folder_name: Option<&str>,
) -> Result<ReleaseValues, ImportError> {
    read_file_tags(audio_files, folder_name)
}

/// Each track's number in the draft the reading makes: the number the tags
/// state, else the track's place on its disc.
fn draft_numbers(values: ReleaseValues) -> Vec<u32> {
    crate::import::release_values::committed_rows(values)
        .tracks
        .iter()
        .map(|track| track.track_number.get())
        .collect()
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

#[test]
fn stored_file_tag_facts_project_without_opening_the_source_file() {
    use crate::import::folder_scanner::{CandidateFile, CategorizedFiles, FileRole, ScannedFile};

    let path = PathBuf::from("/source-does-not-exist/01.flac");
    let files = CategorizedFiles {
        files: vec![CandidateFile {
            file: ScannedFile::new(path, "01.flac".to_string(), 123, 1).with_test_flac_audio(),
            role: FileRole::Audio,
        }],
        parts: Vec::new(),
    };
    let snapshot = crate::import::file_tag_snapshot::FileTagSnapshot {
        scan_generation: 7,
        file_edit_revision: 3,
        files: vec![crate::import::file_tag_snapshot::FileTagFact {
            observation: crate::import::file_tag_snapshot::FileObservation {
                relative_path: "01.flac".to_string(),
                size: 123,
                modified_at_ns: 456,
            },
            title: Some("Track Alpha".to_string()),
            track_artist: Some("Artist Alpha".to_string()),
            album_title: Some("Album Alpha".to_string()),
            album_artist: Some("Artist Alpha".to_string()),
            year: Some(2001),
            numbers: crate::import::track_numbers::TrackNumbers {
                disc: None,
                track: Some(1),
            },
            isrc: None,
            copyright: None,
            label: None,
            store: None,
        }],
        embedded_cover: None,
    };

    let parsed =
        file_metadata_values(&files, &snapshot, Some("Folder Alpha")).unwrap();

    assert_eq!(parsed.album_title, "Album Alpha");
    assert_eq!(parsed.tracks[0].title, "Track Alpha");
    assert!(parsed.pressing.facts.media.is_empty());
}

/// A folder holding one disc image per sheet, each sheet bound to its image
/// and assigned its disc in order, read through the file-metadata mapper with
/// tags that name nothing.
fn map_cue_folder(sheets: Vec<CueSheet>, folder_name: Option<&str>) -> ReleaseValues {
    use crate::import::file_tag_snapshot::{FileObservation, FileTagFact};
    use crate::import::folder_scanner::{
        CandidateFile, CategorizedFiles, FileRole, ScannedFile, SheetAudioFile, SheetBinding,
        SheetDisc,
    };
    let mut entries = Vec::new();
    for (index, sheet) in sheets.into_iter().enumerate() {
        let image = format!("disc{}.flac", index + 1);
        let sheet_name = format!("disc{}.cue", index + 1);
        entries.push(CandidateFile {
            file: ScannedFile::new(PathBuf::from("/source").join(&image), image.clone(), 123, 1)
                .with_test_flac_audio(),
            role: FileRole::Audio,
        });
        entries.push(CandidateFile {
            file: ScannedFile::new(
                PathBuf::from("/source").join(&sheet_name),
                sheet_name,
                123,
                1,
            ),
            role: FileRole::TrackSheet {
                sheet,
                binding: SheetBinding::Resolved {
                    files: vec![SheetAudioFile {
                        file_reference: "image.flac".into(),
                        file_id: image,
                    }],
                },
                disc: SheetDisc::Disc {
                    number: index as u32 + 1,
                },
            },
        });
    }
    let files = CategorizedFiles {
        files: entries,
        parts: Vec::new(),
    };
    let snapshot = FileTagSnapshot {
        scan_generation: 1,
        file_edit_revision: 0,
        embedded_cover: None,
        files: files
            .audio()
            .map(|file| FileTagFact {
                observation: FileObservation {
                    relative_path: file.relative_path.clone(),
                    size: file.size,
                    modified_at_ns: file.modified_at_ns,
                },
                title: None,
                track_artist: None,
                album_title: None,
                album_artist: None,
                year: None,
                numbers: crate::import::track_numbers::TrackNumbers {
                    disc: None,
                    track: None,
                },
                isrc: None,
                copyright: None,
                label: None,
                store: None,
            })
            .collect(),
    };
    file_metadata_values(&files, &snapshot, folder_name)
    .unwrap()
}

#[test]
fn cue_sheet_seeds_one_track_per_cue_entry_not_per_image_file() {
    let mut sheet = cue_sheet(
        "Album Title",
        vec![
            cue_track(1, "Track One"),
            cue_track(2, "Track Two"),
            cue_track(3, "Track Three"),
        ],
    );
    sheet.date = Some("1970".to_string());
    for track in &mut sheet.tracks {
        track.performer = Some("Artist Name".to_string());
    }
    let parsed = map_cue_folder(vec![sheet], Some("Folder Name"));

    // The single-file image is NOT collapsed to one track: one DbTrack per
    // CUE TRACK entry, in order, carrying the CUE's own titles.
    assert_eq!(
        parsed
            .tracks
            .iter()
            .map(|t| t.title.as_str())
            .collect::<Vec<_>>(),
        ["Track One", "Track Two", "Track Three"]
    );
    assert_eq!(crate::import::release_values::value_labels(&parsed), ["1", "2", "3"]);
    assert_eq!(parsed.album_title, "Album Title");
    assert_eq!(parsed.album_year, Some(1970));
    assert_eq!(parsed.album_credits[0].name, "Artist Name");
}

fn cue_track(number: u32, title: &str) -> crate::cue_flac::CueTrack {
    use crate::cue_flac::{CueIndex, CueTrack, CueTrackMode};
    CueTrack {
        number,
        mode: CueTrackMode::Audio,
        title: Some(title.to_string()),
        performer: None,
        indexes: vec![CueIndex {
            number: 1,
            frames: 0,
            file_reference: "image.flac".to_string(),
        }],
        file_reference: "image.flac".to_string(),
        start_cue_frames: 0,
        generated_pregap_frames: None,
        end_cue_frames: None,
    }
}

fn cue_sheet(title: &str, tracks: Vec<crate::cue_flac::CueTrack>) -> CueSheet {
    CueSheet {
        title: Some(title.to_string()),
        performer: Some("Artist Name".to_string()),
        catalog: None,
        date: None,
        ripper: None,
        tracks,
    }
}

/// Multi-disc CUE rip: one sheet per disc. Side is the disc the sheet is
/// assigned; track numbers restart per sheet.
#[test]
fn cue_multi_sheet_assigns_side_per_disc() {
    let disc1 = cue_sheet(
        "Album Title",
        vec![cue_track(1, "D1 T1"), cue_track(2, "D1 T2")],
    );
    let disc2 = cue_sheet(
        "Album Title",
        vec![cue_track(1, "D2 T1"), cue_track(2, "D2 T2")],
    );
    let parsed = map_cue_folder(vec![disc1, disc2], Some("Folder"));

    assert_eq!(
        crate::import::release_values::value_labels(&parsed),
        ["1-1", "1-2", "2-1", "2-2"]
    );
}

/// Copy a fixture into `dest_dir/{name}` and stamp it with the given
/// tag values. Returns the destination path.
fn copy_and_tag(
    source: &Path,
    dest_dir: &Path,
    name: &str,
    tag_type: TagType,
    title: Option<&str>,
    artist: Option<&str>,
    album_title: Option<&str>,
    album_artist: Option<&str>,
    year: Option<u16>,
    track: Option<u32>,
    disk: Option<u32>,
) -> PathBuf {
    let dest = dest_dir.join(name);
    fs::copy(source, &dest).expect("copy fixture");

    let mut tagged = lofty::read_from_path(&dest).expect("read for tagging");
    let mut tag = Tag::new(tag_type);
    if let Some(t) = title {
        tag.set_title(t.to_string());
    }
    if let Some(a) = artist {
        tag.set_artist(a.to_string());
    }
    if let Some(at) = album_title {
        tag.set_album(at.to_string());
    }
    if let Some(aa) = album_artist {
        tag.insert_text(ItemKey::AlbumArtist, aa.to_string());
    }
    if let Some(y) = year {
        tag.set_date(Timestamp {
            year: y,
            month: None,
            day: None,
            hour: None,
            minute: None,
            second: None,
        });
    }
    if let Some(n) = track {
        tag.set_track(n);
    }
    if let Some(d) = disk {
        tag.set_disk(d);
    }
    tagged.insert_tag(tag);
    tagged
        .save_to_path(&dest, WriteOptions::default())
        .expect("save tags");
    dest
}

/// FLAC + Vorbis comments: titles, artist, album, year, and track numbers
/// land on the tracks. The source codec is not release media. No identity rows.
#[test]
fn flac_with_vorbis_comments_basic() {
    let temp = TempDir::new().unwrap();
    let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
    let src2 = fixtures_dir().join("flac").join("02 Test Track 2.flac");

    let f1 = copy_and_tag(
        &src,
        temp.path(),
        "01.flac",
        TagType::VorbisComments,
        Some("Track One"),
        Some("Artist Name"),
        Some("Album Title"),
        Some("Artist Name"),
        Some(1999),
        Some(1),
        None,
    );
    let f2 = copy_and_tag(
        &src2,
        temp.path(),
        "02.flac",
        TagType::VorbisComments,
        Some("Track Two"),
        Some("Artist Name"),
        Some("Album Title"),
        Some("Artist Name"),
        Some(1999),
        Some(2),
        None,
    );

    let parsed = map_tags(&[f1, f2]).unwrap();

    assert_eq!(parsed.album_title, "Album Title");
    assert_eq!(parsed.album_year, Some(1999));
    assert_eq!(parsed.pressing.year, Some(1999));
    assert!(parsed.pressing.facts.media.is_empty());

    assert_eq!(parsed.tracks.len(), 2);
    assert_eq!(parsed.tracks[0].title, "Track One");
    assert_eq!(parsed.tracks[1].title, "Track Two");
    assert_eq!(crate::import::release_values::value_labels(&parsed), ["1", "2"]);

    assert_eq!(
        parsed
            .album_credits
            .iter()
            .map(|credit| credit.name.as_str())
            .collect::<Vec<_>>(),
        ["Artist Name"],
        "single artist for whole album"
    );
    // Every track ARTIST tag is the track's own credit, even when it matches
    // the album artist (as the catalogs' readers credit each track).
    assert!(parsed.tracks.iter().all(|track| matches!(
        &track.credits,
        TrackCredits::Own(credits) if credits.len() == 1 && credits[0].name == "Artist Name"
    )));
}

/// Multi-disc rip: DISCNUMBER + TRACKNUMBER produce side groupings.
#[test]
fn flac_multi_disc_groups_by_discnumber() {
    let temp = TempDir::new().unwrap();
    let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");

    let mut files = Vec::new();
    for (disc, track, name) in [
        (1u32, 1u32, "d1t1.flac"),
        (1, 2, "d1t2.flac"),
        (2, 1, "d2t1.flac"),
        (2, 2, "d2t2.flac"),
    ] {
        files.push(copy_and_tag(
            &src,
            temp.path(),
            name,
            TagType::VorbisComments,
            Some(&format!("Track {}-{}", disc, track)),
            Some("Album Artist"),
            Some("Album Title"),
            Some("Album Artist"),
            None,
            Some(track),
            Some(disc),
        ));
    }

    let parsed = map_tags(&files).unwrap();

    assert_eq!(crate::import::release_values::value_labels(&parsed), ["1-1", "1-2", "2-1", "2-2"]);
    assert!(
        parsed.mediums.iter().all(|medium| medium.carrier.is_none()),
        "tags state no carrier"
    );
}

/// The largest number a tag can state is a track's number as stated.
#[test]
fn the_largest_tag_number_is_kept_as_stated() {
    let temp = TempDir::new().unwrap();
    let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");

    let file = copy_and_tag(
        &src,
        temp.path(),
        "wrapped-tag-numbers.flac",
        TagType::VorbisComments,
        Some("Track Title"),
        Some("Artist Name"),
        Some("Album Title"),
        Some("Artist Name"),
        None,
        Some(u32::MAX),
        Some(u32::MAX),
    );

    let parsed = map_tags(&[file]).unwrap();

    assert_eq!(draft_numbers(parsed), [u32::MAX]);
}

/// Per-track artist different from album artist → adds an extra
/// artist row, and every track ARTIST tag emits a junction row
/// (the matching one points at the album artist; the divergent one
/// at the per-track artist). The external-source mappers likewise emit a
/// junction for every credit.
#[test]
fn flac_per_track_artist_emits_junction_row() {
    let temp = TempDir::new().unwrap();
    let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
    let src2 = fixtures_dir().join("flac").join("02 Test Track 2.flac");

    let f1 = copy_and_tag(
        &src,
        temp.path(),
        "01.flac",
        TagType::VorbisComments,
        Some("Track One"),
        Some("Album Artist"),
        Some("Album Title"),
        Some("Album Artist"),
        None,
        Some(1),
        None,
    );
    let f2 = copy_and_tag(
        &src2,
        temp.path(),
        "02.flac",
        TagType::VorbisComments,
        Some("Track Two"),
        Some("Featured Artist"),
        Some("Album Title"),
        Some("Album Artist"),
        None,
        Some(2),
        None,
    );

    let parsed = map_tags(&[f1, f2]).unwrap();

    let track_artist = |index: usize| match &parsed.tracks[index].credits {
        TrackCredits::Own(credits) => credits
            .iter()
            .map(|credit| credit.name.as_str())
            .collect::<Vec<_>>(),
        TrackCredits::AlbumArtists => Vec::new(),
    };
    assert_eq!(track_artist(0), ["Album Artist"]);
    assert_eq!(track_artist(1), ["Featured Artist"]);

    // The divergent per-track artist is an album artist too, after the
    // album's own: the file-tag reader credits the album with every artist
    // its tracks name.
    assert_eq!(
        parsed
            .album_credits
            .iter()
            .map(|credit| credit.name.as_str())
            .collect::<Vec<_>>(),
        ["Album Artist", "Featured Artist"]
    );
}

/// Missing optional tags (no year, no DISCNUMBER) → year is None,
/// all tracks land on side 1, track numbers fall back to file order.
#[test]
fn missing_optional_tags_yields_none_and_defaults() {
    let temp = TempDir::new().unwrap();
    let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
    let src2 = fixtures_dir().join("flac").join("02 Test Track 2.flac");

    let f1 = copy_and_tag(
        &src,
        temp.path(),
        "01.flac",
        TagType::VorbisComments,
        Some("Track One"),
        Some("Artist"),
        Some("Album"),
        None,
        None,
        None,
        None,
    );
    let f2 = copy_and_tag(
        &src2,
        temp.path(),
        "02.flac",
        TagType::VorbisComments,
        Some("Track Two"),
        Some("Artist"),
        Some("Album"),
        None,
        None,
        None,
        None,
    );

    let parsed = map_tags(&[f1, f2]).unwrap();
    assert_eq!(parsed.album_year, None);
    assert_eq!(parsed.pressing.year, None);
    assert_eq!(crate::import::release_values::value_labels(&parsed), ["1", "2"]);
}

/// Supplied numbers remain unchanged; absent numbers use ordered position.
#[test]
fn partial_track_numbers_default_to_order() {
    let temp = TempDir::new().unwrap();
    let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
    let src2 = fixtures_dir().join("flac").join("02 Test Track 2.flac");

    // Duplicate displayed numbers do not determine track identity or order.
    let f1 = copy_and_tag(
        &src,
        temp.path(),
        "a.flac",
        TagType::VorbisComments,
        Some("Track A"),
        Some("Artist"),
        Some("Album"),
        None,
        None,
        None,
        None,
    );
    let f2 = copy_and_tag(
        &src2,
        temp.path(),
        "b.flac",
        TagType::VorbisComments,
        Some("Track B"),
        Some("Artist"),
        Some("Album"),
        None,
        None,
        Some(1),
        None,
    );

    let parsed = map_tags(&[f1, f2]).unwrap();
    assert_eq!(
        draft_numbers(parsed),
        [1, 1],
        "an untagged file uses its ordered position"
    );
}

/// The album-title ladder (ALBUM tag → folder name → empty) and the
/// album-artist ladder (ALBUMARTIST → ARTIST → empty), with blank and
/// whitespace-only tags dropping to `None` and taking the same ladder.
/// The file-metadata path never hard-fails on a missing album-level tag — the
/// editable form gates save on the non-empty fields.
#[test]
fn album_and_artist_fallback_ladder() {
    struct Row {
        name: &'static str,
        album: Option<&'static str>,
        artist: Option<&'static str>,
        album_artist: Option<&'static str>,
        folder: Option<&'static str>,
        expect_title: &'static str,
        expect_artist: &'static str,
    }
    let rows = [
        // Album title ladder.
        Row {
            name: "no-album-folder",
            album: None,
            artist: Some("Track Artist"),
            album_artist: None,
            folder: Some("Folder Name"),
            expect_title: "Folder Name",
            expect_artist: "Track Artist",
        },
        Row {
            name: "no-album-no-folder",
            album: None,
            artist: Some("Track Artist"),
            album_artist: None,
            folder: None,
            expect_title: "",
            expect_artist: "Track Artist",
        },
        Row {
            name: "empty-album-folder",
            album: Some(""),
            artist: Some("Track Artist"),
            album_artist: None,
            folder: Some("Folder Name"),
            expect_title: "Folder Name",
            expect_artist: "Track Artist",
        },
        Row {
            name: "ws-album-folder",
            album: Some("   "),
            artist: Some("Track Artist"),
            album_artist: None,
            folder: Some("Folder Name"),
            expect_title: "Folder Name",
            expect_artist: "Track Artist",
        },
        // Album-artist ladder: prefer ALBUMARTIST, fall back to ARTIST,
        // then empty; blank tags drop to None.
        Row {
            name: "albumartist-preferred",
            album: Some("Album Title"),
            artist: Some("Track Artist"),
            album_artist: Some("Album Artist"),
            folder: None,
            expect_title: "Album Title",
            expect_artist: "Album Artist",
        },
        Row {
            name: "artist-fallback",
            album: Some("Album Title"),
            artist: Some("Track Artist"),
            album_artist: None,
            folder: None,
            expect_title: "Album Title",
            expect_artist: "Track Artist",
        },
        Row {
            name: "no-artist-empty",
            album: Some("Album Title"),
            artist: None,
            album_artist: None,
            folder: None,
            expect_title: "Album Title",
            expect_artist: "",
        },
        Row {
            name: "blank-artist-empty",
            album: Some("Album Title"),
            artist: Some(""),
            album_artist: Some(""),
            folder: None,
            expect_title: "Album Title",
            expect_artist: "",
        },
    ];

    let temp = TempDir::new().unwrap();
    let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
    for row in rows {
        let f = copy_and_tag(
            &src,
            temp.path(),
            &format!("{}.flac", row.name),
            TagType::VorbisComments,
            Some("Track Title"),
            row.artist,
            row.album,
            row.album_artist,
            None,
            None,
            None,
        );
        let parsed = map_tags_with_folder(&[f], row.folder).unwrap();
        assert_eq!(
            parsed.album_title, row.expect_title,
            "title for {}",
            row.name
        );
        // The first album credit is the album artist; a divergent per-track
        // ARTIST may add a second, which isn't the ladder's concern here.
        assert_eq!(
            parsed.album_credits[0].name, row.expect_artist,
            "artist for {}",
            row.name
        );
        assert_eq!(parsed.tracks[0].title, "Track Title", "{}", row.name);
    }
}

/// A file stating no number takes its place on its own disc.
#[test]
fn absent_numbers_take_their_place_on_their_own_disc() {
    let temp = TempDir::new().unwrap();
    let src = fixtures_dir().join("flac/01 Test Track 1.flac");
    let files = (1..=2)
        .map(|disc| {
            copy_and_tag(
                &src,
                temp.path(),
                &format!("disc{disc}.flac"),
                TagType::VorbisComments,
                Some("Track"),
                Some("Artist"),
                Some("Album"),
                None,
                None,
                None,
                Some(disc),
            )
        })
        .collect::<Vec<_>>();
    let parsed = map_tags(&files).unwrap();
    assert_eq!(
        draft_numbers(parsed),
        [1, 1],
        "each disc's first track is its number one"
    );
}

#[test]
fn cue_numbers_are_preserved_instead_of_recreated() {
    let sheet = cue_sheet(
        "Album",
        vec![cue_track(4, "Fourth"), cue_track(7, "Seventh")],
    );
    let parsed = map_cue_folder(vec![sheet], None);
    assert_eq!(crate::import::release_values::value_labels(&parsed), ["4", "7"]);
}

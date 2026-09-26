use super::*;
use crate::import::folder_scanner::{CandidateFile, CandidateFileEdits, ScannedFile, SheetDisc};
use crate::import::watched_folder::host_root;

/// A folder of two FLAC tracks at `relative` under the watched root `root`.
fn folder_at(root: &str, relative: &str) -> FolderCandidate {
    let path = PathBuf::from(host_root(root)).join(relative);
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    FolderCandidate {
        path: path.clone(),
        file_root: path.clone(),
        name,
        watched_folder_path: host_root(root),
        scope: ReleaseFileScope::Direct,
        file_edit_revision: 0,
        display_path: relative.into(),
        grouping: None,
        files: CategorizedFiles {
            files: ["01.flac", "02.flac"]
                .into_iter()
                .map(|name| CandidateFile {
                    file: ScannedFile::new(path.join(name), name.into(), 100, 12)
                        .with_test_flac_audio(),
                    role: FileRole::Audio,
                    proposed_audio: true,
                })
                .collect(),
            parts: Vec::new(),
        },
    }
}

/// `members` read as one release, with no file decision stored for any.
fn composed(members: &[FolderCandidate]) -> FolderCandidate {
    let members: Vec<(FolderCandidate, CandidateFileEdits)> = members
        .iter()
        .map(|member| (member.clone(), CandidateFileEdits::default()))
        .collect();
    let (mut release, inherited) =
        compose("grouping:test", &members[0].0.watched_folder_path, &members, &[]).unwrap();
    release.files.apply_candidate_file_edits(&inherited).unwrap();
    release
}

fn plain(members: &[FolderCandidate]) -> Vec<(FolderCandidate, CandidateFileEdits)> {
    members
        .iter()
        .map(|member| (member.clone(), CandidateFileEdits::default()))
        .collect()
}

fn layout(release: &FolderCandidate) -> Vec<(Option<i32>, Option<i32>)> {
    crate::import::track_slots::direct_entry_track_rows(&release.files)
        .iter()
        .map(|track| (track.side, track.track_number))
        .collect()
}

/// Two disc folders under one album read as the album would: each file under
/// its disc folder's name, each disc a run of its own, tracks numbered from
/// one on each — and not one file's place on disk moves.
#[test]
fn folders_under_one_folder_keep_their_paths_below_it_and_a_disc_each() {
    let members = [
        folder_at("/music", "Artist/Album/Disc 1"),
        folder_at("/music", "Artist/Album/Disc 2"),
    ];
    let release = composed(&members);

    assert_eq!(release.path, PathBuf::from(host_root("/music")).join("Artist/Album"));
    assert_eq!(release.display_path, "Artist/Album/Disc 1");
    assert_eq!(release.key(), "grouping:test");
    assert_eq!(
        release
            .files
            .release_files()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["Disc 1/01.flac", "Disc 1/02.flac", "Disc 2/01.flac", "Disc 2/02.flac"]
    );
    assert_eq!(
        release
            .files
            .release_files()
            .map(|file| &file.path)
            .collect::<Vec<_>>(),
        members
            .iter()
            .flat_map(|member| member.files.release_files().map(|file| &file.path))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        release.source_folders(),
        members.iter().map(|member| member.path.clone()).collect::<Vec<_>>()
    );
    assert_eq!(
        layout(&release),
        [(Some(1), Some(1)), (Some(1), Some(2)), (Some(2), Some(1)), (Some(2), Some(2))]
    );
}

/// Folders under different watched folders share no folder, so each takes a
/// prefix of its position and name, in the order they were given.
#[test]
fn folders_under_different_roots_take_their_position_and_name() {
    let release = composed(&[
        folder_at("/music", "Volume B"),
        folder_at("/archive", "Volume A"),
    ]);
    assert_eq!(
        release
            .files
            .release_files()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>(),
        [
            "01 - Volume B/01.flac",
            "01 - Volume B/02.flac",
            "02 - Volume A/01.flac",
            "02 - Volume A/02.flac"
        ]
    );
    assert_eq!(
        layout(&release),
        [(Some(1), Some(1)), (Some(1), Some(2)), (Some(2), Some(1)), (Some(2), Some(2))]
    );
}

#[test]
fn rejects_duplicate_members_and_overlapping_files() {
    let first = folder_at("/music", "Volume A");
    assert!(compose("grouping:test", &host_root("/music"), &plain(&[first.clone(), first.clone()]), &[]).is_err());
    let mut second = folder_at("/music", "Volume B");
    second.files.files[0] = first.files.files[0].clone();
    assert!(compose("grouping:test", &host_root("/music"), &plain(&[first, second]), &[]).is_err());
}

/// A folder read from its file tags keeps the release's own disc layout,
/// whatever disc number the tags state.
#[test]
fn file_metadata_keeps_the_releases_numbering_instead_of_the_tags() {
    use crate::import::file_tag_snapshot::{FileObservation, FileTagFact, FileTagSnapshot};
    let clock = coven::FixedClock(
        chrono::DateTime::parse_from_rfc3339("2026-01-15T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    let release = composed(&[folder_at("/music", "Volume B"), folder_at("/archive", "Volume A")]);
    let snapshot = FileTagSnapshot {
        scan_generation: 1,
        file_edit_revision: 0,
        embedded_cover: None,
        files: release
            .files
            .audio()
            .enumerate()
            .map(|(index, file)| FileTagFact {
                observation: FileObservation {
                    relative_path: file.relative_path.clone(),
                    size: file.size,
                    modified_at_ns: file.modified_at_ns,
                },
                title: Some(format!("Tagged Track {index}")),
                track_artist: Some("Artist".into()),
                album_title: Some("Album".into()),
                album_artist: Some("Artist".into()),
                year: None,
                track_number: Some(1),
                disc_number: Some(7),
            })
            .collect(),
    };
    let edit = release
        .file_tag_edit(&snapshot, &clock, &coven::UuidProvider)
        .unwrap();
    assert_eq!(
        edit.tracks
            .iter()
            .map(|track| (track.side, track.track_number))
            .collect::<Vec<_>>(),
        layout(&release)
    );
    assert_eq!(edit.tracks[2].title, "Tagged Track 2");
}

/// A file taken out of a folder's tracklist stays out of the release the
/// folder joins: the folder's decisions are the release's starting point.
#[test]
fn a_members_file_decisions_carry_into_the_release() {
    use crate::import::folder_scanner::FileRoleChoice;
    let first = folder_at("/music", "Album/Disc 1");
    let mut decided = CandidateFileEdits::default();
    decided
        .file_roles
        .set("02.flac".into(), FileRoleChoice::NotATrack);
    let members = vec![
        (first, decided),
        (folder_at("/music", "Album/Disc 2"), CandidateFileEdits::default()),
    ];
    let (mut release, inherited) =
        compose("grouping:test", &host_root("/music"), &members, &[]).unwrap();
    release.files.apply_candidate_file_edits(&inherited).unwrap();
    assert_eq!(release.files.audio().count(), 3);
    assert_eq!(
        layout(&release),
        [(Some(1), Some(1)), (Some(2), Some(1)), (Some(2), Some(2))]
    );
}

/// A folder carved by track sheets takes a disc per sheet within its own run,
/// after its loose audio; the next folder's run starts after them.
#[test]
fn sheets_take_discs_within_their_folders_run() {
    use crate::cue_flac::{CuePregap, CueSheet, CueTrack, CueTrackMode};
    use crate::import::folder_scanner::SheetAudioFile;

    let mut first = folder_at("/music", "Album/Disc 1");
    let sheet_name = "01.flac.cue".to_string();
    first.files.files.push(CandidateFile {
        file: ScannedFile::new(first.path.join(&sheet_name), sheet_name, 100, 12),
        role: FileRole::TrackSheet {
            sheet: CueSheet {
                title: None,
                performer: None,
                catalog: None,
                date: None,
                ripper: None,
                tracks: vec![CueTrack {
                    number: 1,
                    mode: CueTrackMode::Audio,
                    title: None,
                    performer: None,
                    indexes: Vec::new(),
                    file_reference: "01.flac".into(),
                    start_cue_frames: 0,
                    pregap: CuePregap::None,
                    end_cue_frames: None,
                }],
            },
            binding: SheetBinding::Resolved {
                files: vec![SheetAudioFile {
                    file_reference: "01.flac".into(),
                    file_id: "01.flac".into(),
                }],
            },
            disc: SheetDisc::Disc { number: 1 },
        },
        proposed_audio: false,
    });
    let release = composed(&[first, folder_at("/music", "Album/Disc 2")]);
    // Disc 1's loose `02.flac` is its first disc, its sheet the second, and
    // Disc 2's loose audio the third.
    let sides: Vec<Option<i32>> = layout(&release).iter().map(|(side, _)| *side).collect();
    assert_eq!(sides.iter().filter(|side| **side == Some(1)).count(), 1);
    assert_eq!(sides.iter().filter(|side| **side == Some(2)).count(), 1);
    assert_eq!(sides.iter().filter(|side| **side == Some(3)).count(), 2);
    assert_eq!(
        crate::import::track_slots::audio_units(&release.files),
        crate::import::track_slots::direct_entry_track_rows(&release.files)
            .into_iter()
            .map(|track| track.file.unwrap())
            .collect::<Vec<_>>()
    );
}

/// The folder picked releases all sit directly in, as the rule reads it.
fn parent_of(members: &[FolderCandidate]) -> Option<PathBuf> {
    shared_parent(
        members
            .iter()
            .map(|member| (member.watched_folder_path.as_str(), member.file_root.as_path())),
    )
}

/// Releases sit in one folder when each is directly in it — two discs of
/// three as much as all of them — and in none when they are at different
/// places, at different depths, one inside another, under different watched
/// folders, or directly in the watched folder.
#[test]
fn releases_sit_in_one_folder_only_when_each_is_directly_in_it() {
    let album = PathBuf::from(host_root("/music")).join("Artist/Album");
    let disc = |relative: &str| folder_at("/music", relative);
    assert_eq!(
        parent_of(&[disc("Artist/Album/Disc 1"), disc("Artist/Album/Disc 2")]),
        Some(album.clone())
    );
    assert_eq!(
        parent_of(&[
            disc("Artist/Album/Disc 1"),
            disc("Artist/Album/Disc 2"),
            disc("Artist/Album/Disc 3"),
        ]),
        Some(album)
    );
    assert_eq!(
        parent_of(&[disc("Artist/Album/Disc 1"), disc("Artist/Other/Disc 1")]),
        None
    );
    assert_eq!(
        parent_of(&[disc("Artist/Album/Disc 1"), disc("Artist/Album/Disc 2/Bonus")]),
        None
    );
    assert_eq!(
        parent_of(&[disc("Artist/Album"), disc("Artist/Album/Disc 1")]),
        None
    );
    assert_eq!(
        parent_of(&[folder_at("/music", "Album/Disc 1"), folder_at("/other", "Album/Disc 2")]),
        None
    );
    assert_eq!(parent_of(&[disc("Album A"), disc("Album B")]), None);
}

/// The folder's files are the release's own, at their paths below it, beside
/// the discs under theirs; files from anywhere else are refused.
#[test]
fn the_folder_the_releases_sit_in_gives_its_files_to_the_release() {
    let album = PathBuf::from(host_root("/music")).join("Album");
    let cover = CandidateFile {
        file: ScannedFile::new(album.join("cover.jpg"), "cover.jpg".into(), 10, 1),
        role: FileRole::Artwork,
        proposed_audio: false,
    };
    let members = plain(&[
        folder_at("/music", "Album/Disc 1"),
        folder_at("/music", "Album/Disc 2"),
    ]);
    let (release, _) = compose(
        "grouping:test",
        &host_root("/music"),
        &members,
        std::slice::from_ref(&cover),
    )
    .unwrap();
    assert_eq!(release.path, album);
    assert_eq!(
        release
            .files
            .files
            .iter()
            .map(|entry| entry.file.relative_path.as_str())
            .collect::<Vec<_>>(),
        [
            "cover.jpg",
            "Disc 1/01.flac",
            "Disc 1/02.flac",
            "Disc 2/01.flac",
            "Disc 2/02.flac"
        ]
    );
    assert_eq!(release.files.artwork().count(), 1);

    let apart = plain(&[
        folder_at("/music", "Album/Disc 1"),
        folder_at("/music", "Other/Disc 2"),
    ]);
    assert!(compose(
        "grouping:test",
        &host_root("/music"),
        &apart,
        std::slice::from_ref(&cover)
    )
    .is_err());
    let elsewhere = CandidateFile {
        file: ScannedFile::new(
            PathBuf::from(host_root("/music")).join("Other/cover.jpg"),
            "cover.jpg".into(),
            10,
            1,
        ),
        ..cover
    };
    assert!(compose("grouping:test", &host_root("/music"), &members, &[elsewhere]).is_err());
}

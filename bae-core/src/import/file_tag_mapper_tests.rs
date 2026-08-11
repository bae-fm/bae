use super::*;
use coven::FixedClock;
use coven::SequentialIdProvider;
use lofty::config::WriteOptions;
use lofty::tag::items::Timestamp;
use lofty::tag::{Tag, TagType};
use std::fs;
use tempfile::TempDir;

/// Run the mapper with deterministic fakes. Exercises the real
/// `map_file_tags_to_db`; only the clock/id inputs are faked.
fn map_tags(audio_files: &[PathBuf]) -> Result<ParsedAlbum, ImportError> {
    map_tags_with_folder(audio_files, None)
}

fn map_tags_with_folder(
    audio_files: &[PathBuf],
    folder_name: Option<&str>,
) -> Result<ParsedAlbum, ImportError> {
    let clock = FixedClock(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    let ids = SequentialIdProvider::new("ft");
    map_file_tags_to_db(audio_files, folder_name, &clock, &ids)
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

#[test]
fn cue_sheet_seeds_one_track_per_cue_entry_not_per_image_file() {
    use crate::cue_flac::{CueIndex, CuePregap, CueSheet, CueTrack, CueTrackMode};
    let mk = |number: u32, title: &str| CueTrack {
        number,
        mode: CueTrackMode::Audio,
        title: Some(title.to_string()),
        performer: Some("Artist Name".to_string()),
        indexes: vec![CueIndex {
            number: 1,
            frames: 0,
            file_reference: "image.flac".to_string(),
        }],
        file_reference: "image.flac".to_string(),
        start_cue_frames: 0,
        pregap: CuePregap::None,
        end_cue_frames: None,
    };
    let sheet = CueSheet {
        title: Some("Album Title".to_string()),
        performer: Some("Artist Name".to_string()),
        catalog: None,
        date: Some("1970".to_string()),
        tracks: vec![mk(1, "Track One"), mk(2, "Track Two"), mk(3, "Track Three")],
    };
    let clock = FixedClock(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    let ids = SequentialIdProvider::new("cue");
    // Nonexistent audio path: the codec probe yields None (fine — the
    // seed still carries every CUE track); the point is track count.
    let audio = Path::new("/nonexistent/image.flac");
    let parsed =
        map_cue_sheets_to_db(&[&sheet], &[audio], Some("Folder Name"), &clock, &ids).unwrap();

    // The single-file image is NOT collapsed to one track: one DbTrack per
    // CUE TRACK entry, in order, carrying the CUE's own titles.
    assert_eq!(parsed.tracks.len(), 3);
    assert_eq!(parsed.tracks[0].title, "Track One");
    assert_eq!(parsed.tracks[1].title, "Track Two");
    assert_eq!(parsed.tracks[2].title, "Track Three");
    assert!(parsed.tracks.iter().all(|t| t.side == 1));
    assert_eq!(
        parsed
            .tracks
            .iter()
            .map(|t| t.track_number)
            .collect::<Vec<_>>(),
        vec![Some(1), Some(2), Some(3)]
    );
    assert_eq!(parsed.album.title, "Album Title");
    assert_eq!(parsed.album.year, Some(1970));
    assert_eq!(parsed.artists[0].name, "Artist Name");
}

fn cue_track(number: u32, title: &str) -> crate::cue_flac::CueTrack {
    use crate::cue_flac::{CueIndex, CuePregap, CueTrack, CueTrackMode};
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
        pregap: CuePregap::None,
        end_cue_frames: None,
    }
}

fn cue_sheet(title: &str, tracks: Vec<crate::cue_flac::CueTrack>) -> CueSheet {
    CueSheet {
        title: Some(title.to_string()),
        performer: Some("Artist Name".to_string()),
        catalog: None,
        date: None,
        tracks,
    }
}

/// Multi-disc CUE rip: one sheet per disc. Side is the 1-based disc index
/// (sheet order); track numbers restart per sheet.
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
    let clock = FixedClock(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    let ids = SequentialIdProvider::new("cue");
    let a1 = Path::new("/nonexistent/disc1.flac");
    let a2 = Path::new("/nonexistent/disc2.flac");

    let parsed =
        map_cue_sheets_to_db(&[&disc1, &disc2], &[a1, a2], Some("Folder"), &clock, &ids).unwrap();

    assert_eq!(parsed.tracks.len(), 4);
    assert_eq!(parsed.tracks[0].side, 1);
    assert_eq!(parsed.tracks[0].track_number, Some(1));
    assert_eq!(parsed.tracks[1].side, 1);
    assert_eq!(parsed.tracks[1].track_number, Some(2));
    assert_eq!(parsed.tracks[2].side, 2);
    assert_eq!(parsed.tracks[2].track_number, Some(1));
    assert_eq!(parsed.tracks[3].side, 2);
    assert_eq!(parsed.tracks[3].track_number, Some(2));
}

/// No sheets → error (an album has at least one disc).
#[test]
fn cue_empty_sheets_returns_error() {
    let clock = FixedClock(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    let ids = SequentialIdProvider::new("cue");
    let err = map_cue_sheets_to_db(&[], &[], Some("Folder"), &clock, &ids)
        .expect_err("expected empty sheets to error");
    assert!(
        matches!(&err, ImportError::FileTags { detail } if detail.contains("at least one sheet")),
        "got: {err}"
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

/// FLAC + Vorbis comments: titles, artist, album, year, track numbers
/// land on the tracks. Format is "FLAC". No identity rows.
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

    assert_eq!(parsed.album.title, "Album Title");
    assert_eq!(parsed.album.year, Some(1999));
    assert_eq!(parsed.release.pressing.year, Some(1999));
    assert_eq!(parsed.release.pressing.format.as_deref(), Some("FLAC"));
    assert_eq!(
        parsed.release.metadata_source,
        ReleaseMetadataSource::FileTags
    );
    assert!(parsed.release.metadata_source_release_id.is_none());

    assert_eq!(parsed.tracks.len(), 2);
    assert_eq!(parsed.tracks[0].title, "Track One");
    assert_eq!(parsed.tracks[0].side, 1);
    assert_eq!(parsed.tracks[0].track_number, Some(1));
    assert_eq!(parsed.tracks[1].title, "Track Two");
    assert_eq!(parsed.tracks[1].track_number, Some(2));

    assert_eq!(parsed.artists.len(), 1, "single artist for whole album");
    assert_eq!(parsed.artists[0].name, "Artist Name");
    assert!(parsed.album_artists.is_empty());
    assert_eq!(
        parsed.track_artists.len(),
        2,
        "every track ARTIST tag emits a junction row, even when it \
         matches the album artist (mirrors MB/Discogs mappers)"
    );
    assert!(parsed
        .track_artists
        .iter()
        .all(|ta| ta.artist_id == parsed.artists[0].id));

    assert!(
        parsed.identities.is_empty(),
        "Unknown imports never claim identity"
    );
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

    assert_eq!(parsed.tracks.len(), 4);
    assert_eq!(parsed.tracks[0].side, 1);
    assert_eq!(parsed.tracks[0].track_number, Some(1));
    assert_eq!(parsed.tracks[1].side, 1);
    assert_eq!(parsed.tracks[1].track_number, Some(2));
    assert_eq!(parsed.tracks[2].side, 2);
    assert_eq!(parsed.tracks[2].track_number, Some(1));
    assert_eq!(parsed.tracks[3].side, 2);
    assert_eq!(parsed.tracks[3].track_number, Some(2));
}

#[test]
fn out_of_range_tag_numbers_do_not_wrap_negative() {
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

    assert_eq!(parsed.tracks[0].side, i32::MAX);
    assert_eq!(parsed.tracks[0].track_number, None);
}

/// Per-track artist different from album artist → adds an extra
/// artist row, and every track ARTIST tag emits a junction row
/// (the matching one points at the album artist; the divergent one
/// at the per-track artist). Mirrors map_mb_response_to_db /
/// map_discogs_to_db, which emit a junction for every credit.
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

    assert_eq!(parsed.artists.len(), 2);
    assert_eq!(parsed.artists[0].name, "Album Artist");
    assert_eq!(parsed.artists[1].name, "Featured Artist");

    assert_eq!(parsed.track_artists.len(), 2);
    assert_eq!(parsed.track_artists[0].track_id, parsed.tracks[0].id);
    assert_eq!(parsed.track_artists[0].artist_id, parsed.artists[0].id);
    assert_eq!(parsed.track_artists[1].track_id, parsed.tracks[1].id);
    assert_eq!(parsed.track_artists[1].artist_id, parsed.artists[1].id);

    // The divergent per-track artist joins the album-artists junction: the
    // file-tag path scopes album artists to the whole pool, so any artist
    // introduced by a per-track credit also becomes an album artist.
    assert_eq!(parsed.album_artists.len(), 1);
    assert_eq!(parsed.album_artists[0].artist_id, parsed.artists[1].id);
    assert_eq!(parsed.album_artists[0].position, 1);
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
    assert_eq!(parsed.album.year, None);
    assert_eq!(parsed.release.pressing.year, None);
    assert_eq!(parsed.tracks[0].side, 1);
    assert_eq!(parsed.tracks[1].side, 1);
    assert_eq!(parsed.tracks[0].track_number, Some(1));
    assert_eq!(parsed.tracks[1].track_number, Some(2));
}

/// A side where some files carry TRACKNUMBER and some don't: the
/// untagged files must NOT get a positional fallback (it would collide
/// with the real tag values) — they stay `None` for the user to assign.
#[test]
fn partial_tracknumber_side_leaves_untagged_none() {
    let temp = TempDir::new().unwrap();
    let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
    let src2 = fixtures_dir().join("flac").join("02 Test Track 2.flac");

    // First file untagged, second tagged TRACKNUMBER=1 — old positional
    // fallback would assign the first file position 1 too, colliding.
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
        parsed.tracks[0].track_number, None,
        "untagged file on a partially-tagged side stays None"
    );
    assert_eq!(parsed.tracks[1].track_number, Some(1));
}

/// The album-title ladder (ALBUM tag → folder name → empty) and the
/// album-artist ladder (ALBUMARTIST → ARTIST → empty), with blank and
/// whitespace-only tags dropping to `None` and taking the same ladder.
/// The Unknown path never hard-fails on a missing album-level tag — the
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
            parsed.album.title, row.expect_title,
            "title for {}",
            row.name
        );
        // artists[0] is always the primary (album) artist; a divergent
        // per-track ARTIST may add a second row, which isn't the ladder's
        // concern here.
        assert_eq!(
            parsed.artists[0].name, row.expect_artist,
            "artist for {}",
            row.name
        );
        assert_eq!(parsed.album.artist_id, parsed.artists[0].id, "{}", row.name);
        assert_eq!(parsed.tracks[0].title, "Track Title", "{}", row.name);
    }
}

/// All tracks completely untagged → folder-name title, empty artist,
/// filename-stem track titles. A fully-untagged rip is still importable
/// via the editable form.
#[test]
fn all_files_untagged_seeds_from_folder_and_filenames() {
    let temp = TempDir::new().unwrap();
    let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
    let dest = temp.path().join("untitled-track.flac");
    fs::copy(&src, &dest).unwrap();
    // Don't write any tags — the source FLAC fixture only carries an
    // "encoder=Lavf..." comment which is not ALBUM/ARTIST/TITLE.

    let parsed = map_tags_with_folder(&[dest], Some("Mystery Rip")).unwrap();
    assert_eq!(parsed.album.title, "Mystery Rip");
    assert_eq!(parsed.artists[0].name, "");
    assert_eq!(parsed.tracks[0].title, "untitled-track");
}

/// Empty input → error.
#[test]
fn empty_input_returns_error() {
    let err = map_tags(&[]).unwrap_err();
    assert!(
        matches!(&err, ImportError::FileTags { detail } if detail.contains("at least one")),
        "got: {err}"
    );
}

/// Title falls back to the filename stem when the TITLE tag is
/// absent (some rips only have a partial set). Other tracks in the
/// same rip can still satisfy the presence check.
#[test]
fn missing_title_falls_back_to_filename_stem() {
    let temp = TempDir::new().unwrap();
    let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
    let src2 = fixtures_dir().join("flac").join("02 Test Track 2.flac");

    let f1 = copy_and_tag(
        &src,
        temp.path(),
        "no-title.flac",
        TagType::VorbisComments,
        None,
        None,
        None,
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
        Some("Has Title"),
        Some("Artist"),
        Some("Album"),
        None,
        None,
        None,
        None,
    );

    let parsed = map_tags(&[f1, f2]).unwrap();
    assert_eq!(parsed.tracks[0].title, "no-title");
    assert_eq!(parsed.tracks[1].title, "Has Title");
}

/// MP3 with ID3v2 tags: same projection rules apply, format
/// derives to "MP3". Uses the production MP3 encoder to mint a
/// short silence file at test time — no checked-in MP3 fixture
/// needed.
#[test]
fn mp3_with_id3v2_tags() {
    crate::audio_codec::init();
    // 0.5s of stereo 44.1kHz silence is enough for FFmpeg to emit a
    // valid MP3 the tag writer can attach to.
    let samples = vec![0i32; 44_100 / 2 * 2];
    let mp3_bytes = crate::audio_codec::encode_i32(
        crate::audio_codec::EncodeFormat::Mp3 { bitrate_kbps: 320 },
        &samples,
        44_100,
        2,
    )
    .expect("encode mp3");

    let temp = TempDir::new().unwrap();
    let mp3_path = temp.path().join("01.mp3");
    fs::write(&mp3_path, &mp3_bytes).unwrap();

    let f1 = copy_and_tag(
        &mp3_path,
        temp.path(),
        "tagged.mp3",
        TagType::Id3v2,
        Some("MP3 Track"),
        Some("MP3 Artist"),
        Some("MP3 Album"),
        Some("MP3 Artist"),
        Some(2010),
        Some(1),
        None,
    );

    let parsed = map_tags(&[f1]).unwrap();
    assert_eq!(parsed.album.title, "MP3 Album");
    assert_eq!(parsed.album.year, Some(2010));
    assert_eq!(parsed.release.pressing.format.as_deref(), Some("MP3"));
    assert_eq!(parsed.tracks.len(), 1);
    assert_eq!(parsed.tracks[0].title, "MP3 Track");
    assert_eq!(parsed.artists[0].name, "MP3 Artist");
    assert!(parsed.identities.is_empty());
}

/// M4A with MP4 ilst tags. Format derives from the probed codec, so
/// ALAC-in-MP4 does not collapse to a container label.
#[test]
fn m4a_with_mp4_ilst_tags() {
    let temp = TempDir::new().unwrap();
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-fixtures")
        .join("alac")
        .join("silence-alac.m4a");

    let f1 = copy_and_tag(
        &src,
        temp.path(),
        "01.m4a",
        TagType::Mp4Ilst,
        Some("Track One"),
        Some("Artist Name"),
        Some("Album Title"),
        Some("Artist Name"),
        Some(2020),
        Some(1),
        None,
    );

    let parsed = map_tags(&[f1]).unwrap();

    assert_eq!(parsed.album.title, "Album Title");
    assert_eq!(parsed.album.year, Some(2020));
    assert_eq!(parsed.release.pressing.format.as_deref(), Some("ALAC"));
    assert_eq!(parsed.tracks.len(), 1);
    assert_eq!(parsed.tracks[0].title, "Track One");
    assert!(parsed.identities.is_empty());
}

#[test]
fn aac_m4a_with_mp4_ilst_tags_uses_aac_label() {
    let temp = TempDir::new().unwrap();
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-fixtures")
        .join("alac")
        .join("silence-aac.m4a");

    let f1 = copy_and_tag(
        &src,
        temp.path(),
        "01.m4a",
        TagType::Mp4Ilst,
        Some("Track One"),
        Some("Artist Name"),
        Some("Album Title"),
        Some("Artist Name"),
        Some(2020),
        Some(1),
        None,
    );

    let parsed = map_tags(&[f1]).unwrap();

    assert_eq!(parsed.release.pressing.format.as_deref(), Some("AAC"));
}

/// A few JPEG SOI/EOI bytes — enough to round-trip as opaque cover
/// data; the embedded-cover read never decodes the image.
const JPEG_BYTES: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0xFF, 0xD9];

/// Copy the FLAC fixture into `dir/{name}` and embed a picture with the
/// given type and MIME. Returns the destination path.
fn copy_with_picture(
    dir: &Path,
    name: &str,
    pic_type: lofty::picture::PictureType,
    mime: lofty::picture::MimeType,
    data: &[u8],
) -> PathBuf {
    use lofty::config::WriteOptions;
    use lofty::picture::Picture;

    let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
    let dest = dir.join(name);
    fs::copy(&src, &dest).unwrap();
    let mut tagged = lofty::read_from_path(&dest).unwrap();
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title("Track".to_string());
    tag.push_picture(
        Picture::unchecked(data.to_vec())
            .pic_type(pic_type)
            .mime_type(mime)
            .build(),
    );
    tagged.insert_tag(tag);
    tagged.save_to_path(&dest, WriteOptions::default()).unwrap();
    dest
}

/// An embedded front cover is read and mapped to its `ContentType`.
#[test]
fn read_embedded_cover_returns_front_cover() {
    let temp = TempDir::new().unwrap();
    let f = copy_with_picture(
        temp.path(),
        "01.flac",
        lofty::picture::PictureType::CoverFront,
        lofty::picture::MimeType::Jpeg,
        JPEG_BYTES,
    );
    let (bytes, content_type) = read_embedded_cover(&[f]).unwrap().expect("cover present");
    assert_eq!(bytes, JPEG_BYTES);
    assert_eq!(content_type, ContentType::Jpeg);
}

/// No embedded picture → None (the caller falls through to no cover).
#[test]
fn read_embedded_cover_none_when_no_picture() {
    let temp = TempDir::new().unwrap();
    let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
    let dest = temp.path().join("01.flac");
    fs::copy(&src, &dest).unwrap();
    assert!(read_embedded_cover(&[dest]).unwrap().is_none());
}

/// When a file carries both a back and a front cover, the front cover
/// wins regardless of which was pushed first.
#[test]
fn read_embedded_cover_prefers_front_over_other() {
    use lofty::config::WriteOptions;
    use lofty::picture::{MimeType, Picture, PictureType};

    let temp = TempDir::new().unwrap();
    let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
    let dest = temp.path().join("01.flac");
    fs::copy(&src, &dest).unwrap();
    let mut tagged = lofty::read_from_path(&dest).unwrap();
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title("Track".to_string());
    // Back cover pushed first, front cover second.
    tag.push_picture(
        Picture::unchecked(vec![0x89, b'P', b'N', b'G'])
            .pic_type(PictureType::CoverBack)
            .mime_type(MimeType::Png)
            .build(),
    );
    tag.push_picture(
        Picture::unchecked(JPEG_BYTES.to_vec())
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::Jpeg)
            .build(),
    );
    tagged.insert_tag(tag);
    tagged.save_to_path(&dest, WriteOptions::default()).unwrap();

    let (bytes, content_type) = read_embedded_cover(&[dest])
        .unwrap()
        .expect("cover present");
    assert_eq!(bytes, JPEG_BYTES, "front cover wins over back");
    assert_eq!(content_type, ContentType::Jpeg);
}

/// With no front cover, the first embedded picture of any type is used —
/// a back-cover-only file still yields a cover to seed.
#[test]
fn read_embedded_cover_falls_back_to_non_front_picture() {
    let temp = TempDir::new().unwrap();
    let f = copy_with_picture(
        temp.path(),
        "01.flac",
        lofty::picture::PictureType::CoverBack,
        lofty::picture::MimeType::Jpeg,
        JPEG_BYTES,
    );
    let (bytes, content_type) = read_embedded_cover(&[f]).unwrap().expect("cover present");
    assert_eq!(bytes, JPEG_BYTES, "back cover used when no front cover");
    assert_eq!(content_type, ContentType::Jpeg);
}

/// A picture in an unsupported MIME (lofty `Tiff`) maps to None rather
/// than being stored as a cover the library can't render.
#[test]
fn read_embedded_cover_none_for_unsupported_mime() {
    let temp = TempDir::new().unwrap();
    let f = copy_with_picture(
        temp.path(),
        "01.flac",
        lofty::picture::PictureType::CoverFront,
        lofty::picture::MimeType::Tiff,
        &[0x49, 0x49, 0x2A, 0x00],
    );
    assert!(read_embedded_cover(&[f]).unwrap().is_none());
}

#[test]
fn read_embedded_cover_returns_err_when_audio_file_cannot_open() {
    let temp = TempDir::new().unwrap();
    let missing = temp.path().join("missing.flac");

    let err = read_embedded_cover(std::slice::from_ref(&missing)).unwrap_err();

    assert!(
        matches!(&err, ImportError::FileTags { detail }
            if detail.contains("failed to open")
                && detail.contains("for embedded cover read")
                && detail.contains(&missing.display().to_string())),
        "expected embedded-cover open error, got {err:?}"
    );
}

#[test]
fn read_embedded_cover_returns_err_when_audio_tags_cannot_be_read() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-fixtures")
        .join("audio-format")
        .join("placeholder-dsd.dsf");

    let err = read_embedded_cover(std::slice::from_ref(&path)).unwrap_err();

    assert!(
        matches!(&err, ImportError::FileTags { detail }
            if detail.contains("failed to read embedded cover tags")
                && detail.contains(&path.display().to_string())),
        "expected embedded-cover tag-read error, got {err:?}"
    );
}

#[test]
fn year_from_tag_reads_structured_date_first() {
    // The structured date() field (ID3v2.4 TDRC / Vorbis DATE) is the
    // primary source, taken ahead of any text-key fallback.
    let mut tag = Tag::new(TagType::Id3v2);
    tag.set_date(Timestamp {
        year: 2020,
        month: None,
        day: None,
        hour: None,
        minute: None,
        second: None,
    });
    tag.insert_text(ItemKey::Year, "1999".to_string());
    assert_eq!(year_from_tag(&tag), Some(2020));

    // A tag with no date information at all yields no year.
    assert_eq!(year_from_tag(&Tag::new(TagType::Id3v2)), None);
}

/// With no structured `date()`, year_from_tag falls back to the text keys
/// in order: Year, then ReleaseDate (TDRL), then OriginalReleaseDate. The
/// date-shaped keys take the leading four digits.
#[test]
fn year_from_tag_reads_text_key_fallbacks() {
    // VorbisComments accepts these text keys (Id3v2 remaps Year onto its
    // structured recording-time frame, so it can't hold a bare Year text
    // item). No structured date() is set, so year_from_tag takes the
    // text-key branch under test.
    let with_key = |key: ItemKey, value: &str| {
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.insert_text(key, value.to_string());
        // Guard: the key must actually be stored, or the test would pass
        // vacuously via the None fallthrough.
        assert!(
            tag.get_string(key).is_some(),
            "VorbisComments did not store {key:?}"
        );
        tag
    };

    assert_eq!(year_from_tag(&with_key(ItemKey::Year, "1988")), Some(1988));
    assert_eq!(
        year_from_tag(&with_key(ItemKey::ReleaseDate, "1991-05-01")),
        Some(1991)
    );
    assert_eq!(
        year_from_tag(&with_key(ItemKey::OriginalReleaseDate, "1972-11")),
        Some(1972)
    );

    // A non-numeric text value is ignored rather than mis-parsed.
    assert_eq!(year_from_tag(&with_key(ItemKey::Year, "notayear")), None);
}

#[test]
fn year_from_cue_date_reads_first_four_digit_run() {
    assert_eq!(year_from_cue_date(Some("1970")), Some(1970));
    assert_eq!(year_from_cue_date(Some("1970-02-03")), Some(1970));
    assert_eq!(year_from_cue_date(Some("2000 / 2004")), Some(2000));
    assert_eq!(year_from_cue_date(Some("rem 99")), None);
    assert_eq!(year_from_cue_date(None), None);
}

#[test]
fn image_content_type_maps_known_and_rejects_non_images() {
    use lofty::picture::MimeType;
    assert_eq!(image_content_type(&MimeType::Jpeg), Some(ContentType::Jpeg));
    assert_eq!(image_content_type(&MimeType::Png), Some(ContentType::Png));
    assert_eq!(image_content_type(&MimeType::Gif), Some(ContentType::Gif));
    assert_eq!(image_content_type(&MimeType::Bmp), Some(ContentType::Bmp));
    // WebP reaches us only as Unknown — lofty has no WebP variant.
    assert_eq!(
        image_content_type(&MimeType::Unknown("image/webp".to_string())),
        Some(ContentType::Webp)
    );
    // A non-image Unknown mime is rejected.
    assert_eq!(
        image_content_type(&MimeType::Unknown("application/octet-stream".to_string())),
        None
    );
    // A non-image known variant is rejected too.
    assert_eq!(image_content_type(&MimeType::Tiff), None);
}

#[test]
fn probe_content_type_labels_m4a_by_codec() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(
        probe_content_type(&manifest.join("test-fixtures/alac/silence-alac.m4a")),
        Some(ContentType::Alac)
    );
    assert_eq!(
        probe_content_type(&manifest.join("test-fixtures/alac/silence-aac.m4a")),
        Some(ContentType::Aac)
    );
}

#[test]
fn file_tag_format_labels_come_from_probed_content_type() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture_dir = manifest.join("test-fixtures").join("audio-format");
    for (name, expected) in [
        ("placeholder-pcm.wav", "PCM"),
        ("placeholder-pcm.aiff", "PCM"),
        ("placeholder-opus.opus", "Opus"),
        ("placeholder-vorbis.ogg", "Vorbis"),
        ("placeholder-wavpack.wv", "WavPack"),
    ] {
        let parsed = map_tags_with_folder(&[fixture_dir.join(name)], Some("Album Title"))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            parsed.release.pressing.format.as_deref(),
            Some(expected),
            "{name}"
        );
    }
}

#[test]
fn dsd_file_tag_seeding_fails_through_tag_reader() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-fixtures")
        .join("audio-format")
        .join("placeholder-dsd.dsf");
    let err = map_tags_with_folder(&[path], Some("Album Title"))
        .expect_err("lofty does not read DSF tags");
    assert!(
        matches!(&err, ImportError::FileTags { detail } if detail.contains("failed to read tags")),
        "{err}"
    );
}

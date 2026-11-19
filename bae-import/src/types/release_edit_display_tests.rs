use super::*;
use bae_types::mediums::TrackPosition;

/// The track `id` on `side`, playing `file`: carved out of it by the sheet
/// `sheet`, or the whole file where `sheet` is `None`.
fn context(side: TrackSide, id: &str, file: &str, sheet: Option<&str>) -> ReleaseEditTrackContext {
    ReleaseEditTrackContext {
        track_id: id.to_string(),
        side,
        source: ReleaseEditTrackSource {
            file_id: file.to_string(),
            name: format!("{file}.flac"),
            layout: if sheet.is_some() { SourceAudioLayout::Cue } else { SourceAudioLayout::File },
        },
        start_ms: 0,
        duration_ms: 1_000,
        listed_length_ms: None,
        position_text: String::new(),
        sheet: sheet.map(|sheet| ReleaseEditSheet {
            file_id: sheet.to_string(),
            name: format!("{sheet}.cue"),
            disc_number: 1,
        }),
    }
}

fn side(letter: &str) -> TrackSide {
    TrackSide::Sided { side_letter: letter.to_string() }
}

/// The form row of the track `id`.
fn form_row(id: &str) -> crate::RawTrackEdit {
    crate::RawTrackEdit {
        id: id.to_string(),
        title: format!("Title {id}"),
        artist_assignments: crate::TrackArtistAssignments::AlbumArtists,
        file: None,
    }
}

/// The sides of `tracks`, each beside a form row of its own.
fn lay_out(tracks: Vec<ReleaseEditTrackContext>) -> Vec<ReleaseEditTrackSide> {
    ReleaseEditTrackSide::lay_out(tracks.into_iter().map(|context| ReleaseEditTrackRow {
        edit: form_row(&context.track_id),
        context,
        artist_standing: None,
    }))
}

/// Each run as the track ids it holds, and the file a sheet run names.
fn runs(side: &ReleaseEditTrackSide) -> Vec<(Option<&str>, Vec<&str>)> {
    sheet_runs(side)
        .into_iter()
        .map(|(carving, rows)| (carving.map(|(_, file)| file), rows))
        .collect()
}

/// A run as the sheet and file a sheet run names, and the track ids it holds.
type SheetRun<'a> = (Option<(&'a str, &'a str)>, Vec<&'a str>);

/// Each run as the track ids it holds, and the sheet and file a sheet run
/// names.
fn sheet_runs(side: &ReleaseEditTrackSide) -> Vec<SheetRun<'_>> {
    side.runs
        .iter()
        .map(|run| {
            let source = match run {
                ReleaseEditTrackRun::Track(_) => None,
                ReleaseEditTrackRun::Sheet { sheet, source, .. } => {
                    Some((sheet.file_id.as_str(), source.file_id.as_str()))
                }
            };
            (source, run.rows().iter().map(|row| row.context.track_id.as_str()).collect())
        })
        .collect()
}

#[test]
fn tracks_lay_out_a_side_per_run_of_one_side_and_a_run_per_sheet_file() {
    let sheet = Some("sheet");
    let sides = lay_out(vec![
        context(side("A"), "a1", "image-a", sheet),
        context(side("A"), "a2", "image-a", sheet),
        context(side("A"), "a3", "image-a", sheet),
        context(side("A"), "a4", "loose", None),
        context(side("B"), "b1", "image-b", sheet),
        context(side("B"), "b2", "image-b", sheet),
        context(side("B"), "b3", "image-c", sheet),
        context(side("B"), "b4", "image-c", sheet),
        context(side("B"), "b5", "image-d", sheet),
    ]);

    assert_eq!(sides.iter().map(|side| &side.side).collect::<Vec<_>>(), [&side("A"), &side("B")]);
    assert_eq!(runs(&sides[0]), [(Some("image-a"), vec!["a1", "a2", "a3"]), (None, vec!["a4"]),]);
    // Two sheets back to back are two runs; a file one track alone is
    // carved from stays in that track's row.
    assert_eq!(
        runs(&sides[1]),
        [
            (Some("image-b"), vec!["b1", "b2"]),
            (Some("image-c"), vec!["b3", "b4"]),
            (None, vec!["b5"]),
        ]
    );
}

#[test]
fn a_sheet_run_ends_at_its_side() {
    let sides = lay_out(vec![
        context(side("A"), "a1", "image", Some("sheet")),
        context(side("A"), "a2", "image", Some("sheet")),
        context(side("B"), "b1", "image", Some("sheet")),
    ]);

    assert_eq!(runs(&sides[0]), [(Some("image"), vec!["a1", "a2"])]);
    assert_eq!(runs(&sides[1]), [(None, vec!["b1"])]);
}

/// A sheet run names the sheet that carves its tracks, and two sheets that
/// carve one file make a run each.
#[test]
fn a_sheet_run_holds_the_tracks_one_sheet_carves_from_one_file() {
    let sides = lay_out(vec![
        context(TrackSide::Flat, "t1", "image", Some("first")),
        context(TrackSide::Flat, "t2", "image", Some("first")),
        context(TrackSide::Flat, "t3", "image", Some("second")),
        context(TrackSide::Flat, "t4", "image", Some("second")),
    ]);

    assert_eq!(
        sheet_runs(&sides[0]),
        [
            (Some(("first", "image")), vec!["t1", "t2"]),
            (Some(("second", "image")), vec!["t3", "t4"]),
        ]
    );
}

/// No files beside the tracks' audio.
fn no_files() -> bae_records::DbReleaseFiles {
    bae_records::DbReleaseFiles {
        files: Vec::new(),
        images: Vec::new(),
        covers: Vec::new(),
        others: Vec::new(),
        has_others: false,
        count: 0,
        image_count: 0,
        total_size: 0,
        source_audio: None,
    }
}

/// One track whose audio holds `main_segment_count` main segments, the
/// first over `main`.
fn one_track(
    main_segment_count: i64,
    main: Option<bae_records::DbReleaseEditSegment>,
) -> Vec<bae_records::DbReleaseEditTrack> {
    let release = bae_records::DbRelease::new_test("album-id", "release-id");
    vec![bae_records::DbReleaseEditTrack {
        track: bae_records::DbTrack::new_test(&release.id, "track-id", "Track Title", 1),
        position: TrackPosition::Flat { number: 1 },
        listed_length_ms: None,
        audio_format_count: 1,
        main_segment_count,
        main_segment: main,
        sheet: None,
    }]
}

/// A main segment over the stored file `file_id`.
fn segment(file_id: &str) -> bae_records::DbReleaseEditSegment {
    bae_records::DbReleaseEditSegment {
        file_id: file_id.to_string(),
        start_ms: 0,
        file: Some(bae_records::DbReleaseEditSourceFile {
            name: format!("{file_id}.flac"),
            layout: Some(SourceAudioLayout::Cue),
        }),
    }
}

#[test]
fn a_track_without_one_body_is_an_editor_load_error() {
    assert!(matches!(
        ReleaseEditDisplayContext::from_raw(&no_files(), one_track(0, None)),
        Err(ReleaseEditLoadError::MissingMainSegment { track_id }) if track_id == "track-id"
    ));
    assert!(matches!(
        ReleaseEditDisplayContext::from_raw(&no_files(), one_track(2, Some(segment("file")))),
        Err(ReleaseEditLoadError::MultipleMainSegments { track_id }) if track_id == "track-id"
    ));
}

#[test]
fn missing_segment_file_is_an_editor_load_error() {
    let missing = bae_records::DbReleaseEditSegment {
        file_id: "missing-file-id".to_string(),
        start_ms: 0,
        file: None,
    };

    assert!(matches!(
        ReleaseEditDisplayContext::from_raw(&no_files(), one_track(1, Some(missing))),
        Err(ReleaseEditLoadError::MissingSourceFile { file_id, .. })
            if file_id == "missing-file-id"
    ));
}

/// A stored file of the release, named `name`, in `role`, with audio facts
/// laid out as `layout` when it is audio.
fn stored_file(
    name: &str,
    role: bae_records::ReleaseFileRole,
    layout: Option<SourceAudioLayout>,
) -> bae_records::DbFile {
    let audio = role == bae_records::ReleaseFileRole::Audio;
    bae_records::DbFile {
        id: format!("id-{name}"),
        release_id: "release-id".to_string(),
        original_filename: name.to_string(),
        file_size: 1_024,
        content_type: if audio {
            bae_util::content_type::ContentType::Flac
        } else {
            bae_util::content_type::ContentType::PlainText
        },
        role,
        source_audio: audio.then(|| bae_records::source_audio::SourceAudioFile {
            layout,
            content_type: bae_util::content_type::ContentType::Flac,
            container: bae_util::audio_container::AudioContainer::from_name("flac"),
            duration_ms: 60_000,
            format: bae_types::audio_format::AudioFormat {
                codec: "FLAC".to_string(),
                sample_rate_hz: 44_100,
                bits_per_sample: Some(16),
                bitrate_kbps: None,
                channels: 2,
            },
        }),
        file: coven::FileId(uuid::Uuid::nil()),
        stored: false,
        created_at: time::UtcDateTime::UNIX_EPOCH,
    }
}

/// A track keeps the listed length its row read and the sheet that carves
/// it.
#[test]
fn a_track_keeps_its_listed_length_and_its_sheet() {
    let sheet = bae_records::DbReleaseEditSheet {
        file_id: "id-disc.cue".to_string(),
        name: "disc.cue".to_string(),
        disc_number: 2,
    };
    let mut tracks = one_track(1, Some(segment("id-disc.flac")));
    tracks[0].listed_length_ms = Some(183_001);
    tracks[0].sheet = Some(sheet);
    let display = ReleaseEditDisplayContext::from_raw(&no_files(), tracks).unwrap();
    assert_eq!(display.tracks[0].listed_length_ms, Some(183_001));
    assert_eq!(
        display.tracks[0].sheet,
        Some(ReleaseEditSheet {
            file_id: "id-disc.cue".to_string(),
            name: "disc.cue".to_string(),
            disc_number: 2,
        })
    );
}

/// The Files section lists every file no track plays from, in the release's
/// order, each with what it is: a track sheet reads as a document, and an
/// image the scan found damaged says so.
#[test]
fn the_files_no_track_plays_from_are_listed_with_their_roles() {
    use crate::MappingRole;
    use bae_records::ReleaseFileRole::{Artwork, Audio, DamagedImage, Document, Other, TrackSheet};
    let mut files = no_files();
    files.others = vec![
        stored_file("back.png", DamagedImage, None),
        stored_file("cover.jpg", Artwork, None),
        stored_file("disc.cue", TrackSheet, None),
        stored_file("notes.nfo", Other, None),
        stored_file("rip.log", Document(bae_records::DocumentKind::RipLog), None),
    ];
    files.files = files.others.clone();
    files.files.push(stored_file("disc.flac", Audio, Some(SourceAudioLayout::Cue)));

    let display =
        ReleaseEditDisplayContext::from_raw(&files, one_track(1, Some(segment("id-disc.flac"))))
            .unwrap();

    assert_eq!(
        display
            .files
            .iter()
            .map(|file| (file.file_id.as_str(), file.name.as_str(), file.size, file.role))
            .collect::<Vec<_>>(),
        [
            ("id-back.png", "back.png", 1_024, MappingRole::DamagedImage),
            ("id-cover.jpg", "cover.jpg", 1_024, MappingRole::Image),
            ("id-disc.cue", "disc.cue", 1_024, MappingRole::Document),
            ("id-notes.nfo", "notes.nfo", 1_024, MappingRole::Other),
            ("id-rip.log", "rip.log", 1_024, MappingRole::Document),
        ]
    );
}

use super::*;
use bae_types::mediums::TrackPosition;

fn context(
    side: TrackSide,
    id: &str,
    file: &str,
    layout: SourceAudioLayout,
) -> ReleaseEditTrackContext {
    ReleaseEditTrackContext {
        track_id: id.to_string(),
        side,
        source: ReleaseEditTrackSource {
            file_id: file.to_string(),
            name: format!("{file}.flac"),
            layout,
        },
        duration_ms: 1_000,
        position_text: String::new(),
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
    ReleaseEditTrackSide::lay_out(
        tracks
            .into_iter()
            .map(|context| ReleaseEditTrackRow { edit: form_row(&context.track_id), context }),
    )
}

/// Each run as the track ids it holds, and the file a sheet run names.
fn runs(side: &ReleaseEditTrackSide) -> Vec<(Option<&str>, Vec<&str>)> {
    side.runs
        .iter()
        .map(|run| {
            let source = match run {
                ReleaseEditTrackRun::Track(_) => None,
                ReleaseEditTrackRun::Sheet { source, .. } => Some(source.file_id.as_str()),
            };
            (source, run.rows().iter().map(|row| row.context.track_id.as_str()).collect())
        })
        .collect()
}

#[test]
fn tracks_lay_out_a_side_per_run_of_one_side_and_a_run_per_sheet_file() {
    use SourceAudioLayout::{Cue, File};
    let sides = lay_out(vec![
        context(side("A"), "a1", "image-a", Cue),
        context(side("A"), "a2", "image-a", Cue),
        context(side("A"), "a3", "image-a", Cue),
        context(side("A"), "a4", "loose", File),
        context(side("B"), "b1", "image-b", Cue),
        context(side("B"), "b2", "image-b", Cue),
        context(side("B"), "b3", "image-c", Cue),
        context(side("B"), "b4", "image-c", Cue),
        context(side("B"), "b5", "image-d", Cue),
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
    use SourceAudioLayout::Cue;
    let sides = lay_out(vec![
        context(side("A"), "a1", "image", Cue),
        context(side("A"), "a2", "image", Cue),
        context(side("B"), "b1", "image", Cue),
    ]);

    assert_eq!(runs(&sides[0]), [(Some("image"), vec!["a1", "a2"])]);
    assert_eq!(runs(&sides[1]), [(None, vec!["b1"])]);
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
        audio_format_count: 1,
        main_segment_count,
        main_segment: main,
    }]
}

/// A main segment over the stored file `file_id`.
fn segment(file_id: &str) -> bae_records::DbReleaseEditSegment {
    bae_records::DbReleaseEditSegment {
        file_id: file_id.to_string(),
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
    let missing =
        bae_records::DbReleaseEditSegment { file_id: "missing-file-id".to_string(), file: None };

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

use super::*;
use bae_records::source_audio::SourceAudioLayout;
use bae_types::mediums::TrackPosition;

/// The row of the track `id` on `side`, playing `file`: carved out of it by
/// the sheet `sheet` of one `FILE` line, or the whole file where `sheet` is
/// `None`.
fn context(side: TrackSide, id: &str, file: &str, sheet: Option<&str>) -> ReleaseEditTrackRow {
    let context = ReleaseEditTrackContext {
        side,
        source: crate::MappingContainer { file_id: file.to_string(), name: format!("{file}.flac") },
        start_ms: 0,
        duration_ms: 1_000,
        listed_length_ms: None,
        position_text: String::new(),
        sheet: sheet.map(|sheet| bae_records::DbReleaseEditSheet {
            file_id: sheet.to_string(),
            name: format!("{sheet}.cue"),
            place: bae_records::DbSheetSideChoice {
                side: bae_types::mediums::MediumSide::first_of(1),
                heading: TrackSide::Disc { disc: 1 },
            },
            line_count: 1,
            unbound_lines: Vec::new(),
            file_reference: format!("{file}.flac"),
            audio_options: Vec::new(),
        }),
    };
    ReleaseEditTrackRow {
        edit: form_row(id),
        context,
        audition: crate::TrackAudition::LibraryTrack,
        artists: None,
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

/// The sides of `tracks`.
fn lay_out(tracks: Vec<ReleaseEditTrackRow>) -> Vec<crate::TrackTableSide> {
    lay_out_on_discs(tracks, 1)
}

/// The sides of `tracks` on a release of `disc_count` discs.
fn lay_out_on_discs(
    tracks: Vec<ReleaseEditTrackRow>,
    disc_count: u32,
) -> Vec<crate::TrackTableSide> {
    lay_out_tracks(
        tracks,
        &(1..=disc_count)
            .map(|disc| bae_records::DbSheetSideChoice {
                side: bae_types::mediums::MediumSide::first_of(disc),
                heading: TrackSide::Disc { disc },
            })
            .collect::<Vec<_>>(),
    )
}

/// Each run as the sheet a sheet run names, and the track ids it holds.
fn runs(side: &crate::TrackTableSide) -> Vec<(Option<&str>, Vec<&str>)> {
    side.runs
        .iter()
        .map(|run| {
            let sheet = match run {
                crate::TrackTableRun::Track(_) => None,
                crate::TrackTableRun::Sheet { sheet, .. } => Some(sheet.sheet_id.as_str()),
            };
            (sheet, run.rows().iter().map(|row| row.track.id.as_str()).collect())
        })
        .collect()
}

/// The caption of the only run of `side`.
fn only_caption(side: &crate::TrackTableSide) -> &crate::SheetGroup {
    let [crate::TrackTableRun::Sheet { sheet, .. }] = side.runs.as_slice() else {
        panic!("the side holds one sheet run: {:?}", side.runs);
    };
    sheet
}

/// A side holds a run per sheet that carves consecutive tracks on it,
/// however few, and a run per track that plays a whole file.
#[test]
fn tracks_lay_out_a_side_per_run_of_one_side_and_a_run_per_sheet() {
    let sides = lay_out(vec![
        context(side("A"), "a1", "image-a", Some("first")),
        context(side("A"), "a2", "image-a", Some("first")),
        context(side("A"), "a3", "loose", None),
        context(side("B"), "b1", "image-b", Some("second")),
        context(side("B"), "b2", "image-b", Some("second")),
        context(side("B"), "b3", "image-c", Some("third")),
    ]);

    assert_eq!(sides.iter().map(|side| &side.side).collect::<Vec<_>>(), [&side("A"), &side("B")]);
    assert_eq!(runs(&sides[0]), [(Some("first"), vec!["a1", "a2"]), (None, vec!["a3"])]);
    assert_eq!(runs(&sides[1]), [(Some("second"), vec!["b1", "b2"]), (Some("third"), vec!["b3"])]);
}

/// A track that plays a whole file names it in its row, and a track a
/// sheet carves gives where it starts; each plays the library's release
/// from itself.
#[test]
fn a_row_names_its_file_or_its_start() {
    let mut carved = context(TrackSide::Flat, "t2", "image", Some("sheet"));
    carved.context.start_ms = 61_000;
    let sides = lay_out(vec![context(TrackSide::Flat, "t1", "loose", None), carved]);
    let [crate::TrackTableRun::Track(row), crate::TrackTableRun::Sheet { rows, .. }] =
        sides[0].runs.as_slice()
    else {
        panic!("a file run, then a sheet run: {:?}", sides[0].runs);
    };
    assert_eq!(row.source, crate::TrackSource::File { name: "loose.flac".to_string() });
    assert_eq!(row.audition, crate::TrackAudition::LibraryTrack);
    assert_eq!(
        rows.iter().map(|row| &row.source).collect::<Vec<_>>(),
        [&crate::TrackSource::SheetSlice { start_ms: 61_000 }]
    );
}

/// A sheet's caption offers every side and disc the release holds, and
/// keeps its tracks, so it offers no *Ignored*.
#[test]
fn a_sheet_caption_offers_every_disc_of_the_release() {
    let sides = lay_out_on_discs(
        vec![
            context(side("A"), "a1", "image", Some("sheet")),
            context(side("A"), "a2", "image", Some("sheet")),
        ],
        3,
    );
    let sheet = only_caption(&sides[0]);
    assert_eq!(
        sheet.assigned.as_ref().map(|choice| choice.side),
        Some(bae_types::mediums::MediumSide::first_of(1))
    );
    assert_eq!(
        sheet.side_choices.iter().map(|choice| choice.side.medium).collect::<Vec<_>>(),
        [1, 2, 3]
    );
}

/// A sheet's caption offers the audio files its line can play from as the
/// import pane offers them: a file in a codec bae can't carve is refused
/// naming the codec, and one the line's tracks don't fit is refused for its
/// timing.
#[test]
fn a_sheet_caption_offers_its_lines_audio_files_as_the_import_pane_does() {
    use crate::folder_scanner::{SheetBindingOffer, SheetBindingOption, SheetReferenceOptions};
    use bae_util::content_type::ContentType;
    let option = |file_id: &str, content_type: ContentType, bound: bool, fits: bool| {
        bae_records::DbSheetAudioOption {
            file_id: file_id.to_string(),
            name: format!("{file_id} name"),
            content_type,
            bound,
            fits,
        }
    };
    let mut tracks = vec![
        context(side("A"), "a1", "image", Some("sheet")),
        context(side("A"), "a2", "image", Some("sheet")),
    ];
    for track in &mut tracks {
        track.context.sheet.as_mut().expect("the sheet carves the track").audio_options = vec![
            option("image", ContentType::Flac, true, true),
            option("lossy", ContentType::Mp3, false, true),
            option("short", ContentType::Flac, false, false),
        ];
    }
    let sides = lay_out(tracks);
    let sheet = only_caption(&sides[0]);
    let choice = |file_id: &str, offer| SheetBindingOption {
        file_id: file_id.to_string(),
        name: format!("{file_id} name"),
        offer,
    };
    assert_eq!(
        sheet.bound,
        crate::SheetBound::Describes(crate::MappingContainer {
            file_id: "image".to_string(),
            name: "image.flac".to_string(),
        })
    );
    assert_eq!(
        sheet.bound_audio,
        crate::SheetBoundAudio::One(SheetReferenceOptions {
            file_reference: "image.flac".to_string(),
            bound: Some(crate::MappingContainer {
                file_id: "image".to_string(),
                name: "image.flac".to_string(),
            }),
            options: vec![
                choice("image", SheetBindingOffer::Offered),
                choice("lossy", SheetBindingOffer::RefusedCodec { codec: "MP3".to_string() }),
                choice("short", SheetBindingOffer::RefusedTiming),
            ],
        })
    );
    assert_eq!(sheet.unbound_references, None);
}

/// A sheet whose lines play from two files heads all its tracks once, and
/// its caption offers a choice of file for each line, in the order its
/// tracks reach them.
#[test]
fn a_sheet_of_two_lines_heads_its_tracks_once() {
    let mut tracks = vec![
        context(TrackSide::Flat, "t1", "image-1", Some("sheet")),
        context(TrackSide::Flat, "t2", "image-1", Some("sheet")),
        context(TrackSide::Flat, "t3", "image-2", Some("sheet")),
    ];
    for track in &mut tracks {
        track.context.sheet.as_mut().expect("the sheet carves the track").line_count = 2;
    }
    let sides = lay_out(tracks);
    assert_eq!(runs(&sides[0]), [(Some("sheet"), vec!["t1", "t2", "t3"])]);
    let sheet = only_caption(&sides[0]);
    assert_eq!(sheet.bound, crate::SheetBound::DescribesFiles { audio_file_count: 2 });
    let crate::SheetBoundAudio::Several(lines) = &sheet.bound_audio else {
        panic!("each line has its own choices: {:?}", sheet.bound_audio);
    };
    assert_eq!(
        lines
            .iter()
            .map(|line| {
                (
                    line.file_reference.as_str(),
                    line.bound.as_ref().map(|file| file.file_id.as_str()),
                )
            })
            .collect::<Vec<_>>(),
        [("image-1.flac", Some("image-1")), ("image-2.flac", Some("image-2"))]
    );
}

/// A sheet the release splits across sides heads its tracks again on the
/// next side.
#[test]
fn a_sheet_run_ends_at_its_side() {
    let sides = lay_out(vec![
        context(side("A"), "a1", "image", Some("sheet")),
        context(side("A"), "a2", "image", Some("sheet")),
        context(side("B"), "b1", "image", Some("sheet")),
    ]);

    assert_eq!(runs(&sides[0]), [(Some("sheet"), vec!["a1", "a2"])]);
    assert_eq!(runs(&sides[1]), [(Some("sheet"), vec!["b1"])]);
}

/// Two sheets that carve one file make a run each.
#[test]
fn a_sheet_run_holds_the_tracks_one_sheet_carves() {
    let sides = lay_out(vec![
        context(TrackSide::Flat, "t1", "image", Some("first")),
        context(TrackSide::Flat, "t2", "image", Some("first")),
        context(TrackSide::Flat, "t3", "image", Some("second")),
        context(TrackSide::Flat, "t4", "image", Some("second")),
    ]);

    assert_eq!(
        runs(&sides[0]),
        [(Some("first"), vec!["t1", "t2"]), (Some("second"), vec!["t3", "t4"])]
    );
}

/// No files beside the tracks' audio.
fn no_files() -> bae_records::DbReleaseFiles {
    bae_records::DbReleaseFiles {
        files: Vec::new(),
        images: Vec::new(),
        others: Vec::new(),
        carried: Vec::new(),
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
        place: bae_records::DbSheetSideChoice {
            side: bae_types::mediums::MediumSide::first_of(2),
            heading: TrackSide::Disc { disc: 2 },
        },
        line_count: 1,
        unbound_lines: Vec::new(),
        file_reference: "disc.flac".to_string(),
        audio_options: Vec::new(),
    };
    let mut tracks = one_track(1, Some(segment("id-disc.flac")));
    tracks[0].listed_length_ms = Some(183_001);
    tracks[0].sheet = Some(sheet.clone());
    let display = ReleaseEditDisplayContext::from_raw(&no_files(), tracks).unwrap();
    assert_eq!(display.tracks[0].listed_length_ms, Some(183_001));
    assert_eq!(display.tracks[0].sheet, Some(sheet));
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

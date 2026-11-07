use super::*;
use bae_types::mediums::TrackPosition;

fn context(id: &str, file: &str, layout: SourceAudioLayout) -> ReleaseEditTrackContext {
    ReleaseEditTrackContext {
        track_id: id.to_string(),
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

/// Each run as the track ids it holds, and the file a sheet run names.
fn runs(side: &ReleaseEditTrackSide) -> Vec<(Option<&str>, Vec<&str>)> {
    side.runs
        .iter()
        .map(|run| {
            let source = match run {
                ReleaseEditTrackRun::Track(_) => None,
                ReleaseEditTrackRun::Sheet { source, .. } => Some(source.file_id.as_str()),
            };
            (source, run.tracks().iter().map(|track| track.track_id.as_str()).collect())
        })
        .collect()
}

#[test]
fn tracks_lay_out_a_side_per_run_of_one_side_and_a_run_per_sheet_file() {
    use SourceAudioLayout::{Cue, File};
    let sides = ReleaseEditTrackSide::lay_out([
        (side("A"), context("a1", "image-a", Cue)),
        (side("A"), context("a2", "image-a", Cue)),
        (side("A"), context("a3", "image-a", Cue)),
        (side("A"), context("a4", "loose", File)),
        (side("B"), context("b1", "image-b", Cue)),
        (side("B"), context("b2", "image-b", Cue)),
        (side("B"), context("b3", "image-c", Cue)),
        (side("B"), context("b4", "image-c", Cue)),
        (side("B"), context("b5", "image-d", Cue)),
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
    let sides = ReleaseEditTrackSide::lay_out([
        (side("A"), context("a1", "image", Cue)),
        (side("A"), context("a2", "image", Cue)),
        (side("B"), context("b1", "image", Cue)),
    ]);

    assert_eq!(runs(&sides[0]), [(Some("image"), vec!["a1", "a2"])]);
    assert_eq!(runs(&sides[1]), [(None, vec!["b1"])]);
}

#[test]
fn placed_tracks_are_laid_out_again_at_their_new_places() {
    use SourceAudioLayout::File;
    let display = ReleaseEditDisplayContext {
        source_audio: None,
        sides: ReleaseEditTrackSide::lay_out(
            ["t1", "t2", "t3"].map(|id| (TrackSide::Flat, context(id, id, File))),
        ),
    };

    let placed = display.placed([
        TrackPosition::Disc { disc: 1, number: 1 },
        TrackPosition::Disc { disc: 1, number: 2 },
        TrackPosition::Disc { disc: 2, number: 1 },
    ]);

    assert_eq!(
        placed.sides.iter().map(|side| &side.side).collect::<Vec<_>>(),
        [&TrackSide::Disc { disc: 1 }, &TrackSide::Disc { disc: 2 }]
    );
    assert_eq!(
        placed
            .tracks()
            .map(|track| (track.track_id.as_str(), track.position_text.as_str()))
            .collect::<Vec<_>>(),
        [("t1", "1"), ("t2", "2"), ("t3", "1")]
    );
}

/// One track's release with its audio as the editor reads it, the track's
/// segments in order.
fn one_track(
    segments: Vec<bae_records::DbReleaseEditSegment>,
) -> (bae_records::DbReleaseDetail, Vec<bae_records::DbReleaseEditTrack>) {
    let release = bae_records::DbRelease::new_test("album-id", "release-id");
    let track = bae_records::DbTrack::new_test(&release.id, "track-id", "Track Title", 1);
    let raw = bae_records::DbReleaseDetail {
        mediums: bae_records::DbReleaseMediums(vec![bae_records::DbReleaseMedium::new_test(
            &release.id,
        )]),
        release,
        tracks: vec![bae_records::DbTrackWithArtists {
            track: track.clone(),
            artists: Vec::new(),
            shown_artists: Vec::new(),
        }],
        files: Vec::new(),
        records: Vec::new(),
        unstored_audio: Vec::new(),
        pinned: false,
        keeping_here: false,
        remote: false,
        upload: None,
        cover_version: None,
    };
    (raw, vec![bae_records::DbReleaseEditTrack { track, audio_format_count: 1, segments }])
}

/// A segment of `role` over the stored file `file_id`.
fn segment(
    role: bae_records::DbAudioSegmentRole,
    file_id: &str,
) -> bae_records::DbReleaseEditSegment {
    bae_records::DbReleaseEditSegment {
        role,
        file_id: file_id.to_string(),
        file: Some(bae_records::DbReleaseEditSourceFile {
            name: format!("{file_id}.flac"),
            layout: Some(SourceAudioLayout::Cue),
        }),
    }
}

/// A track whose pregap is stored at the tail of the file before names only
/// the file its body plays from.
#[test]
fn a_track_s_source_is_its_body_s_file_not_its_pregap_s() {
    use bae_records::DbAudioSegmentRole::{AudioPregap, Main};
    let (raw, tracks) =
        one_track(vec![segment(AudioPregap, "first-file"), segment(Main, "second-file")]);

    let display = ReleaseEditDisplayContext::from_raw(&raw, tracks).unwrap();

    let sources: Vec<_> = display.tracks().map(|track| track.source.name.as_str()).collect();
    assert_eq!(sources, ["second-file.flac"]);
}

#[test]
fn a_track_without_one_body_is_an_editor_load_error() {
    use bae_records::DbAudioSegmentRole::{AudioPregap, Main};
    let (raw, tracks) = one_track(vec![segment(AudioPregap, "file")]);
    assert!(matches!(
        ReleaseEditDisplayContext::from_raw(&raw, tracks),
        Err(ReleaseEditLoadError::MissingMainSegment { track_id }) if track_id == "track-id"
    ));

    let (raw, tracks) = one_track(vec![segment(Main, "file"), segment(Main, "other-file")]);
    assert!(matches!(
        ReleaseEditDisplayContext::from_raw(&raw, tracks),
        Err(ReleaseEditLoadError::MultipleMainSegments { track_id }) if track_id == "track-id"
    ));
}

#[test]
fn missing_segment_file_is_an_editor_load_error() {
    let (raw, tracks) = one_track(vec![bae_records::DbReleaseEditSegment {
        role: bae_records::DbAudioSegmentRole::Main,
        file_id: "missing-file-id".to_string(),
        file: None,
    }]);

    assert!(matches!(
        ReleaseEditDisplayContext::from_raw(&raw, tracks),
        Err(ReleaseEditLoadError::MissingSourceFile { file_id, .. })
            if file_id == "missing-file-id"
    ));
}

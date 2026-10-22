use crate::db::DbTrack;
/// Deterministic clock for the audio-format tests — the exact instant is
/// immaterial to what they assert, only that the same one feeds every row.
fn test_clock() -> FixedClock {
    FixedClock::new((chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)).into())
}

/// Build the CUE-backed track list for a single-file CUE album, reusing
/// the same codec probe the scanner stores on the candidate.
fn cue_backed_tracks(dir: &str) -> Vec<TrackFile> {
    let audio_path = PathBuf::from(format!("{dir}/Test Album.ape"));
    let cue_path = PathBuf::from(format!("{dir}/Test Album.cue"));
    let cue_sheet = crate::cue_flac::parse_cue_sheet(&cue_path).expect("parse cue");
    let probe = crate::audio_codec::probe_audio_from_path(
        audio_path.to_str().expect("fixture path is UTF-8"),
    )
    .expect("analyze ape");
    let cue_pair = Arc::new(crate::import::types::CueFlacAnalysis {
        cue_sheet,
        audio_files: vec![crate::import::types::CueAnalyzedAudioFile {
            file_reference: "Test Album.ape".to_string(),
            path: audio_path.clone(),
            probe,
        }],
    });
    (0..cue_pair.cue_sheet.tracks.len())
        .map(|index| TrackFile {
            db_track: DbTrack {
                id: format!("track-{index}"),
                release_id: "rel".to_string(),
                title: format!("Track {index}"),
                side: Some(1),
                track_number: Some(index as i32 + 1),
                duration_ms: 180_000,
                created_at: chrono::DateTime::<chrono::Utc>::from(coven::Clock::now(&test_clock())),
            },
            audio: crate::import::TrackAudio::CueBacked {
                cue_pair: Arc::clone(&cue_pair),
                cue_index: index,
            },
        })
        .collect()
}

/// A CUE track's read-ahead ceiling is its `end_byte`; playback fills up to it
/// and stops, so every non-last track must carry a real end byte or the fill
/// streams the whole rest of the shared file. Ends derive from the next track's
/// start byte (`start[N+1]`), computed via `seek_landing_bytes` -- the AVIO
/// landing, defined for every format, including APE whose packets carry no byte
/// position. This drives `build_audio_formats` over the APE CUE fixture and
/// asserts the user-visible outcome: the two non-last tracks get `Some`,
/// ascending, in-file end bytes and the last track runs to EOF (`None`).
#[test]
fn build_audio_formats_gives_ape_cue_tracks_real_end_bytes() {
    crate::audio_codec::init();
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/cue_ape");
    let tracks = cue_backed_tracks(dir);
    assert_eq!(
        tracks.len(),
        3,
        "fixture is a 3-track single-file CUE album"
    );

    let file_size = std::fs::metadata(format!("{dir}/Test Album.ape"))
        .unwrap()
        .len();
    let mut file_ids = HashMap::new();
    file_ids.insert(
        PathBuf::from(format!("{dir}/Test Album.ape")),
        "file-1".to_string(),
    );

    let built = ImportService::build_audio_formats(
        &tracks,
        &file_ids,
        &test_clock(),
        &SequentialIds::new(),
    )
    .expect("build_audio_formats");

    let main_segments: Vec<_> = built
        .audio_segments
        .iter()
        .filter(|segment| segment.role == crate::db::DbAudioSegmentRole::Main)
        .collect();
    let ends: Vec<Option<u64>> = main_segments
        .iter()
        .map(|segment| segment.end_byte)
        .collect();
    // Non-last tracks carry a real, ascending, in-file end byte.
    let e0 = ends[0].expect("track 1 (non-last) must have an end byte");
    let e1 = ends[1].expect("track 2 (non-last) must have an end byte");
    assert!(
        e0 > 0 && e0 < file_size,
        "track 1 end within file: {e0} of {file_size}"
    );
    assert!(
        e1 > 0 && e1 < file_size,
        "track 2 end within file: {e1} of {file_size}"
    );
    assert!(e1 > e0, "end bytes ascend track to track: {ends:?}");
    // The last track runs to EOF.
    assert_eq!(ends[2], None, "the last track runs to EOF");

    // Each track's end is the next track's start byte -- one boundary, not two.
    assert_eq!(
        main_segments[1].start_byte,
        Some(e0),
        "track 2 starts where track 1 ends"
    );
    assert_eq!(
        main_segments[2].start_byte,
        Some(e1),
        "track 3 starts where track 2 ends"
    );
}

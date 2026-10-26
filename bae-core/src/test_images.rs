//! Audio images a test imports.

use crate::import::{CueAnalyzedAudioFile, CueFlacAnalysis, TrackFile};
use std::collections::HashMap;
use std::sync::Arc;

/// Ten seconds of a stereo tone encoded as `format`, written into `dir` as
/// `name` with a CUE sheet that starts track 2 three seconds in. Returns the
/// image's bytes and the segments import builds for its two tracks: track 1
/// first, then track 2, each in the file `"image"`.
pub(crate) fn cue_image(
    dir: &std::path::Path,
    name: &str,
    format: bae_audio::EncodeFormat,
) -> (Vec<u8>, Vec<crate::db::DbAudioSegment>) {
    let samples: Vec<i32> = (0..44_100 * 10 * 2)
        .map(|i| (((i / 2) as f64 * 0.031).sin() * 0.3 * i32::MAX as f64) as i32)
        .collect();
    let bytes = bae_audio::encode_i32(format, &samples, 44_100, 2).expect("encode the image");
    let path = dir.join(name);
    std::fs::write(&path, &bytes).expect("write the image");
    let cue_path = dir.join(format!("{name}.cue"));
    std::fs::write(
        &cue_path,
        format!(
            "FILE \"{name}\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  \
             TRACK 02 AUDIO\n    INDEX 01 00:03:00\n"
        ),
    )
    .expect("write the cue");
    let probe =
        bae_audio::probe_audio_from_path(path.to_str().expect("UTF-8 path")).expect("probe");
    let cue_pair = Arc::new(CueFlacAnalysis {
        cue_sheet: crate::cue_flac::parse_cue_sheet(&cue_path).expect("parse cue"),
        audio_files: vec![CueAnalyzedAudioFile {
            file_reference: name.to_string(),
            path: path.clone(),
            probe,
        }],
    });
    let tracks: Vec<_> = (0..2u32)
        .map(|index| TrackFile {
            db_track: crate::db::DbTrack::new_test(
                "release-id",
                &format!("track-{index}"),
                "Track Title",
                index + 1,
            ),
            audio: crate::import::TrackAudio::CueBacked {
                cue_pair: Arc::clone(&cue_pair),
                cue_index: index as usize,
            },
        })
        .collect();
    let built = crate::import::ImportService::build_audio_formats(
        &tracks,
        &HashMap::from([(path, "image".to_string())]),
        &coven::FixedClock::new(std::time::SystemTime::UNIX_EPOCH),
        &coven::SequentialIds::new(),
    )
    .expect("build audio formats");
    (bytes, built.audio_segments)
}

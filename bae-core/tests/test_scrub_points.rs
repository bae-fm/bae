#![cfg(feature = "test-utils")]
//! What import stores for a scrub to start at in a FLAC the demuxer can only
//! binary-search: no SEEKTABLE, or one holding only placeholder points. Each
//! segment gets its first frame, then the first frame each 10 seconds of audio
//! on, every one an exact frame start the file's own packets confirm. A FLAC
//! with a real seektable gets none. And in every MP3 (a whole file: bae binds
//! no CUE sheet to lossy audio): for the first frame each 10 seconds of audio
//! on, an earlier frame a decode can start at and play it exactly.

use bae_catalogs::discogs::models::DiscogsRelease;
use bae_core::db::ScrubPoint;
use bae_core::library::{LibraryManager, ResolvedTrackAudio};
use bae_test_support as support;
use std::path::Path;
use tempfile::TempDir;

const SAMPLE_RATE: u32 = 44_100;
const TEN_SECONDS: u64 = 10 * SAMPLE_RATE as u64;

/// `seconds` of 16-bit stereo noise, encoded as a FLAC with no seektable
/// (FFmpeg's FLAC muxer writes none).
fn noise_flac(seconds: u32, seed: u32) -> Vec<u8> {
    let mut state = seed | 1;
    let samples: Vec<i32> = (0..SAMPLE_RATE * 2 * seconds)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as i32 >> 16) << 16
        })
        .collect();
    bae_audio::init();
    bae_audio::encode_i32(
        bae_audio::EncodeFormat::Flac {
            bits_per_sample: 16,
        },
        &samples,
        SAMPLE_RATE,
        2,
    )
    .expect("encode the noise FLAC")
}

/// `flac` with a SEEKTABLE of `count` placeholder points after its STREAMINFO,
/// the block a ripper reserves to fill in later.
fn with_placeholder_seektable(flac: &[u8], count: usize) -> Vec<u8> {
    const STREAMINFO_END: usize = 4 + 4 + 34;
    assert_eq!(&flac[..4], b"fLaC");
    assert_eq!(flac[4] & 0x7f, 0, "STREAMINFO comes first");
    let streaminfo_last = flac[4] & 0x80 != 0;
    let mut out = flac[..STREAMINFO_END].to_vec();
    out[4] &= 0x7f;
    let length = 18 * count;
    out.push(if streaminfo_last { 0x83 } else { 0x03 });
    out.extend_from_slice(&(length as u32).to_be_bytes()[1..]);
    for _ in 0..count {
        out.extend_from_slice(&u64::MAX.to_be_bytes());
        out.extend_from_slice(&[0; 10]);
    }
    out.extend_from_slice(&flac[STREAMINFO_END..]);
    out
}

/// `seconds` of 16-bit stereo noise, encoded as a 128 kbps MP3.
fn noise_mp3(seconds: u32, seed: u32) -> Vec<u8> {
    let mut state = seed | 1;
    let samples: Vec<i32> = (0..SAMPLE_RATE * 2 * seconds)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as i32 >> 18) << 16
        })
        .collect();
    bae_audio::init();
    bae_audio::encode_i32(
        bae_audio::EncodeFormat::Mp3 { bitrate_kbps: 128 },
        &samples,
        SAMPLE_RATE,
        2,
    )
    .expect("encode the noise MP3")
}

/// Every frame of the MP3 at `path`, as its demuxer reads it from the start:
/// the frame's first sample, past the encoder delay its first frame begins
/// before sample 0 by, and the byte it begins at.
fn mp3_frames(path: &Path) -> Vec<(i64, u64)> {
    let mut input = ffmpeg_next::format::input(&path).expect("open the MP3");
    let stream = input.stream(0).expect("the MP3's stream");
    let time_base = stream.time_base();
    let start = stream.start_time();
    let per_sample = i64::from(time_base.denominator()) / i64::from(SAMPLE_RATE);
    input
        .packets()
        .map(|(_, packet)| {
            let pts = packet.pts().expect("an MP3 packet's timestamp");
            ((pts - start) / per_sample, packet.position() as u64)
        })
        .collect()
}

/// Check a whole-file MP3's `points` against its frames: for the first frame
/// each 10 seconds of audio past the stream's first, a frame before it that
/// plays it exactly, every one an exact frame start. The stream's first frame
/// has none: a decode from the stream's start serves it.
fn assert_mp3_points(points: &[ScrubPoint], frames: &[(i64, u64)]) {
    let mut served = vec![frames[0].0];
    for &(sample, _) in frames {
        if sample >= served.last().unwrap() + TEN_SECONDS as i64 {
            served.push(sample);
        }
    }
    served.remove(0);
    assert_eq!(
        points.len(),
        served.len(),
        "points {points:?} for frames at {served:?}"
    );
    for (point, served) in points.iter().zip(served) {
        assert!(
            frames.contains(&(point.sample as i64, point.byte)),
            "{point:?} is a frame start"
        );
        assert!(
            point.sample < point.exact_from && point.exact_from as i64 <= served,
            "{point:?} plays the frame at {served}"
        );
    }
}

/// Every frame of the FLAC at `path`, as its demuxer reads it from the start:
/// the frame's first sample and the byte it begins at. A FLAC frame decodes
/// alone, so a decode from it plays exactly from its own sample.
fn frames(path: &Path) -> Vec<ScrubPoint> {
    let mut input = ffmpeg_next::format::input(&path).expect("open the FLAC");
    input
        .packets()
        .map(|(_, packet)| {
            let sample = packet.pts().expect("a FLAC packet's timestamp") as u64;
            ScrubPoint {
                sample,
                byte: packet.position() as u64,
                exact_from: sample,
            }
        })
        .collect()
}

/// Check `points` against the frames of their file for a segment from
/// `start_sample` to `end_sample`: the frame the segment starts in, then the
/// first frame each 10 seconds of audio after the last point, all before the
/// segment's end. Noise at 44.1 kHz puts 10 s well inside one fill window, so
/// time alone spaces them.
fn assert_points_every_ten_seconds(
    points: &[ScrubPoint],
    frames: &[ScrubPoint],
    start_sample: u64,
    end_sample: u64,
) {
    let first = frames
        .iter()
        .rev()
        .find(|frame| frame.sample <= start_sample)
        .expect("a frame holds the segment's first sample");
    let mut expected = vec![*first];
    for frame in frames {
        let last = expected.last().unwrap();
        if frame.sample < end_sample && frame.sample >= last.sample + TEN_SECONDS {
            expected.push(*frame);
        }
    }
    assert_eq!(points, expected.as_slice());
}

async fn tracks(library_manager: &LibraryManager, release_id: &str) -> Vec<ResolvedTrackAudio> {
    let mut resolved = Vec::new();
    for track in library_manager
        .get_tracks_for_release(release_id)
        .await
        .expect("the release's tracks")
    {
        resolved.push(
            library_manager
                .resolve_track_audio(&track.id)
                .await
                .expect("resolve the track's audio"),
        );
    }
    resolved
}

fn release(id: &str, tracks: &[(&str, &str)]) -> DiscogsRelease {
    DiscogsRelease {
        master_id: Some(format!("{id}-master")),
        ..support::discogs_test_release(id, "Album Title", tracks)
    }
}

/// What an import of a written folder landed: its tracks' resolved audio,
/// the folder the files sit in, and the directory that owns both.
struct Imported {
    tracks: Vec<ResolvedTrackAudio>,
    album_dir: std::path::PathBuf,
    _temp_dir: TempDir,
}

/// Import `release` from the folder `write_files` fills.
async fn import(release: DiscogsRelease, write_files: impl FnOnce(&Path)) -> Imported {
    let (library_manager, imported) = support::imported_release_setup(
        release,
        "test",
        uuid::Uuid::new_v4().to_string(),
        write_files,
    )
    .await
    .expect("import the release");
    Imported {
        tracks: tracks(&library_manager, &imported.release_id).await,
        album_dir: imported.album_dir,
        _temp_dir: imported.temp_dir,
    }
}

#[tokio::test]
async fn a_single_file_album_without_a_seektable_stores_points_per_segment() {
    support::tracing_init();
    let album = noise_flac(75, 7);
    let imported = import(
        release(
            "scrub-points-cue",
            &[
                ("Track One", "0:23"),
                ("Track Two", "0:25"),
                ("Track Three", "0:25"),
            ],
        ),
        move |dir| {
            std::fs::write(dir.join("Album Title.flac"), &album).unwrap();
            std::fs::write(
                dir.join("Album Title.cue"),
                "\
PERFORMER \"Artist Name\"
TITLE \"Album Title\"
FILE \"Album Title.flac\" WAVE
  TRACK 01 AUDIO
    TITLE \"Track One\"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE \"Track Two\"
    INDEX 00 00:23:00
    INDEX 01 00:25:00
  TRACK 03 AUDIO
    TITLE \"Track Three\"
    INDEX 01 00:50:00
",
            )
            .unwrap();
        },
    )
    .await;
    let frames = frames(&imported.album_dir.join("Album Title.flac"));

    let tracks = &imported.tracks;
    let segments: Vec<_> = tracks.iter().flat_map(|track| &track.segments).collect();
    assert_eq!(
        segments.len(),
        4,
        "three main segments and track two's pregap"
    );
    for segment in segments {
        assert_points_every_ten_seconds(
            segment.scrub_points.as_slice(),
            &frames,
            segment.span.start_sample,
            segment.span.end_sample.unwrap_or(u64::MAX),
        );
    }
    let counts: Vec<usize> = tracks
        .iter()
        .flat_map(|track| &track.segments)
        .map(|segment| segment.scrub_points.as_slice().len())
        .collect();
    // 23 s, then track two's 2 s pregap and its 25 s, then 25 s.
    assert_eq!(counts, [3, 1, 3, 3]);
}

#[tokio::test]
async fn per_track_files_without_a_seektable_store_points() {
    support::tracing_init();
    let files = [noise_flac(25, 11), noise_flac(32, 13)];
    let written = files.clone();
    let imported = import(
        release(
            "scrub-points-per-track",
            &[("Track One", "0:25"), ("Track Two", "0:32")],
        ),
        move |dir| {
            std::fs::write(dir.join("01 Track One.flac"), &written[0]).unwrap();
            std::fs::write(dir.join("02 Track Two.flac"), &written[1]).unwrap();
        },
    )
    .await;
    let names = ["01 Track One.flac", "02 Track Two.flac"];
    assert_eq!(imported.tracks.len(), 2);
    for (index, track) in imported.tracks.iter().enumerate() {
        let frames = frames(&imported.album_dir.join(names[index]));
        let points = track.segments[0].scrub_points.as_slice();
        assert_points_every_ten_seconds(points, &frames, 0, u64::MAX);
        assert_eq!(points.len(), [3, 4][index], "{points:?}");
    }
}

/// A SEEKTABLE of only placeholder points gives FFmpeg's FLAC demuxer one
/// index entry, before the first sample: a seek through it lands on the first
/// frame and decodes the whole file up to the target. The file needs points
/// as much as one with no table.
#[tokio::test]
async fn a_seektable_of_placeholders_counts_as_none() {
    support::tracing_init();
    let flac = with_placeholder_seektable(&noise_flac(25, 17), 4);
    let imported = import(
        release("scrub-points-placeholders", &[("Track One", "0:25")]),
        move |dir| std::fs::write(dir.join("01 Track One.flac"), &flac).unwrap(),
    )
    .await;
    let frames = frames(&imported.album_dir.join("01 Track One.flac"));
    let points = imported.tracks[0].segments[0].scrub_points.as_slice();
    assert_points_every_ten_seconds(points, &frames, 0, u64::MAX);
    assert_eq!(points.len(), 3);
}

/// The CUE/FLAC fixture carries a real SEEKTABLE: the demuxer seeks it by its
/// own index, so import stores no points.
#[tokio::test]
async fn a_file_with_a_seektable_stores_no_points() {
    support::tracing_init();
    let fixture_dir = support::fixture_dir!("cue_flac");
    let imported = import(
        release(
            "scrub-points-seektable",
            &[
                ("Track One (Silence)", "0:10"),
                ("Track Two (White Noise)", "0:10"),
                ("Track Three (Brown Noise)", "0:10"),
            ],
        ),
        move |dir| {
            for name in ["Test Album.flac", "Test Album.cue"] {
                std::fs::write(
                    dir.join(name),
                    support::read_fixture(&fixture_dir.join(name)),
                )
                .unwrap();
            }
        },
    )
    .await;
    assert_eq!(imported.tracks.len(), 3);
    for segment in imported.tracks.iter().flat_map(|track| &track.segments) {
        assert!(
            segment.scrub_points.is_empty(),
            "a segment at sample {} has points",
            segment.span.start_sample
        );
    }
}

#[tokio::test]
async fn per_track_mp3s_store_points() {
    support::tracing_init();
    let files = [noise_mp3(25, 11), noise_mp3(32, 13)];
    let written = files.clone();
    let imported = import(
        release(
            "scrub-points-per-track-mp3",
            &[("Track One", "0:25"), ("Track Two", "0:32")],
        ),
        move |dir| {
            std::fs::write(dir.join("01 Track One.mp3"), &written[0]).unwrap();
            std::fs::write(dir.join("02 Track Two.mp3"), &written[1]).unwrap();
        },
    )
    .await;
    let names = ["01 Track One.mp3", "02 Track Two.mp3"];
    assert_eq!(imported.tracks.len(), 2);
    for (index, track) in imported.tracks.iter().enumerate() {
        let frames = mp3_frames(&imported.album_dir.join(names[index]));
        let points = track.segments[0].scrub_points.as_slice();
        assert_mp3_points(points, &frames);
        // The frames 10 and 20 seconds in, and 30 in the longer file; the
        // first seconds decode from the stream's start.
        assert_eq!(points.len(), [2, 3][index], "{points:?}");
    }
}

#![cfg(feature = "test-utils")]
//! The loudness pass and a save read a WAV or AIFF image's track as the
//! track's own audio: each track of these images is a tone of its own pitch
//! and level, so a track read from anywhere else measures and saves
//! differently. Both demuxers restart their timestamps at 0 after a jump by
//! byte, so a track that jumped to its recorded start byte would be read from
//! twice its start.

use bae_audio::sparse_buffer::{create_sparse_buffer, SharedSparseBuffer};
use bae_audio::EncodeFormat;
use bae_core::discogs::models::DiscogsRelease;
use bae_test_support as support;

const SAMPLE_RATE: u32 = 44_100;
/// Every track's length.
const TRACK_SECONDS: u32 = 3;
/// Each track's tone: its frequency in Hz and its level.
const TRACK_TONES: [(f64, f64); 3] = [(220.0, 0.02), (440.0, 0.5), (880.0, 0.1)];

/// The three tracks' tones one after the other, as 16-bit-range stereo.
fn image_samples() -> Vec<i32> {
    let track_frames = (SAMPLE_RATE * TRACK_SECONDS) as usize;
    (0..track_frames * TRACK_TONES.len() * 2)
        .map(|i| {
            let frame = i / 2;
            let (frequency, level) = TRACK_TONES[frame / track_frames];
            let t = frame as f64 / f64::from(SAMPLE_RATE);
            let v = level * (2.0 * std::f64::consts::PI * frequency * t).sin();
            ((v * f64::from(i16::MAX)) as i32) << 16
        })
        .collect()
}

fn buffer_from(bytes: &[u8]) -> SharedSparseBuffer {
    let buffer = create_sparse_buffer(bytes.len() as u64);
    buffer.append_at(0, bytes);
    buffer
}

/// Interleaved samples of track 2, as a decode of the whole image puts them out.
fn track2_of(image: &[u8]) -> Vec<i32> {
    let whole = bae_audio::decode_audio(buffer_from(image), None, None).expect("decode the image");
    let track_frames = (SAMPLE_RATE * TRACK_SECONDS) as usize;
    whole.samples[track_frames * 2..track_frames * 2 * 2].to_vec()
}

/// Import an image of `format` named `Album Title.{extension}` with a CUE
/// sheet of the three tracks. Returns the library, track 2's id, the image's
/// bytes, and the directory that owns it all.
async fn import_image(
    extension: &str,
    format: EncodeFormat,
) -> (
    bae_core::library::LibraryManager,
    String,
    Vec<u8>,
    tempfile::TempDir,
) {
    support::tracing_init();
    bae_audio::init();
    let image =
        bae_audio::encode_i32(format, &image_samples(), SAMPLE_RATE, 2).expect("encode the image");
    let written = image.clone();
    let name = format!("Album Title.{extension}");
    let release = DiscogsRelease {
        master_id: Some(format!("image-{extension}-master")),
        ..support::discogs_test_release(
            &format!("image-{extension}"),
            "Album Title",
            &[
                ("Track One", "0:03"),
                ("Track Two", "0:03"),
                ("Track Three", "0:03"),
            ],
        )
    };
    let (library_manager, imported) = support::imported_release_setup(
        release,
        "test",
        uuid::Uuid::new_v4().to_string(),
        move |dir| {
            std::fs::write(dir.join(&name), &written).unwrap();
            std::fs::write(
                dir.join("Album Title.cue"),
                format!(
                    "FILE \"{name}\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Track One\"\n    INDEX 01 00:00:00\n  \
                     TRACK 02 AUDIO\n    TITLE \"Track Two\"\n    INDEX 01 00:03:00\n  \
                     TRACK 03 AUDIO\n    TITLE \"Track Three\"\n    INDEX 01 00:06:00\n"
                ),
            )
            .unwrap();
        },
    )
    .await
    .expect("import the image");
    assert_eq!(imported.track_ids.len(), 3, "the image's three tracks");
    (
        library_manager,
        imported.track_ids[1].clone(),
        image,
        imported.temp_dir,
    )
}

/// The loudness of `samples` as the import's meter reads it.
fn loudness_of(samples: &[i32]) -> f64 {
    let mut meter = bae_core::loudness::LoudnessMeter::new(2, SAMPLE_RATE).expect("a meter");
    meter.add_chunk(samples).expect("meter the samples");
    meter
        .finish()
        .expect("finish the meter")
        .1
        .expect("an audible track")
        .loudness_lufs
}

/// Import an image and check the loudness its import measured for track 2 is
/// track 2's own.
async fn check_track2_loudness(extension: &str, format: EncodeFormat) {
    let (library_manager, track2, image, _dir) = import_image(extension, format).await;
    let measured = library_manager
        .resolve_track_audio(&track2)
        .await
        .expect("resolve track 2")
        .track_loudness_lufs
        .expect("track 2 is measured");
    let own = loudness_of(&track2_of(&image));
    assert!(
        (measured - own).abs() < 1e-9,
        "{extension}: track 2 measured {measured} LUFS, its own audio {own} LUFS"
    );
}

/// Import an image, save track 2 as FLAC, and check the saved file holds
/// track 2's own samples.
async fn check_track2_save(extension: &str, format: EncodeFormat) {
    let (library_manager, track2, image, dir) = import_image(extension, format).await;
    let out = dir.path().join("saved.flac");
    library_manager
        .save_track(&track2, &out, "flac")
        .await
        .expect("save track 2");
    let saved = bae_audio::decode_audio(buffer_from(&std::fs::read(&out).unwrap()), None, None)
        .expect("decode the saved track");
    let own = track2_of(&image);
    assert_eq!(
        saved.samples.len(),
        own.len(),
        "{extension}: saved sample count"
    );
    assert!(
        saved.samples == own,
        "{extension}: the saved track's samples are not track 2's own"
    );
}

#[tokio::test]
async fn a_wav_images_track_is_measured_from_its_own_audio() {
    check_track2_loudness(
        "wav",
        EncodeFormat::PcmWav {
            bits_per_sample: 16,
        },
    )
    .await;
}

#[tokio::test]
async fn an_aiff_images_track_is_measured_from_its_own_audio() {
    check_track2_loudness(
        "aiff",
        EncodeFormat::PcmAiff {
            bits_per_sample: 16,
        },
    )
    .await;
}

#[tokio::test]
async fn a_wav_images_track_saves_its_own_audio() {
    check_track2_save(
        "wav",
        EncodeFormat::PcmWav {
            bits_per_sample: 16,
        },
    )
    .await;
}

#[tokio::test]
async fn an_aiff_images_track_saves_its_own_audio() {
    check_track2_save(
        "aiff",
        EncodeFormat::PcmAiff {
            bits_per_sample: 16,
        },
    )
    .await;
}

use super::*;
use crate::test_samples::{filled_sparse_buffer, wav_with_fmt};
use crate::{
    decode_audio, decode_audio_to_sink_with_seek, encode_i32, init, DecodeStart, DecodedSink,
    EncodeFormat,
};
use std::sync::Arc;

/// `seek_landing_bytes` actually opens the file, seeks (no decode), and
/// reports ascending byte positions within the file that follow the sample
/// positions -- what the import-time byte boundary computation relies on.
#[test]
fn seek_landing_bytes_track_sample_positions() {
    init();
    let sample_rate = 44100u32;
    let seconds = 4usize;
    let total = sample_rate as usize * seconds;
    let samples: Vec<i32> =
        (0..total).map(|i| ((i as f64 * 0.02).sin() * 0.5 * i32::MAX as f64) as i32).collect();
    let flac =
        encode_i32(EncodeFormat::Flac { bits_per_sample: 16 }, &samples, sample_rate, 1).unwrap();

    let mut file = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut file, &flac).unwrap();
    std::io::Write::flush(&mut file).unwrap();
    let path = file.path();
    let file_size = flac.len() as u64;

    // Probe at second boundaries (0, 1s, 2s, 3s).
    let probes: Vec<u64> = (0..seconds as u64).map(|s| s * sample_rate as u64).collect();
    let offsets = seek_landing_bytes(path, &probes).expect("seek_landing_bytes");
    assert_eq!(offsets.len(), probes.len());

    // Non-decreasing and within the file.
    for w in offsets.windows(2) {
        assert!(w[1] >= w[0], "byte offsets must ascend with samples: {offsets:?}");
    }
    assert!(
        *offsets.last().unwrap() < file_size,
        "offsets stay within the file ({offsets:?} vs {file_size})"
    );
    // A later sample is strictly later in the file, and the 3/4 sample is
    // well into the file -- the offsets follow the content, not a constant.
    assert!(offsets[seconds - 1] > offsets[0]);
    assert!(
        offsets[seconds - 1] as f64 > file_size as f64 * 0.4,
        "the 3/4 sample should be well into the file: {} of {file_size}",
        offsets[seconds - 1]
    );
}

/// `seek_landing_bytes` against the real single-file CUE/FLAC fixture:
/// silence -> white noise -> brown noise, so the bitrate swings widely from
/// track to track. The seek offsets follow the *content*, not time -- the
/// silence track compresses to almost nothing, so the 10s track-2 boundary
/// lands at a small slice of the file, far below where a time-proportional
/// estimate puts it. That gap is why the offsets are computed at import by
/// seeking rather than estimated from time at playback.
#[test]
fn seek_landing_bytes_follow_content_not_time_when_bitrate_swings() {
    init();
    let path = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/cue_flac/Test Album.flac"
    ));
    let probe = probe_audio_from_path(path).expect("probe fixture");
    let sr = u64::from(probe.sample_rate.get());
    let total_samples = (probe.duration.as_secs_f64() * sr as f64) as u64;
    let file_size = std::fs::metadata(path).unwrap().len();

    // Track starts from the CUE: 0s (silence), 10s (white noise), 20s (brown).
    let starts = [0u64, 10 * sr, 20 * sr];
    let exact = seek_landing_bytes(path, &starts).expect("offsets");

    // Time-proportional estimate -- what we'd get without seeking.
    let proportional: Vec<u64> = starts
        .iter()
        .map(|&s| (s as f64 / total_samples as f64 * file_size as f64) as u64)
        .collect();

    // The 10s boundary (end of the silence track) is where they diverge most:
    // silence is nearly free to store, so the exact offset is a small slice of
    // the file while the estimate puts it ~10/total of the way in.
    assert!(
        exact[1] < file_size / 5,
        "silence track should occupy a small byte slice; exact[1]={} of {file_size}",
        exact[1]
    );
    assert!(
        proportional[1] > exact[1] * 2,
        "the time estimate over-places the boundary: prop={} exact={}",
        proportional[1],
        exact[1]
    );
    // Ascending and within the file.
    assert!(exact[0] <= exact[1] && exact[1] <= exact[2] && exact[2] < file_size);
    // The brown-noise track (20s start) lands deep in the file -- it compresses
    // far worse than the silence, so most of the file is its bytes. Confirms the
    // landings track content in both directions, not just the silent-track dip.
    assert!(
        exact[2] as f64 > file_size as f64 * 0.4,
        "brown-noise track start lands deep in the file: {} of {file_size}",
        exact[2]
    );
}

#[test]
fn content_type_from_codec_id_maps_every_named_codec() {
    use ffmpeg_sys_next::AVCodecID;
    assert_eq!(content_type_from_codec_id(AVCodecID::AV_CODEC_ID_FLAC), ContentType::Flac);
    assert_eq!(content_type_from_codec_id(AVCodecID::AV_CODEC_ID_MP3), ContentType::Mp3);
    assert_eq!(content_type_from_codec_id(AVCodecID::AV_CODEC_ID_APE), ContentType::Ape);
    assert_eq!(content_type_from_codec_id(AVCodecID::AV_CODEC_ID_ALAC), ContentType::Alac);
    assert_eq!(content_type_from_codec_id(AVCodecID::AV_CODEC_ID_AAC), ContentType::Aac);
    assert_eq!(content_type_from_codec_id(AVCodecID::AV_CODEC_ID_PCM_S16LE), ContentType::Pcm);
    assert_eq!(content_type_from_codec_id(AVCodecID::AV_CODEC_ID_PCM_F64BE), ContentType::Pcm);
    assert_eq!(content_type_from_codec_id(AVCodecID::AV_CODEC_ID_OPUS), ContentType::Opus);
    assert_eq!(content_type_from_codec_id(AVCodecID::AV_CODEC_ID_VORBIS), ContentType::Vorbis);
    assert_eq!(content_type_from_codec_id(AVCodecID::AV_CODEC_ID_WAVPACK), ContentType::WavPack);
    assert_eq!(content_type_from_codec_id(AVCodecID::AV_CODEC_ID_DSD_LSBF), ContentType::Dsd);
    assert_eq!(content_type_from_codec_id(AVCodecID::AV_CODEC_ID_DSD_MSBF), ContentType::Dsd);
    assert_eq!(
        content_type_from_codec_id(AVCodecID::AV_CODEC_ID_DSD_LSBF_PLANAR),
        ContentType::Dsd
    );
    assert_eq!(
        content_type_from_codec_id(AVCodecID::AV_CODEC_ID_DSD_MSBF_PLANAR),
        ContentType::Dsd
    );
}

#[test]
fn content_type_from_codec_id_unknown_falls_into_other() {
    use ffmpeg_sys_next::AVCodecID;
    // Any codec outside our whitelist must round-trip as `Other(...)` so
    // the DB preserves the ID and nothing downstream silently treats it
    // as a known format.
    let ct = content_type_from_codec_id(AVCodecID::AV_CODEC_ID_AC3);
    match ct {
        ContentType::Other(s) => assert!(s.starts_with("codec:")),
        other => panic!("expected Other(codec:...), got {:?}", other),
    }
}

/// One second of 16-bit stereo silence at 44.1 kHz, as a WAV file in `dir`
/// under `name`.
fn silent_wav_in(dir: &std::path::Path, name: &std::ffi::OsStr) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, wav_with_fmt(1, 16, 2, 44_100, &vec![0; 44_100 * 4])).unwrap();
    path
}

/// The probe reads the file through the handle Rust opens at the path, and
/// a path nothing can be opened at gives nothing.
#[test]
fn probe_reads_the_file_through_its_handle() {
    init();
    let dir = tempfile::tempdir().unwrap();
    let path = silent_wav_in(dir.path(), std::ffi::OsStr::new("Track.wav"));

    let probe = probe_audio_from_path(&path).expect("probe");

    assert_eq!(probe.content_type, ContentType::Pcm);
    assert_eq!((probe.sample_rate.get(), probe.channels.get()), (44_100, 2));
    assert_eq!(probe.duration, std::time::Duration::from_secs(1));
    assert!(matches!(
        probe_audio_from_path(&dir.path().join("Absent.wav")),
        Err(ProbeError::Io(_))
    ));
}

/// A file whose bytes can't be read is an `Io` failure, not a file that holds
/// no audio: a directory opens but every read of it errors.
#[cfg(unix)]
#[test]
fn a_failed_read_is_an_io_error_not_a_file_without_audio() {
    init();
    let dir = tempfile::tempdir().unwrap();

    assert!(matches!(probe_audio_from_path(dir.path()), Err(ProbeError::Io(_))));
}

/// Bytes that read fine but are no audio are `NotAudio`.
#[test]
fn bytes_that_are_no_audio_are_not_audio() {
    init();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Track.flac");
    std::fs::write(&path, vec![0u8; 4096]).unwrap();

    assert!(matches!(probe_audio_from_path(&path), Err(ProbeError::NotAudio(_))));
}

/// A file whose name isn't UTF-8 probes like any other: FFmpeg is handed the
/// file, never its name.
#[cfg(target_os = "linux")]
#[test]
fn probe_reads_a_file_whose_name_is_not_utf8() {
    use std::os::unix::ffi::OsStrExt;

    init();
    let dir = tempfile::tempdir().unwrap();
    let path = silent_wav_in(dir.path(), std::ffi::OsStr::from_bytes(b"Caf\xE9.wav"));

    let probe = probe_audio_from_path(&path).expect("probe");

    assert_eq!(probe.duration, std::time::Duration::from_secs(1));
}

#[test]
fn probe_audio_from_path_maps_audio_format_fixtures() {
    init();
    let fixture =
        |name: &str| format!("{}/test-fixtures/audio-format/{name}", env!("CARGO_MANIFEST_DIR"));
    for (name, expected, container) in [
        ("placeholder-pcm.wav", ContentType::Pcm, AudioContainer::Wav),
        ("placeholder-pcm.aiff", ContentType::Pcm, AudioContainer::Aiff),
        ("placeholder-mp3.mp3", ContentType::Mp3, AudioContainer::Mp3),
        ("placeholder-aac.m4a", ContentType::Aac, AudioContainer::Mp4),
        ("placeholder-opus.opus", ContentType::Opus, AudioContainer::Ogg),
        ("placeholder-vorbis.ogg", ContentType::Vorbis, AudioContainer::Ogg),
        ("placeholder-wavpack.wv", ContentType::WavPack, AudioContainer::WavPack),
        ("placeholder-dsd.dsf", ContentType::Dsd, AudioContainer::Dsf),
        ("placeholder-dsd.dff", ContentType::Dsd, AudioContainer::Dsdiff),
    ] {
        let path = fixture(name);
        let probe = probe_audio_from_path(std::path::Path::new(&path))
            .unwrap_or_else(|e| panic!("probe {name}: {e}"));
        assert_eq!(probe.content_type, expected, "{name}");
        assert_eq!(probe.container, container, "{name}");
    }
}

/// The container is read from the bytes, never the name: FLAC bytes named
/// `.wav` are in FLAC's own container.
#[test]
fn probe_reads_the_container_from_the_bytes() {
    init();
    let (flac, _) = stereo_flac_file(4_410);
    let dir = tempfile::tempdir().unwrap();
    let misnamed = dir.path().join("track.wav");
    std::fs::copy(flac.path(), &misnamed).unwrap();

    let probe = probe_audio_from_path(&misnamed).expect("probe");

    assert_eq!(probe.content_type, ContentType::Flac);
    assert_eq!(probe.container, AudioContainer::Flac);
}

/// Collects a decode's interleaved samples.
#[derive(Default)]
struct CollectedSamples(Vec<i32>);

impl DecodedSink for CollectedSamples {
    fn on_format(&mut self, _sample_rate: u32, _channels: u32) {}

    fn on_samples(&mut self, samples: &[i32]) {
        self.0.extend_from_slice(samples);
    }
}

/// A 16-bit stereo FLAC of `frames` frames, each channel `level(frame)` of full
/// scale plus a little noise, written to a temp file. Returns the file and its
/// bytes.
fn stereo_flac_file(frames: usize) -> (tempfile::NamedTempFile, Vec<u8>) {
    let mut noise: u64 = 1;
    let samples: Vec<i32> = (0..frames * 2)
        .map(|i| {
            noise = noise
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let jitter = (noise >> 33) as f64 / (1u64 << 31) as f64 - 0.5;
            (((i as f64 * 0.013).sin() * 0.5 + jitter * 0.05) * i32::MAX as f64) as i32
        })
        .collect();
    let flac = encode_i32(EncodeFormat::Flac { bits_per_sample: 16 }, &samples, 44_100, 2).unwrap();
    let mut file = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut file, &flac).unwrap();
    std::io::Write::flush(&mut file).unwrap();
    (file, flac)
}

/// Decode `flac` by jumping to the byte `landing`, as playback starts a track:
/// trimmed to `start` when given, else everything from the landing's frame.
fn decode_from_landing(flac: &[u8], landing: u64, start: Option<u64>) -> Vec<i32> {
    let mut decoded = CollectedSamples::default();
    decode_audio_to_sink_with_seek(
        filled_sparse_buffer(flac),
        DecodeStart::Byte(landing),
        start,
        None,
        &mut decoded,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .expect("decode from the landing");
    decoded.0
}

/// A target in the middle of a long FLAC is found by searching, not by reading
/// the file up to it: the bytes read stay a small part of the file, and the
/// landing is the target's frame.
#[test]
fn seek_landing_bytes_search_a_long_flac_rather_than_read_it() {
    init();
    let frames = 4_608 * 1_000;
    let (file, flac) = stereo_flac_file(frames);
    let path = file.path();
    let target = frames as u64 / 2 + 123;

    let landing = seek_landing_bytes(path, &[target]).expect("landing")[0];

    let read = crate::test_reads::landing_search(file.path());
    assert!(read > 0, "the search's reads are counted");
    assert!(
        read < flac.len() as u64 / 10,
        "finding one landing read {read} of the file's {} bytes",
        flac.len()
    );
    let ground =
        decode_audio(filled_sparse_buffer(&flac), None, None).expect("ground-truth decode");
    let decoded = decode_from_landing(&flac, landing, Some(target));
    assert!(
        decoded == ground.samples[target as usize * 2..],
        "the landing decodes from the target sample on"
    );
}

/// A target in a FLAC's last frame lands on the frame before it, the last one
/// a byte-seeked decode can date, and one in the third-to-last frame -- where
/// FFmpeg's binary search fails -- lands on its own frame. Both decode exactly.
#[test]
fn seek_landing_bytes_land_targets_in_a_flacs_final_frames() {
    init();
    let frames = 4_608 * 30 + 1_000;
    let (file, flac) = stereo_flac_file(frames);
    let path = file.path();
    // STREAMINFO follows the 4-byte marker and its 4-byte block header; its
    // maximum block size is bytes 2..4, and this encoder's blocks are all that
    // size but the last.
    let block = u64::from(u16::from_be_bytes([flac[10], flac[11]]));
    let total = frames as u64;
    let last_frame_start = (total - 1) / block * block;
    let third_to_last = last_frame_start - 2 * block;
    let targets = [total - 1, third_to_last + 100];

    let landings = seek_landing_bytes(path, &targets).expect("landings");

    let ground =
        decode_audio(filled_sparse_buffer(&flac), None, None).expect("ground-truth decode");
    assert_eq!(
        decode_from_landing(&flac, landings[0], None).len() as u64 / 2,
        total - (last_frame_start - block),
        "the last frame's target lands on the frame before it"
    );
    assert_eq!(
        decode_from_landing(&flac, landings[1], None).len() as u64 / 2,
        total - third_to_last,
        "the third-to-last frame's target lands on its frame"
    );
    for (&target, &landing) in targets.iter().zip(&landings) {
        assert!(
            decode_from_landing(&flac, landing, Some(target))
                == ground.samples[target as usize * 2..],
            "target {target} decodes exactly from its landing"
        );
    }
}

/// FFmpeg states a stream's sample rate and channel count as signed ints. One
/// that isn't positive states none, and is never read as a huge unsigned
/// count.
#[test]
fn a_count_ffmpeg_states_as_zero_or_less_states_none() {
    assert_eq!(stated_count(-1), None);
    assert_eq!(stated_count(i32::MIN), None);
    assert_eq!(stated_count(0), None);
    assert_eq!(stated_count(44_100).map(std::num::NonZeroU32::get), Some(44_100));
}

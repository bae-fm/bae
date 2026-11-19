use super::*;
use crate::init;
use crate::test_samples::wav_with_fmt;

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
fn silent_wav_in(
    dir: &bae_util::test_files::TestFolder,
    name: &std::ffi::OsStr,
) -> std::path::PathBuf {
    dir.write(name, wav_with_fmt(1, 16, 2, 44_100, &vec![0; 44_100 * 4]))
}

/// The probe reads the file through the handle Rust opens at the path, and
/// a path nothing can be opened at gives nothing.
#[test]
fn probe_reads_the_file_through_its_handle() {
    init();
    let dir = bae_util::test_files::TestFolder::new();
    let path = silent_wav_in(&dir, std::ffi::OsStr::new("Track.wav"));

    let probe = crate::test_probe::probe_file(&path).expect("probe");

    assert_eq!(probe.content_type, ContentType::Pcm);
    assert_eq!((probe.sample_rate.get(), probe.channels.get()), (44_100, 2));
    assert_eq!(probe.duration, std::time::Duration::from_secs(1));
    assert!(matches!(
        crate::test_probe::probe_file(&dir.path().join("Absent.wav")),
        Err(ProbeError::Io(_))
    ));
}

/// A file whose bytes can't be read is an `Io` failure, not a file that holds
/// no audio: a directory opens but every read of it errors.
#[cfg(unix)]
#[test]
fn a_failed_read_is_an_io_error_not_a_file_without_audio() {
    init();
    let dir = bae_util::test_files::TestFolder::new();

    assert!(matches!(crate::test_probe::probe_file(dir.path()), Err(ProbeError::Io(_))));
}

/// Bytes that read fine but are no audio are `NotAudio`.
#[test]
fn bytes_that_are_no_audio_are_not_audio() {
    init();
    let dir = bae_util::test_files::TestFolder::new();
    let path = dir.write("Track.flac", vec![0u8; 4096]);

    assert!(matches!(crate::test_probe::probe_file(&path), Err(ProbeError::NotAudio(_))));
}

/// A file whose name isn't UTF-8 probes like any other: FFmpeg is handed the
/// file, never its name.
#[cfg(target_os = "linux")]
#[test]
fn probe_reads_a_file_whose_name_is_not_utf8() {
    use std::os::unix::ffi::OsStrExt;

    init();
    let dir = bae_util::test_files::TestFolder::new();
    let path = silent_wav_in(&dir, std::ffi::OsStr::from_bytes(b"Caf\xE9.wav"));

    let probe = crate::test_probe::probe_file(&path).expect("probe");

    assert_eq!(probe.duration, std::time::Duration::from_secs(1));
}

#[test]
fn probe_audio_maps_audio_format_fixtures() {
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
        let probe = crate::test_probe::probe_file(std::path::Path::new(&path))
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
    let flac = crate::test_fixtures::cue_flac_dir().join("Test Album.flac");
    let dir = bae_util::test_files::TestFolder::new();
    let misnamed = dir.copy_fixture(&flac, "track.wav");

    let probe = crate::test_probe::probe_file(&misnamed).expect("probe");

    assert_eq!(probe.content_type, ContentType::Flac);
    assert_eq!(probe.container, AudioContainer::Flac);
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

/// A probe closes the file it reads, which gives the file's place back when
/// it returns, the error path included.
#[test]
fn a_probe_gives_its_place_back_when_it_returns() {
    let path = crate::test_fixtures::cue_flac_dir().join("Test Album.flac");
    let budget = coven::FileBudget::for_test();
    let watch = budget.watch();
    let lease = budget.lease_blocking(1).unwrap();

    let file = bae_util::test_files::open_in(lease.place_blocking(), &path).expect("open fixture");
    assert_eq!(watch.holding(), 1);
    probe_audio(file, &path).expect("probe fixture");
    assert_eq!(watch.holding(), 0);

    let text = bae_util::test_files::TestFolder::new();
    let not_audio = text.write("notes.txt", "not audio");
    let file = bae_util::test_files::open_in(lease.place_blocking(), &not_audio).expect("open");
    assert!(matches!(probe_audio(file, &not_audio), Err(ProbeError::NotAudio(_))));
    assert_eq!(watch.holding(), 0, "a file that holds no audio gives its place back");

    let absent = path.with_file_name("Absent.flac");
    assert!(bae_util::test_files::open_in(lease.place_blocking(), &absent).is_err());
    assert_eq!(watch.holding(), 0, "a file that would not open gives its place back");
}

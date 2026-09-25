/// The OS error kind and text of a decode that stopped on a source read, or
/// `None` for any other outcome.
fn source_read_failure(result: Result<(), DecodeError>) -> Option<(std::io::ErrorKind, String)> {
    match result {
        Err(DecodeError::SourceRead(error)) => match error.as_ref() {
            crate::playback::PlaybackError::Io { source, .. } => {
                Some((source.kind(), error.to_string()))
            }
            _ => None,
        },
        _ => None,
    }
}

/// A minute of stereo noise as 16-bit WAV (~10 MiB): several fill windows, so
/// a decode reads well past the first fetch.
fn multi_window_wav() -> Vec<u8> {
    let sample_rate = 44_100u32;
    let samples: Vec<i32> = (0..sample_rate as usize * 60 * 2)
        .map(|i| (i as u32).wrapping_mul(2_654_435_761) as i32)
        .collect();
    let wav = encode_i32(
        EncodeFormat::PcmWav {
            bits_per_sample: 16,
        },
        &samples,
        sample_rate,
        2,
    )
    .unwrap();
    assert!(
        wav.len() as u64 > 2 * crate::playback::sparse_buffer::FILL_WINDOW_SIZE,
        "the source must span several fill windows"
    );
    wav
}

/// Fill `buffer` from `bytes` through the production fill loop, failing every
/// fetch that reaches byte `fail_from` or past it with a full-disk error. (The
/// fill's fetches follow the reader's demand, so they aren't window-aligned;
/// keying on the range's end makes the fetch covering `fail_from` the one that
/// fails.)
fn spawn_fill_failing_from(
    buffer: &crate::playback::SharedSparseBuffer,
    bytes: Vec<u8>,
    fail_from: u64,
) {
    tokio::spawn(buffer.clone().fill_on_demand(move |start, len| {
        let result = if start + len <= fail_from {
            Ok(bytes[start as usize..(start + len) as usize].to_vec())
        } else {
            Err(crate::playback::PlaybackError::io(
                format!("Failed to read {len} bytes at {start} from /volume/track.wav"),
                std::io::Error::from(std::io::ErrorKind::StorageFull),
            ))
        };
        async move { result }
    }));
}

struct NullSink;
impl DecodedSink for NullSink {
    fn on_format(&mut self, _sample_rate: u32, _channels: u32) {}
    fn on_samples(&mut self, _samples: &[i32]) {}
}

/// A decode over a source whose first read fails reports that read's own
/// error, not the "Invalid data found" FFmpeg derives from an empty probe.
#[tokio::test]
async fn i32_decode_names_the_read_error_when_the_first_read_fails() {
    use crate::playback::data_source::{AudioDataReader, LocalReader};
    use crate::playback::sparse_buffer::create_sparse_buffer;

    init();

    // A directory is a source this platform can't read: it refuses the open
    // or the first read (EISDIR on Unix, access denied on Windows). Whatever
    // error std reports for it is the one the decode must name.
    let dir = tempfile::tempdir().unwrap();
    let expected = std::fs::File::open(dir.path())
        .and_then(|mut file| std::io::Read::read(&mut file, &mut [0; 1]))
        .expect_err("reading a directory fails")
        .kind();
    let buffer = create_sparse_buffer(64 * 1024);
    Box::new(LocalReader::new(dir.path()))
        .start_reading(buffer.clone(), Box::new(|_| {}));

    let result = tokio::task::spawn_blocking(move || decode_audio(buffer, None, None).map(|_| ()))
        .await
        .expect("decode task");

    let (kind, text) = source_read_failure(result.clone())
        .unwrap_or_else(|| panic!("an unreadable source is a read failure: {result:?}"));
    assert_eq!(kind, expected, "{text}");
    assert!(!text.contains("Invalid data found"), "{text}");
}

/// A read that fails after the decode is under way (the demuxer opened on the
/// first window) is the decode's outcome too: FFmpeg's packet loop ends on it
/// the way it ends on EOF, and the decode must not pass that off as a clean
/// (truncated) window.
#[tokio::test]
async fn i32_decode_names_the_read_error_when_a_later_read_fails() {
    use crate::playback::sparse_buffer::{create_sparse_buffer, FILL_WINDOW_SIZE};

    init();

    let wav = multi_window_wav();
    let buffer = create_sparse_buffer(wav.len() as u64);
    // The third window fails: its fetch goes out once the decode reads past
    // the first, well after the demuxer opened.
    spawn_fill_failing_from(&buffer, wav, 2 * FILL_WINDOW_SIZE);

    let result = tokio::task::spawn_blocking(move || {
        decode_audio_to_sink(
            buffer,
            None,
            None,
            &mut NullSink,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
    })
    .await
    .expect("decode task");

    let (kind, text) = source_read_failure(result.clone())
        .unwrap_or_else(|| panic!("a mid-stream read failure is a read failure: {result:?}"));
    assert_eq!(kind, std::io::ErrorKind::StorageFull, "{text}");
    assert!(text.contains("/volume/track.wav"), "{text}");
}

/// The import verifier's decode distinguishes the two failures: a source
/// whose first read fails is a read failure, the same as a later one.
#[tokio::test]
async fn verifying_decode_reports_a_first_read_failure_as_a_read_failure() {
    use crate::playback::sparse_buffer::create_sparse_buffer;

    init();

    let wav = multi_window_wav();
    let buffer = create_sparse_buffer(wav.len() as u64);
    spawn_fill_failing_from(&buffer, wav, 0);

    let result = tokio::task::spawn_blocking(move || {
        decode_audio_to_verifying_sink(
            buffer,
            Some(0),
            None,
            &mut NullSink,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
    })
    .await
    .expect("decode task");

    let (kind, text) = source_read_failure(result.clone())
        .unwrap_or_else(|| panic!("an unreadable source is a read failure: {result:?}"));
    assert_eq!(kind, std::io::ErrorKind::StorageFull, "{text}");
    assert!(!text.contains("Invalid data found"), "{text}");
}

/// Playback's streaming decode stops on the read failure as its own outcome
/// (the fill reports it to the command loop), not as a cancel or a decode
/// error.
#[tokio::test]
async fn streaming_decode_stops_on_a_read_failure_as_a_read_failure() {
    use crate::playback::create_track_stream_pair_with_capacity;
    use crate::playback::sparse_buffer::{create_sparse_buffer, FILL_WINDOW_SIZE};

    init();

    let wav = multi_window_wav();
    let buffer = create_sparse_buffer(wav.len() as u64);
    // The third window fails: its fetch goes out once the decode reads past
    // the first, well after the demuxer opened.
    spawn_fill_failing_from(&buffer, wav, 2 * FILL_WINDOW_SIZE);

    let result = tokio::task::spawn_blocking(move || {
        let (mut sink, _source, _ready) = create_track_stream_pair_with_capacity(44_100, 2, 1 << 22);
        let token = Arc::new(std::sync::atomic::AtomicBool::new(false));
        decode_audio_streaming(buffer, &mut sink, None, None, None, None, None, token).map(|_| ())
    })
    .await
    .expect("decode task");

    let (kind, _) = source_read_failure(result.clone())
        .unwrap_or_else(|| panic!("a mid-stream read failure is a read failure: {result:?}"));
    assert_eq!(kind, std::io::ErrorKind::StorageFull);
}

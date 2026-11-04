//! The codec decoding through a buffer that a `LocalReader` fills: what a
//! preview and an import read their files through.

use crate::data_source::tests::{
    capturing_error_handler, drain_async, failure_kind, next_fill_error, WINDOW,
};
use crate::data_source::{AudioDataReader, LocalReader};
use bae_audio::sparse_buffer::create_sparse_buffer;
use bae_audio::test_samples::filled_sparse_buffer;
use bae_audio::{
    decode_audio, decode_audio_to_sink, encode_i32, init, DecodeError, DecodedSink, EncodeFormat,
};
use bae_util::test_channels::HANG_GUARD;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::time::timeout;

/// The OS error kind and text of a decode that stopped on a source read, or
/// `None` for any other outcome.
fn source_read_failure(result: Result<(), DecodeError>) -> Option<(std::io::ErrorKind, String)> {
    match result {
        Err(DecodeError::SourceRead(error)) => std::error::Error::source(&error)
            .and_then(|source| source.downcast_ref::<std::io::Error>())
            .map(|source| (source.kind(), error.to_string())),
        _ => None,
    }
}

/// The i32 decode path reads through a buffer being filled *concurrently* by a
/// `LocalReader`: the decode blocks on the fill and produces exactly the
/// pre-filled result — and, because the sink path sets no read-ahead ceiling,
/// the fill keeps only a window ahead and evicts behind the decode, so the
/// buffer never holds the whole file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn i32_decode_streams_from_a_live_fill_and_stays_windowed() {
    init();

    /// Order-sensitive digest of the decode, so two decodes can be compared
    /// without holding both PCM streams.
    #[derive(Default)]
    struct DigestSink {
        format: Option<(u32, u32)>,
        count: u64,
        digest: u64,
    }
    impl DecodedSink for DigestSink {
        fn on_format(&mut self, sample_rate: u32, channels: u32) {
            self.format = Some((sample_rate, channels));
        }
        fn on_samples(&mut self, samples: &[i32]) {
            for &sample in samples {
                self.count += 1;
                self.digest =
                    self.digest.wrapping_mul(1099511628211).wrapping_add(sample as u32 as u64);
            }
        }
    }

    // ~100s of pseudo-noise as WAV (~17 MiB): comfortably larger than the
    // fill's read-ahead window plus its keep-behind margin, so an unwindowed
    // buffer would blow the residency bound below.
    let sample_rate = 44_100u32;
    let seconds = 100usize;
    let samples: Vec<i32> = (0..sample_rate as usize * seconds * 2)
        .map(|i| (i as u32).wrapping_mul(2_654_435_761) as i32)
        .collect();
    let wav =
        encode_i32(EncodeFormat::PcmWav { bits_per_sample: 16 }, &samples, sample_rate, 2).unwrap();
    drop(samples);

    let never = || Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Ground truth: the same window over a fully pre-filled buffer.
    let prefilled = filled_sparse_buffer(&wav);
    let expected = tokio::task::spawn_blocking({
        let cancel = never();
        move || {
            let mut sink = DigestSink::default();
            decode_audio_to_sink(prefilled, None, None, &mut sink, cancel).expect("prefilled");
            sink
        }
    })
    .await
    .expect("prefilled decode task");

    // Live fill: the decoder starts against an empty buffer and blocks on the
    // LocalReader's windows as they land.
    let mut temp = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut temp, &wav).unwrap();
    std::io::Write::flush(&mut temp).unwrap();
    let buffer = create_sparse_buffer(wav.len() as u64);
    Box::new(LocalReader::new(temp.path(), bae_diagnostics::Diagnostics::noop()))
        .start_reading(buffer.clone(), Box::new(|_| {}));

    let streamed = tokio::task::spawn_blocking({
        let buffer = buffer.clone();
        let cancel = never();
        move || {
            let mut sink = DigestSink::default();
            decode_audio_to_sink(buffer, None, None, &mut sink, cancel).expect("streamed");
            sink
        }
    })
    .await
    .expect("streamed decode task");

    assert_eq!(streamed.format, expected.format);
    assert_eq!(
        streamed.count, expected.count,
        "live-fill decode must produce the same sample count"
    );
    assert_eq!(
        streamed.digest, expected.digest,
        "live-fill decode must be byte-identical to the pre-filled decode"
    );

    // The residency bound: one 4 MiB read-ahead window, the 4 MiB
    // keep-behind margin and the file's 4 MiB first window, with slack for
    // in-flight windows — far below the whole file. Without eviction (or with a whole-file ceiling) the buffer
    // would hold all ~17 MiB.
    let bound = 13 * 1024 * 1024;
    let buffered = buffer.total_buffered();
    assert!(
        buffered <= bound,
        "buffer must stay windowed: held {buffered} bytes, bound {bound}, whole file {}",
        wav.len()
    );
}

/// A decode over a source whose first read fails reports that read's own
/// error, not the "Invalid data found" FFmpeg derives from an empty probe.
#[tokio::test]
async fn i32_decode_names_the_read_error_when_the_first_read_fails() {
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
    Box::new(LocalReader::new(dir.path(), bae_diagnostics::Diagnostics::noop()))
        .start_reading(buffer.clone(), Box::new(|_| {}));

    let result = tokio::task::spawn_blocking(move || decode_audio(buffer, None, None).map(|_| ()))
        .await
        .expect("decode task");

    let (kind, text) = source_read_failure(result.clone())
        .unwrap_or_else(|| panic!("an unreadable source is a read failure: {result:?}"));
    assert_eq!(kind, expected, "{text}");
    assert!(!text.contains("Invalid data found"), "{text}");
}

#[tokio::test]
async fn test_local_file_reader_full_file() {
    // Create temp file with test data
    let mut temp_file = NamedTempFile::new().unwrap();
    let test_data = b"Hello, this is test audio data for streaming!";
    temp_file.write_all(test_data).unwrap();
    temp_file.flush().unwrap();

    let reader = Box::new(LocalReader::new(temp_file.path(), bae_diagnostics::Diagnostics::noop()));
    let buffer = create_sparse_buffer(test_data.len() as u64);

    reader.start_reading(buffer.clone(), Box::new(|_| {}));
    let result = drain_async(buffer.clone()).await;

    assert_eq!(result, test_data);
}

/// A deep seek into a large local file is served through the shared
/// demand-driven fill loop: a reader parked at `6*WINDOW` before the loop
/// starts reads the exact file bytes at that offset (not the prefix). The
/// byte pattern is position-dependent, so wrong-offset fill would mismatch.
#[tokio::test]
async fn local_seek_into_large_file_serves_target_via_shared_fill() {
    let source_size = 8 * WINDOW;
    let data: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();
    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(&data).unwrap();
    temp_file.flush().unwrap();

    let reader = Box::new(LocalReader::new(temp_file.path(), bae_diagnostics::Diagnostics::noop()));
    let buffer = create_sparse_buffer(source_size);

    // Register the seek demand before the fill loop starts (the seek case).
    let mut seek_reader = buffer.new_reader();
    assert!(seek_reader.seek(6 * WINDOW));

    reader.start_reading(buffer.clone(), Box::new(|_| {}));

    let chunk_len = 4096usize;
    let read = tokio::task::spawn_blocking(move || {
        let mut out = vec![0u8; chunk_len];
        let n = seek_reader.read(&mut out).ok();
        drop(seek_reader);
        (n, out)
    });
    let (n, out) = timeout(HANG_GUARD, read)
        .await
        .expect("a deep local seek must be served, not hang")
        .expect("seek read task");

    let n = n.expect("seek read returned data");
    let target = 6 * WINDOW as usize;
    assert_eq!(
        &out[..n],
        &data[target..target + n],
        "the bytes at the seek target must match the file at that offset"
    );
}

#[tokio::test]
async fn test_local_file_reader_nonexistent_file() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let test_data = b"deleted before read";
    temp_file.write_all(test_data).unwrap();
    temp_file.flush().unwrap();
    let path = temp_file.path().to_path_buf();
    drop(temp_file);

    let reader = Box::new(LocalReader::new(path, bae_diagnostics::Diagnostics::noop()));
    let buffer = create_sparse_buffer(test_data.len() as u64);

    let (on_error, mut error_rx) = capturing_error_handler();
    reader.start_reading(buffer.clone(), on_error);
    let result = drain_async(buffer.clone()).await;

    // The buffer fails with the open's own error, kind intact.
    assert_eq!(
        failure_kind(&buffer),
        Some(std::io::ErrorKind::NotFound),
        "the buffer carries the open's NotFound"
    );
    assert!(result.is_empty());
    let error =
        next_fill_error(&mut error_rx, "local open failure should report a fill error").await;
    assert!(error.contains("Failed to open"), "expected open failure, got: {error}");
}

#[tokio::test]
async fn test_local_file_reader_read_error_reports_error() {
    let dir = tempfile::tempdir().unwrap();

    let reader = Box::new(LocalReader::new(dir.path(), bae_diagnostics::Diagnostics::noop()));
    let buffer = create_sparse_buffer(1);

    let (on_error, mut error_rx) = capturing_error_handler();
    reader.start_reading(buffer.clone(), on_error);
    let result = drain_async(buffer.clone()).await;

    assert!(buffer.failure().is_some(), "the buffer carries the read failure");
    assert!(result.is_empty());
    let error =
        next_fill_error(&mut error_rx, "local read failure should report a fill error").await;
    // The source is a directory, which the OS refuses to serve as a file: a
    // POSIX open succeeds and the first read fails, while Windows refuses at
    // open. Either way the reader must report a fill error and cancel — the
    // phase the refusal lands in is the OS's, not the contract's.
    assert!(
        error.contains("Failed to read") || error.contains("Failed to open"),
        "expected a read or open failure, got: {error}"
    );
}

/// Cancelling a buffer closes its file even while the buffer itself is still
/// held: the fill, parked with nothing demanded, wakes to the stop and exits.
#[tokio::test]
async fn cancelling_a_local_buffer_closes_its_file_while_the_buffer_lives() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("track.flac");
    std::fs::write(&path, vec![7u8; 1024]).unwrap();
    let buffer = create_sparse_buffer(1024);
    Box::new(LocalReader::new(&path, bae_diagnostics::Diagnostics::noop()))
        .start_reading(buffer.clone(), Box::new(|_| {}));
    wait_for_open_files(dir.path(), 1).await;

    buffer.cancel();

    wait_for_open_files(dir.path(), 0).await;
    drop(buffer);
}

/// Wait until exactly `count` files under `dir` are open, failing after a
/// few seconds with the files still open.
async fn wait_for_open_files(dir: &std::path::Path, count: usize) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let open = coven::open_files_under(dir);
        if open.len() == count {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "expected {count} open files under {}, found {open:?}",
            dir.display()
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

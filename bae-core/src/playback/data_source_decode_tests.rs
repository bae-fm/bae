//! The codec decoding through a buffer that a `LocalReader` fills: what a
//! preview and an import read their files through.

use super::{AudioDataReader, LocalReader};
use bae_audio::sparse_buffer::{create_sparse_buffer, SharedSparseBuffer};
use bae_audio::{
    decode_audio, decode_audio_to_sink, encode_i32, init, DecodeError, DecodedSink, EncodeFormat,
};
use std::sync::Arc;

/// A sparse buffer pre-filled with the whole byte slice, so a decode exercises
/// the window logic without waiting on a fill.
fn buffer_from(bytes: &[u8]) -> SharedSparseBuffer {
    let buffer = create_sparse_buffer(bytes.len() as u64);
    buffer.append_at(0, bytes);
    buffer
}

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
                self.digest = self
                    .digest
                    .wrapping_mul(1099511628211)
                    .wrapping_add(sample as u32 as u64);
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
    let wav = encode_i32(
        EncodeFormat::PcmWav {
            bits_per_sample: 16,
        },
        &samples,
        sample_rate,
        2,
    )
    .unwrap();
    drop(samples);

    let never = || Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Ground truth: the same window over a fully pre-filled buffer.
    let prefilled = buffer_from(&wav);
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
    Box::new(LocalReader::new(temp.path())).start_reading(buffer.clone(), Box::new(|_| {}));

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

    // The residency bound: one 4 MiB read-ahead window plus the 4 MiB
    // keep-behind margin, with slack for in-flight windows — far below the
    // whole file. Without eviction (or with a whole-file ceiling) the buffer
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
    Box::new(LocalReader::new(dir.path())).start_reading(buffer.clone(), Box::new(|_| {}));

    let result = tokio::task::spawn_blocking(move || decode_audio(buffer, None, None).map(|_| ()))
        .await
        .expect("decode task");

    let (kind, text) = source_read_failure(result.clone())
        .unwrap_or_else(|| panic!("an unreadable source is a read failure: {result:?}"));
    assert_eq!(kind, expected, "{text}");
    assert!(!text.contains("Invalid data found"), "{text}");
}

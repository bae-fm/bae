//! Audio bytes to decode, and comparing captured playback samples against a
//! decoded reference.

/// A sparse buffer pre-filled with the whole byte slice, so a decode exercises
/// the window logic without waiting on a fill.
pub fn filled_sparse_buffer(bytes: &[u8]) -> crate::sparse_buffer::SharedSparseBuffer {
    let buffer = crate::sparse_buffer::create_sparse_buffer(bytes.len() as u64);
    buffer.append_at(0, bytes);
    buffer
}

/// Convert a decode's full-range i32 PCM samples to the f32 shape emitted by
/// streaming playback, for comparing a ground-truth decode against captured
/// playback output.
pub fn samples_as_f32(decoded: &crate::DecodedAudio) -> Vec<f32> {
    let scale = i32::MAX as f32;
    decoded.samples.iter().map(|&s| s as f32 / scale).collect()
}

/// Align captured playback samples against a decoded reference and assert they
/// match. The streaming decoder can begin a captured region slightly before the
/// reference (a seek lands on a codec frame boundary), so this searches for the
/// whole-sample offset in `[0, max_alignment]` that minimizes the per-sample max
/// difference against the reference's first 500 frames, asserts that best offset
/// is within `tolerance`, then asserts up to `compare_samples` samples from it
/// match within `tolerance` (capped so neither buffer is over-read). `label`
/// names the subject in panic messages. Returns the aligned offset in samples.
pub fn assert_captured_matches_reference(
    captured: &[f32],
    reference: &[f32],
    channels: usize,
    sample_rate: u32,
    max_alignment: usize,
    compare_samples: usize,
    tolerance: f32,
    label: &str,
) -> usize {
    let sample_ms = |sample: usize| sample as f64 / channels as f64 / sample_rate as f64 * 1000.0;
    let snippet_len = 500 * channels;
    let limit = max_alignment.min(captured.len().saturating_sub(snippet_len));

    let mut best_max_diff = f32::MAX;
    let mut best_offset = 0usize;
    for offset in 0..=limit {
        let mut max_diff = 0.0f32;
        for i in 0..snippet_len {
            let diff = (captured[offset + i] - reference[i]).abs();
            max_diff = max_diff.max(diff);
            if max_diff > best_max_diff {
                break;
            }
        }
        if max_diff < best_max_diff {
            best_max_diff = max_diff;
            best_offset = offset;
        }
    }

    assert!(
        best_max_diff < tolerance,
        "could not align {label} with the reference within tolerance {tolerance:.6}: \
         best offset {:.1}ms, max sample diff {best_max_diff:.6}",
        sample_ms(best_offset),
    );

    let compare_count = compare_samples.min(captured.len() - best_offset).min(reference.len());
    for i in 0..compare_count {
        let diff = (captured[best_offset + i] - reference[i]).abs();
        assert!(
            diff < tolerance,
            "{label} audio mismatch at index {i} ({:.1}ms): captured={:.6}, reference={:.6}",
            sample_ms(i),
            captured[best_offset + i],
            reference[i],
        );
    }
    best_offset
}

/// A WAV file of `data` behind a plain 16-byte `fmt ` chunk: `format_tag` 1 for
/// integer PCM, 3 for float.
pub fn wav_with_fmt(
    format_tag: u16,
    bits_per_sample: u16,
    channels: u16,
    sample_rate: u32,
    data: &[u8],
) -> Vec<u8> {
    let block_align = channels * bits_per_sample / 8;
    let byte_rate = sample_rate * block_align as u32;
    let riff_size = 36 + data.len() as u32;
    let mut wav = Vec::with_capacity(44 + data.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&format_tag.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(data);
    wav
}

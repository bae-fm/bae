use super::probe::content_type_from_codec_id;
use super::*;
use crate::util::content_type::ContentType;
use std::sync::Arc;

/// A sparse buffer pre-filled with the whole byte slice, so a decode exercises
/// the window logic without waiting on a fill.
fn buffer_from(bytes: &[u8]) -> crate::playback::SharedSparseBuffer {
    let buffer = crate::playback::sparse_buffer::create_sparse_buffer(bytes.len() as u64);
    buffer.append_at(0, bytes);
    buffer
}

fn wav_with_fmt(
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

fn wav_extensible_pcm(
    bits_per_sample: u16,
    channels: u16,
    sample_rate: u32,
    data: &[u8],
) -> Vec<u8> {
    let block_align = channels * bits_per_sample / 8;
    let byte_rate = sample_rate * block_align as u32;
    let fmt_size = 40u32;
    let riff_size = 4 + 8 + fmt_size + 8 + data.len() as u32;
    let mut wav = Vec::with_capacity((riff_size + 8) as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&fmt_size.to_le_bytes());
    wav.extend_from_slice(&0xFFFEu16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    wav.extend_from_slice(&22u16.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    wav.extend_from_slice(&0u32.to_le_bytes());
    wav.extend_from_slice(&[
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b,
        0x71,
    ]);
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(data);
    wav
}

fn truncated_flac_packet_stream() -> Vec<u8> {
    let original_samples: Vec<i32> = (0..44100)
        .map(|i| ((i as f64 * 0.01).sin() * 0.5 * i32::MAX as f64) as i32)
        .collect();
    let mut flac_data = encode_i32(
        EncodeFormat::Flac {
            bits_per_sample: 16,
        },
        &original_samples,
        44100,
        1,
    )
    .unwrap();
    flac_data.truncate(flac_data.len() / 2);
    flac_data
}

fn mp3_with_invalid_terminal_packet() -> (Vec<u8>, usize) {
    let sample_rate = 44_100u32;
    let samples: Vec<i32> = (0..sample_rate as usize * 2)
        .map(|i| {
            let t = (i / 2) as f64 / sample_rate as f64;
            (0.5 * i32::MAX as f64 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i32
        })
        .collect();
    let clean = encode_i32(
        EncodeFormat::Mp3 { bitrate_kbps: 128 },
        &samples,
        sample_rate,
        2,
    )
    .unwrap();
    let expected_samples = decode_audio(buffer_from(&clean), None, None)
        .expect("clean decode")
        .samples
        .len();
    let frame_start = clean
        .windows(2)
        .position(|bytes| bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0)
        .expect("encoded MP3 frame");
    let mut damaged = clean.clone();
    damaged.extend_from_slice(&clean[frame_start..frame_start + 16]);
    damaged.extend_from_slice(b"LYRICSBEGININD0000000LYRICS200TAG");
    (damaged, expected_samples)
}

include!("tests/decode_and_encode.rs");
include!("tests/seek_and_stream.rs");

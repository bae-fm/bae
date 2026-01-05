//! Unified audio codec module using FFmpeg.
//!
//! Provides decoding (any format to PCM), encoding (PCM to FLAC), and
//! seektable generation. Uses temp files for in-memory operations since
//! FFmpeg's high-level API is file-oriented.
//!
//! Also provides streaming decode via AVIO custom I/O for real-time playback.

use crate::playback::{SharedStreamingBuffer, StreamingPcmSink};
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;
use tracing::{debug, info, warn};

/// Decoded audio metadata and samples
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    pub samples: Vec<i32>,
    pub sample_rate: u32,
    pub channels: u32,
    pub bits_per_sample: u32,
}

/// A seek point entry mapping sample number to byte offset
#[derive(Debug, Clone, Copy)]
pub struct SeekEntry {
    pub sample_number: u64,
    pub byte_offset: u64,
}

/// Initialize FFmpeg (call once at startup)
pub fn init() {
    ffmpeg_next::init().expect("Failed to initialize FFmpeg");
}

/// Decode any audio format to PCM samples.
///
/// Supports FLAC, MP3, APE, AAC/M4A, OGG Vorbis, WAV, AIFF, and more.
/// If start_ms/end_ms are provided, only that time range is decoded.
/// Returns interleaved i32 samples.
pub fn decode_audio(
    data: &[u8],
    start_ms: Option<u64>,
    end_ms: Option<u64>,
) -> Result<DecodedAudio, String> {
    // Write data to temp file
    let mut temp_file =
        NamedTempFile::new().map_err(|e| format!("Failed to create temp file: {}", e))?;
    temp_file
        .write_all(data)
        .map_err(|e| format!("Failed to write temp file: {}", e))?;
    let temp_path = temp_file.path();

    decode_audio_file(temp_path, start_ms, end_ms)
}

/// Decode an audio file to PCM samples
fn decode_audio_file(
    path: &Path,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
) -> Result<DecodedAudio, String> {
    use ffmpeg_next::media::Type;

    let path_str = path
        .to_str()
        .ok_or_else(|| "Invalid path encoding".to_string())?;

    let mut ictx =
        ffmpeg_next::format::input(&path_str).map_err(|e| format!("Failed to open audio file: {}", e))?;

    // Find best audio stream
    let input_stream = ictx
        .streams()
        .best(Type::Audio)
        .ok_or("No audio stream found")?;

    let stream_index = input_stream.index();

    // Get stream parameters
    let codec_params = input_stream.parameters();
    let decoder_context = ffmpeg_next::codec::context::Context::from_parameters(codec_params)
        .map_err(|e| format!("Failed to create decoder context: {}", e))?;

    let mut decoder = decoder_context
        .decoder()
        .audio()
        .map_err(|e| format!("Failed to create audio decoder: {}", e))?;

    let sample_rate = decoder.rate();
    let channels = decoder.channels() as u32;

    // Determine bits per sample from format
    let (bits_per_sample, is_float) = match decoder.format() {
        ffmpeg_next::format::Sample::I16(_) => (16, false),
        ffmpeg_next::format::Sample::I32(_) => (32, false),
        ffmpeg_next::format::Sample::I64(_) => (64, false),
        ffmpeg_next::format::Sample::F32(_) => (32, true),
        ffmpeg_next::format::Sample::F64(_) => (64, true),
        ffmpeg_next::format::Sample::U8(_) => (8, false),
        ffmpeg_next::format::Sample::None => (16, false), // Default fallback
    };

    // Calculate sample boundaries
    let start_sample = start_ms.map(|ms| (ms * sample_rate as u64) / 1000);
    let end_sample = end_ms.map(|ms| (ms * sample_rate as u64) / 1000);

    // Seek to start position if specified
    if let Some(start_ms) = start_ms {
        let timestamp = (start_ms as i64) * 1000; // microseconds
        if ictx.seek(timestamp, ..).is_err() {
            debug!("Seek failed, will decode from beginning");
        }
    }

    let mut samples: Vec<i32> = Vec::new();
    let mut current_sample: u64 = 0;
    let mut collecting = start_sample.is_none();

    // Process packets
    for (stream, packet) in ictx.packets() {
        if stream.index() != stream_index {
            continue;
        }

        decoder
            .send_packet(&packet)
            .map_err(|e| format!("Failed to send packet: {}", e))?;

        let mut decoded = ffmpeg_next::util::frame::audio::Audio::empty();
        while decoder.receive_frame(&mut decoded).is_ok() {
            let frame_samples = decoded.samples() as u64;
            let frame_start = current_sample;
            let frame_end = current_sample + frame_samples;

            // Check if we should start collecting
            if let Some(start) = start_sample {
                if frame_end > start {
                    collecting = true;
                }
            }

            // Check if we should stop
            if let Some(end) = end_sample {
                if frame_start >= end {
                    break;
                }
            }

            if collecting {
                // Extract samples based on format
                let frame_samples_vec = extract_samples_from_frame(&decoded, channels as usize, is_float);

                // Calculate which samples to take based on range
                let skip_start = if let Some(start) = start_sample {
                    if frame_start < start {
                        ((start - frame_start) as usize) * channels as usize
                    } else {
                        0
                    }
                } else {
                    0
                };

                let take_end = if let Some(end) = end_sample {
                    if frame_end > end {
                        ((end - frame_start) as usize) * channels as usize
                    } else {
                        frame_samples_vec.len()
                    }
                } else {
                    frame_samples_vec.len()
                };

                if skip_start < take_end && take_end <= frame_samples_vec.len() {
                    samples.extend_from_slice(&frame_samples_vec[skip_start..take_end]);
                }
            }

            current_sample = frame_end;
        }

        // Check if we've passed the end
        if let Some(end) = end_sample {
            if current_sample >= end {
                break;
            }
        }
    }

    // Flush decoder
    decoder
        .send_eof()
        .map_err(|e| format!("Failed to send EOF: {}", e))?;

    let mut decoded = ffmpeg_next::util::frame::audio::Audio::empty();
    while decoder.receive_frame(&mut decoded).is_ok() {
        if collecting {
            let frame_samples_vec = extract_samples_from_frame(&decoded, channels as usize, is_float);
            samples.extend_from_slice(&frame_samples_vec);
        }
    }

    debug!(
        "Decoded {} samples ({} frames) from audio file",
        samples.len(),
        samples.len() / channels.max(1) as usize
    );

    Ok(DecodedAudio {
        samples,
        sample_rate,
        channels,
        bits_per_sample,
    })
}

/// Extract samples from a decoded frame as i32
fn extract_samples_from_frame(
    frame: &ffmpeg_next::util::frame::audio::Audio,
    channels: usize,
    is_float: bool,
) -> Vec<i32> {
    let num_samples = frame.samples();
    let mut samples = Vec::with_capacity(num_samples * channels);

    // Get the data from the frame
    // For packed format, all samples are in plane 0
    // For planar format, each channel is in a separate plane
    let is_planar = frame.is_planar();

    if is_planar {
        // Interleave from separate channel planes
        for i in 0..num_samples {
            for ch in 0..channels {
                let plane = frame.data(ch);
                if is_float {
                    // F32 planar
                    let offset = i * 4;
                    if offset + 4 <= plane.len() {
                        let f = f32::from_ne_bytes([
                            plane[offset],
                            plane[offset + 1],
                            plane[offset + 2],
                            plane[offset + 3],
                        ]);
                        samples.push((f * i32::MAX as f32) as i32);
                    }
                } else {
                    // Integer planar (typically S16P or S32P)
                    let bytes_per_sample = frame.format().bytes();
                    let offset = i * bytes_per_sample;
                    if offset + bytes_per_sample <= plane.len() {
                        let sample = match bytes_per_sample {
                            2 => {
                                i16::from_ne_bytes([plane[offset], plane[offset + 1]]) as i32
                            }
                            4 => i32::from_ne_bytes([
                                plane[offset],
                                plane[offset + 1],
                                plane[offset + 2],
                                plane[offset + 3],
                            ]),
                            _ => 0,
                        };
                        samples.push(sample);
                    }
                }
            }
        }
    } else {
        // Packed format - all samples interleaved in plane 0
        let data = frame.data(0);
        let bytes_per_sample = frame.format().bytes();

        for i in 0..(num_samples * channels) {
            let offset = i * bytes_per_sample;
            if offset + bytes_per_sample <= data.len() {
                let sample = if is_float {
                    let f = f32::from_ne_bytes([
                        data[offset],
                        data[offset + 1],
                        data[offset + 2],
                        data[offset + 3],
                    ]);
                    (f * i32::MAX as f32) as i32
                } else {
                    match bytes_per_sample {
                        1 => (data[offset] as i8) as i32 * 256,
                        2 => i16::from_ne_bytes([data[offset], data[offset + 1]]) as i32,
                        4 => i32::from_ne_bytes([
                            data[offset],
                            data[offset + 1],
                            data[offset + 2],
                            data[offset + 3],
                        ]),
                        _ => 0,
                    }
                };
                samples.push(sample);
            }
        }
    }

    samples
}

/// Encode PCM samples to FLAC format.
///
/// Takes interleaved i32 samples and returns the encoded FLAC data as bytes.
/// Uses the ffmpeg CLI for reliable encoding.
pub fn encode_to_flac(
    samples: &[i32],
    sample_rate: u32,
    channels: u32,
    bits_per_sample: u32,
) -> Result<Vec<u8>, String> {
    use std::process::{Command, Stdio};

    // Create temp files with unique names
    let temp_dir = std::env::temp_dir();
    let unique_id = format!(
        "{}_{:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let input_path = temp_dir.join(format!("bae_pcm_{}.raw", unique_id));
    let output_path = temp_dir.join(format!("bae_flac_{}.flac", unique_id));

    // Determine sample format string for ffmpeg
    let sample_fmt = match bits_per_sample {
        8 => "u8",
        16 => "s16le",
        24 => "s24le",
        32 => "s32le",
        _ => "s16le",
    };

    // Convert samples to raw PCM bytes
    let pcm_bytes: Vec<u8> = match bits_per_sample {
        8 => samples
            .iter()
            .map(|&s| ((s >> 24) as i8 as u8).wrapping_add(128))
            .collect(),
        16 => samples
            .iter()
            .flat_map(|&s| (s as i16).to_le_bytes())
            .collect(),
        24 => samples
            .iter()
            .flat_map(|&s| {
                let bytes = s.to_le_bytes();
                [bytes[1], bytes[2], bytes[3]] // Take upper 24 bits
            })
            .collect(),
        32 => samples.iter().flat_map(|&s| s.to_le_bytes()).collect(),
        _ => samples
            .iter()
            .flat_map(|&s| (s as i16).to_le_bytes())
            .collect(),
    };

    // Write raw PCM to temp file
    std::fs::write(&input_path, &pcm_bytes)
        .map_err(|e| format!("Failed to write PCM temp file: {}", e))?;

    // Run ffmpeg to encode
    let output = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            sample_fmt,
            "-ar",
            &sample_rate.to_string(),
            "-ac",
            &channels.to_string(),
            "-i",
            input_path.to_str().unwrap(),
            "-compression_level",
            "5",
            output_path.to_str().unwrap(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

    // Clean up input file
    let _ = std::fs::remove_file(&input_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = std::fs::remove_file(&output_path);
        return Err(format!("ffmpeg encoding failed: {}", stderr));
    }

    // Read the encoded file
    let flac_data =
        std::fs::read(&output_path).map_err(|e| format!("Failed to read encoded file: {}", e))?;

    // Clean up output file
    let _ = std::fs::remove_file(&output_path);

    debug!("Encoded {} bytes of FLAC data", flac_data.len());

    Ok(flac_data)
}

/// Build a frame-accurate seektable by scanning FLAC frames.
///
/// This scans the FLAC byte stream for frame sync codes (0xFF 0xF8/0xF9),
/// validates headers with CRC-8, and builds a map of sample_number -> byte_offset.
/// Returns byte offsets relative to the start of audio data (after headers).
///
/// For CUE/FLAC imports, we need to extract specific byte ranges from a FLAC file -
/// one range per track defined in the CUE sheet. This requires ~93ms precision
/// (one entry per FLAC frame at 44.1kHz).
pub fn build_seektable(flac_data: &[u8]) -> Result<Vec<SeekEntry>, String> {
    // Parse FLAC headers to get metadata
    if flac_data.len() < 4 || &flac_data[0..4] != b"fLaC" {
        return Err("Invalid FLAC signature".to_string());
    }

    let mut pos = 4;
    let mut sample_rate = 0u32;
    let mut total_samples = 0u64;
    let mut min_block_size = 0u32;
    let mut min_frame_size = 0u32;

    // Parse metadata blocks
    loop {
        if pos + 4 > flac_data.len() {
            return Err("Unexpected end of file in metadata".to_string());
        }

        let header_byte = flac_data[pos];
        let is_last = (header_byte & 0x80) != 0;
        let block_type = header_byte & 0x7F;
        let block_size = u32::from_be_bytes([
            0,
            flac_data[pos + 1],
            flac_data[pos + 2],
            flac_data[pos + 3],
        ]) as usize;

        if pos + 4 + block_size > flac_data.len() {
            return Err("Block extends beyond file".to_string());
        }

        if block_type == 0 && block_size >= 18 {
            // STREAMINFO block
            let block = &flac_data[pos + 4..pos + 4 + block_size];
            min_block_size = ((block[0] as u32) << 8) | (block[1] as u32);
            min_frame_size =
                ((block[4] as u32) << 16) | ((block[5] as u32) << 8) | (block[6] as u32);
            sample_rate =
                ((block[10] as u32) << 12) | ((block[11] as u32) << 4) | ((block[12] as u32) >> 4);
            total_samples = (((block[13] & 0x0F) as u64) << 32)
                | ((block[14] as u64) << 24)
                | ((block[15] as u64) << 16)
                | ((block[16] as u64) << 8)
                | (block[17] as u64);
        }

        pos += 4 + block_size;
        if is_last {
            break;
        }
    }

    let audio_data_start = pos;
    let audio_data_end = flac_data.len();

    if sample_rate == 0 || total_samples == 0 {
        return Err("Invalid FLAC: no samples or sample rate".to_string());
    }

    // Use minimum frame size from STREAMINFO, or a reasonable default
    let skip_size = if min_frame_size > 0 {
        min_frame_size as usize
    } else {
        1000 // Conservative default ~1KB
    };

    // Scan for frame sync codes
    let mut seektable = Vec::new();
    let mut scan_pos = audio_data_start;
    let mut last_sample_number: Option<u64> = None;

    while scan_pos + 16 < audio_data_end {
        // FLAC frame sync: 14 bits of 1s followed by 0 = 0xFF 0xF8 or 0xFF 0xF9
        if flac_data[scan_pos] == 0xFF && (flac_data[scan_pos + 1] & 0xFE) == 0xF8 {
            // Validate frame header
            if validate_flac_frame_header(flac_data, scan_pos) {
                // Read actual sample number from frame header
                if let Some(sample_number) =
                    parse_flac_frame_sample_number(flac_data, scan_pos, min_block_size)
                {
                    // Only add if this is a new sample position (avoid duplicates)
                    // Also reject sample_number > total_samples
                    if sample_number <= total_samples
                        && (last_sample_number.is_none()
                            || sample_number > last_sample_number.unwrap())
                    {
                        let stream_offset = (scan_pos - audio_data_start) as u64;
                        seektable.push(SeekEntry {
                            sample_number,
                            byte_offset: stream_offset,
                        });
                        last_sample_number = Some(sample_number);

                        // Skip ahead by minimum frame size
                        scan_pos += skip_size;
                        continue;
                    }
                }
            }
        }
        scan_pos += 1;
    }

    // Add final entry
    seektable.push(SeekEntry {
        sample_number: total_samples,
        byte_offset: (audio_data_end - audio_data_start) as u64,
    });

    let precision_ms = if seektable.len() > 1 && sample_rate > 0 {
        (total_samples as f64 / seektable.len() as f64) / sample_rate as f64 * 1000.0
    } else {
        0.0
    };

    info!(
        "Scanned FLAC: {} frames, {:.1}ms precision",
        seektable.len(),
        precision_ms
    );

    Ok(seektable)
}

/// Parse the sample number from a FLAC frame header.
fn parse_flac_frame_sample_number(data: &[u8], pos: usize, min_block_size: u32) -> Option<u64> {
    if pos + 5 >= data.len() {
        return None;
    }

    // Byte 1 bit 0: blocking strategy (0 = fixed, 1 = variable)
    let variable_block_size = (data[pos + 1] & 0x01) != 0;

    // The frame/sample number starts at byte 4
    let num_start = pos + 4;

    // Decode UTF-8-like variable length number
    let first_byte = data[num_start];
    let (value, _bytes_used) = if first_byte & 0x80 == 0 {
        (first_byte as u64, 1)
    } else if first_byte & 0xE0 == 0xC0 {
        if num_start + 1 >= data.len() {
            return None;
        }
        let val = ((first_byte as u64 & 0x1F) << 6) | (data[num_start + 1] as u64 & 0x3F);
        (val, 2)
    } else if first_byte & 0xF0 == 0xE0 {
        if num_start + 2 >= data.len() {
            return None;
        }
        let val = ((first_byte as u64 & 0x0F) << 12)
            | ((data[num_start + 1] as u64 & 0x3F) << 6)
            | (data[num_start + 2] as u64 & 0x3F);
        (val, 3)
    } else if first_byte & 0xF8 == 0xF0 {
        if num_start + 3 >= data.len() {
            return None;
        }
        let val = ((first_byte as u64 & 0x07) << 18)
            | ((data[num_start + 1] as u64 & 0x3F) << 12)
            | ((data[num_start + 2] as u64 & 0x3F) << 6)
            | (data[num_start + 3] as u64 & 0x3F);
        (val, 4)
    } else if first_byte & 0xFC == 0xF8 {
        if num_start + 4 >= data.len() {
            return None;
        }
        let val = ((first_byte as u64 & 0x03) << 24)
            | ((data[num_start + 1] as u64 & 0x3F) << 18)
            | ((data[num_start + 2] as u64 & 0x3F) << 12)
            | ((data[num_start + 3] as u64 & 0x3F) << 6)
            | (data[num_start + 4] as u64 & 0x3F);
        (val, 5)
    } else if first_byte & 0xFE == 0xFC {
        if num_start + 5 >= data.len() {
            return None;
        }
        let val = ((first_byte as u64 & 0x01) << 30)
            | ((data[num_start + 1] as u64 & 0x3F) << 24)
            | ((data[num_start + 2] as u64 & 0x3F) << 18)
            | ((data[num_start + 3] as u64 & 0x3F) << 12)
            | ((data[num_start + 4] as u64 & 0x3F) << 6)
            | (data[num_start + 5] as u64 & 0x3F);
        (val, 6)
    } else if first_byte == 0xFE {
        if num_start + 6 >= data.len() {
            return None;
        }
        let val = ((data[num_start + 1] as u64 & 0x3F) << 30)
            | ((data[num_start + 2] as u64 & 0x3F) << 24)
            | ((data[num_start + 3] as u64 & 0x3F) << 18)
            | ((data[num_start + 4] as u64 & 0x3F) << 12)
            | ((data[num_start + 5] as u64 & 0x3F) << 6)
            | (data[num_start + 6] as u64 & 0x3F);
        (val, 7)
    } else {
        return None;
    };

    if variable_block_size {
        Some(value)
    } else {
        // Value is frame number, multiply by block size
        let block_size = if min_block_size > 0 {
            min_block_size as u64
        } else {
            4096
        };
        Some(value * block_size)
    }
}

/// Validate a FLAC frame header with CRC-8.
fn validate_flac_frame_header(data: &[u8], pos: usize) -> bool {
    if pos + 4 >= data.len() {
        return false;
    }

    // Check sync code
    if data[pos] != 0xFF || (data[pos + 1] & 0xFE) != 0xF8 {
        return false;
    }

    // Byte 2: block size code (high nibble) and sample rate code (low nibble)
    let block_size_code = (data[pos + 2] >> 4) & 0x0F;
    let sample_rate_code = data[pos + 2] & 0x0F;

    if block_size_code == 0 || sample_rate_code == 15 {
        return false;
    }

    // Byte 3: channel assignment and sample size
    let channel_assignment = (data[pos + 3] >> 4) & 0x0F;
    let sample_size_code = (data[pos + 3] >> 1) & 0x07;

    if channel_assignment > 10 || sample_size_code == 3 || sample_size_code == 7 {
        return false;
    }

    // Reserved bit must be 0
    if data[pos + 3] & 0x01 != 0 {
        return false;
    }

    // Calculate header length
    let header_len = match calculate_flac_frame_header_length(data, pos, block_size_code, sample_rate_code) {
        Some(len) => len,
        None => return false,
    };

    if pos + header_len >= data.len() {
        return false;
    }

    // Verify CRC-8
    let crc_pos = pos + header_len - 1;
    let expected_crc = data[crc_pos];
    let computed_crc = compute_flac_crc8(&data[pos..crc_pos]);

    computed_crc == expected_crc
}

/// Calculate FLAC frame header length.
fn calculate_flac_frame_header_length(
    data: &[u8],
    pos: usize,
    block_size_code: u8,
    sample_rate_code: u8,
) -> Option<usize> {
    let mut len = 4; // Fixed part

    let num_start = pos + 4;
    if num_start >= data.len() {
        return None;
    }

    let first_byte = data[num_start];
    let utf8_len = if first_byte & 0x80 == 0 {
        1
    } else if first_byte & 0xE0 == 0xC0 {
        2
    } else if first_byte & 0xF0 == 0xE0 {
        3
    } else if first_byte & 0xF8 == 0xF0 {
        4
    } else if first_byte & 0xFC == 0xF8 {
        5
    } else if first_byte & 0xFE == 0xFC {
        6
    } else if first_byte == 0xFE {
        7
    } else {
        return None;
    };

    len += utf8_len;

    // Optional block size
    if block_size_code == 6 {
        len += 1;
    } else if block_size_code == 7 {
        len += 2;
    }

    // Optional sample rate
    if sample_rate_code == 12 {
        len += 1;
    } else if sample_rate_code == 13 || sample_rate_code == 14 {
        len += 2;
    }

    // CRC-8
    len += 1;

    Some(len)
}

/// Compute CRC-8 using FLAC's polynomial (0x07).
fn compute_flac_crc8(data: &[u8]) -> u8 {
    const CRC8_TABLE: [u8; 256] = [
        0x00, 0x07, 0x0E, 0x09, 0x1C, 0x1B, 0x12, 0x15, 0x38, 0x3F, 0x36, 0x31, 0x24, 0x23, 0x2A,
        0x2D, 0x70, 0x77, 0x7E, 0x79, 0x6C, 0x6B, 0x62, 0x65, 0x48, 0x4F, 0x46, 0x41, 0x54, 0x53,
        0x5A, 0x5D, 0xE0, 0xE7, 0xEE, 0xE9, 0xFC, 0xFB, 0xF2, 0xF5, 0xD8, 0xDF, 0xD6, 0xD1, 0xC4,
        0xC3, 0xCA, 0xCD, 0x90, 0x97, 0x9E, 0x99, 0x8C, 0x8B, 0x82, 0x85, 0xA8, 0xAF, 0xA6, 0xA1,
        0xB4, 0xB3, 0xBA, 0xBD, 0xC7, 0xC0, 0xC9, 0xCE, 0xDB, 0xDC, 0xD5, 0xD2, 0xFF, 0xF8, 0xF1,
        0xF6, 0xE3, 0xE4, 0xED, 0xEA, 0xB7, 0xB0, 0xB9, 0xBE, 0xAB, 0xAC, 0xA5, 0xA2, 0x8F, 0x88,
        0x81, 0x86, 0x93, 0x94, 0x9D, 0x9A, 0x27, 0x20, 0x29, 0x2E, 0x3B, 0x3C, 0x35, 0x32, 0x1F,
        0x18, 0x11, 0x16, 0x03, 0x04, 0x0D, 0x0A, 0x57, 0x50, 0x59, 0x5E, 0x4B, 0x4C, 0x45, 0x42,
        0x6F, 0x68, 0x61, 0x66, 0x73, 0x74, 0x7D, 0x7A, 0x89, 0x8E, 0x87, 0x80, 0x95, 0x92, 0x9B,
        0x9C, 0xB1, 0xB6, 0xBF, 0xB8, 0xAD, 0xAA, 0xA3, 0xA4, 0xF9, 0xFE, 0xF7, 0xF0, 0xE5, 0xE2,
        0xEB, 0xEC, 0xC1, 0xC6, 0xCF, 0xC8, 0xDD, 0xDA, 0xD3, 0xD4, 0x69, 0x6E, 0x67, 0x60, 0x75,
        0x72, 0x7B, 0x7C, 0x51, 0x56, 0x5F, 0x58, 0x4D, 0x4A, 0x43, 0x44, 0x19, 0x1E, 0x17, 0x10,
        0x05, 0x02, 0x0B, 0x0C, 0x21, 0x26, 0x2F, 0x28, 0x3D, 0x3A, 0x33, 0x34, 0x4E, 0x49, 0x40,
        0x47, 0x52, 0x55, 0x5C, 0x5B, 0x76, 0x71, 0x78, 0x7F, 0x6A, 0x6D, 0x64, 0x63, 0x3E, 0x39,
        0x30, 0x37, 0x22, 0x25, 0x2C, 0x2B, 0x06, 0x01, 0x08, 0x0F, 0x1A, 0x1D, 0x14, 0x13, 0xAE,
        0xA9, 0xA0, 0xA7, 0xB2, 0xB5, 0xBC, 0xBB, 0x96, 0x91, 0x98, 0x9F, 0x8A, 0x8D, 0x84, 0x83,
        0xDE, 0xD9, 0xD0, 0xD7, 0xC2, 0xC5, 0xCC, 0xCB, 0xE6, 0xE1, 0xE8, 0xEF, 0xFA, 0xFD, 0xF4,
        0xF3,
    ];

    let mut crc = 0u8;
    for &byte in data {
        crc = CRC8_TABLE[(crc ^ byte) as usize];
    }
    crc
}

/// Information about streaming audio being decoded
#[derive(Debug, Clone)]
pub struct StreamingAudioInfo {
    pub sample_rate: u32,
    pub channels: u32,
    pub bits_per_sample: u32,
}

/// Decode audio from a streaming buffer and push f32 samples to sink.
///
/// This function runs in a blocking thread, reading from the streaming buffer
/// as data becomes available, decoding with FFmpeg, and pushing f32 samples
/// to the ring buffer sink.
///
/// Returns audio info on success, or error message on failure.
pub fn decode_audio_streaming(
    buffer: SharedStreamingBuffer,
    sink: &mut StreamingPcmSink,
) -> Result<StreamingAudioInfo, String> {
    // Read all data from streaming buffer (blocking until EOF or cancelled)
    let mut audio_data = Vec::new();
    let mut read_buf = [0u8; 32768]; // 32KB chunks

    loop {
        if sink.is_cancelled() {
            return Err("Decode cancelled".to_string());
        }

        match buffer.read(&mut read_buf) {
            Some(0) => break, // EOF
            Some(n) => {
                audio_data.extend_from_slice(&read_buf[..n]);
            }
            None => return Err("Buffer cancelled".to_string()),
        }
    }

    if audio_data.is_empty() {
        return Err("No audio data received".to_string());
    }

    debug!("Streaming decoder received {} bytes", audio_data.len());

    // Decode the audio data
    let decoded = decode_audio(&audio_data, None, None)?;

    let info = StreamingAudioInfo {
        sample_rate: decoded.sample_rate,
        channels: decoded.channels,
        bits_per_sample: decoded.bits_per_sample,
    };

    // Convert i32 samples to f32 and push to sink
    let scale = 1.0 / (i32::MAX as f32);
    let f32_samples: Vec<f32> = decoded.samples.iter().map(|&s| s as f32 * scale).collect();

    debug!(
        "Pushing {} f32 samples to streaming sink",
        f32_samples.len()
    );

    // Push samples in chunks to allow cancellation checks
    const CHUNK_SIZE: usize = 8192;
    for chunk in f32_samples.chunks(CHUNK_SIZE) {
        if sink.is_cancelled() {
            warn!("Streaming decode cancelled during push");
            return Err("Cancelled".to_string());
        }
        sink.push_samples_blocking(chunk);
    }

    sink.mark_finished();

    info!(
        "Streaming decode complete: {} samples, {}Hz, {} channels",
        f32_samples.len(),
        info.sample_rate,
        info.channels
    );

    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_encode_roundtrip() {
        init();

        // Create test samples (1 second of silence at 44100Hz stereo)
        let original_samples: Vec<i32> = vec![0i32; 44100 * 2];

        // Encode to FLAC
        let flac_data = encode_to_flac(&original_samples, 44100, 2, 16).unwrap();

        // Verify FLAC signature
        assert!(flac_data.len() > 42);
        assert_eq!(&flac_data[0..4], b"fLaC");

        // Decode back
        let decoded = decode_audio(&flac_data, None, None).unwrap();

        assert_eq!(decoded.sample_rate, 44100);
        assert_eq!(decoded.channels, 2);
        // Sample counts should be approximately equal (may differ slightly due to padding)
        assert!(
            (decoded.samples.len() as i64 - original_samples.len() as i64).abs() < 1000,
            "Sample count mismatch: {} vs {}",
            decoded.samples.len(),
            original_samples.len()
        );
    }

    #[test]
    fn test_encode_mono() {
        init();

        let samples = vec![0i32; 44100];
        let result = encode_to_flac(&samples, 44100, 1, 16);
        assert!(result.is_ok(), "Failed to encode mono: {:?}", result.err());
    }

    #[test]
    fn test_build_seektable() {
        init();

        // Create and encode some test audio
        let samples: Vec<i32> = (0..44100 * 10)
            .map(|i| ((i as f64 * 0.1).sin() * 1000.0) as i32)
            .collect();

        let flac_data = encode_to_flac(&samples, 44100, 1, 16).unwrap();
        let seektable = build_seektable(&flac_data).unwrap();

        // Should have at least one entry
        assert!(!seektable.is_empty(), "Seektable should not be empty");

        // Sample numbers should be monotonically increasing
        for window in seektable.windows(2) {
            assert!(
                window[1].sample_number >= window[0].sample_number,
                "Sample numbers should be monotonically increasing"
            );
        }
    }

    #[test]
    fn test_streaming_decode() {
        use crate::playback::{create_streaming_buffer, create_streaming_pair_with_capacity};
        use std::thread;

        init();

        // Create test FLAC data
        let samples: Vec<i32> = (0..44100)
            .map(|i| ((i as f64 * 0.01).sin() * 10000.0) as i32)
            .collect();
        let flac_data = encode_to_flac(&samples, 44100, 1, 16).unwrap();

        // Create streaming infrastructure
        let buffer = create_streaming_buffer();
        let (mut sink, mut source) = create_streaming_pair_with_capacity(44100, 1, 100000);

        // Spawn decoder thread
        let decoder_buffer = buffer.clone();
        let decoder_handle = thread::spawn(move || decode_audio_streaming(decoder_buffer, &mut sink));

        // Feed data to buffer (simulating download)
        buffer.append(&flac_data);
        buffer.mark_eof();

        // Wait for decoder
        let result = decoder_handle.join().unwrap();
        assert!(result.is_ok(), "Decode failed: {:?}", result.err());

        let info = result.unwrap();
        assert_eq!(info.sample_rate, 44100);
        assert_eq!(info.channels, 1);

        // Pull samples from source
        let mut decoded_samples = Vec::new();
        let mut buf = [0.0f32; 1024];
        loop {
            let n = source.pull_samples(&mut buf);
            if n == 0 && source.is_finished() {
                break;
            }
            decoded_samples.extend_from_slice(&buf[..n]);
        }

        // Should have approximately the same number of samples
        assert!(
            (decoded_samples.len() as i64 - samples.len() as i64).abs() < 1000,
            "Sample count mismatch: {} vs {}",
            decoded_samples.len(),
            samples.len()
        );
    }
}

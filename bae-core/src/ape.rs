//! APE (Monkey's Audio) header parser.
//!
//! Parses APE file headers to extract audio metadata (sample rate, channels,
//! bit depth, and total sample count).
//! Supports versions 3800..=3990 (matching FFmpeg's ape.c).

use thiserror::Error;

// Format flag constants (from FFmpeg ape.c)
const MAC_FORMAT_FLAG_8_BIT: u16 = 1;
const MAC_FORMAT_FLAG_24_BIT: u16 = 8;

#[derive(Debug, Error)]
pub enum ApeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not an APE file: missing MAC magic")]
    BadMagic,
    #[error("Unsupported APE version {0} (supported: 3800..=3990)")]
    UnsupportedVersion(u16),
    #[error("Invalid header: {0}")]
    InvalidHeader(String),
    #[error("Data too short: need {needed} bytes, have {have}")]
    TooShort { needed: usize, have: usize },
}

/// Parsed APE file header information.
#[derive(Debug, Clone)]
pub struct ApeInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub total_samples: u64,
    /// Total file size (for computing last frame's byte range).
    pub file_size: u64,
}

/// Read a little-endian u16 from a byte slice at the given offset.
fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

/// Read a little-endian u32 from a byte slice at the given offset.
fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

/// Parse an APE header from an in-memory byte slice.
pub fn parse_ape_header_from_data(data: &[u8], file_size: u64) -> Result<ApeInfo, ApeError> {
    if data.len() < 6 {
        return Err(ApeError::TooShort {
            needed: 6,
            have: data.len(),
        });
    }

    // Check magic
    if &data[0..4] != b"MAC " {
        return Err(ApeError::BadMagic);
    }

    let version = read_u16_le(data, 4);
    if !(3800..=3990).contains(&version) {
        return Err(ApeError::UnsupportedVersion(version));
    }

    if version >= 3980 {
        parse_modern(data, file_size)
    } else {
        parse_legacy(data, file_size, version)
    }
}

/// Parse v3980+ (modern) APE format.
fn parse_modern(data: &[u8], file_size: u64) -> Result<ApeInfo, ApeError> {
    // Descriptor is at least 52 bytes
    if data.len() < 52 {
        return Err(ApeError::TooShort {
            needed: 52,
            have: data.len(),
        });
    }

    let descriptor_length = read_u32_le(data, 8) as usize;

    // Header starts after descriptor
    let header_offset = descriptor_length;
    if data.len() < header_offset + 24 {
        return Err(ApeError::TooShort {
            needed: header_offset + 24,
            have: data.len(),
        });
    }

    let blocks_per_frame = read_u32_le(data, header_offset + 4);
    let final_frame_blocks = read_u32_le(data, header_offset + 8);
    let total_frames = read_u32_le(data, header_offset + 12);
    let bits_per_sample = read_u16_le(data, header_offset + 16);
    let channels = read_u16_le(data, header_offset + 18);
    let sample_rate = read_u32_le(data, header_offset + 20);

    if total_frames == 0 {
        return Err(ApeError::InvalidHeader("totalframes is 0".to_string()));
    }
    if blocks_per_frame == 0 {
        return Err(ApeError::InvalidHeader("blocksperframe is 0".to_string()));
    }

    let total_samples = if total_frames > 1 {
        final_frame_blocks as u64 + blocks_per_frame as u64 * (total_frames as u64 - 1)
    } else {
        final_frame_blocks as u64
    };

    Ok(ApeInfo {
        sample_rate,
        channels,
        bits_per_sample,
        total_samples,
        file_size,
    })
}

/// Parse v3800..3979 (legacy) APE format.
fn parse_legacy(data: &[u8], file_size: u64, version: u16) -> Result<ApeInfo, ApeError> {
    if data.len() < 32 {
        return Err(ApeError::TooShort {
            needed: 32,
            have: data.len(),
        });
    }

    let compression_type = read_u16_le(data, 6);
    let format_flags = read_u16_le(data, 8);
    let channels = read_u16_le(data, 10);
    let sample_rate = read_u32_le(data, 12);
    let total_frames = read_u32_le(data, 24);
    let final_frame_blocks = read_u32_le(data, 28);

    if total_frames == 0 {
        return Err(ApeError::InvalidHeader("totalframes is 0".to_string()));
    }

    // BPS for legacy versions
    let bits_per_sample = if format_flags & MAC_FORMAT_FLAG_8_BIT != 0 {
        8
    } else if format_flags & MAC_FORMAT_FLAG_24_BIT != 0 {
        24
    } else {
        16
    };

    // Blocks per frame for legacy versions
    let blocks_per_frame: u64 = if version >= 3950 {
        73728 * 4
    } else if version >= 3900 || (version >= 3800 && compression_type >= 4000) {
        73728
    } else {
        9216
    };

    let total_samples = if total_frames > 1 {
        final_frame_blocks as u64 + blocks_per_frame * (total_frames as u64 - 1)
    } else {
        final_frame_blocks as u64
    };

    Ok(ApeInfo {
        sample_rate,
        channels,
        bits_per_sample,
        total_samples,
        file_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid v3990 APE file (modern format).
    fn build_test_ape_modern(
        sample_rate: u32,
        channels: u16,
        bps: u16,
        compression: u16,
        blocks_per_frame: u32,
        total_frames: u32,
        final_frame_blocks: u32,
        frame_offsets: &[u32],
    ) -> Vec<u8> {
        let seektable_length = total_frames as usize * 4;
        let descriptor_length = 52usize;
        let header_length = 24usize;
        let audio_data_start = descriptor_length + header_length + seektable_length;

        // We'll place some fake audio data after headers
        let fake_audio_size = 4096u32;
        let total_size = audio_data_start + fake_audio_size as usize;

        let mut buf = Vec::with_capacity(total_size);

        // Descriptor (52 bytes)
        buf.extend_from_slice(b"MAC ");
        buf.extend_from_slice(&3990u16.to_le_bytes()); // version
        buf.extend_from_slice(&0u16.to_le_bytes()); // padding
        buf.extend_from_slice(&(descriptor_length as u32).to_le_bytes());
        buf.extend_from_slice(&(header_length as u32).to_le_bytes());
        buf.extend_from_slice(&(seektable_length as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // wavheaderlength
        buf.extend_from_slice(&fake_audio_size.to_le_bytes()); // audiodatalength
        buf.extend_from_slice(&0u32.to_le_bytes()); // audiodatalength_high
        buf.extend_from_slice(&0u32.to_le_bytes()); // wavtaillength
        buf.extend_from_slice(&[0u8; 16]); // md5

        // Header (24 bytes)
        buf.extend_from_slice(&compression.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // formatflags
        buf.extend_from_slice(&blocks_per_frame.to_le_bytes());
        buf.extend_from_slice(&final_frame_blocks.to_le_bytes());
        buf.extend_from_slice(&total_frames.to_le_bytes());
        buf.extend_from_slice(&bps.to_le_bytes());
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());

        // Seek table
        for &offset in frame_offsets {
            buf.extend_from_slice(&offset.to_le_bytes());
        }

        // Pad to match expected size for default offsets
        while buf.len() < total_size {
            buf.push(0xAA);
        }

        buf
    }

    /// Build a minimal valid legacy (v3950) APE file.
    fn build_test_ape_legacy(
        version: u16,
        sample_rate: u32,
        channels: u16,
        compression: u16,
        format_flags: u16,
        total_frames: u32,
        final_frame_blocks: u32,
        frame_offsets: &[u32],
    ) -> Vec<u8> {
        let mut buf = Vec::new();

        // Header (32 bytes minimum)
        buf.extend_from_slice(b"MAC ");
        buf.extend_from_slice(&version.to_le_bytes());
        buf.extend_from_slice(&compression.to_le_bytes());
        buf.extend_from_slice(&format_flags.to_le_bytes());
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // wavheaderlength
        buf.extend_from_slice(&0u32.to_le_bytes()); // wavtaillength
        buf.extend_from_slice(&total_frames.to_le_bytes());
        buf.extend_from_slice(&final_frame_blocks.to_le_bytes());

        // No peak level, no HAS_SEEK_ELEMENTS flag, so seek table = totalframes entries
        for &offset in frame_offsets {
            buf.extend_from_slice(&offset.to_le_bytes());
        }

        // Some fake audio data
        buf.extend(std::iter::repeat_n(0xBB, 1024));

        buf
    }

    #[test]
    fn parse_modern_basic() {
        let offsets = vec![100, 200, 300];
        let data = build_test_ape_modern(44100, 2, 16, 2000, 73728, 3, 10000, &offsets);
        let file_size = data.len() as u64;

        let info = parse_ape_header_from_data(&data, file_size).unwrap();

        assert_eq!(info.sample_rate, 44100);
        assert_eq!(info.channels, 2);
        assert_eq!(info.bits_per_sample, 16);
        assert_eq!(info.total_samples, 10000 + 73728 * 2);
    }

    #[test]
    fn parse_modern_single_frame() {
        let offsets = vec![100];
        let data = build_test_ape_modern(48000, 2, 24, 2000, 73728, 1, 5000, &offsets);
        let file_size = data.len() as u64;

        let info = parse_ape_header_from_data(&data, file_size).unwrap();

        // Single frame (total_frames == 1): total_samples is just final_frame_blocks.
        assert_eq!(info.total_samples, 5000);
    }

    #[test]
    fn parse_legacy_v3950() {
        let offsets = vec![500, 600, 700, 800];
        let data = build_test_ape_legacy(3950, 44100, 2, 2000, 0, 4, 20000, &offsets);
        let file_size = data.len() as u64;

        let info = parse_ape_header_from_data(&data, file_size).unwrap();

        assert_eq!(info.sample_rate, 44100);
        assert_eq!(info.channels, 2);
        assert_eq!(info.bits_per_sample, 16); // no 8-bit or 24-bit flags
                                              // v3950+ blocks-per-frame is 294912, so total_samples folds it in.
        assert_eq!(info.total_samples, 20000 + 294912u64 * 3);
    }

    #[test]
    fn parse_legacy_v3900() {
        let offsets = vec![400, 500];
        let data = build_test_ape_legacy(3900, 44100, 2, 2000, 0, 2, 30000, &offsets);
        let file_size = data.len() as u64;

        let info = parse_ape_header_from_data(&data, file_size).unwrap();

        // v3900 with compression 2000 < 4000 → blocks-per-frame 73728.
        assert_eq!(info.total_samples, 30000 + 73728);
    }

    #[test]
    fn parse_legacy_v3800_high_compression() {
        let offsets = vec![400, 500];
        let data = build_test_ape_legacy(3800, 44100, 2, 4000, 0, 2, 30000, &offsets);
        let file_size = data.len() as u64;

        let info = parse_ape_header_from_data(&data, file_size).unwrap();

        // v3800 with compression >= 4000 → blocks-per-frame 73728.
        assert_eq!(info.total_samples, 30000 + 73728);
    }

    #[test]
    fn parse_legacy_v3800_low_compression() {
        let offsets = vec![400, 500];
        let data = build_test_ape_legacy(3800, 44100, 2, 2000, 0, 2, 30000, &offsets);
        let file_size = data.len() as u64;

        let info = parse_ape_header_from_data(&data, file_size).unwrap();

        // v3800 with low compression → blocks-per-frame 9216.
        assert_eq!(info.total_samples, 30000 + 9216);
    }

    #[test]
    fn parse_legacy_8bit_flag() {
        let offsets = vec![400];
        let data = build_test_ape_legacy(
            3900,
            44100,
            1,
            2000,
            MAC_FORMAT_FLAG_8_BIT,
            1,
            1000,
            &offsets,
        );
        let file_size = data.len() as u64;

        let info = parse_ape_header_from_data(&data, file_size).unwrap();

        assert_eq!(info.bits_per_sample, 8);
    }

    #[test]
    fn parse_legacy_24bit_flag() {
        let offsets = vec![400];
        let data = build_test_ape_legacy(
            3900,
            44100,
            2,
            2000,
            MAC_FORMAT_FLAG_24_BIT,
            1,
            1000,
            &offsets,
        );
        let file_size = data.len() as u64;

        let info = parse_ape_header_from_data(&data, file_size).unwrap();

        assert_eq!(info.bits_per_sample, 24);
    }

    #[test]
    fn bad_magic() {
        let data = b"RIFF____";
        let err = parse_ape_header_from_data(data, 8).unwrap_err();
        assert!(matches!(err, ApeError::BadMagic));
    }

    #[test]
    fn unsupported_version_low() {
        let mut data = Vec::new();
        data.extend_from_slice(b"MAC ");
        data.extend_from_slice(&3799u16.to_le_bytes());
        let err = parse_ape_header_from_data(&data, data.len() as u64).unwrap_err();
        assert!(matches!(err, ApeError::UnsupportedVersion(3799)));
    }

    #[test]
    fn unsupported_version_high() {
        let mut data = Vec::new();
        data.extend_from_slice(b"MAC ");
        data.extend_from_slice(&3991u16.to_le_bytes());
        let err = parse_ape_header_from_data(&data, data.len() as u64).unwrap_err();
        assert!(matches!(err, ApeError::UnsupportedVersion(3991)));
    }

    #[test]
    fn too_short() {
        let data = b"MAC";
        let err = parse_ape_header_from_data(data, 3).unwrap_err();
        assert!(matches!(err, ApeError::TooShort { .. }));
    }
}

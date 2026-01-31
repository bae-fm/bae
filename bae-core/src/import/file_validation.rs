//! File header validation for detecting corrupt or incomplete downloads.
//!
//! Simple magic-byte and size checks. No deep parsing, no heuristics.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// Check if a file is a valid FLAC by reading the header.
///
/// Validates:
/// 1. `fLaC` magic bytes
/// 2. STREAMINFO block header (block type 0, length 34)
/// 3. File size vs declared sample count (catches obvious truncation)
///
/// Returns `Ok(true)` if valid, `Ok(false)` if corrupt/truncated, `Err` on IO failure.
pub fn is_valid_flac(path: &Path) -> io::Result<bool> {
    let mut file = fs::File::open(path)?;
    let file_size = file.metadata()?.len();

    if file_size == 0 {
        return Ok(false);
    }

    // Read fLaC magic (4 bytes) + STREAMINFO block header (4 bytes) + STREAMINFO data (34 bytes)
    let mut header = [0u8; 42];
    let bytes_read = file.read(&mut header)?;
    if bytes_read < 42 {
        return Ok(false);
    }

    // Check fLaC magic
    if &header[0..4] != b"fLaC" {
        return Ok(false);
    }

    // STREAMINFO block header: byte 4 is (last-block-flag << 7 | block_type)
    let block_type = header[4] & 0x7F;
    if block_type != 0 {
        return Ok(false);
    }

    // Block length (3 bytes big-endian)
    let block_length = ((header[5] as u32) << 16) | ((header[6] as u32) << 8) | (header[7] as u32);
    if block_length != 34 {
        return Ok(false);
    }

    // Parse STREAMINFO (34 bytes starting at offset 8)
    // Bytes 10-11 (within STREAMINFO, offset 18-19 in header): bits per sample, channels, sample rate
    // Layout of STREAMINFO:
    //   [0..1]   min block size
    //   [2..3]   max block size
    //   [4..6]   min frame size
    //   [7..9]   max frame size
    //   [10..13] sample rate (20 bits) | channels-1 (3 bits) | bits_per_sample-1 (5 bits) | total_samples high 4 bits
    //   [14..17] total_samples low 32 bits
    //   [18..33] MD5 signature

    let si = &header[8..42]; // STREAMINFO data

    // Sample rate: top 20 bits of si[10..14]
    let sample_rate = ((si[10] as u32) << 12) | ((si[11] as u32) << 4) | ((si[12] as u32) >> 4);

    // Channels: bits 4-6 of si[12] (3 bits, stored as channels-1)
    let channels = ((si[12] >> 1) & 0x07) as u32 + 1;

    // Bits per sample: bit 0 of si[12] (high bit) + bits 7-4 of si[13] (4 bits) = 5 bits total, stored as bps-1
    let bps = ((((si[12] & 0x01) as u32) << 4) | ((si[13] >> 4) as u32)) + 1;

    // Total samples: 4 bits from si[13] (low nibble) + 32 bits from si[14..18]
    let total_samples_high = (si[13] & 0x0F) as u64;
    let total_samples_low = ((si[14] as u64) << 24)
        | ((si[15] as u64) << 16)
        | ((si[16] as u64) << 8)
        | (si[17] as u64);
    let total_samples = (total_samples_high << 32) | total_samples_low;

    // If total_samples is 0, it means unknown length (valid in streaming FLAC) — skip size check
    if total_samples == 0 || sample_rate == 0 {
        return Ok(true);
    }

    // Compute expected raw PCM size
    let bytes_per_sample = bps.div_ceil(8);
    let expected_raw_size = total_samples * channels as u64 * bytes_per_sample as u64;

    // If actual file size < 10% of raw PCM size, it's obviously truncated.
    // FLAC typically compresses to 50-70% of raw, so 10% is extremely generous.
    let min_expected = expected_raw_size / 10;
    if file_size < min_expected {
        return Ok(false);
    }

    Ok(true)
}

/// Check if an image file has valid magic bytes for its extension.
///
/// Unknown extensions are assumed valid (don't block on formats we don't recognize).
/// Returns `Ok(true)` if valid, `Ok(false)` if corrupt, `Err` on IO failure.
pub fn is_valid_image(path: &Path) -> io::Result<bool> {
    let file_size = fs::metadata(path)?.len();
    if file_size == 0 {
        return Ok(false);
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    // Read enough bytes for the longest magic we check (PNG = 8 bytes, WEBP = 12 bytes)
    let mut buf = [0u8; 12];
    let mut file = fs::File::open(path)?;
    let bytes_read = file.read(&mut buf)?;

    match ext.as_str() {
        "jpg" | "jpeg" => {
            // JPEG: FF D8 FF
            Ok(bytes_read >= 3 && buf[0] == 0xFF && buf[1] == 0xD8 && buf[2] == 0xFF)
        }
        "png" => {
            // PNG: 89 50 4E 47 0D 0A 1A 0A
            Ok(bytes_read >= 8 && buf[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
        }
        "webp" => {
            // WEBP: RIFF____WEBP (bytes 0-3 = "RIFF", bytes 8-11 = "WEBP")
            Ok(bytes_read >= 12 && &buf[0..4] == b"RIFF" && &buf[8..12] == b"WEBP")
        }
        "gif" => {
            // GIF: GIF8 (GIF87a or GIF89a)
            Ok(bytes_read >= 4 && &buf[0..4] == b"GIF8")
        }
        "bmp" => {
            // BMP: BM
            Ok(bytes_read >= 2 && &buf[0..2] == b"BM")
        }
        _ => {
            // Unknown extension — assume valid
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Build a minimal valid FLAC header (42 bytes).
    /// total_samples and sample_rate can be customized for truncation tests.
    fn make_flac_header(sample_rate: u32, channels: u32, bps: u32, total_samples: u64) -> Vec<u8> {
        let mut buf = Vec::new();

        // fLaC magic
        buf.extend_from_slice(b"fLaC");

        // STREAMINFO block header: type=0, length=34
        buf.push(0x00); // last-block=0, type=0
        buf.push(0x00);
        buf.push(0x00);
        buf.push(34); // length=34

        // STREAMINFO data (34 bytes)
        // min block size (2 bytes)
        buf.extend_from_slice(&[0x10, 0x00]); // 4096
                                              // max block size (2 bytes)
        buf.extend_from_slice(&[0x10, 0x00]); // 4096
                                              // min frame size (3 bytes)
        buf.extend_from_slice(&[0x00, 0x00, 0x00]);
        // max frame size (3 bytes)
        buf.extend_from_slice(&[0x00, 0x00, 0x00]);

        // sample rate (20 bits) | channels-1 (3 bits) | bps-1 (5 bits) | total_samples high (4 bits)
        let ch_minus_1 = (channels - 1) & 0x07;
        let bps_minus_1 = (bps - 1) & 0x1F;
        let ts_high = ((total_samples >> 32) & 0x0F) as u32;

        // Byte 10: sample_rate >> 12
        buf.push((sample_rate >> 12) as u8);
        // Byte 11: (sample_rate >> 4) & 0xFF
        buf.push(((sample_rate >> 4) & 0xFF) as u8);
        // Byte 12: (sample_rate & 0x0F) << 4 | (ch_minus_1 << 1) | (bps_minus_1 >> 4)
        buf.push(
            (((sample_rate & 0x0F) as u8) << 4)
                | ((ch_minus_1 as u8) << 1)
                | ((bps_minus_1 >> 4) as u8),
        );
        // Byte 13: (bps_minus_1 & 0x0F) << 4 | ts_high
        buf.push(((bps_minus_1 & 0x0F) as u8) << 4 | ts_high as u8);

        // total_samples low 32 bits (4 bytes)
        let ts_low = (total_samples & 0xFFFFFFFF) as u32;
        buf.push((ts_low >> 24) as u8);
        buf.push(((ts_low >> 16) & 0xFF) as u8);
        buf.push(((ts_low >> 8) & 0xFF) as u8);
        buf.push((ts_low & 0xFF) as u8);

        // MD5 signature (16 bytes of zeros)
        buf.extend_from_slice(&[0u8; 16]);

        assert_eq!(buf.len(), 42);
        buf
    }

    fn write_temp_file(extension: &str, data: &[u8]) -> NamedTempFile {
        let mut file = tempfile::Builder::new()
            .suffix(&format!(".{}", extension))
            .tempfile()
            .unwrap();
        file.write_all(data).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_valid_flac_magic() {
        // 44100 Hz, 2 channels, 16-bit, 10 million samples (~226 sec)
        // Raw PCM = 10_000_000 * 2 * 2 = 40_000_000 bytes
        // We need file size >= 4_000_000 (10%)
        let mut data = make_flac_header(44100, 2, 16, 10_000_000);
        // Pad to a realistic size (5 MB — well above 10% threshold)
        data.resize(5_000_000, 0xAA);
        let file = write_temp_file("flac", &data);
        assert!(is_valid_flac(file.path()).unwrap());
    }

    #[test]
    fn test_invalid_flac_magic() {
        let data = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05];
        let file = write_temp_file("flac", &data);
        assert!(!is_valid_flac(file.path()).unwrap());
    }

    #[test]
    fn test_truncated_flac() {
        // Valid header declaring 10M samples at 44100/2ch/16bit → raw = 40MB
        // 10% threshold = 4MB. File is only 1KB → truncated.
        let mut data = make_flac_header(44100, 2, 16, 10_000_000);
        data.resize(1024, 0xAA);
        let file = write_temp_file("flac", &data);
        assert!(!is_valid_flac(file.path()).unwrap());
    }

    #[test]
    fn test_zero_byte_flac() {
        let file = write_temp_file("flac", &[]);
        assert!(!is_valid_flac(file.path()).unwrap());
    }

    #[test]
    fn test_flac_unknown_length() {
        // total_samples = 0 means unknown length — should pass (skip size check)
        let mut data = make_flac_header(44100, 2, 16, 0);
        data.resize(100, 0xAA); // tiny file but that's OK with unknown length
        let file = write_temp_file("flac", &data);
        assert!(is_valid_flac(file.path()).unwrap());
    }

    #[test]
    fn test_valid_jpeg_magic() {
        let data = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let file = write_temp_file("jpg", &data);
        assert!(is_valid_image(file.path()).unwrap());
    }

    #[test]
    fn test_valid_png_magic() {
        let data = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
        let file = write_temp_file("png", &data);
        assert!(is_valid_image(file.path()).unwrap());
    }

    #[test]
    fn test_valid_webp_magic() {
        let data = b"RIFF\x00\x00\x00\x00WEBP";
        let file = write_temp_file("webp", data);
        assert!(is_valid_image(file.path()).unwrap());
    }

    #[test]
    fn test_valid_gif_magic() {
        let data = b"GIF89a\x00\x00";
        let file = write_temp_file("gif", data);
        assert!(is_valid_image(file.path()).unwrap());
    }

    #[test]
    fn test_valid_bmp_magic() {
        let data = b"BM\x00\x00\x00\x00";
        let file = write_temp_file("bmp", data);
        assert!(is_valid_image(file.path()).unwrap());
    }

    #[test]
    fn test_invalid_image_magic() {
        // Random bytes that don't match JPEG magic
        let data = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05];
        let file = write_temp_file("jpg", &data);
        assert!(!is_valid_image(file.path()).unwrap());
    }

    #[test]
    fn test_invalid_png_magic() {
        let data = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let file = write_temp_file("png", &data);
        assert!(!is_valid_image(file.path()).unwrap());
    }

    #[test]
    fn test_zero_byte_image() {
        let file = write_temp_file("jpg", &[]);
        assert!(!is_valid_image(file.path()).unwrap());
    }

    #[test]
    fn test_unknown_image_extension_assumed_valid() {
        let data = [0x00, 0x01, 0x02, 0x03];
        let file = write_temp_file("tiff", &data);
        assert!(is_valid_image(file.path()).unwrap());
    }
}

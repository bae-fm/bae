//! File header validation for detecting corrupt downloads.
//!
//! Simple magic-byte and minimum-size checks. No deep parsing, no heuristics.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// Check if a file is a valid FLAC by reading the header.
///
/// Validates:
/// 1. `fLaC` magic bytes
/// 2. STREAMINFO block header (block type 0, length 34)
///
/// Returns `Ok(true)` if valid, `Ok(false)` if corrupt, `Err` on IO failure.
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

    Ok(true)
}

/// Check if an audio file has valid magic bytes for its format.
///
/// Dispatches by file extension:
/// - `.flac` → `is_valid_flac()`
/// - `.mp3` → `is_valid_mp3()` (ID3v2 header or MPEG sync word)
/// - `.ape` → `is_valid_ape()` ("MAC " magic)
/// - Unknown → `Ok(true)` (don't block on unrecognized formats)
pub fn is_valid_audio(path: &Path) -> io::Result<bool> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "flac" => is_valid_flac(path),
        "mp3" => is_valid_mp3(path),
        "ape" => is_valid_ape(path),
        "wav" => has_magic_at(path, &[(0, b"RIFF"), (8, b"WAVE")]),
        "aif" | "aiff" | "aifc" => is_valid_aiff(path),
        "ogg" | "oga" | "opus" => has_magic_at(path, &[(0, b"OggS")]),
        "wv" => has_magic_at(path, &[(0, b"wvpk")]),
        "dsf" => has_magic_at(path, &[(0, b"DSD ")]),
        "dff" => has_magic_at(path, &[(0, b"FRM8")]),
        _ => Ok(true),
    }
}

fn has_magic_at(path: &Path, checks: &[(usize, &[u8])]) -> io::Result<bool> {
    let file_size = fs::metadata(path)?.len();
    if file_size == 0 {
        return Ok(false);
    }
    let read_len = checks
        .iter()
        .map(|(offset, magic)| offset + magic.len())
        .max()
        .unwrap_or(0);
    let mut buf = vec![0u8; read_len];
    let mut file = fs::File::open(path)?;
    let bytes_read = file.read(&mut buf)?;
    if bytes_read < read_len {
        return Ok(false);
    }
    Ok(checks
        .iter()
        .all(|(offset, magic)| &buf[*offset..*offset + magic.len()] == *magic))
}

fn is_valid_aiff(path: &Path) -> io::Result<bool> {
    let file_size = fs::metadata(path)?.len();
    if file_size == 0 {
        return Ok(false);
    }
    let mut buf = [0u8; 12];
    let mut file = fs::File::open(path)?;
    let bytes_read = file.read(&mut buf)?;
    if bytes_read < buf.len() {
        return Ok(false);
    }
    Ok(&buf[0..4] == b"FORM" && (&buf[8..12] == b"AIFF" || &buf[8..12] == b"AIFC"))
}

/// Check if a file is a valid MP3 by reading the header.
///
/// Validates either:
/// - ID3v2 header (first 3 bytes == "ID3")
/// - MPEG sync word (first byte 0xFF, second byte top 3 bits set)
///
/// Returns `Ok(true)` if valid, `Ok(false)` if corrupt, `Err` on IO failure.
pub fn is_valid_mp3(path: &Path) -> io::Result<bool> {
    let mut file = fs::File::open(path)?;
    let file_size = file.metadata()?.len();

    if file_size == 0 {
        return Ok(false);
    }

    let mut buf = [0u8; 3];
    let bytes_read = file.read(&mut buf)?;
    if bytes_read < 3 {
        return Ok(false);
    }

    // ID3v2 header
    if &buf[0..3] == b"ID3" {
        return Ok(true);
    }

    // MPEG sync word: 0xFF followed by byte with top 3 bits set
    if buf[0] == 0xFF && (buf[1] & 0xE0) == 0xE0 {
        return Ok(true);
    }

    Ok(false)
}

/// Check if a file is a valid APE (Monkey's Audio) by reading the header.
///
/// Validates the "MAC " magic bytes (first 4 bytes).
///
/// Returns `Ok(true)` if valid, `Ok(false)` if corrupt, `Err` on IO failure.
pub fn is_valid_ape(path: &Path) -> io::Result<bool> {
    let mut file = fs::File::open(path)?;
    let file_size = file.metadata()?.len();

    if file_size == 0 {
        return Ok(false);
    }

    let mut buf = [0u8; 4];
    let bytes_read = file.read(&mut buf)?;
    if bytes_read < 4 {
        return Ok(false);
    }

    Ok(&buf[0..4] == b"MAC ")
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
    /// total_samples and sample_rate can be customized for header-shape tests.
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
        // 44100 Hz, 2 channels, 16-bit, 10 million samples (~226 sec).
        let mut data = make_flac_header(44100, 2, 16, 10_000_000);
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
    fn test_highly_compressed_flac_header_is_valid() {
        // Valid header declaring 10M samples at 44100/2ch/16bit. A tiny body
        // can still be a valid FLAC when the audio is highly compressible.
        let mut data = make_flac_header(44100, 2, 16, 10_000_000);
        data.resize(1024, 0xAA);
        let file = write_temp_file("flac", &data);
        assert!(is_valid_flac(file.path()).unwrap());
    }

    #[test]
    fn test_zero_byte_flac() {
        let file = write_temp_file("flac", &[]);
        assert!(!is_valid_flac(file.path()).unwrap());
    }

    #[test]
    fn test_flac_unknown_length() {
        // total_samples = 0 is a valid streaming-length STREAMINFO value.
        let mut data = make_flac_header(44100, 2, 16, 0);
        data.resize(100, 0xAA);
        let file = write_temp_file("flac", &data);
        assert!(is_valid_flac(file.path()).unwrap());
    }

    #[test]
    fn test_flac_rejected_when_first_block_is_not_streaminfo() {
        // STREAMINFO must be the first metadata block (type 0); any other type
        // at the front is not a well-formed FLAC stream.
        let mut data = make_flac_header(44100, 2, 16, 10_000_000);
        data[4] = 0x01; // block type 1 (PADDING) instead of 0 (STREAMINFO)
        data.resize(5_000_000, 0xAA);
        let file = write_temp_file("flac", &data);
        assert!(!is_valid_flac(file.path()).unwrap());
    }

    #[test]
    fn test_flac_rejected_when_streaminfo_length_wrong() {
        // STREAMINFO is fixed at 34 bytes; a different declared length is malformed.
        let mut data = make_flac_header(44100, 2, 16, 10_000_000);
        data[7] = 33; // declare a 33-byte block
        data.resize(5_000_000, 0xAA);
        let file = write_temp_file("flac", &data);
        assert!(!is_valid_flac(file.path()).unwrap());
    }

    #[test]
    fn test_flac_rejected_when_header_truncated() {
        // Correct magic but fewer than the 42 bytes needed to read STREAMINFO.
        let file = write_temp_file("flac", b"fLaC\x00\x00\x00");
        assert!(!is_valid_flac(file.path()).unwrap());
    }

    #[test]
    fn test_flac_zero_sample_rate_header_shape_is_valid() {
        // Semantic format validation belongs to probe/decode; this header
        // validator only checks the FLAC container prefix and STREAMINFO shape.
        let mut data = make_flac_header(0, 2, 16, 10_000_000);
        data.resize(100, 0xAA);
        let file = write_temp_file("flac", &data);
        assert!(is_valid_flac(file.path()).unwrap());
    }

    #[test]
    fn test_valid_mp3_id3v2() {
        // ID3v2 header
        let data = b"ID3\x04\x00\x00\x00\x00\x00\x00";
        let file = write_temp_file("mp3", data);
        assert!(is_valid_mp3(file.path()).unwrap());
    }

    #[test]
    fn test_valid_mp3_sync_word() {
        // MPEG sync word: 0xFF 0xFB (MPEG1 Layer3)
        let data = [0xFF, 0xFB, 0x90, 0x00];
        let file = write_temp_file("mp3", &data);
        assert!(is_valid_mp3(file.path()).unwrap());
    }

    #[test]
    fn test_invalid_mp3() {
        let data = [0x00, 0x01, 0x02, 0x03];
        let file = write_temp_file("mp3", &data);
        assert!(!is_valid_mp3(file.path()).unwrap());
    }

    #[test]
    fn test_zero_byte_mp3() {
        let file = write_temp_file("mp3", &[]);
        assert!(!is_valid_mp3(file.path()).unwrap());
    }

    #[test]
    fn test_valid_ape() {
        let data = b"MAC \x00\x00\x00\x00";
        let file = write_temp_file("ape", data);
        assert!(is_valid_ape(file.path()).unwrap());
    }

    #[test]
    fn test_invalid_ape() {
        let data = [0x00, 0x01, 0x02, 0x03];
        let file = write_temp_file("ape", &data);
        assert!(!is_valid_ape(file.path()).unwrap());
    }

    #[test]
    fn test_zero_byte_ape() {
        let file = write_temp_file("ape", &[]);
        assert!(!is_valid_ape(file.path()).unwrap());
    }

    #[test]
    fn test_is_valid_audio_dispatch() {
        // FLAC
        let mut data = make_flac_header(44100, 2, 16, 10_000_000);
        data.resize(5_000_000, 0xAA);
        let file = write_temp_file("flac", &data);
        assert!(is_valid_audio(file.path()).unwrap());

        // MP3
        let file = write_temp_file("mp3", b"ID3\x04\x00\x00\x00\x00\x00\x00");
        assert!(is_valid_audio(file.path()).unwrap());

        // APE
        let file = write_temp_file("ape", b"MAC \x00\x00\x00\x00");
        assert!(is_valid_audio(file.path()).unwrap());

        // WAV
        let file = write_temp_file("wav", b"RIFF\x24\x00\x00\x00WAVE");
        assert!(is_valid_audio(file.path()).unwrap());

        // AIFF
        let file = write_temp_file("aiff", b"FORM\x00\x00\x00\x04AIFF");
        assert!(is_valid_audio(file.path()).unwrap());

        // Ogg
        let file = write_temp_file("ogg", b"OggS\x00\x02");
        assert!(is_valid_audio(file.path()).unwrap());

        // WavPack
        let file = write_temp_file("wv", b"wvpk\x00\x00\x00\x00");
        assert!(is_valid_audio(file.path()).unwrap());

        // DSF
        let file = write_temp_file("dsf", b"DSD \x00\x00\x00\x00");
        assert!(is_valid_audio(file.path()).unwrap());

        // DFF
        let file = write_temp_file("dff", b"FRM8\x00\x00\x00\x04DSD ");
        assert!(is_valid_audio(file.path()).unwrap());
    }

    #[test]
    fn test_new_audio_magic_rejects_malformed_headers() {
        for ext in [
            "wav", "aif", "aiff", "aifc", "ogg", "oga", "opus", "wv", "dsf", "dff",
        ] {
            let file = write_temp_file(ext, b"NOPE");
            assert!(
                !is_valid_audio(file.path()).unwrap(),
                "{ext} malformed bytes must be rejected"
            );
        }
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

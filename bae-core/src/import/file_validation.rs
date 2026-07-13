//! File header validation for detecting corrupt downloads.
//!
//! Magic-byte and minimum-size checks. No deep parsing, no heuristics.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// One accepted header shape: every `(offset, magic)` in it must match.
type Magic = &'static [(usize, &'static [u8])];

const APE: Magic = &[(0, b"MAC ")];
const WAV: Magic = &[(0, b"RIFF"), (8, b"WAVE")];
const AIFF: Magic = &[(0, b"FORM"), (8, b"AIFF")];
const AIFC: Magic = &[(0, b"FORM"), (8, b"AIFC")];
const OGG: Magic = &[(0, b"OggS")];
const WAVPACK: Magic = &[(0, b"wvpk")];
const DSF: Magic = &[(0, b"DSD ")];
const DFF: Magic = &[(0, b"FRM8")];

const JPEG: Magic = &[(0, &[0xFF, 0xD8, 0xFF])];
const PNG: Magic = &[(0, &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])];
const WEBP: Magic = &[(0, b"RIFF"), (8, b"WEBP")];
const GIF: Magic = &[(0, b"GIF8")]; // GIF87a and GIF89a share the prefix.
const BMP: Magic = &[(0, b"BM")];

/// The file's first `len` bytes, or `None` if it is empty or shorter than `len`
/// — either way it cannot be carrying the header in question.
fn read_header(path: &Path, len: usize) -> io::Result<Option<Vec<u8>>> {
    if fs::metadata(path)?.len() == 0 {
        return Ok(None);
    }
    let mut buf = vec![0u8; len];
    let read = fs::File::open(path)?.read(&mut buf)?;
    Ok((read >= len).then_some(buf))
}

/// Whether the file opens with any one of `alternatives` (AIFF *or* AIFC, say).
fn matches_any(path: &Path, alternatives: &[Magic]) -> io::Result<bool> {
    let len = alternatives
        .iter()
        .flat_map(|checks| checks.iter())
        .map(|(offset, magic)| offset + magic.len())
        .max()
        .unwrap_or(0);
    let Some(header) = read_header(path, len)? else {
        return Ok(false);
    };
    Ok(alternatives.iter().any(|checks| {
        checks
            .iter()
            .all(|(offset, magic)| &header[*offset..*offset + magic.len()] == *magic)
    }))
}

/// Whether the file's header is a well-formed FLAC prefix: the `fLaC` magic
/// followed by a STREAMINFO block header (block type 0, length 34). More than a
/// magic check, which is why it isn't a table entry.
pub(crate) fn is_valid_flac(path: &Path) -> io::Result<bool> {
    // fLaC magic (4) + STREAMINFO block header (4) + STREAMINFO data (34).
    let Some(header) = read_header(path, 42)? else {
        return Ok(false);
    };
    if &header[0..4] != b"fLaC" {
        return Ok(false);
    }
    // Byte 4 is (last-block flag << 7 | block type). STREAMINFO must come first.
    if header[4] & 0x7F != 0 {
        return Ok(false);
    }
    // Block length: 3 bytes big-endian. STREAMINFO is fixed at 34.
    let block_length = ((header[5] as u32) << 16) | ((header[6] as u32) << 8) | (header[7] as u32);
    Ok(block_length == 34)
}

/// Whether the file opens with an ID3v2 tag or an MPEG sync word. The sync word
/// is a bit pattern rather than a fixed magic, so this isn't a table entry either.
pub(crate) fn is_valid_mp3(path: &Path) -> io::Result<bool> {
    let Some(header) = read_header(path, 3)? else {
        return Ok(false);
    };
    // Sync word: 0xFF, then a byte with its top three bits set.
    Ok(&header[0..3] == b"ID3" || (header[0] == 0xFF && header[1] & 0xE0 == 0xE0))
}

/// Whether the file opens with APE (Monkey's Audio)'s `MAC ` magic.
pub(crate) fn is_valid_ape(path: &Path) -> io::Result<bool> {
    matches_any(path, &[APE])
}

/// Whether an audio file's magic bytes match its extension. An extension with no
/// known magic (`.m4a`, say) yields `Ok(true)` — an unrecognized format is not
/// evidence of corruption, so it must not block the import.
pub(crate) fn is_valid_audio(path: &Path) -> io::Result<bool> {
    match extension(path).as_str() {
        "flac" => is_valid_flac(path),
        "mp3" => is_valid_mp3(path),
        "ape" => is_valid_ape(path),
        "wav" => matches_any(path, &[WAV]),
        "aif" | "aiff" | "aifc" => matches_any(path, &[AIFF, AIFC]),
        "ogg" | "oga" | "opus" => matches_any(path, &[OGG]),
        "wv" => matches_any(path, &[WAVPACK]),
        "dsf" => matches_any(path, &[DSF]),
        "dff" => matches_any(path, &[DFF]),
        _ => Ok(true),
    }
}

/// Whether an image file's magic bytes match its extension. As with
/// [`is_valid_audio`], an unrecognized extension is assumed valid.
pub(crate) fn is_valid_image(path: &Path) -> io::Result<bool> {
    // Ahead of the dispatch, unlike the audio side: an empty file is corrupt
    // whatever its extension, so an unrecognized one must not pass it through.
    if fs::metadata(path)?.len() == 0 {
        return Ok(false);
    }
    match extension(path).as_str() {
        "jpg" | "jpeg" => matches_any(path, &[JPEG]),
        "png" => matches_any(path, &[PNG]),
        "webp" => matches_any(path, &[WEBP]),
        "gif" => matches_any(path, &[GIF]),
        "bmp" => matches_any(path, &[BMP]),
        _ => Ok(true),
    }
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default()
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
    fn truncated_mp3_header_is_rejected() {
        // is_valid_mp3 needs 3 bytes to see either the ID3 tag or the MPEG
        // sync word; a 1- or 2-byte file is a short read, not a valid MP3.
        for bytes in [&b"\xff"[..], &b"ID"[..]] {
            let file = write_temp_file("mp3", bytes);
            assert!(
                !is_valid_mp3(file.path()).unwrap(),
                "{}-byte mp3 must be rejected",
                bytes.len(),
            );
        }
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
    fn is_valid_audio_dispatches_by_extension() {
        // A recognized audio extension routes to its magic validator: valid
        // magic passes, and wrong magic fails — proving it dispatched rather
        // than falling through the unknown-extension passthrough.
        let mut flac = make_flac_header(44100, 2, 16, 10_000_000);
        flac.resize(5_000_000, 0xAA);
        let file = write_temp_file("flac", &flac);
        assert!(is_valid_audio(file.path()).unwrap());
        let file = write_temp_file("flac", b"NOPE and then some more bytes");
        assert!(!is_valid_audio(file.path()).unwrap());

        // An unrecognized extension is assumed valid (passthrough branch).
        let file = write_temp_file("xyz", b"whatever bytes");
        assert!(is_valid_audio(file.path()).unwrap());
    }

    #[test]
    fn is_valid_audio_accepts_aiff_and_aifc_via_dispatch() {
        // is_valid_aiff accepts both the AIFF and AIFC FORM types, reached
        // through the aiff/aif/aifc dispatch arms.
        for ext in ["aiff", "aif", "aifc"] {
            let file = write_temp_file(ext, b"FORM\x00\x00\x00\x04AIFC");
            assert!(
                is_valid_audio(file.path()).unwrap(),
                ".{ext} with AIFC magic must be accepted",
            );
        }
        let file = write_temp_file("aiff", b"FORM\x00\x00\x00\x04AIFF");
        assert!(is_valid_audio(file.path()).unwrap());
    }

    #[test]
    fn is_valid_audio_accepts_oga_and_opus_via_dispatch() {
        // .ogg, .oga, and .opus all dispatch to the OggS magic check.
        for ext in ["ogg", "oga", "opus"] {
            let file = write_temp_file(ext, b"OggS\x00\x02\x00\x00");
            assert!(
                is_valid_audio(file.path()).unwrap(),
                ".{ext} with OggS magic must be accepted",
            );
        }
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
    fn truncated_image_headers_are_rejected() {
        // Each image magic is checked against `bytes_read`, so a file shorter
        // than its magic (webp=12, gif=4, bmp=2) is rejected without an
        // out-of-bounds read.
        for (ext, bytes) in [
            ("webp", &b"RIFF\x00\x00\x00\x00WEB"[..]), // 11 bytes, needs 12
            ("gif", &b"GIF"[..]),                      // 3 bytes, needs 4
            ("bmp", &b"B"[..]),                        // 1 byte, needs 2
        ] {
            let file = write_temp_file(ext, bytes);
            assert!(
                !is_valid_image(file.path()).unwrap(),
                ".{ext} with a truncated magic must be rejected",
            );
        }
    }

    #[test]
    fn test_unknown_image_extension_assumed_valid() {
        let data = [0x00, 0x01, 0x02, 0x03];
        let file = write_temp_file("tiff", &data);
        assert!(is_valid_image(file.path()).unwrap());
    }

    /// Zero-byte files are rejected by every magic validator (audio and
    /// image), collapsing the former per-format zero-byte tests.
    #[test]
    fn zero_byte_files_are_rejected() {
        for ext in [
            "flac", "mp3", "ape", "wav", "aiff", "ogg", "wv", "dsf", "dff",
        ] {
            let file = write_temp_file(ext, &[]);
            assert!(
                !is_valid_audio(file.path()).unwrap(),
                "zero-byte .{ext} audio must be rejected",
            );
        }
        for ext in ["jpg", "png", "webp", "gif", "bmp"] {
            let file = write_temp_file(ext, &[]);
            assert!(
                !is_valid_image(file.path()).unwrap(),
                "zero-byte .{ext} image must be rejected",
            );
        }
    }
}

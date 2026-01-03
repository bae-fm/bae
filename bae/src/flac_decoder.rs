//! FLAC decoding using libflac-sys FFI bindings.
//!
//! Provides a safe wrapper around libFLAC's streaming decoder.
//! More tolerant of non-standard FLAC files than symphonia.
extern crate libflac_sys;
use tracing::{debug, info};
/// Decoded FLAC metadata and samples
pub struct DecodedFlac {
    pub samples: Vec<i32>,
    pub sample_rate: u32,
    pub channels: u32,
    pub bits_per_sample: u32,
}
/// Decode a FLAC file (or range) to PCM samples using libFLAC.
///
/// If start_ms/end_ms are provided, seeks to that time range.
/// Returns interleaved i32 samples.
pub fn decode_flac_range(
    flac_data: &[u8],
    start_ms: Option<u64>,
    end_ms: Option<u64>,
) -> Result<DecodedFlac, String> {
    struct DecoderState {
        file_data: Vec<u8>,
        file_pos: usize,
        samples: Vec<i32>,
        sample_rate: u32,
        channels: u32,
        bits_per_sample: u32,
        total_samples: u64,
        current_sample: u64,
        start_sample: Option<u64>,
        end_sample: Option<u64>,
        collecting: bool,
    }
    let state = DecoderState {
        file_data: flac_data.to_vec(),
        file_pos: 0,
        samples: Vec::new(),
        sample_rate: 0,
        channels: 0,
        bits_per_sample: 0,
        total_samples: 0,
        current_sample: 0,
        start_sample: None,
        end_sample: None,
        collecting: start_ms.is_none(),
    };
    extern "C" fn read_callback(
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        buffer: *mut u8,
        bytes: *mut libc::size_t,
        client_data: *mut libc::c_void,
    ) -> libflac_sys::FLAC__StreamDecoderReadStatus {
        let state = unsafe { &mut *(client_data as *mut DecoderState) };
        let bytes_needed = unsafe { *bytes };
        let remaining = state.file_data.len().saturating_sub(state.file_pos);
        if remaining == 0 {
            unsafe { *bytes = 0 };
            return libflac_sys::FLAC__STREAM_DECODER_READ_STATUS_END_OF_STREAM;
        }
        let to_read = bytes_needed.min(remaining);
        unsafe {
            std::ptr::copy_nonoverlapping(
                state.file_data.as_ptr().add(state.file_pos),
                buffer,
                to_read,
            );
        }
        state.file_pos += to_read;
        unsafe { *bytes = to_read as libc::size_t };
        libflac_sys::FLAC__STREAM_DECODER_READ_STATUS_CONTINUE
    }
    extern "C" fn seek_callback(
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        absolute_byte_offset: u64,
        client_data: *mut libc::c_void,
    ) -> libflac_sys::FLAC__StreamDecoderSeekStatus {
        let state = unsafe { &mut *(client_data as *mut DecoderState) };
        if absolute_byte_offset as usize > state.file_data.len() {
            return libflac_sys::FLAC__STREAM_DECODER_SEEK_STATUS_ERROR;
        }
        state.file_pos = absolute_byte_offset as usize;
        libflac_sys::FLAC__STREAM_DECODER_SEEK_STATUS_OK
    }
    extern "C" fn tell_callback(
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        absolute_byte_offset: *mut u64,
        client_data: *mut libc::c_void,
    ) -> libflac_sys::FLAC__StreamDecoderTellStatus {
        let state = unsafe { &*(client_data as *const DecoderState) };
        unsafe { *absolute_byte_offset = state.file_pos as u64 };
        libflac_sys::FLAC__STREAM_DECODER_TELL_STATUS_OK
    }
    extern "C" fn length_callback(
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        stream_length: *mut u64,
        client_data: *mut libc::c_void,
    ) -> libflac_sys::FLAC__StreamDecoderLengthStatus {
        let state = unsafe { &*(client_data as *const DecoderState) };
        unsafe { *stream_length = state.file_data.len() as u64 };
        libflac_sys::FLAC__STREAM_DECODER_LENGTH_STATUS_OK
    }
    extern "C" fn eof_callback(
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        client_data: *mut libc::c_void,
    ) -> libflac_sys::FLAC__bool {
        let state = unsafe { &*(client_data as *const DecoderState) };
        (state.file_pos >= state.file_data.len()) as libflac_sys::FLAC__bool
    }
    extern "C" fn write_callback(
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        frame: *const libflac_sys::FLAC__Frame,
        buffer: *const *const i32,
        client_data: *mut libc::c_void,
    ) -> libflac_sys::FLAC__StreamDecoderWriteStatus {
        let state = unsafe { &mut *(client_data as *mut DecoderState) };
        let frame_ref = unsafe { &*frame };
        let blocksize = frame_ref.header.blocksize as usize;
        let channels = frame_ref.header.channels as usize;
        let frame_sample =
            if frame_ref.header.number_type == libflac_sys::FLAC__FRAME_NUMBER_TYPE_SAMPLE_NUMBER {
                unsafe { frame_ref.header.number.sample_number }
            } else {
                state.current_sample
            };
        if let Some(start) = state.start_sample {
            if frame_sample + blocksize as u64 > start {
                state.collecting = true;
            }
        }
        if let Some(end) = state.end_sample {
            if frame_sample >= end {
                return libflac_sys::FLAC__STREAM_DECODER_WRITE_STATUS_ABORT;
            }
        }
        if state.collecting {
            for i in 0..blocksize {
                let sample_pos = frame_sample + i as u64;
                if let Some(start) = state.start_sample {
                    if sample_pos < start {
                        continue;
                    }
                }
                if let Some(end) = state.end_sample {
                    if sample_pos >= end {
                        break;
                    }
                }
                for ch in 0..channels {
                    let channel_buffer = unsafe { *buffer.add(ch) };
                    let sample = unsafe { *channel_buffer.add(i) };
                    state.samples.push(sample);
                }
            }
        }
        state.current_sample = frame_sample + blocksize as u64;
        libflac_sys::FLAC__STREAM_DECODER_WRITE_STATUS_CONTINUE
    }
    extern "C" fn metadata_callback(
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        metadata: *const libflac_sys::FLAC__StreamMetadata,
        client_data: *mut libc::c_void,
    ) {
        let state = unsafe { &mut *(client_data as *mut DecoderState) };
        let metadata_ref = unsafe { &*metadata };
        if metadata_ref.type_ == libflac_sys::FLAC__METADATA_TYPE_STREAMINFO {
            let streaminfo = unsafe { &metadata_ref.data.stream_info };
            state.sample_rate = streaminfo.sample_rate;
            state.channels = streaminfo.channels;
            state.bits_per_sample = streaminfo.bits_per_sample;
            state.total_samples = streaminfo.total_samples;
            if let Some(start_ms) = state.start_sample.map(|_| ()).and(None::<u64>).or(None) {
                state.start_sample = Some((start_ms * state.sample_rate as u64) / 1000);
            }
        }
    }
    extern "C" fn error_callback(
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        _status: libflac_sys::FLAC__StreamDecoderErrorStatus,
        _client_data: *mut libc::c_void,
    ) {
    }
    let decoder = unsafe { libflac_sys::FLAC__stream_decoder_new() };
    if decoder.is_null() {
        return Err("Failed to create FLAC decoder".to_string());
    }
    let mut state = Box::new(state);
    let state_ptr = state.as_mut() as *mut DecoderState as *mut libc::c_void;
    let init_status = unsafe {
        libflac_sys::FLAC__stream_decoder_init_stream(
            decoder,
            Some(read_callback),
            Some(seek_callback),
            Some(tell_callback),
            Some(length_callback),
            Some(eof_callback),
            Some(write_callback),
            Some(metadata_callback),
            Some(error_callback),
            state_ptr,
        )
    };
    if init_status != libflac_sys::FLAC__STREAM_DECODER_INIT_STATUS_OK {
        unsafe { libflac_sys::FLAC__stream_decoder_delete(decoder) };
        return Err(format!("Failed to initialize decoder: {}", init_status));
    }
    let metadata_ok =
        unsafe { libflac_sys::FLAC__stream_decoder_process_until_end_of_metadata(decoder) };
    if metadata_ok == 0 {
        unsafe {
            libflac_sys::FLAC__stream_decoder_finish(decoder);
            libflac_sys::FLAC__stream_decoder_delete(decoder);
        }
        return Err("Failed to process metadata".to_string());
    }
    if state.sample_rate > 0 {
        if let Some(ms) = start_ms {
            state.start_sample = Some((ms * state.sample_rate as u64) / 1000);
            state.collecting = false;
        }
        if let Some(ms) = end_ms {
            state.end_sample = Some((ms * state.sample_rate as u64) / 1000);
        }
    }
    if let Some(start_sample) = state.start_sample {
        let seek_ok =
            unsafe { libflac_sys::FLAC__stream_decoder_seek_absolute(decoder, start_sample) };
        if seek_ok == 0 {
            debug!("Seek failed, will scan from beginning");
            unsafe {
                libflac_sys::FLAC__stream_decoder_reset(decoder);
                libflac_sys::FLAC__stream_decoder_process_until_end_of_metadata(decoder);
            }
        } else {
            state.collecting = true;
            state.current_sample = start_sample;
        }
    }
    loop {
        let process_ok = unsafe { libflac_sys::FLAC__stream_decoder_process_single(decoder) };
        let decoder_state = unsafe { libflac_sys::FLAC__stream_decoder_get_state(decoder) };
        if decoder_state == libflac_sys::FLAC__STREAM_DECODER_END_OF_STREAM {
            break;
        }
        if decoder_state == libflac_sys::FLAC__STREAM_DECODER_ABORTED {
            break;
        }
        if process_ok == 0 {
            break;
        }
        if let Some(end) = state.end_sample {
            if state.current_sample >= end {
                break;
            }
        }
    }
    unsafe {
        libflac_sys::FLAC__stream_decoder_finish(decoder);
        libflac_sys::FLAC__stream_decoder_delete(decoder);
    }
    debug!(
        "Decoded {} samples ({} frames) from FLAC",
        state.samples.len(),
        state.samples.len() / state.channels.max(1) as usize
    );
    Ok(DecodedFlac {
        samples: state.samples,
        sample_rate: state.sample_rate,
        channels: state.channels,
        bits_per_sample: state.bits_per_sample,
    })
}

/// A seek point entry for building seektables
#[derive(Debug, Clone, Copy)]
pub struct SeekEntry {
    pub sample_number: u64,
    pub byte_offset: u64,
}

/// Scan result with seektable entries
pub struct FlacScanResult {
    pub seektable: Vec<SeekEntry>,
}

/// Build a frame-accurate seektable by scanning for FLAC frame sync codes.
///
/// FLAC frames start with a 14-bit sync code (0x3FFE) followed by frame header.
/// We validate each potential frame header and read the actual sample number
/// from the header (not tracked manually) to ensure accuracy.
pub fn scan_flac_frames(flac_data: &[u8]) -> Result<FlacScanResult, String> {
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
            // STREAMINFO block layout:
            // Bytes 0-1: min block size (16 bits)
            // Bytes 2-3: max block size (16 bits)
            // Bytes 4-6: min frame size (24 bits)
            // Bytes 7-9: max frame size (24 bits)
            // Bytes 10-17: sample rate, channels, bits, total samples
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
            if validate_frame_header(flac_data, scan_pos) {
                // Read actual sample number from frame header
                if let Some(sample_number) =
                    parse_frame_sample_number(flac_data, scan_pos, min_block_size)
                {
                    // Only add if this is a new sample position (avoid duplicates)
                    if last_sample_number.is_none() || sample_number > last_sample_number.unwrap() {
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

    Ok(FlacScanResult { seektable })
}

/// Parse the sample number from a FLAC frame header.
///
/// FLAC frames encode either a frame number (fixed block size) or sample number
/// (variable block size) using a UTF-8-like variable-length encoding.
fn parse_frame_sample_number(data: &[u8], pos: usize, min_block_size: u32) -> Option<u64> {
    if pos + 5 >= data.len() {
        return None;
    }

    // Byte 1 bit 0: blocking strategy (0 = fixed, 1 = variable)
    let variable_block_size = (data[pos + 1] & 0x01) != 0;

    // The frame/sample number starts at byte 4 (after sync, block size, sample rate, channel info)
    let num_start = pos + 4;

    // Decode UTF-8-like variable length number
    let first_byte = data[num_start];
    let (value, _bytes_used) = if first_byte & 0x80 == 0 {
        // 1-byte: 0xxxxxxx
        (first_byte as u64, 1)
    } else if first_byte & 0xE0 == 0xC0 {
        // 2-byte: 110xxxxx 10xxxxxx
        if num_start + 1 >= data.len() {
            return None;
        }
        let val = ((first_byte as u64 & 0x1F) << 6) | (data[num_start + 1] as u64 & 0x3F);
        (val, 2)
    } else if first_byte & 0xF0 == 0xE0 {
        // 3-byte: 1110xxxx 10xxxxxx 10xxxxxx
        if num_start + 2 >= data.len() {
            return None;
        }
        let val = ((first_byte as u64 & 0x0F) << 12)
            | ((data[num_start + 1] as u64 & 0x3F) << 6)
            | (data[num_start + 2] as u64 & 0x3F);
        (val, 3)
    } else if first_byte & 0xF8 == 0xF0 {
        // 4-byte: 11110xxx 10xxxxxx 10xxxxxx 10xxxxxx
        if num_start + 3 >= data.len() {
            return None;
        }
        let val = ((first_byte as u64 & 0x07) << 18)
            | ((data[num_start + 1] as u64 & 0x3F) << 12)
            | ((data[num_start + 2] as u64 & 0x3F) << 6)
            | (data[num_start + 3] as u64 & 0x3F);
        (val, 4)
    } else if first_byte & 0xFC == 0xF8 {
        // 5-byte: 111110xx 10xxxxxx 10xxxxxx 10xxxxxx 10xxxxxx
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
        // 6-byte: 1111110x 10xxxxxx 10xxxxxx 10xxxxxx 10xxxxxx 10xxxxxx
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
        // 7-byte: 11111110 10xxxxxx 10xxxxxx 10xxxxxx 10xxxxxx 10xxxxxx 10xxxxxx
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
        // Value is sample number directly
        Some(value)
    } else {
        // Value is frame number, multiply by block size to get sample number
        // Use min_block_size from STREAMINFO (for fixed block size files, all frames
        // except possibly the last have this size)
        let block_size = if min_block_size > 0 {
            min_block_size as u64
        } else {
            4096 // Default fallback
        };
        Some(value * block_size)
    }
}

/// Validate that a potential frame sync is actually a valid FLAC frame header.
///
/// Performs two levels of validation:
/// 1. Basic sanity checks on header fields (reserved values, valid codes)
/// 2. CRC-8 verification over the entire header
///
/// The CRC-8 check is essential because random compressed audio data frequently
/// contains 0xFF 0xF8 patterns that pass basic field validation but fail CRC.
fn validate_frame_header(data: &[u8], pos: usize) -> bool {
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

    // Block size code 0 is reserved
    if block_size_code == 0 {
        return false;
    }

    // Sample rate code 15 is invalid
    if sample_rate_code == 15 {
        return false;
    }

    // Byte 3: channel assignment and sample size
    let channel_assignment = (data[pos + 3] >> 4) & 0x0F;
    let sample_size_code = (data[pos + 3] >> 1) & 0x07;

    // Channel assignment > 10 is reserved
    if channel_assignment > 10 {
        return false;
    }

    // Sample size code 3 and 7 are reserved
    if sample_size_code == 3 || sample_size_code == 7 {
        return false;
    }

    // Reserved bit (LSB of byte 3) must be 0
    if data[pos + 3] & 0x01 != 0 {
        return false;
    }

    // Calculate header length to find CRC-8 position
    let header_len =
        match calculate_frame_header_length(data, pos, block_size_code, sample_rate_code) {
            Some(len) => len,
            None => return false,
        };

    // Verify CRC-8 over header bytes
    if pos + header_len >= data.len() {
        return false;
    }

    let crc_pos = pos + header_len - 1;
    let expected_crc = data[crc_pos];
    let computed_crc = compute_crc8(&data[pos..crc_pos]);

    computed_crc == expected_crc
}

/// Calculate the length of a FLAC frame header (including CRC-8).
///
/// Header structure:
/// - 2 bytes: sync code + blocking strategy
/// - 1 byte: block size code + sample rate code
/// - 1 byte: channel assignment + sample size + reserved
/// - 1-7 bytes: UTF-8 encoded frame/sample number
/// - 0-2 bytes: optional block size (if code 0110 or 0111)
/// - 0-2 bytes: optional sample rate (if code 1100, 1101, 1110)
/// - 1 byte: CRC-8
fn calculate_frame_header_length(
    data: &[u8],
    pos: usize,
    block_size_code: u8,
    sample_rate_code: u8,
) -> Option<usize> {
    // Fixed part: sync (2) + block/rate (1) + channel/size (1) = 4 bytes
    let mut len = 4;

    // UTF-8 encoded frame/sample number starts at byte 4
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
        return None; // Invalid UTF-8 sequence
    };

    len += utf8_len;

    // Optional block size (8 or 16 bits)
    if block_size_code == 6 {
        len += 1; // 8-bit block size - 1
    } else if block_size_code == 7 {
        len += 2; // 16-bit block size - 1
    }

    // Optional sample rate (8 or 16 bits)
    if sample_rate_code == 12 {
        len += 1; // 8-bit sample rate in kHz
    } else if sample_rate_code == 13 || sample_rate_code == 14 {
        len += 2; // 16-bit sample rate
    }

    // CRC-8 at the end
    len += 1;

    Some(len)
}

/// Compute CRC-8 using FLAC's polynomial (0x07).
fn compute_crc8(data: &[u8]) -> u8 {
    // FLAC CRC-8 lookup table (polynomial 0x07)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flac_encoder::encode_to_flac;

    #[test]
    fn test_decode_roundtrip() {
        let original_samples: Vec<i32> = (0..44100)
            .map(|i| ((i as f64 * 0.1).sin() * 16000.0) as i32)
            .collect();
        let flac_data = encode_to_flac(&original_samples, 44100, 1, 16).unwrap();
        let decoded = decode_flac_range(&flac_data, None, None).unwrap();
        assert_eq!(decoded.sample_rate, 44100);
        assert_eq!(decoded.channels, 1);
        assert_eq!(decoded.bits_per_sample, 16);
        assert_eq!(decoded.samples.len(), original_samples.len());
    }
}

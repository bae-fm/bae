//! FLAC decoding using libflac-sys FFI bindings.
//!
//! Provides a safe wrapper around libFLAC's streaming decoder.
//! More tolerant of non-standard FLAC files than symphonia.
extern crate libflac_sys;
use tracing::debug;
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

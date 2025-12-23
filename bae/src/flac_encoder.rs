//! FLAC encoding using libflac-sys FFI bindings.
//!
//! Provides a safe wrapper around libFLAC's streaming encoder.

extern crate libflac_sys;

/// Encode PCM samples to FLAC format.
///
/// Takes interleaved i32 samples and returns the encoded FLAC data as bytes.
/// This uses libFLAC's streaming encoder with in-memory callbacks.
pub fn encode_to_flac(
    samples: &[i32],
    sample_rate: u32,
    channels: u32,
    bits_per_sample: u32,
) -> Result<Vec<u8>, String> {
    struct EncoderState {
        output: Vec<u8>,
    }

    extern "C" fn write_callback(
        _encoder: *const libflac_sys::FLAC__StreamEncoder,
        buffer: *const libflac_sys::FLAC__byte,
        bytes: usize,
        _samples: u32,
        _current_frame: u32,
        client_data: *mut libc::c_void,
    ) -> libflac_sys::FLAC__StreamEncoderWriteStatus {
        let state = unsafe { &mut *(client_data as *mut EncoderState) };

        let slice = unsafe { std::slice::from_raw_parts(buffer, bytes) };
        state.output.extend_from_slice(slice);

        libflac_sys::FLAC__STREAM_ENCODER_WRITE_STATUS_OK
    }

    let encoder = unsafe { libflac_sys::FLAC__stream_encoder_new() };
    if encoder.is_null() {
        return Err("Failed to create FLAC encoder".to_string());
    }

    // Configure encoder
    unsafe {
        if libflac_sys::FLAC__stream_encoder_set_channels(encoder, channels) == 0 {
            libflac_sys::FLAC__stream_encoder_delete(encoder);
            return Err("Failed to set channels".to_string());
        }
        if libflac_sys::FLAC__stream_encoder_set_bits_per_sample(encoder, bits_per_sample) == 0 {
            libflac_sys::FLAC__stream_encoder_delete(encoder);
            return Err("Failed to set bits per sample".to_string());
        }
        if libflac_sys::FLAC__stream_encoder_set_sample_rate(encoder, sample_rate) == 0 {
            libflac_sys::FLAC__stream_encoder_delete(encoder);
            return Err("Failed to set sample rate".to_string());
        }
        if libflac_sys::FLAC__stream_encoder_set_blocksize(encoder, 4096) == 0 {
            libflac_sys::FLAC__stream_encoder_delete(encoder);
            return Err("Failed to set block size".to_string());
        }

        // Set compression level (5 is default, good balance)
        if libflac_sys::FLAC__stream_encoder_set_compression_level(encoder, 5) == 0 {
            libflac_sys::FLAC__stream_encoder_delete(encoder);
            return Err("Failed to set compression level".to_string());
        }

        // Calculate total samples
        let total_samples = (samples.len() / channels as usize) as u64;
        if libflac_sys::FLAC__stream_encoder_set_total_samples_estimate(encoder, total_samples) == 0
        {
            libflac_sys::FLAC__stream_encoder_delete(encoder);
            return Err("Failed to set total samples estimate".to_string());
        }
    }

    let mut state = Box::new(EncoderState { output: Vec::new() });
    let state_ptr = state.as_mut() as *mut EncoderState as *mut libc::c_void;

    // Initialize encoder with write callback only (no seek/tell/metadata for simple in-memory encoding)
    let init_status = unsafe {
        libflac_sys::FLAC__stream_encoder_init_stream(
            encoder,
            Some(write_callback),
            None, // seek_callback
            None, // tell_callback
            None, // metadata_callback
            state_ptr,
        )
    };

    if init_status != libflac_sys::FLAC__STREAM_ENCODER_INIT_STATUS_OK {
        let error_msg = match init_status {
            libflac_sys::FLAC__STREAM_ENCODER_INIT_STATUS_ENCODER_ERROR => "Encoder error",
            libflac_sys::FLAC__STREAM_ENCODER_INIT_STATUS_UNSUPPORTED_CONTAINER => {
                "Unsupported container"
            }
            libflac_sys::FLAC__STREAM_ENCODER_INIT_STATUS_INVALID_CALLBACKS => "Invalid callbacks",
            libflac_sys::FLAC__STREAM_ENCODER_INIT_STATUS_INVALID_NUMBER_OF_CHANNELS => {
                "Invalid number of channels"
            }
            libflac_sys::FLAC__STREAM_ENCODER_INIT_STATUS_INVALID_BITS_PER_SAMPLE => {
                "Invalid bits per sample"
            }
            libflac_sys::FLAC__STREAM_ENCODER_INIT_STATUS_INVALID_SAMPLE_RATE => {
                "Invalid sample rate"
            }
            libflac_sys::FLAC__STREAM_ENCODER_INIT_STATUS_INVALID_BLOCK_SIZE => {
                "Invalid block size"
            }
            libflac_sys::FLAC__STREAM_ENCODER_INIT_STATUS_INVALID_MAX_LPC_ORDER => {
                "Invalid max LPC order"
            }
            libflac_sys::FLAC__STREAM_ENCODER_INIT_STATUS_INVALID_QLP_COEFF_PRECISION => {
                "Invalid QLP coefficient precision"
            }
            libflac_sys::FLAC__STREAM_ENCODER_INIT_STATUS_BLOCK_SIZE_TOO_SMALL_FOR_LPC_ORDER => {
                "Block size too small for LPC order"
            }
            libflac_sys::FLAC__STREAM_ENCODER_INIT_STATUS_NOT_STREAMABLE => "Not streamable",
            libflac_sys::FLAC__STREAM_ENCODER_INIT_STATUS_INVALID_METADATA => "Invalid metadata",
            libflac_sys::FLAC__STREAM_ENCODER_INIT_STATUS_ALREADY_INITIALIZED => {
                "Already initialized"
            }
            _ => "Unknown error",
        };
        unsafe { libflac_sys::FLAC__stream_encoder_delete(encoder) };
        return Err(format!("Failed to initialize encoder: {}", error_msg));
    }

    // Process samples (interleaved format)
    let num_samples = samples.len() / channels as usize;
    let ok = unsafe {
        libflac_sys::FLAC__stream_encoder_process_interleaved(
            encoder,
            samples.as_ptr(),
            num_samples as u32,
        )
    };

    if ok == 0 {
        let encoder_state = unsafe { libflac_sys::FLAC__stream_encoder_get_state(encoder) };
        let error_msg = match encoder_state {
            libflac_sys::FLAC__STREAM_ENCODER_OK => "OK (unexpected failure)",
            libflac_sys::FLAC__STREAM_ENCODER_UNINITIALIZED => "Uninitialized",
            libflac_sys::FLAC__STREAM_ENCODER_OGG_ERROR => "OGG error",
            libflac_sys::FLAC__STREAM_ENCODER_VERIFY_DECODER_ERROR => "Verify decoder error",
            libflac_sys::FLAC__STREAM_ENCODER_VERIFY_MISMATCH_IN_AUDIO_DATA => {
                "Verify mismatch in audio data"
            }
            libflac_sys::FLAC__STREAM_ENCODER_CLIENT_ERROR => "Client error",
            libflac_sys::FLAC__STREAM_ENCODER_IO_ERROR => "I/O error",
            libflac_sys::FLAC__STREAM_ENCODER_FRAMING_ERROR => "Framing error",
            libflac_sys::FLAC__STREAM_ENCODER_MEMORY_ALLOCATION_ERROR => "Memory allocation error",
            _ => "Unknown error",
        };
        unsafe {
            libflac_sys::FLAC__stream_encoder_finish(encoder);
            libflac_sys::FLAC__stream_encoder_delete(encoder);
        }
        return Err(format!("Failed to encode samples: {}", error_msg));
    }

    // Finish encoding
    let finish_ok = unsafe { libflac_sys::FLAC__stream_encoder_finish(encoder) };
    unsafe { libflac_sys::FLAC__stream_encoder_delete(encoder) };

    if finish_ok == 0 {
        return Err("Failed to finish encoding".to_string());
    }

    Ok(state.output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_silence() {
        // 1 second of silence at 44100Hz stereo 16-bit
        let samples = vec![0i32; 44100 * 2];
        let result = encode_to_flac(&samples, 44100, 2, 16);
        assert!(result.is_ok());

        let flac_data = result.unwrap();
        // Should have valid FLAC header
        assert!(flac_data.len() > 42); // Minimum FLAC header size
        assert_eq!(&flac_data[0..4], b"fLaC"); // FLAC magic number
    }

    #[test]
    fn test_encode_mono() {
        let samples = vec![0i32; 44100];
        let result = encode_to_flac(&samples, 44100, 1, 16);
        assert!(result.is_ok());
    }
}

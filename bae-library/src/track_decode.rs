use std::sync::Arc;

/// One track's stored audio, open for a single decode into a
/// [`DecodedSink`](bae_audio::DecodedSink).
pub struct TrackDecode {
    decode: bae_track_audio::StreamDecodeParams,
    sample_rate: u32,
    channels: u32,
    /// The library's, which a read of the decode that panicked is reported
    /// to.
    diagnostics: bae_diagnostics::Diagnostics,
}

impl TrackDecode {
    pub(crate) fn new(
        decode: bae_track_audio::StreamDecodeParams,
        sample_rate: u32,
        channels: u32,
        diagnostics: bae_diagnostics::Diagnostics,
    ) -> Self {
        Self { decode, sample_rate, channels, diagnostics }
    }

    /// Decode the track into `sink`, announcing its stored format first: why
    /// it stopped short, if it did. A read that panicked is reported as a bug
    /// and fails the decode with the panic's message. Blocking; run it off the
    /// async runtime.
    pub fn run_to_sink(
        &self,
        sink: &mut dyn bae_audio::DecodedSink,
        cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<(), String> {
        self.decode.run_to_sink(self.sample_rate, self.channels, sink, cancel).map_err(|error| {
            match error {
                bae_audio::DecodeError::ReadPanicked(bug) => {
                    self.diagnostics.panicked(bug).to_string()
                }
                error => error.to_string(),
            }
        })
    }

    /// Block until every file the decode reads has delivered its first bytes,
    /// or return why one can't be read, so a caller learns of an unreadable
    /// file before it commits to output. Blocking; run it off the async
    /// runtime.
    pub fn wait_readable(&self) -> Result<(), bae_audio::sparse_buffer::BufferStop> {
        self.decode.wait_readable()
    }
}

#[cfg(test)]
#[path = "track_decode_tests.rs"]
mod tests;

use super::*;

/// The import verifier's sink decode. Invalid compressed packets are discarded
/// and reported through [`DecodedSink::add_discarded_packet_count`], so the sink
/// can decide whether decoded-frame coverage proves the track remains usable.
/// Whole-file collection and save keep rejecting the same packet; streaming
/// playback applies the same discard policy and reports the packet as an error.
///
/// Seeks as playback does: a jump by byte to a recorded landing
/// (`seek_to_byte`), or by sample; the lead-in is trimmed at `start_at_sample`
/// and the decode stops at `stop_at_sample`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_audio_to_verifying_sink(
    buffer: SharedSparseBuffer,
    seek_to_byte: Option<u64>,
    seek_to_sample: Option<u64>,
    start_at_sample: Option<u64>,
    stop_at_sample: Option<u64>,
    sink: &mut dyn DecodedSink,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), DecodeError> {
    decode_audio_to_sink_with_handling(
        buffer,
        seek_to_byte,
        seek_to_sample,
        start_at_sample,
        stop_at_sample,
        InvalidPacketHandling::Discard,
        sink,
        cancel,
    )
}
